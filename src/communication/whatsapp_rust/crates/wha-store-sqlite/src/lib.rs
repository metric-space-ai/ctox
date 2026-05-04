//! SQLite-backed implementation of every `wha-store` trait.
//!
//! Mirrors the on-disk layout used by whatsmeow's `sqlstore` package — the
//! schema lives in [`schema`] and is applied idempotently by [`SqliteStore::open`].
//! Each rusqlite call is wrapped in `tokio::task::spawn_blocking` so the async
//! traits don't violate the runtime contract.
//!
//! Locking strategy: SQLite is single-writer, so we keep one [`Connection`]
//! behind an `Arc<Mutex<…>>`. The mutex is held only across a single blocking
//! op; nothing in this crate awaits while holding it.

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use rand::rngs::OsRng;
use rusqlite::{params, Connection, OptionalExtension};

use wha_crypto::{KeyPair, PreKey};
use wha_store::persist::{decode_device, encode_blob, DeviceBlob};
use wha_store::{
    AppStateMutationMacStore, AppStateSyncKey, AppStateSyncKeyStore, Device, IdentityStore,
    InspectStore, LidStore, MsgSecretStore, NewsletterKeyStore, PreKeyStore, PrivacyTokenStore,
    SenderKeyStore, SessionStore, StoreError,
};
use wha_types::Jid;

pub mod schema;

/// Concrete store backed by a single SQLite connection.
#[derive(Clone)]
pub struct SqliteStore {
    pub(crate) conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Open (or create) a SQLite database at `path` and run all pending
    /// migrations. Pass `":memory:"` for an ephemeral test DB.
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let mut conn = Connection::open(path).map_err(map_err)?;
        // Better defaults for a single-file device store.
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "foreign_keys", "ON").map_err(map_err)?;
        conn.pragma_update(None, "synchronous", "NORMAL").ok();

        schema::apply(&mut conn).map_err(map_err)?;

        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Hand the underlying connection to a closure on a blocking thread.
    ///
    /// Every public async method uses this; it keeps the rusqlite call off
    /// the tokio reactor and translates errors uniformly.
    async fn run<F, T>(&self, f: F) -> Result<T, StoreError>
    where
        F: FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock();
            f(&mut guard)
        })
        .await
        .map_err(|e| StoreError::Backend(format!("join error: {e}")))?
        .map_err(map_err)
    }
}

impl SqliteStore {
    /// Persist `device` into the singleton row of the `device` table. The
    /// blob format is the one defined by `wha-store::persist`. Mirrors
    /// whatsmeow's `Container.PutDevice` for the persistable subset of
    /// [`Device`] (the trait-object store handles aren't part of the blob —
    /// they're reattached by [`Self::load_device`]).
    pub async fn save_device(self: &Arc<Self>, device: &Device) -> Result<(), StoreError> {
        let blob = encode_blob(&DeviceBlob::from_device(device));
        // Container schema (v5): one row per paired device, keyed by AD-JID.
        // Pre-pairing devices have `id == None`; we slot them under the
        // `__legacy__:<reg_id>` placeholder so a single-account DB still
        // works without breaking when the user finally pairs (the
        // post-pair save replaces the row keyed by the real JID).
        let jid = device
            .id
            .as_ref()
            .map(|j| j.to_string())
            .unwrap_or_else(|| format!("__legacy__:{}", device.registration_id));
        let lid = device.lid.as_ref().map(|j| j.to_string());
        self.run(move |c| {
            c.execute(
                "INSERT INTO device (jid, lid, blob) VALUES (?1, ?2, ?3)
                 ON CONFLICT(jid) DO UPDATE SET lid = excluded.lid, blob = excluded.blob",
                params![jid, lid, blob],
            )?;
            Ok(())
        })
        .await
    }

    /// Load the singleton device blob from the `device` table, decode it,
    /// and reattach `self` as every store handle. Returns `Ok(None)` when
    /// the table is empty (i.e. the caller has never paired on this DB).
    /// Mirrors `Container.GetFirstDevice` upstream.
    pub async fn load_device(self: &Arc<Self>) -> Result<Option<Device>, StoreError> {
        // Container-aware: `load_device` is the "first device" convenience.
        // For deterministic ordering we pick the row with the smallest jid.
        let raw: Option<Vec<u8>> = self
            .run(|c| {
                c.query_row(
                    "SELECT blob FROM device ORDER BY jid LIMIT 1",
                    [],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
            })
            .await?;

        let bytes = match raw {
            Some(b) => b,
            None => return Ok(None),
        };

        let blob = decode_device(&bytes)?;
        let (noise_key, identity_key, signed_pre_key) = blob.rebuild_keys();
        let id_jid = blob.id_jid()?;
        let lid_jid = blob.lid_jid()?;

        Ok(Some(Device {
            noise_key,
            identity_key,
            signed_pre_key,
            registration_id: blob.registration_id,
            adv_secret_key: blob.adv_secret_key,

            id: id_jid,
            lid: lid_jid,
            platform: blob.platform,
            business_name: blob.business_name,
            push_name: blob.push_name,

            initialized: blob.initialized,
            deleted: false,
            tc_token: None,

            identities: self.clone(),
            sessions: self.clone(),
            pre_keys: self.clone(),
            sender_keys: self.clone(),
            app_state_keys: self.clone(),
            app_state_mutations: self.clone(),
            lids: self.clone(),
            msg_secrets: self.clone(),
            privacy_tokens: self.clone(),
            newsletter_keys: self.clone(),
        }))
    }

    /// Build a fresh [`Device`] backed entirely by this SQLite store, for
    /// the very first run before the user has paired. Mirrors
    /// `MemoryStore::new_device` exactly: 14-bit registration id, fresh
    /// noise + identity keypairs, signed pre-key with id=1.
    ///
    /// Crucially, the freshly-minted signed pre-key is also written into
    /// the `pre_keys` table, so a subsequent `decrypt_pkmsg` that looks
    /// the key up via `pre_keys.get_pre_key(id)` finds it. `decrypt_pkmsg`
    /// itself reads the signed pre-key directly off `device.signed_pre_key`
    /// (see `crates/wha-client/src/recv_message.rs`), but other paths
    /// (notification, retry) rely on store presence too.
    pub fn new_device(self: &Arc<Self>) -> Device {
        let mut rng = OsRng;
        let noise_key = KeyPair::generate(&mut rng);
        let identity_key = KeyPair::generate(&mut rng);
        let signed = PreKey::new(1, KeyPair::generate(&mut rng))
            .signed_by(&identity_key)
            .expect("sign signed pre-key");
        let mut adv_secret_key = [0u8; 32];
        use rand::RngCore;
        rng.fill_bytes(&mut adv_secret_key);

        // Persist the signed pre-key in the prekeys table so anything that
        // looks it up via the trait API still finds it. We use a direct
        // blocking call here because `new_device` is sync; callers run it
        // on init, off the hot async path.
        let priv_bytes = signed.key_pair.private;
        let id = signed.key_id;
        {
            let guard = self.conn.lock();
            // Best-effort insert. A duplicate id (re-running new_device on
            // an existing DB) is benign — we just keep the existing row.
            let _ = guard.execute(
                "INSERT OR IGNORE INTO whatsmeow_pre_keys (key_id, key, uploaded) VALUES (?1, ?2, 0)",
                params![id as i64, priv_bytes.as_slice()],
            );
        }

        Device {
            noise_key,
            identity_key,
            signed_pre_key: signed,
            registration_id: rand::random::<u32>() & 0x3FFF, // 14 bits, mirroring upstream
            adv_secret_key,

            id: None,
            lid: None,
            platform: String::new(),
            business_name: String::new(),
            push_name: String::new(),

            initialized: false,
            deleted: false,
            tc_token: None,

            identities: self.clone(),
            sessions: self.clone(),
            pre_keys: self.clone(),
            sender_keys: self.clone(),
            app_state_keys: self.clone(),
            app_state_mutations: self.clone(),
            lids: self.clone(),
            msg_secrets: self.clone(),
            privacy_tokens: self.clone(),
            newsletter_keys: self.clone(),
        }
    }
}

fn map_err(e: rusqlite::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

// --- IdentityStore -----------------------------------------------------------

#[async_trait]
impl IdentityStore for SqliteStore {
    async fn put_identity(&self, address: &str, key: [u8; 32]) -> Result<(), StoreError> {
        let address = address.to_owned();
        self.run(move |c| {
            c.execute(
                "INSERT INTO whatsmeow_identity_keys (their_id, identity) VALUES (?1, ?2)
                 ON CONFLICT(their_id) DO UPDATE SET identity = excluded.identity",
                params![address, key.as_slice()],
            )?;
            Ok(())
        })
        .await
    }

    async fn delete_all_identities(&self, phone: &str) -> Result<(), StoreError> {
        let pattern = format!("{phone}%");
        self.run(move |c| {
            c.execute(
                "DELETE FROM whatsmeow_identity_keys WHERE their_id LIKE ?1",
                params![pattern],
            )?;
            Ok(())
        })
        .await
    }

    async fn delete_identity(&self, address: &str) -> Result<(), StoreError> {
        let address = address.to_owned();
        self.run(move |c| {
            c.execute(
                "DELETE FROM whatsmeow_identity_keys WHERE their_id = ?1",
                params![address],
            )?;
            Ok(())
        })
        .await
    }

    async fn is_trusted_identity(&self, address: &str, key: [u8; 32]) -> Result<bool, StoreError> {
        let address = address.to_owned();
        self.run(move |c| {
            let existing: Option<Vec<u8>> = c
                .query_row(
                    "SELECT identity FROM whatsmeow_identity_keys WHERE their_id = ?1",
                    params![address],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(match existing {
                // Trust-on-first-use, mirroring upstream + MemoryStore.
                None => true,
                Some(bytes) => bytes.as_slice() == key.as_slice(),
            })
        })
        .await
    }
}

// --- SessionStore ------------------------------------------------------------

#[async_trait]
impl SessionStore for SqliteStore {
    async fn get_session(&self, address: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let address = address.to_owned();
        self.run(move |c| {
            c.query_row(
                "SELECT session FROM whatsmeow_sessions WHERE their_id = ?1",
                params![address],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
        })
        .await
    }

    async fn has_session(&self, address: &str) -> Result<bool, StoreError> {
        let address = address.to_owned();
        self.run(move |c| {
            let count: i64 = c.query_row(
                "SELECT COUNT(1) FROM whatsmeow_sessions WHERE their_id = ?1",
                params![address],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
        .await
    }

    async fn put_session(&self, address: &str, session: Vec<u8>) -> Result<(), StoreError> {
        let address = address.to_owned();
        self.run(move |c| {
            c.execute(
                "INSERT INTO whatsmeow_sessions (their_id, session) VALUES (?1, ?2)
                 ON CONFLICT(their_id) DO UPDATE SET session = excluded.session",
                params![address, session],
            )?;
            Ok(())
        })
        .await
    }

    async fn delete_all_sessions(&self, phone: &str) -> Result<(), StoreError> {
        let pattern = format!("{phone}%");
        self.run(move |c| {
            c.execute(
                "DELETE FROM whatsmeow_sessions WHERE their_id LIKE ?1",
                params![pattern],
            )?;
            Ok(())
        })
        .await
    }

    async fn delete_session(&self, address: &str) -> Result<(), StoreError> {
        let address = address.to_owned();
        self.run(move |c| {
            c.execute(
                "DELETE FROM whatsmeow_sessions WHERE their_id = ?1",
                params![address],
            )?;
            Ok(())
        })
        .await
    }
}

// --- PreKeyStore -------------------------------------------------------------

#[async_trait]
impl PreKeyStore for SqliteStore {
    async fn gen_one_pre_key(&self) -> Result<PreKey, StoreError> {
        // Generate the keypair off the DB lock; only the insert is blocking.
        let key_pair = KeyPair::generate(&mut OsRng);
        let priv_bytes = key_pair.private;
        self.run(move |c| {
            // Allocate a fresh id atomically. SQLite single-writer + this txn
            // is enough — no two callers will see the same MAX.
            let tx = c.transaction()?;
            let next_id: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(key_id), 0) + 1 FROM whatsmeow_pre_keys",
                    [],
                    |row| row.get(0),
                )?;
            tx.execute(
                "INSERT INTO whatsmeow_pre_keys (key_id, key, uploaded) VALUES (?1, ?2, 0)",
                params![next_id, priv_bytes.as_slice()],
            )?;
            tx.commit()?;
            let pk = PreKey::new(next_id as u32, KeyPair::from_private(priv_bytes));
            Ok(pk)
        })
        .await
    }

    async fn get_or_gen_pre_keys(&self, count: u32) -> Result<Vec<PreKey>, StoreError> {
        let mut out = Vec::with_capacity(count.max(1) as usize);
        for _ in 0..count {
            out.push(self.gen_one_pre_key().await?);
        }
        Ok(out)
    }

    async fn get_pre_key(&self, id: u32) -> Result<Option<PreKey>, StoreError> {
        self.run(move |c| {
            let row = c
                .query_row(
                    "SELECT key FROM whatsmeow_pre_keys WHERE key_id = ?1",
                    params![id as i64],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?;
            Ok(row.and_then(|bytes| {
                if bytes.len() != 32 {
                    return None;
                }
                let mut priv_bytes = [0u8; 32];
                priv_bytes.copy_from_slice(&bytes);
                Some(PreKey::new(id, KeyPair::from_private(priv_bytes)))
            }))
        })
        .await
    }

    async fn remove_pre_key(&self, id: u32) -> Result<(), StoreError> {
        self.run(move |c| {
            c.execute(
                "DELETE FROM whatsmeow_pre_keys WHERE key_id = ?1",
                params![id as i64],
            )?;
            Ok(())
        })
        .await
    }

    async fn mark_pre_keys_uploaded(&self, up_to_id: u32) -> Result<(), StoreError> {
        self.run(move |c| {
            c.execute(
                "UPDATE whatsmeow_pre_keys SET uploaded = 1 WHERE key_id <= ?1",
                params![up_to_id as i64],
            )?;
            Ok(())
        })
        .await
    }

    async fn uploaded_pre_key_count(&self) -> Result<usize, StoreError> {
        self.run(|c| {
            let count: i64 = c.query_row(
                "SELECT COUNT(1) FROM whatsmeow_pre_keys WHERE uploaded = 1",
                [],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        })
        .await
    }
}

// --- SenderKeyStore ----------------------------------------------------------

#[async_trait]
impl SenderKeyStore for SqliteStore {
    async fn put_sender_key(
        &self,
        group: &str,
        user: &str,
        session: Vec<u8>,
    ) -> Result<(), StoreError> {
        let group = group.to_owned();
        let user = user.to_owned();
        self.run(move |c| {
            c.execute(
                "INSERT INTO whatsmeow_sender_keys (chat_id, sender_id, sender_key) VALUES (?1, ?2, ?3)
                 ON CONFLICT(chat_id, sender_id) DO UPDATE SET sender_key = excluded.sender_key",
                params![group, user, session],
            )?;
            Ok(())
        })
        .await
    }

    async fn get_sender_key(&self, group: &str, user: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let group = group.to_owned();
        let user = user.to_owned();
        self.run(move |c| {
            c.query_row(
                "SELECT sender_key FROM whatsmeow_sender_keys WHERE chat_id = ?1 AND sender_id = ?2",
                params![group, user],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
        })
        .await
    }
}

// --- AppStateSyncKeyStore ----------------------------------------------------

#[async_trait]
impl AppStateSyncKeyStore for SqliteStore {
    async fn put_app_state_sync_key(
        &self,
        id: Vec<u8>,
        key: AppStateSyncKey,
    ) -> Result<(), StoreError> {
        let inserted_at = chrono_now_millis();
        self.run(move |c| {
            c.execute(
                "INSERT INTO whatsmeow_app_state_sync_keys
                    (key_id, key_data, fingerprint, timestamp, inserted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(key_id) DO UPDATE SET
                    key_data = excluded.key_data,
                    fingerprint = excluded.fingerprint,
                    timestamp = excluded.timestamp,
                    inserted_at = excluded.inserted_at",
                params![id, key.data, key.fingerprint, key.timestamp, inserted_at],
            )?;
            Ok(())
        })
        .await
    }

    async fn get_app_state_sync_key(
        &self,
        id: &[u8],
    ) -> Result<Option<AppStateSyncKey>, StoreError> {
        let id = id.to_vec();
        self.run(move |c| {
            c.query_row(
                "SELECT key_data, fingerprint, timestamp
                   FROM whatsmeow_app_state_sync_keys WHERE key_id = ?1",
                params![id],
                |row| {
                    Ok(AppStateSyncKey {
                        data: row.get(0)?,
                        fingerprint: row.get(1)?,
                        timestamp: row.get(2)?,
                    })
                },
            )
            .optional()
        })
        .await
    }

    async fn get_latest_app_state_sync_key_id(&self) -> Result<Option<Vec<u8>>, StoreError> {
        self.run(|c| {
            c.query_row(
                "SELECT key_id FROM whatsmeow_app_state_sync_keys
                  ORDER BY inserted_at DESC LIMIT 1",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
        })
        .await
    }
}

// --- AppStateMutationMacStore ------------------------------------------------

#[async_trait]
impl AppStateMutationMacStore for SqliteStore {
    async fn put_app_state_version(
        &self,
        name: &str,
        version: u64,
        hash: [u8; 128],
    ) -> Result<(), StoreError> {
        let name = name.to_owned();
        self.run(move |c| {
            c.execute(
                "INSERT INTO whatsmeow_app_state_version (name, version, hash)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(name) DO UPDATE SET
                    version = excluded.version,
                    hash = excluded.hash",
                params![name, version as i64, hash.as_slice()],
            )?;
            Ok(())
        })
        .await
    }

    async fn get_app_state_version(
        &self,
        name: &str,
    ) -> Result<Option<(u64, [u8; 128])>, StoreError> {
        let name = name.to_owned();
        self.run(move |c| {
            let row = c
                .query_row(
                    "SELECT version, hash FROM whatsmeow_app_state_version WHERE name = ?1",
                    params![name],
                    |row| {
                        let version: i64 = row.get(0)?;
                        let bytes: Vec<u8> = row.get(1)?;
                        Ok((version, bytes))
                    },
                )
                .optional()?;
            Ok(row.and_then(|(v, b)| {
                if b.len() != 128 {
                    return None;
                }
                let mut hash = [0u8; 128];
                hash.copy_from_slice(&b);
                Some((v as u64, hash))
            }))
        })
        .await
    }

    async fn delete_app_state_version(&self, name: &str) -> Result<(), StoreError> {
        let name = name.to_owned();
        self.run(move |c| {
            let tx = c.transaction()?;
            tx.execute(
                "DELETE FROM whatsmeow_app_state_version WHERE name = ?1",
                params![name],
            )?;
            tx.execute(
                "DELETE FROM whatsmeow_app_state_mutation_macs WHERE name = ?1",
                params![name],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn put_app_state_mutation_mac(
        &self,
        name: &str,
        version: u64,
        index_mac: &[u8],
        value_mac: &[u8],
    ) -> Result<(), StoreError> {
        let name = name.to_owned();
        let index_mac = index_mac.to_vec();
        let value_mac = value_mac.to_vec();
        self.run(move |c| {
            c.execute(
                "INSERT INTO whatsmeow_app_state_mutation_macs (name, version, index_mac, value_mac)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(name, version, index_mac) DO UPDATE SET
                    value_mac = excluded.value_mac",
                params![name, version as i64, index_mac, value_mac],
            )?;
            Ok(())
        })
        .await
    }

    async fn delete_app_state_mutation_macs(
        &self,
        name: &str,
        index_macs: &[Vec<u8>],
    ) -> Result<(), StoreError> {
        if index_macs.is_empty() {
            return Ok(());
        }
        let name = name.to_owned();
        let macs: Vec<Vec<u8>> = index_macs.to_vec();
        self.run(move |c| {
            let tx = c.transaction()?;
            for im in &macs {
                tx.execute(
                    "DELETE FROM whatsmeow_app_state_mutation_macs WHERE name = ?1 AND index_mac = ?2",
                    params![name, im],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn get_app_state_mutation_mac(
        &self,
        name: &str,
        index_mac: &[u8],
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let name = name.to_owned();
        let index_mac = index_mac.to_vec();
        self.run(move |c| {
            // Most-recent version wins, mirroring upstream's "newest SET wins".
            c.query_row(
                "SELECT value_mac FROM whatsmeow_app_state_mutation_macs
                  WHERE name = ?1 AND index_mac = ?2
                  ORDER BY version DESC LIMIT 1",
                params![name, index_mac],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
        })
        .await
    }
}

// --- LidStore ----------------------------------------------------------------

#[async_trait]
impl LidStore for SqliteStore {
    async fn put_lid_pn_mapping(&self, lid: Jid, pn: Jid) -> Result<(), StoreError> {
        let lid_s = lid.to_string();
        let pn_s = pn.to_string();
        self.run(move |c| {
            // Two-phase upsert. `pn` is declared UNIQUE in the schema so a
            // naive INSERT can collide with an existing row that maps a
            // *different* lid to the same pn (e.g. after a re-pair). Drop
            // any conflicting row by-pn first, then upsert by-lid.
            c.execute(
                "DELETE FROM whatsmeow_lid_map WHERE pn = ?1 AND lid != ?2",
                params![pn_s, lid_s],
            )?;
            c.execute(
                "INSERT INTO whatsmeow_lid_map (lid, pn) VALUES (?1, ?2)
                 ON CONFLICT(lid) DO UPDATE SET pn = excluded.pn",
                params![lid_s, pn_s],
            )?;
            Ok(())
        })
        .await
    }

    async fn get_pn_for_lid(&self, lid: &Jid) -> Result<Option<Jid>, StoreError> {
        let lid_s = lid.to_string();
        let raw: Option<String> = self
            .run(move |c| {
                c.query_row(
                    "SELECT pn FROM whatsmeow_lid_map WHERE lid = ?1",
                    params![lid_s],
                    |row| row.get(0),
                )
                .optional()
            })
            .await?;
        match raw {
            Some(s) => Jid::parse(&s)
                .map(Some)
                .map_err(|e| StoreError::Backend(format!("bad pn jid: {e}"))),
            None => Ok(None),
        }
    }

    async fn get_lid_for_pn(&self, pn: &Jid) -> Result<Option<Jid>, StoreError> {
        let pn_s = pn.to_string();
        let raw: Option<String> = self
            .run(move |c| {
                c.query_row(
                    "SELECT lid FROM whatsmeow_lid_map WHERE pn = ?1",
                    params![pn_s],
                    |row| row.get(0),
                )
                .optional()
            })
            .await?;
        match raw {
            Some(s) => Jid::parse(&s)
                .map(Some)
                .map_err(|e| StoreError::Backend(format!("bad lid jid: {e}"))),
            None => Ok(None),
        }
    }
}

// --- MsgSecretStore ----------------------------------------------------------

#[async_trait]
impl MsgSecretStore for SqliteStore {
    async fn put_msg_secret(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
        secret: [u8; 32],
    ) -> Result<(), StoreError> {
        let chat = chat.to_owned();
        let sender = sender.to_owned();
        let msg_id = msg_id.to_owned();
        self.run(move |c| {
            // ON CONFLICT DO NOTHING — first writer wins, mirroring upstream's
            // `putMsgSecret` (`whatsmeow/store/sqlstore/store.go`).
            c.execute(
                "INSERT INTO msg_secrets (chat, sender, msg_id, secret)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(chat, sender, msg_id) DO NOTHING",
                params![chat, sender, msg_id, secret.as_slice()],
            )?;
            Ok(())
        })
        .await
    }

    async fn get_msg_secret(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
    ) -> Result<Option<[u8; 32]>, StoreError> {
        let chat = chat.to_owned();
        let sender = sender.to_owned();
        let msg_id = msg_id.to_owned();
        self.run(move |c| {
            let row: Option<Vec<u8>> = c
                .query_row(
                    "SELECT secret FROM msg_secrets
                       WHERE chat = ?1 AND sender = ?2 AND msg_id = ?3",
                    params![chat, sender, msg_id],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(row.and_then(|bytes| {
                if bytes.len() != 32 {
                    return None;
                }
                let mut out = [0u8; 32];
                out.copy_from_slice(&bytes);
                Some(out)
            }))
        })
        .await
    }
}

// --- PrivacyTokenStore -------------------------------------------------------
//
// Backed by the `whatsmeow_privacy_tokens` table created in `schema.rs`. The
// upstream upsert (`PutPrivacyTokens`) overwrites prior rows for the same
// `their_id`; we mirror that with `ON CONFLICT(their_id) DO UPDATE`.

#[async_trait]
impl PrivacyTokenStore for SqliteStore {
    async fn put_privacy_token(
        &self,
        jid: Jid,
        token: Vec<u8>,
        timestamp: i64,
    ) -> Result<(), StoreError> {
        let key = jid.to_string();
        self.run(move |c| {
            c.execute(
                "INSERT INTO whatsmeow_privacy_tokens (their_id, token, timestamp)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(their_id) DO UPDATE SET
                    token = excluded.token,
                    timestamp = excluded.timestamp",
                params![key, token, timestamp],
            )?;
            Ok(())
        })
        .await
    }

    async fn get_privacy_token(
        &self,
        jid: &Jid,
    ) -> Result<Option<(Vec<u8>, i64)>, StoreError> {
        let key = jid.to_string();
        self.run(move |c| {
            c.query_row(
                "SELECT token, timestamp FROM whatsmeow_privacy_tokens WHERE their_id = ?1",
                params![key],
                |row| {
                    let token: Vec<u8> = row.get(0)?;
                    let ts: i64 = row.get(1)?;
                    Ok((token, ts))
                },
            )
            .optional()
        })
        .await
    }
}

// --- NewsletterKeyStore ------------------------------------------------------

#[async_trait]
impl NewsletterKeyStore for SqliteStore {
    async fn put_newsletter_key(
        &self,
        channel: &Jid,
        key: [u8; 32],
    ) -> Result<(), StoreError> {
        let chan = channel.to_string();
        self.run(move |c| {
            c.execute(
                "INSERT INTO newsletter_keys (channel, key) VALUES (?1, ?2)
                 ON CONFLICT(channel) DO UPDATE SET key = excluded.key",
                params![chan, key.as_slice()],
            )?;
            Ok(())
        })
        .await
    }

    async fn get_newsletter_key(
        &self,
        channel: &Jid,
    ) -> Result<Option<[u8; 32]>, StoreError> {
        let chan = channel.to_string();
        self.run(move |c| {
            let row: Option<Vec<u8>> = c
                .query_row(
                    "SELECT key FROM newsletter_keys WHERE channel = ?1",
                    params![chan],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(row.and_then(|bytes| {
                if bytes.len() != 32 {
                    return None;
                }
                let mut out = [0u8; 32];
                out.copy_from_slice(&bytes);
                Some(out)
            }))
        })
        .await
    }
}

// --- InspectStore ------------------------------------------------------------

#[async_trait]
impl InspectStore for SqliteStore {
    async fn list_session_addresses(&self) -> Result<Vec<String>, StoreError> {
        self.run(|c| {
            let mut stmt =
                c.prepare("SELECT their_id FROM whatsmeow_sessions ORDER BY their_id")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .await
    }

    async fn count_pre_keys(&self) -> Result<usize, StoreError> {
        self.run(|c| {
            let count: i64 =
                c.query_row("SELECT COUNT(1) FROM whatsmeow_pre_keys", [], |row| row.get(0))?;
            Ok(count as usize)
        })
        .await
    }

    async fn list_app_state_versions(
        &self,
    ) -> Result<std::collections::HashMap<String, u64>, StoreError> {
        self.run(|c| {
            let mut stmt = c.prepare("SELECT name, version FROM whatsmeow_app_state_version")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            let mut out = std::collections::HashMap::new();
            for row in rows {
                let (name, version) = row?;
                out.insert(name, version as u64);
            }
            Ok(out)
        })
        .await
    }
}

// --- Container ---------------------------------------------------------------

/// Multi-device container. Mirrors
/// `_upstream/whatsmeow/store/sqlstore/container.go::Container`. A Container
/// holds zero or more paired devices in the same SQLite file; each device
/// is keyed by its AD-JID and persisted in the `device` table (schema v5).
///
/// Single-account callers can keep using [`SqliteStore::open`] +
/// [`SqliteStore::load_device`]; under the hood the singleton path simply
/// uses the first row of the `device` table.
pub struct Container {
    store: Arc<SqliteStore>,
    path: String,
}

impl Container {
    /// Open or create a Container at `path`. Mirrors `sqlstore.New` upstream
    /// — applies pending migrations on first open.
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let store = Arc::new(SqliteStore::open(path)?);
        Ok(Self { store, path: path.to_owned() })
    }

    /// The on-disk path the Container was opened from.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get a reference-counted handle to the underlying [`SqliteStore`].
    pub fn store(&self) -> Arc<SqliteStore> {
        self.store.clone()
    }

    /// Enumerate every paired-device blob in the Container. Mirrors
    /// `Container.GetAllDevices` upstream.
    pub async fn list_devices(&self) -> Result<Vec<DeviceBlob>, StoreError> {
        let blobs: Vec<Vec<u8>> = self
            .store
            .run(|c| {
                let mut stmt = c.prepare("SELECT blob FROM device ORDER BY jid")?;
                let rows = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .await?;
        blobs.into_iter().map(|bytes| decode_device(&bytes)).collect()
    }

    /// Look up a paired device by JID. Mirrors `Container.GetDevice`. Returns
    /// the underlying [`SqliteStore`] handle on hit, `None` when no row matches.
    pub async fn get_device_by_jid(
        &self,
        jid: &Jid,
    ) -> Result<Option<Arc<SqliteStore>>, StoreError> {
        let jid_s = jid.to_string();
        let exists: Option<i64> = self
            .store
            .run(move |c| {
                c.query_row(
                    "SELECT 1 FROM device WHERE jid = ?1",
                    params![jid_s],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
            })
            .await?;
        Ok(exists.map(|_| self.store.clone()))
    }

    /// Convenience wrapper that returns the first paired device. Mirrors
    /// `Container.GetFirstDevice`.
    pub async fn get_first_device(&self) -> Result<Option<Arc<SqliteStore>>, StoreError> {
        let any: Option<i64> = self
            .store
            .run(|c| {
                c.query_row(
                    "SELECT 1 FROM device LIMIT 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
            })
            .await?;
        Ok(any.map(|_| self.store.clone()))
    }

    /// Mint a fresh [`SqliteStore`] handle for a brand-new device and
    /// persist it. Mirrors `Container.NewDevice` upstream.
    pub async fn new_device(&self) -> Result<Arc<SqliteStore>, StoreError> {
        let device = self.store.new_device();
        self.store.save_device(&device).await?;
        Ok(self.store.clone())
    }

    /// Delete a paired device by JID. Mirrors `Container.DeleteDevice`.
    pub async fn delete_device(&self, jid: &Jid) -> Result<(), StoreError> {
        let jid_s = jid.to_string();
        self.store
            .run(move |c| {
                c.execute("DELETE FROM device WHERE jid = ?1", params![jid_s])?;
                Ok(())
            })
            .await
    }
}

// `chrono` isn't a dep — `inserted_at` only needs to be monotonic across rows
// inserted from the same process, so use SystemTime millis directly.
fn chrono_now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> SqliteStore {
        SqliteStore::open(":memory:").expect("open in-memory")
    }

    #[tokio::test]
    async fn open_creates_tables() {
        let s = fresh();
        // Sanity: every required table exists and is queryable.
        let tables = [
            "whatsmeow_device",
            "whatsmeow_identity_keys",
            "whatsmeow_sessions",
            "whatsmeow_pre_keys",
            "whatsmeow_sender_keys",
            "whatsmeow_app_state_sync_keys",
            "whatsmeow_app_state_version",
            "whatsmeow_app_state_mutation_macs",
            "whatsmeow_lid_map",
            "whatsmeow_version",
            "device",
            "msg_secrets",
            "newsletter_keys",
        ];
        for t in tables {
            let table = t.to_string();
            let count: i64 = s
                .run(move |c| {
                    c.query_row(
                        "SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name = ?1",
                        params![table],
                        |row| row.get(0),
                    )
                })
                .await
                .unwrap();
            assert_eq!(count, 1, "expected table {t} to exist");
        }

        // Re-opening (well, re-applying migrations) is idempotent.
        let mut conn = Connection::open(":memory:").unwrap();
        schema::apply(&mut conn).unwrap();
        schema::apply(&mut conn).unwrap();
    }

    #[tokio::test]
    async fn identity_round_trip() {
        let s = fresh();
        let key = [3u8; 32];

        // TOFU on missing.
        assert!(s.is_trusted_identity("alice@s.whatsapp.net", key).await.unwrap());

        s.put_identity("alice@s.whatsapp.net", key).await.unwrap();
        assert!(s.is_trusted_identity("alice@s.whatsapp.net", key).await.unwrap());
        assert!(!s.is_trusted_identity("alice@s.whatsapp.net", [4u8; 32]).await.unwrap());

        // Overwrite.
        let new_key = [9u8; 32];
        s.put_identity("alice@s.whatsapp.net", new_key).await.unwrap();
        assert!(s.is_trusted_identity("alice@s.whatsapp.net", new_key).await.unwrap());

        // Delete → back to TOFU (any key trusts).
        s.delete_identity("alice@s.whatsapp.net").await.unwrap();
        assert!(s.is_trusted_identity("alice@s.whatsapp.net", [0u8; 32]).await.unwrap());
    }

    #[tokio::test]
    async fn prekey_round_trip() {
        let s = fresh();

        let a = s.gen_one_pre_key().await.unwrap();
        let b = s.gen_one_pre_key().await.unwrap();
        assert_ne!(a.key_id, b.key_id);

        // Round-trip preserves the key material.
        let fetched = s.get_pre_key(a.key_id).await.unwrap().expect("present");
        assert_eq!(fetched.key_id, a.key_id);
        assert_eq!(fetched.key_pair.private, a.key_pair.private);
        assert_eq!(fetched.key_pair.public, a.key_pair.public);

        // Initially nothing is uploaded.
        assert_eq!(s.uploaded_pre_key_count().await.unwrap(), 0);

        // Mark up to b → both are counted.
        s.mark_pre_keys_uploaded(b.key_id).await.unwrap();
        assert_eq!(s.uploaded_pre_key_count().await.unwrap(), 2);

        // Removing one drops the count.
        s.remove_pre_key(a.key_id).await.unwrap();
        assert_eq!(s.uploaded_pre_key_count().await.unwrap(), 1);
        assert!(s.get_pre_key(a.key_id).await.unwrap().is_none());

        // get_or_gen_pre_keys creates the requested number.
        let batch = s.get_or_gen_pre_keys(3).await.unwrap();
        assert_eq!(batch.len(), 3);
    }

    #[tokio::test]
    async fn save_then_load_round_trips_device_fields() {
        // Create a device on a fresh in-memory DB, mutate every persisted
        // field, save, reload, and assert equivalence on identity material
        // + post-pair JIDs + the user-visible strings.
        let store = Arc::new(SqliteStore::open(":memory:").expect("open"));
        let mut device = store.new_device();

        // Populate the optional/string fields so the round-trip exercises
        // every branch of the blob format.
        use std::str::FromStr;
        device.id = Some(wha_types::Jid::from_str("1234.5:7@s.whatsapp.net").unwrap());
        device.lid = Some(wha_types::Jid::from_str("9876@lid").unwrap());
        device.platform = "android".to_string();
        device.business_name = "Acme Inc.".to_string();
        device.push_name = "Alice".to_string();
        device.initialized = true;

        store.save_device(&device).await.expect("save");

        let loaded = store.load_device().await.expect("load").expect("present");
        assert_eq!(loaded.registration_id, device.registration_id);
        assert_eq!(loaded.noise_key.private, device.noise_key.private);
        assert_eq!(loaded.noise_key.public, device.noise_key.public);
        assert_eq!(loaded.identity_key.private, device.identity_key.private);
        assert_eq!(loaded.identity_key.public, device.identity_key.public);
        assert_eq!(loaded.signed_pre_key.key_id, device.signed_pre_key.key_id);
        assert_eq!(
            loaded.signed_pre_key.key_pair.private,
            device.signed_pre_key.key_pair.private
        );
        assert_eq!(loaded.signed_pre_key.signature, device.signed_pre_key.signature);
        assert_eq!(loaded.adv_secret_key, device.adv_secret_key);
        assert_eq!(loaded.id, device.id);
        assert_eq!(loaded.lid, device.lid);
        assert_eq!(loaded.platform, device.platform);
        assert_eq!(loaded.business_name, device.business_name);
        assert_eq!(loaded.push_name, device.push_name);
        assert_eq!(loaded.initialized, device.initialized);

        // The signed pre-key must also be in the prekeys table so trait-API
        // consumers (notification, retry) can look it up by id.
        let fetched = store.get_pre_key(device.signed_pre_key.key_id).await.unwrap();
        assert!(fetched.is_some(), "signed pre-key must be in prekeys table");

        // A second save/load (e.g. after Phase 2 logged-in updates) overwrites
        // cleanly — singleton row, no duplicates, no errors.
        let mut device2 = loaded;
        device2.push_name = "Alice (updated)".to_string();
        store.save_device(&device2).await.expect("save 2");
        let loaded2 = store.load_device().await.expect("load 2").expect("present");
        assert_eq!(loaded2.push_name, "Alice (updated)");
    }

    #[tokio::test]
    async fn load_returns_none_for_empty_db() {
        let store = Arc::new(SqliteStore::open(":memory:").expect("open"));
        let loaded = store.load_device().await.expect("load");
        assert!(loaded.is_none(), "fresh DB must have no device");
    }

    #[tokio::test]
    async fn app_state_mutation_mac_round_trip_sqlite() {
        let s = Arc::new(SqliteStore::open(":memory:").expect("open"));
        // Version round-trip.
        let mut h = [0u8; 128];
        h[5] = 0xAB;
        s.put_app_state_version("regular_low", 42, h).await.unwrap();
        assert_eq!(
            s.get_app_state_version("regular_low").await.unwrap(),
            Some((42u64, h))
        );

        // Multiple mutation MACs.
        s.put_app_state_mutation_mac("regular_low", 42, b"alpha", b"v_alpha")
            .await
            .unwrap();
        s.put_app_state_mutation_mac("regular_low", 42, b"beta", b"v_beta")
            .await
            .unwrap();

        // Newest version overrides older.
        s.put_app_state_mutation_mac("regular_low", 43, b"alpha", b"v_alpha_new")
            .await
            .unwrap();
        assert_eq!(
            s.get_app_state_mutation_mac("regular_low", b"alpha")
                .await
                .unwrap()
                .as_deref(),
            Some(b"v_alpha_new".as_ref())
        );

        // Bulk-delete leaves untouched rows alone.
        s.delete_app_state_mutation_macs("regular_low", &[b"alpha".to_vec()])
            .await
            .unwrap();
        assert!(s
            .get_app_state_mutation_mac("regular_low", b"alpha")
            .await
            .unwrap()
            .is_none());
        assert!(s
            .get_app_state_mutation_mac("regular_low", b"beta")
            .await
            .unwrap()
            .is_some());

        // delete_app_state_version cascades to the mac table.
        s.delete_app_state_version("regular_low").await.unwrap();
        assert!(s
            .get_app_state_version("regular_low")
            .await
            .unwrap()
            .is_none());
        assert!(s
            .get_app_state_mutation_mac("regular_low", b"beta")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn sender_key_round_trip() {
        let s = fresh();
        s.put_sender_key("group@g.us", "alice@s.whatsapp.net", b"k1".to_vec())
            .await
            .unwrap();
        let got = s
            .get_sender_key("group@g.us", "alice@s.whatsapp.net")
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some(b"k1".as_ref()));

        // Overwrite.
        s.put_sender_key("group@g.us", "alice@s.whatsapp.net", b"k2".to_vec())
            .await
            .unwrap();
        let got = s
            .get_sender_key("group@g.us", "alice@s.whatsapp.net")
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some(b"k2".as_ref()));

        // Different (group, user) returns None.
        let missing = s
            .get_sender_key("group@g.us", "bob@s.whatsapp.net")
            .await
            .unwrap();
        assert!(missing.is_none());
    }

    /// Sanity-check that the SQLite-backed `MsgSecretStore` round-trips a
    /// 32-byte secret and honours `ON CONFLICT DO NOTHING`.
    #[tokio::test]
    async fn msg_secret_round_trip_sqlite() {
        let s = fresh();
        let chat = "111@s.whatsapp.net";
        let sender = "222@s.whatsapp.net";
        let id = "MID-X";
        let secret = [0x77u8; 32];

        // Empty store → None.
        assert!(s.get_msg_secret(chat, sender, id).await.unwrap().is_none());

        // Persist + read back.
        s.put_msg_secret(chat, sender, id, secret).await.unwrap();
        let got = s.get_msg_secret(chat, sender, id).await.unwrap();
        assert_eq!(got, Some(secret));

        // Second insert with a different secret on the same key must NOT
        // overwrite — mirrors upstream's `ON CONFLICT DO NOTHING`.
        let other = [0xFFu8; 32];
        s.put_msg_secret(chat, sender, id, other).await.unwrap();
        let still = s.get_msg_secret(chat, sender, id).await.unwrap();
        assert_eq!(still, Some(secret));

        // Different msg_id → independent row, returns None until written.
        assert!(s
            .get_msg_secret(chat, sender, "OTHER-ID")
            .await
            .unwrap()
            .is_none());
    }

    /// Round-trip every direction of the SQLite-backed `LidStore`. Also
    /// exercises the "same-pn-different-lid" upsert path: re-pairing an
    /// account onto a fresh LID should evict the old (lid, pn) row so the
    /// reverse lookup is consistent. This is the sqlite-specific check —
    /// the in-memory equivalent lives in `wha-store::memory::tests`.
    #[tokio::test]
    async fn lid_pn_mapping_sqlite_round_trip_and_repair() {
        let s = fresh();
        let lid: Jid = "9876@lid".parse().unwrap();
        let pn: Jid = "1234@s.whatsapp.net".parse().unwrap();

        assert!(s.get_pn_for_lid(&lid).await.unwrap().is_none());

        s.put_lid_pn_mapping(lid.clone(), pn.clone()).await.unwrap();
        assert_eq!(s.get_pn_for_lid(&lid).await.unwrap(), Some(pn.clone()));
        assert_eq!(s.get_lid_for_pn(&pn).await.unwrap(), Some(lid.clone()));

        // Re-pair: same PN, fresh LID. The schema declares `pn` UNIQUE so
        // the implementation must drop the conflicting row.
        let new_lid: Jid = "AAAA@lid".parse().unwrap();
        s.put_lid_pn_mapping(new_lid.clone(), pn.clone())
            .await
            .unwrap();
        assert_eq!(s.get_lid_for_pn(&pn).await.unwrap(), Some(new_lid.clone()));
        // Old LID no longer maps to anything.
        assert!(s.get_pn_for_lid(&lid).await.unwrap().is_none());
        // New LID's reverse lookup hits.
        assert_eq!(s.get_pn_for_lid(&new_lid).await.unwrap(), Some(pn));
    }

    /// Round-trip the SQLite-backed `PrivacyTokenStore`. Also exercises the
    /// upsert path — a second `put_privacy_token` for the same JID replaces
    /// the prior row, mirroring upstream's `PutPrivacyTokens`.
    #[tokio::test]
    async fn privacy_token_round_trip_sqlite() {
        let s = fresh();
        let jid: Jid = "555@s.whatsapp.net".parse().unwrap();

        // Empty store → None.
        assert!(s.get_privacy_token(&jid).await.unwrap().is_none());

        // Persist + read back.
        let token = vec![0xCAu8, 0xFE, 0xBA, 0xBE];
        s.put_privacy_token(jid.clone(), token.clone(), 1_700_000_000)
            .await
            .unwrap();
        let got = s.get_privacy_token(&jid).await.unwrap();
        assert_eq!(got, Some((token, 1_700_000_000)));

        // Upsert overwrites the row.
        let token2 = vec![0x11u8, 0x22, 0x33];
        s.put_privacy_token(jid.clone(), token2.clone(), 1_700_000_500)
            .await
            .unwrap();
        let got2 = s.get_privacy_token(&jid).await.unwrap();
        assert_eq!(got2, Some((token2, 1_700_000_500)));

        // Different JID is independent.
        let other: Jid = "777@s.whatsapp.net".parse().unwrap();
        assert!(s.get_privacy_token(&other).await.unwrap().is_none());
    }

    /// Round-trip the SQLite-backed `NewsletterKeyStore`. Mirrors the
    /// upsert path for channel-key rotation.
    #[tokio::test]
    async fn newsletter_key_round_trip_sqlite() {
        let s = fresh();
        let channel: Jid = "111@newsletter".parse().unwrap();

        // Empty → None.
        assert!(s.get_newsletter_key(&channel).await.unwrap().is_none());

        // Persist + read back.
        let key1 = [0x42u8; 32];
        s.put_newsletter_key(&channel, key1).await.unwrap();
        assert_eq!(
            s.get_newsletter_key(&channel).await.unwrap(),
            Some(key1)
        );

        // Rotate — second put overwrites the row.
        let key2 = [0x99u8; 32];
        s.put_newsletter_key(&channel, key2).await.unwrap();
        assert_eq!(
            s.get_newsletter_key(&channel).await.unwrap(),
            Some(key2)
        );

        // Different channel is independent.
        let other: Jid = "999@newsletter".parse().unwrap();
        assert!(s.get_newsletter_key(&other).await.unwrap().is_none());
    }

    /// Container open + new_device should leave a row visible in
    /// `list_devices`. Mirrors the upstream `Container.NewDevice` →
    /// `Container.GetAllDevices` round-trip.
    #[tokio::test]
    async fn container_open_and_new_device_round_trip() {
        let c = Container::open(":memory:").expect("open container");
        assert!(c.list_devices().await.unwrap().is_empty(), "fresh container is empty");

        // new_device persists a placeholder row (jid still unknown until
        // pairing assigns one).
        c.new_device().await.expect("new device");

        let devs = c.list_devices().await.expect("list");
        assert_eq!(devs.len(), 1, "expected exactly one fresh device");
        let blob = &devs[0];
        assert!(blob.id.is_none(), "fresh device has no AD-JID yet");
        assert!(!blob.initialized, "fresh device is not initialized");
    }

    /// `get_first_device` returns `None` on empty, then a handle once we
    /// mint a device. Mirrors `Container.GetFirstDevice`.
    #[tokio::test]
    async fn container_get_first_device() {
        let c = Container::open(":memory:").expect("open container");
        assert!(c.get_first_device().await.unwrap().is_none());

        c.new_device().await.expect("mint device");
        assert!(c.get_first_device().await.unwrap().is_some());
    }

    /// `get_device_by_jid` finds devices by their AD-JID after the
    /// caller saves a paired device. Mirrors `Container.GetDevice`.
    #[tokio::test]
    async fn container_get_device_by_jid_after_pairing() {
        use std::str::FromStr;
        let c = Container::open(":memory:").expect("open container");
        let store = c.store();

        // Build + pair a device manually, then save through the store.
        let mut dev = store.new_device();
        dev.id = Some(Jid::from_str("1234.5:7@s.whatsapp.net").unwrap());
        dev.lid = Some(Jid::from_str("9876@lid").unwrap());
        dev.initialized = true;
        store.save_device(&dev).await.expect("save device");

        // The "first" lookup also works here, but `get_device_by_jid` is
        // the strict per-AD-JID path.
        let jid = dev.id.as_ref().unwrap().clone();
        let unknown_jid: Jid = "9999.9:9@s.whatsapp.net".parse().unwrap();

        assert!(c.get_device_by_jid(&jid).await.unwrap().is_some());
        assert!(c.get_device_by_jid(&unknown_jid).await.unwrap().is_none());

        // list_devices reflects the upserted row, replacing the legacy
        // placeholder that was created by `new_device` if any.
        let listed = c.list_devices().await.unwrap();
        assert!(listed.iter().any(|b| b.id.as_deref() == Some("1234.5:7@s.whatsapp.net")));
    }

    /// `delete_device` removes the row, leaving `list_devices` empty.
    /// Mirrors `Container.DeleteDevice`.
    #[tokio::test]
    async fn container_delete_device() {
        use std::str::FromStr;
        let c = Container::open(":memory:").expect("open container");
        let store = c.store();

        let mut dev = store.new_device();
        dev.id = Some(Jid::from_str("4242@s.whatsapp.net").unwrap());
        dev.initialized = true;
        store.save_device(&dev).await.expect("save");

        let jid = dev.id.as_ref().unwrap().clone();
        assert!(c.get_device_by_jid(&jid).await.unwrap().is_some());

        c.delete_device(&jid).await.expect("delete");
        assert!(c.get_device_by_jid(&jid).await.unwrap().is_none());
        // The placeholder `__legacy__:<reg_id>` row may still be there
        // from `new_device` — that's fine, it's intentional. We only
        // assert that the explicitly-pairing-keyed row is gone.
    }
}
