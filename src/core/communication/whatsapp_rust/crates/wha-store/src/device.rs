use std::sync::Arc;

use wha_crypto::{KeyPair, PreKey};
use wha_types::{Jid, MessageId};

use crate::traits::*;

/// Mirrors `store.AppStateSyncKey` upstream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppStateSyncKey {
    pub data: Vec<u8>,
    pub fingerprint: Vec<u8>,
    pub timestamp: i64,
}

/// Mirrors `store.MessageSecretInsert` upstream — used by `msgsecret.go`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageSecretInsert {
    pub chat: Jid,
    pub sender: Jid,
    pub id: MessageId,
    pub secret: Vec<u8>,
}

/// One device's identity material + handles to every aggregate-specific store.
///
/// The trait objects are `Arc<dyn …Store>` so the same backend instance can be
/// shared across them (and so a sql-backed implementation that satisfies all
/// the traits can be installed once).
#[derive(Clone)]
pub struct Device {
    pub noise_key: KeyPair,
    pub identity_key: KeyPair,
    pub signed_pre_key: PreKey,
    pub registration_id: u32,
    pub adv_secret_key: [u8; 32],

    pub id: Option<Jid>,
    pub lid: Option<Jid>,
    pub platform: String,
    pub business_name: String,
    pub push_name: String,

    pub initialized: bool,
    pub deleted: bool,

    /// Trusted-contact token blob, persisted alongside the device. Mirrors the
    /// in-memory `tcToken` blob used by `_upstream/whatsmeow/tctoken.go` —
    /// see `crates/wha-client/src/tc_token.rs` for the issuance/refresh flow.
    /// `None` means the server has never issued a token for this device.
    pub tc_token: Option<Vec<u8>>,

    pub identities: Arc<dyn IdentityStore>,
    pub sessions: Arc<dyn SessionStore>,
    pub pre_keys: Arc<dyn PreKeyStore>,
    pub sender_keys: Arc<dyn SenderKeyStore>,
    pub app_state_keys: Arc<dyn AppStateSyncKeyStore>,
    pub app_state_mutations: Arc<dyn AppStateMutationMacStore>,
    pub lids: Arc<dyn LidStore>,
    /// Per-message secret store. Mirrors `cli.Store.MsgSecrets` upstream.
    pub msg_secrets: Arc<dyn MsgSecretStore>,
    /// Inbound trusted-contact privacy tokens. Mirrors
    /// `cli.Store.PrivacyTokens` upstream — populated from
    /// `<notification type="privacy_token">` and read by
    /// `subscribe_presence` to attach `<tctoken>`.
    pub privacy_tokens: Arc<dyn PrivacyTokenStore>,
    /// Per-channel newsletter keys. See [`NewsletterKeyStore`] for the
    /// upstream-parity caveat — newsletters are not encrypted on the wire
    /// in upstream whatsmeow today; this handle backs the Rust port's
    /// armadillo round-trip helpers.
    pub newsletter_keys: Arc<dyn NewsletterKeyStore>,
}

impl Device {
    pub fn jid(&self) -> Option<&Jid> {
        self.id.as_ref()
    }
    pub fn lid(&self) -> Option<&Jid> {
        self.lid.as_ref()
    }
}
