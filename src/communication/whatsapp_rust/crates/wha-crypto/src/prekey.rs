use crate::error::CryptoError;
use crate::keypair::KeyPair;

/// A signed pre-key in libsignal's sense — a key pair that we hand out as part
/// of our pre-key bundle, signed by our identity key so peers can verify it.
#[derive(Clone, Debug)]
pub struct PreKey {
    pub key_id: u32,
    pub key_pair: KeyPair,
    pub signature: Option<[u8; 64]>,
}

impl PreKey {
    pub fn new(key_id: u32, key_pair: KeyPair) -> Self {
        PreKey { key_id, key_pair, signature: None }
    }

    /// Create a signed pre-key by signing this key's public point with the
    /// supplied identity key. The signed payload is `0x05 || pub`, mirroring
    /// libsignal's `DjbType` prefix.
    pub fn signed_by(mut self, identity: &KeyPair) -> Result<Self, CryptoError> {
        let mut to_sign = [0u8; 33];
        to_sign[0] = 0x05;
        to_sign[1..].copy_from_slice(&self.key_pair.public);
        self.signature = Some(identity.sign(&to_sign));
        Ok(self)
    }
}
