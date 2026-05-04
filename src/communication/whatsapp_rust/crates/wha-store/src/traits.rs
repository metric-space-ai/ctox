//! Per-aggregate traits. These intentionally mirror the upstream Go
//! interfaces — same method names, same arguments — so the call sites in
//! `wha-client` translate directly.

use async_trait::async_trait;

use wha_crypto::PreKey;
use wha_types::Jid;

use crate::device::AppStateSyncKey;
use crate::error::StoreError;

#[async_trait]
pub trait IdentityStore: Send + Sync {
    async fn put_identity(&self, address: &str, key: [u8; 32]) -> Result<(), StoreError>;
    async fn delete_all_identities(&self, phone: &str) -> Result<(), StoreError>;
    async fn delete_identity(&self, address: &str) -> Result<(), StoreError>;
    async fn is_trusted_identity(&self, address: &str, key: [u8; 32]) -> Result<bool, StoreError>;
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn get_session(&self, address: &str) -> Result<Option<Vec<u8>>, StoreError>;
    async fn has_session(&self, address: &str) -> Result<bool, StoreError>;
    async fn put_session(&self, address: &str, session: Vec<u8>) -> Result<(), StoreError>;
    async fn delete_all_sessions(&self, phone: &str) -> Result<(), StoreError>;
    async fn delete_session(&self, address: &str) -> Result<(), StoreError>;
}

#[async_trait]
pub trait PreKeyStore: Send + Sync {
    async fn gen_one_pre_key(&self) -> Result<PreKey, StoreError>;
    async fn get_or_gen_pre_keys(&self, count: u32) -> Result<Vec<PreKey>, StoreError>;
    async fn get_pre_key(&self, id: u32) -> Result<Option<PreKey>, StoreError>;
    async fn remove_pre_key(&self, id: u32) -> Result<(), StoreError>;
    async fn mark_pre_keys_uploaded(&self, up_to_id: u32) -> Result<(), StoreError>;
    async fn uploaded_pre_key_count(&self) -> Result<usize, StoreError>;
}

#[async_trait]
pub trait SenderKeyStore: Send + Sync {
    async fn put_sender_key(&self, group: &str, user: &str, session: Vec<u8>) -> Result<(), StoreError>;
    async fn get_sender_key(&self, group: &str, user: &str) -> Result<Option<Vec<u8>>, StoreError>;
}

#[async_trait]
pub trait AppStateSyncKeyStore: Send + Sync {
    async fn put_app_state_sync_key(&self, id: Vec<u8>, key: AppStateSyncKey) -> Result<(), StoreError>;
    async fn get_app_state_sync_key(&self, id: &[u8]) -> Result<Option<AppStateSyncKey>, StoreError>;
    async fn get_latest_app_state_sync_key_id(&self) -> Result<Option<Vec<u8>>, StoreError>;
}

/// Per-message master-secret store. Mirrors
/// `_upstream/whatsmeow/store/store.go::MessageSecretStore` —
/// keyed on the `<chat, sender, message_id>` triple, value is the 32-byte
/// `messageContextInfo.messageSecret` of the original message.
///
/// The triple is stored in non-AD form upstream (see
/// `_upstream/whatsmeow/store/sqlstore/store.go::PutMessageSecrets`); callers
/// of this trait pre-strip the AD components before invocation, so the trait
/// itself just deals with strings.
#[async_trait]
pub trait MsgSecretStore: Send + Sync {
    /// Persist a 32-byte message secret. Idempotent on `(chat, sender, msg_id)`
    /// — second writes are dropped (matching upstream's `ON CONFLICT DO NOTHING`).
    async fn put_msg_secret(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
        secret: [u8; 32],
    ) -> Result<(), StoreError>;

    /// Look up a previously-stored secret. `Ok(None)` if no row matches.
    async fn get_msg_secret(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
    ) -> Result<Option<[u8; 32]>, StoreError>;
}

/// Per-channel symmetric key store for the WhatsApp Newsletter / Channels
/// E2EE scheme as ported in `crates/wha-client/src/armadillo_message.rs`.
///
/// **Note on upstream parity.** Upstream whatsmeow
/// (`_upstream/whatsmeow/send.go::sendNewsletter`) currently ships
/// newsletter messages as bare `<plaintext>` content — there is no
/// per-channel symmetric encryption on the wire today. This trait persists
/// the per-channel key the Rust port uses for its self-consistent
/// armadillo round-trip helpers, keyed on the channel JID. When upstream
/// eventually ships the wire format, this is the table the keys will live
/// in.
#[async_trait]
pub trait NewsletterKeyStore: Send + Sync {
    /// Persist a 32-byte channel key. Idempotent on `channel`; later writes
    /// overwrite earlier ones.
    async fn put_newsletter_key(
        &self,
        channel: &Jid,
        key: [u8; 32],
    ) -> Result<(), StoreError>;

    /// Look up a previously-stored channel key. `Ok(None)` if no row matches.
    async fn get_newsletter_key(&self, channel: &Jid) -> Result<Option<[u8; 32]>, StoreError>;
}

/// Persistence for inbound trusted-contact privacy tokens.
///
/// WhatsApp pushes a `<notification type="privacy_token">` carrying a token
/// blob whenever a contact's privacy token is issued or refreshed. We persist
/// the blob locally so subsequent `<presence type="subscribe">` stanzas can
/// attach it as `<tctoken>`. Without the token the server may refuse to
/// deliver presence updates from the contact (depending on the server's
/// `ErrorOnSubscribePresenceWithoutToken` policy).
///
/// Mirrors `store.PrivacyTokenStore` upstream
/// (`_upstream/whatsmeow/store/store.go::PrivacyTokenStore`). The trait
/// surface intentionally drops `DeleteExpiredPrivacyTokens` for the
/// foundation port, which has no time-source plumbing yet.
#[async_trait]
pub trait PrivacyTokenStore: Send + Sync {
    /// Insert / overwrite the token + timestamp for `jid`. Mirrors
    /// `PutPrivacyTokens` upstream — same upsert semantics.
    async fn put_privacy_token(
        &self,
        jid: Jid,
        token: Vec<u8>,
        timestamp: i64,
    ) -> Result<(), StoreError>;

    /// Look up a stored privacy token. `Ok(None)` if no row matches.
    /// Returns the `(token bytes, sender_timestamp)` tuple — the timestamp
    /// is what the server originally tagged the token with.
    async fn get_privacy_token(
        &self,
        jid: &Jid,
    ) -> Result<Option<(Vec<u8>, i64)>, StoreError>;
}

/// Persistence for the LID-PN identity mapping.
///
/// WhatsApp identifies each registered user by both a phone-number JID
/// (`123@s.whatsapp.net`) and a stable LID (hidden-user JID, `…@lid`). The
/// pair is published in `<pair-success>`, in usync `<lid val="…"/>` answers,
/// and in `<receipt type="retry">` participants. We persist it locally so
/// retry-receipts that name a participant only by LID can be resolved back to
/// the PN form (where the Signal session sits).
///
/// Mirrors `store.LIDStore` upstream
/// (`_upstream/whatsmeow/store/store.go`).
#[async_trait]
pub trait LidStore: Send + Sync {
    /// Insert / overwrite the LID↔PN mapping. The store is bi-directional:
    /// after this call both [`get_pn_for_lid`](Self::get_pn_for_lid) and
    /// [`get_lid_for_pn`](Self::get_lid_for_pn) return the corresponding
    /// counterpart.
    async fn put_lid_pn_mapping(&self, lid: Jid, pn: Jid) -> Result<(), StoreError>;

    /// Look up the phone-number JID for a hidden-user LID. `Ok(None)` if the
    /// pair has never been seen.
    async fn get_pn_for_lid(&self, lid: &Jid) -> Result<Option<Jid>, StoreError>;

    /// Reverse direction of [`get_pn_for_lid`](Self::get_pn_for_lid).
    async fn get_lid_for_pn(&self, pn: &Jid) -> Result<Option<Jid>, StoreError>;
}

/// Persistence for the app-state-sync protocol. Mirrors
/// `_upstream/whatsmeow/store/store.go::AppStateStore` — a place to remember
/// the LTHash version+state per collection, plus the value-MACs of every
/// previously-applied SET mutation so future patches can subtract them.
#[async_trait]
pub trait AppStateMutationMacStore: Send + Sync {
    /// Persist the current LTHash state for `name`. Overwrites any prior row.
    async fn put_app_state_version(
        &self,
        name: &str,
        version: u64,
        hash: [u8; 128],
    ) -> Result<(), StoreError>;

    /// Read the current LTHash state for `name`. `Ok(None)` means the
    /// collection has not been synced yet (caller should request a snapshot).
    async fn get_app_state_version(
        &self,
        name: &str,
    ) -> Result<Option<(u64, [u8; 128])>, StoreError>;

    /// Reset a collection's LTHash state — used at the start of a full sync.
    async fn delete_app_state_version(&self, name: &str) -> Result<(), StoreError>;

    /// Persist a single (index_mac → value_mac) at `version`.
    async fn put_app_state_mutation_mac(
        &self,
        name: &str,
        version: u64,
        index_mac: &[u8],
        value_mac: &[u8],
    ) -> Result<(), StoreError>;

    /// Bulk-delete all rows whose `index_mac` is in the supplied list. Used
    /// when a patch contains REMOVE operations.
    async fn delete_app_state_mutation_macs(
        &self,
        name: &str,
        index_macs: &[Vec<u8>],
    ) -> Result<(), StoreError>;

    /// Look up the most recent `value_mac` for an `index_mac`. Returns
    /// `Ok(None)` if no SET has ever been applied for that index.
    async fn get_app_state_mutation_mac(
        &self,
        name: &str,
        index_mac: &[u8],
    ) -> Result<Option<Vec<u8>>, StoreError>;
}

/// Optional debug-inspection surface for store backends.
///
/// Mirrors the spirit of whatsmeow's `DangerousInternalClient` —
/// see `_upstream/whatsmeow/internals.go`, where the upstream type
/// re-exports otherwise-unexported state for diagnostics. Backends that
/// implement this trait expose enumeration helpers used by
/// [`crate::Device`] consumers (e.g. the
/// `wha-client::internals::DangerousInternalClient`) to dump the cache
/// state of a live device. Production code paths do not depend on it —
/// returning empty vectors / `0` is always a valid implementation.
#[async_trait]
pub trait InspectStore: Send + Sync {
    /// Every Signal address with a stored session, sorted lexicographically.
    /// Mirrors the inspection role of the upstream `Sessions` field on the
    /// internal-client surface (see also
    /// `_upstream/whatsmeow/store/sqlstore/sqlstore.go`).
    async fn list_session_addresses(&self) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }

    /// Total number of pre-keys (uploaded + unuploaded) currently in the
    /// store. Mirrors the role of inspection helpers on
    /// `Client.Store.PreKeys` — useful for verifying that a fresh
    /// pre-key batch was persisted.
    async fn count_pre_keys(&self) -> Result<usize, StoreError> {
        Ok(0)
    }

    /// `(collection_name → version)` for every collection that has a
    /// persisted LTHash version. Mirrors the inspection of
    /// `Client.Store.AppStateMutationMACs.GetVersion(name)` per collection
    /// upstream — useful for checking whether `regular`, `regular_low`,
    /// `regular_high` and `critical_block` have been synced.
    async fn list_app_state_versions(
        &self,
    ) -> Result<std::collections::HashMap<String, u64>, StoreError> {
        Ok(std::collections::HashMap::new())
    }
}
