/// What the server returns when you fetch keys for a peer JID. Mirrors
/// libsignal's `PreKeyBundle` struct.
#[derive(Debug, Clone)]
pub struct PreKeyBundle {
    pub registration_id: u32,
    pub device_id: u32,
    pub pre_key_id: Option<u32>,
    pub pre_key_public: Option<[u8; 32]>,
    pub signed_pre_key_id: u32,
    pub signed_pre_key_public: [u8; 32],
    pub signed_pre_key_signature: [u8; 64],
    pub identity_key: [u8; 32],
}
