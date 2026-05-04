//! In-memory implementation of every store trait. Production code wires real
//! backends here; tests across the workspace use this one.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use rand::rngs::OsRng;

use wha_crypto::{KeyPair, PreKey};
use wha_types::Jid;

use crate::device::{AppStateSyncKey, Device};
use crate::error::StoreError;
use crate::traits::*;

/// Default capacity hint for prekey batches generated on demand.
const PREKEY_BATCH_HINT: usize = 30;

#[derive(Default)]
struct Inner {
    identities: HashMap<String, [u8; 32]>,
    sessions: HashMap<String, Vec<u8>>,
    pre_keys: HashMap<u32, PreKey>,
    pre_keys_uploaded_to: u32,
    next_pre_key_id: u32,
    sender_keys: HashMap<(String, String), Vec<u8>>,
    app_state_keys: HashMap<Vec<u8>, AppStateSyncKey>,
    latest_app_state_key: Option<Vec<u8>>,
    /// (collection_name → (version, 128-byte LTHash buffer)).
    app_state_versions: HashMap<String, (u64, [u8; 128])>,
    /// (collection_name, index_mac) → value_mac.
    app_state_mutation_macs: HashMap<(String, Vec<u8>), Vec<u8>>,
    lid_to_pn: HashMap<String, Jid>,
    pn_to_lid: HashMap<String, Jid>,
    /// (chat, sender, msg_id) → 32-byte message-secret. Upstream
    /// whatsmeow scopes the key on `(our_jid, chat, sender, msg_id)`;
    /// each `MemoryStore` instance is per-device, so our_jid is implicit.
    msg_secrets: HashMap<(String, String, String), [u8; 32]>,
    /// jid (stringified) → (token bytes, sender_timestamp). Upserts
    /// replace prior rows matching upstream's `PutPrivacyTokens` semantics.
    privacy_tokens: HashMap<String, (Vec<u8>, i64)>,
    /// (channel JID string) → 32-byte newsletter channel key. See
    /// `NewsletterKeyStore` for the upstream-parity caveat.
    newsletter_keys: HashMap<String, [u8; 32]>,
}

#[derive(Clone, Default)]
pub struct MemoryStore {
    inner: Arc<RwLock<Inner>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner { next_pre_key_id: 1, ..Default::default() })),
        }
    }

    /// Build a fresh [`Device`] backed entirely by this store. Useful for
    /// integration tests and the first run before the user pairs.
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

#[async_trait]
impl IdentityStore for MemoryStore {
    async fn put_identity(&self, address: &str, key: [u8; 32]) -> Result<(), StoreError> {
        self.inner.write().identities.insert(address.to_owned(), key);
        Ok(())
    }
    async fn delete_all_identities(&self, phone: &str) -> Result<(), StoreError> {
        let mut w = self.inner.write();
        w.identities.retain(|k, _| !k.starts_with(phone));
        Ok(())
    }
    async fn delete_identity(&self, address: &str) -> Result<(), StoreError> {
        self.inner.write().identities.remove(address);
        Ok(())
    }
    async fn is_trusted_identity(&self, address: &str, key: [u8; 32]) -> Result<bool, StoreError> {
        let r = self.inner.read();
        match r.identities.get(address) {
            Some(existing) => Ok(*existing == key),
            None => Ok(true), // trust-on-first-use, matching whatsmeow
        }
    }
}

#[async_trait]
impl SessionStore for MemoryStore {
    async fn get_session(&self, address: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.inner.read().sessions.get(address).cloned())
    }
    async fn has_session(&self, address: &str) -> Result<bool, StoreError> {
        Ok(self.inner.read().sessions.contains_key(address))
    }
    async fn put_session(&self, address: &str, session: Vec<u8>) -> Result<(), StoreError> {
        self.inner.write().sessions.insert(address.to_owned(), session);
        Ok(())
    }
    async fn delete_all_sessions(&self, phone: &str) -> Result<(), StoreError> {
        self.inner.write().sessions.retain(|k, _| !k.starts_with(phone));
        Ok(())
    }
    async fn delete_session(&self, address: &str) -> Result<(), StoreError> {
        self.inner.write().sessions.remove(address);
        Ok(())
    }
}

#[async_trait]
impl PreKeyStore for MemoryStore {
    async fn gen_one_pre_key(&self) -> Result<PreKey, StoreError> {
        let mut rng = OsRng;
        let mut w = self.inner.write();
        let id = w.next_pre_key_id;
        w.next_pre_key_id += 1;
        let pk = PreKey::new(id, KeyPair::generate(&mut rng));
        w.pre_keys.insert(id, pk.clone());
        Ok(pk)
    }
    async fn get_or_gen_pre_keys(&self, count: u32) -> Result<Vec<PreKey>, StoreError> {
        let mut out = Vec::with_capacity(count.max(1) as usize);
        for _ in 0..count {
            out.push(self.gen_one_pre_key().await?);
        }
        Ok(out)
    }
    async fn get_pre_key(&self, id: u32) -> Result<Option<PreKey>, StoreError> {
        Ok(self.inner.read().pre_keys.get(&id).cloned())
    }
    async fn remove_pre_key(&self, id: u32) -> Result<(), StoreError> {
        self.inner.write().pre_keys.remove(&id);
        Ok(())
    }
    async fn mark_pre_keys_uploaded(&self, up_to_id: u32) -> Result<(), StoreError> {
        self.inner.write().pre_keys_uploaded_to = up_to_id;
        Ok(())
    }
    async fn uploaded_pre_key_count(&self) -> Result<usize, StoreError> {
        let r = self.inner.read();
        Ok(r.pre_keys.values().filter(|pk| pk.key_id <= r.pre_keys_uploaded_to).count())
    }
}

#[async_trait]
impl SenderKeyStore for MemoryStore {
    async fn put_sender_key(&self, group: &str, user: &str, session: Vec<u8>) -> Result<(), StoreError> {
        self.inner.write().sender_keys.insert((group.to_owned(), user.to_owned()), session);
        Ok(())
    }
    async fn get_sender_key(&self, group: &str, user: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.inner.read().sender_keys.get(&(group.to_owned(), user.to_owned())).cloned())
    }
}

#[async_trait]
impl AppStateSyncKeyStore for MemoryStore {
    async fn put_app_state_sync_key(&self, id: Vec<u8>, key: AppStateSyncKey) -> Result<(), StoreError> {
        let mut w = self.inner.write();
        w.app_state_keys.insert(id.clone(), key);
        w.latest_app_state_key = Some(id);
        Ok(())
    }
    async fn get_app_state_sync_key(&self, id: &[u8]) -> Result<Option<AppStateSyncKey>, StoreError> {
        Ok(self.inner.read().app_state_keys.get(id).cloned())
    }
    async fn get_latest_app_state_sync_key_id(&self) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.inner.read().latest_app_state_key.clone())
    }
}

#[async_trait]
impl AppStateMutationMacStore for MemoryStore {
    async fn put_app_state_version(
        &self,
        name: &str,
        version: u64,
        hash: [u8; 128],
    ) -> Result<(), StoreError> {
        self.inner
            .write()
            .app_state_versions
            .insert(name.to_owned(), (version, hash));
        Ok(())
    }

    async fn get_app_state_version(
        &self,
        name: &str,
    ) -> Result<Option<(u64, [u8; 128])>, StoreError> {
        Ok(self.inner.read().app_state_versions.get(name).copied())
    }

    async fn delete_app_state_version(&self, name: &str) -> Result<(), StoreError> {
        let mut w = self.inner.write();
        w.app_state_versions.remove(name);
        w.app_state_mutation_macs
            .retain(|(n, _), _| n.as_str() != name);
        Ok(())
    }

    async fn put_app_state_mutation_mac(
        &self,
        name: &str,
        _version: u64,
        index_mac: &[u8],
        value_mac: &[u8],
    ) -> Result<(), StoreError> {
        self.inner
            .write()
            .app_state_mutation_macs
            .insert((name.to_owned(), index_mac.to_vec()), value_mac.to_vec());
        Ok(())
    }

    async fn delete_app_state_mutation_macs(
        &self,
        name: &str,
        index_macs: &[Vec<u8>],
    ) -> Result<(), StoreError> {
        let mut w = self.inner.write();
        for im in index_macs {
            w.app_state_mutation_macs
                .remove(&(name.to_owned(), im.clone()));
        }
        Ok(())
    }

    async fn get_app_state_mutation_mac(
        &self,
        name: &str,
        index_mac: &[u8],
    ) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self
            .inner
            .read()
            .app_state_mutation_macs
            .get(&(name.to_owned(), index_mac.to_vec()))
            .cloned())
    }
}

#[async_trait]
impl LidStore for MemoryStore {
    async fn put_lid_pn_mapping(&self, lid: Jid, pn: Jid) -> Result<(), StoreError> {
        let mut w = self.inner.write();
        // `pn` may already point at a different `lid` (account migration);
        // overwrite both directions so the mapping is consistent both ways.
        if let Some(prev_lid) = w.pn_to_lid.get(&pn.to_string()).cloned() {
            if prev_lid != lid {
                w.lid_to_pn.remove(&prev_lid.to_string());
            }
        }
        if let Some(prev_pn) = w.lid_to_pn.get(&lid.to_string()).cloned() {
            if prev_pn != pn {
                w.pn_to_lid.remove(&prev_pn.to_string());
            }
        }
        w.lid_to_pn.insert(lid.to_string(), pn.clone());
        w.pn_to_lid.insert(pn.to_string(), lid);
        Ok(())
    }
    async fn get_pn_for_lid(&self, lid: &Jid) -> Result<Option<Jid>, StoreError> {
        Ok(self.inner.read().lid_to_pn.get(&lid.to_string()).cloned())
    }
    async fn get_lid_for_pn(&self, pn: &Jid) -> Result<Option<Jid>, StoreError> {
        Ok(self.inner.read().pn_to_lid.get(&pn.to_string()).cloned())
    }
}

#[async_trait]
impl InspectStore for MemoryStore {
    async fn list_session_addresses(&self) -> Result<Vec<String>, StoreError> {
        let r = self.inner.read();
        let mut out: Vec<String> = r.sessions.keys().cloned().collect();
        out.sort();
        Ok(out)
    }

    async fn count_pre_keys(&self) -> Result<usize, StoreError> {
        Ok(self.inner.read().pre_keys.len())
    }

    async fn list_app_state_versions(
        &self,
    ) -> Result<HashMap<String, u64>, StoreError> {
        Ok(self
            .inner
            .read()
            .app_state_versions
            .iter()
            .map(|(k, (v, _))| (k.clone(), *v))
            .collect())
    }
}

#[async_trait]
impl MsgSecretStore for MemoryStore {
    async fn put_msg_secret(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
        secret: [u8; 32],
    ) -> Result<(), StoreError> {
        // ON CONFLICT DO NOTHING semantics — upstream's putMsgSecret keeps
        // the original row when the same (chat, sender, msg_id) reappears.
        let key = (chat.to_owned(), sender.to_owned(), msg_id.to_owned());
        let mut w = self.inner.write();
        w.msg_secrets.entry(key).or_insert(secret);
        Ok(())
    }
    async fn get_msg_secret(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
    ) -> Result<Option<[u8; 32]>, StoreError> {
        let key = (chat.to_owned(), sender.to_owned(), msg_id.to_owned());
        Ok(self.inner.read().msg_secrets.get(&key).copied())
    }
}

#[async_trait]
impl NewsletterKeyStore for MemoryStore {
    async fn put_newsletter_key(
        &self,
        channel: &Jid,
        key: [u8; 32],
    ) -> Result<(), StoreError> {
        self.inner
            .write()
            .newsletter_keys
            .insert(channel.to_string(), key);
        Ok(())
    }

    async fn get_newsletter_key(
        &self,
        channel: &Jid,
    ) -> Result<Option<[u8; 32]>, StoreError> {
        Ok(self
            .inner
            .read()
            .newsletter_keys
            .get(&channel.to_string())
            .copied())
    }
}

#[async_trait]
impl PrivacyTokenStore for MemoryStore {
    async fn put_privacy_token(
        &self,
        jid: Jid,
        token: Vec<u8>,
        timestamp: i64,
    ) -> Result<(), StoreError> {
        // Upsert semantics — overwrite any prior token for the same JID.
        // Mirrors upstream's `PutPrivacyTokens` ON CONFLICT DO UPDATE.
        self.inner
            .write()
            .privacy_tokens
            .insert(jid.to_string(), (token, timestamp));
        Ok(())
    }

    async fn get_privacy_token(
        &self,
        jid: &Jid,
    ) -> Result<Option<(Vec<u8>, i64)>, StoreError> {
        Ok(self.inner.read().privacy_tokens.get(&jid.to_string()).cloned())
    }
}

// Don't use the unused-bytes hint on the prekey batch.
#[allow(dead_code)]
const _: usize = PREKEY_BATCH_HINT;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trip_identity() {
        let store = Arc::new(MemoryStore::new());
        let key = [3u8; 32];
        store.put_identity("alice", key).await.unwrap();
        assert!(store.is_trusted_identity("alice", key).await.unwrap());
        assert!(!store.is_trusted_identity("alice", [4u8; 32]).await.unwrap());
    }

    #[tokio::test]
    async fn prekey_generation_is_unique() {
        let store = Arc::new(MemoryStore::new());
        let a = store.gen_one_pre_key().await.unwrap();
        let b = store.gen_one_pre_key().await.unwrap();
        assert_ne!(a.key_id, b.key_id);
        assert_eq!(store.get_pre_key(a.key_id).await.unwrap().unwrap().key_id, a.key_id);
    }

    #[test]
    fn new_device_has_distinct_keys() {
        let store = Arc::new(MemoryStore::new());
        let dev = store.new_device();
        assert_ne!(dev.noise_key.public, dev.identity_key.public);
        assert!(dev.signed_pre_key.signature.is_some());
    }

    #[tokio::test]
    async fn app_state_mutation_mac_round_trip() {
        let store: Arc<MemoryStore> = Arc::new(MemoryStore::new());
        // Initially empty.
        assert!(store
            .get_app_state_version("regular")
            .await
            .unwrap()
            .is_none());

        // Versions persist.
        let mut h = [0u8; 128];
        h[0] = 1;
        store.put_app_state_version("regular", 7, h).await.unwrap();
        let got = store.get_app_state_version("regular").await.unwrap();
        assert_eq!(got, Some((7, h)));

        // Mutation MACs persist + delete works.
        store
            .put_app_state_mutation_mac("regular", 7, b"im1", b"vm1")
            .await
            .unwrap();
        store
            .put_app_state_mutation_mac("regular", 7, b"im2", b"vm2")
            .await
            .unwrap();
        assert_eq!(
            store
                .get_app_state_mutation_mac("regular", b"im1")
                .await
                .unwrap()
                .as_deref(),
            Some(b"vm1".as_ref()),
        );
        store
            .delete_app_state_mutation_macs("regular", &[b"im1".to_vec()])
            .await
            .unwrap();
        assert!(store
            .get_app_state_mutation_mac("regular", b"im1")
            .await
            .unwrap()
            .is_none());
        assert!(store
            .get_app_state_mutation_mac("regular", b"im2")
            .await
            .unwrap()
            .is_some());

        // delete_app_state_version wipes the collection clean.
        store.delete_app_state_version("regular").await.unwrap();
        assert!(store
            .get_app_state_version("regular")
            .await
            .unwrap()
            .is_none());
        assert!(store
            .get_app_state_mutation_mac("regular", b"im2")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn lid_pn_mapping_round_trip_both_directions() {
        use crate::traits::LidStore;
        let store = Arc::new(MemoryStore::new());
        let lid: Jid = "9876@lid".parse().unwrap();
        let pn: Jid = "1234@s.whatsapp.net".parse().unwrap();

        // Initially empty in both directions.
        assert!(store.get_pn_for_lid(&lid).await.unwrap().is_none());
        assert!(store.get_lid_for_pn(&pn).await.unwrap().is_none());

        // Insert; both lookups now resolve.
        store
            .put_lid_pn_mapping(lid.clone(), pn.clone())
            .await
            .unwrap();
        assert_eq!(store.get_pn_for_lid(&lid).await.unwrap(), Some(pn.clone()));
        assert_eq!(store.get_lid_for_pn(&pn).await.unwrap(), Some(lid.clone()));
    }

    #[tokio::test]
    async fn lid_pn_mapping_re_pair_replaces_old_lid() {
        // Account migration: same PN now points at a fresh LID. The old
        // (LID, PN) row must be evicted so reverse lookups don't return
        // a stale answer.
        use crate::traits::LidStore;
        let store = Arc::new(MemoryStore::new());
        let pn: Jid = "1234@s.whatsapp.net".parse().unwrap();
        let old_lid: Jid = "AAAA@lid".parse().unwrap();
        let new_lid: Jid = "BBBB@lid".parse().unwrap();

        store
            .put_lid_pn_mapping(old_lid.clone(), pn.clone())
            .await
            .unwrap();
        store
            .put_lid_pn_mapping(new_lid.clone(), pn.clone())
            .await
            .unwrap();

        // Forward lookup hits the new LID.
        assert_eq!(
            store.get_lid_for_pn(&pn).await.unwrap(),
            Some(new_lid.clone())
        );
        // Old LID's reverse lookup is gone.
        assert!(store.get_pn_for_lid(&old_lid).await.unwrap().is_none());
        // New LID's reverse lookup hits.
        assert_eq!(store.get_pn_for_lid(&new_lid).await.unwrap(), Some(pn));
    }

    #[tokio::test]
    async fn lid_pn_mapping_unknown_lookup_is_none() {
        use crate::traits::LidStore;
        let store = Arc::new(MemoryStore::new());
        let unknown_lid: Jid = "9999@lid".parse().unwrap();
        let unknown_pn: Jid = "8888@s.whatsapp.net".parse().unwrap();
        assert!(store.get_pn_for_lid(&unknown_lid).await.unwrap().is_none());
        assert!(store.get_lid_for_pn(&unknown_pn).await.unwrap().is_none());
    }

    /// Privacy token round-trip in the in-memory store. Mirrors the SQLite
    /// equivalent in `wha-store-sqlite::tests`.
    #[tokio::test]
    async fn privacy_token_round_trip_memory() {
        use crate::traits::PrivacyTokenStore;
        let store = Arc::new(MemoryStore::new());
        let jid: Jid = "555@s.whatsapp.net".parse().unwrap();

        // Empty store → None.
        assert!(store.get_privacy_token(&jid).await.unwrap().is_none());

        // Persist + read back.
        let token = vec![0xAB, 0xCD, 0xEF, 0x42];
        store
            .put_privacy_token(jid.clone(), token.clone(), 1_700_000_000)
            .await
            .unwrap();
        let got = store.get_privacy_token(&jid).await.unwrap();
        assert_eq!(got, Some((token, 1_700_000_000)));

        // Second insert overwrites — upstream's PutPrivacyTokens upserts.
        let token2 = vec![0x11, 0x22];
        store
            .put_privacy_token(jid.clone(), token2.clone(), 1_700_000_500)
            .await
            .unwrap();
        let got2 = store.get_privacy_token(&jid).await.unwrap();
        assert_eq!(got2, Some((token2, 1_700_000_500)));

        // Different JID is independent.
        let other: Jid = "777@s.whatsapp.net".parse().unwrap();
        assert!(store.get_privacy_token(&other).await.unwrap().is_none());
    }
}
