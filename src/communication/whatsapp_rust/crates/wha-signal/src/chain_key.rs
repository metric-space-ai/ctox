//! Symmetric chain-key + message-key derivation.
//!
//! Faithfully ported from `go.mau.fi/libsignal/keys/chain/ChainKey.go` and
//! `keys/message/MessageKey.go`. This is the symmetric half of the Double
//! Ratchet: starting from a root-derived 32-byte seed, advancing the chain
//! one step at a time, and deriving (cipher_key, mac_key, iv) for each
//! outgoing message.
//!
//! Interop-verified against go-libsignal — see
//! `tests/libsignal_interop.rs` for the cross-language test vectors.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use wha_crypto::hkdf_sha256;

type HmacSha256 = Hmac<Sha256>;

/// libsignal's `info` parameter when expanding message keys.
const KDF_INFO_MESSAGE_KEYS: &[u8] = b"WhisperMessageKeys";
const MESSAGE_KEY_SEED: u8 = 0x01;
const CHAIN_KEY_SEED: u8 = 0x02;

/// Per-iteration chain key. Each call to [`next`] advances the chain
/// deterministically; each call to [`message_keys`] derives a one-shot
/// `(cipher_key, mac_key, iv)` triple for a single message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainKey {
    pub key: [u8; 32],
    pub index: u32,
}

/// Derived (cipher, mac, iv) bundle for one message slot. Mirrors
/// `libsignal.keys/message.Keys`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageKeys {
    pub cipher_key: [u8; 32],
    pub mac_key: [u8; 32],
    pub iv: [u8; 16],
    pub index: u32,
}

impl ChainKey {
    pub fn new(key: [u8; 32], index: u32) -> Self {
        Self { key, index }
    }

    /// HMAC-SHA256(self.key, seed) — the base-material trick from libsignal.
    fn base_material(&self, seed: u8) -> [u8; 32] {
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.key).unwrap();
        mac.update(&[seed]);
        let mut out = [0u8; 32];
        out.copy_from_slice(&mac.finalize().into_bytes());
        out
    }

    /// Advance the chain by one step.
    pub fn next(&self) -> ChainKey {
        ChainKey { key: self.base_material(CHAIN_KEY_SEED), index: self.index + 1 }
    }

    /// Derive the message keys for the current chain index.
    pub fn message_keys(&self) -> MessageKeys {
        let seed = self.base_material(MESSAGE_KEY_SEED);
        // libsignal: HKDF-SHA256(IKM=seed, salt=empty, info="WhisperMessageKeys") -> 80 bytes.
        let derived = hkdf_sha256(&seed, &[], KDF_INFO_MESSAGE_KEYS, 80)
            .expect("HKDF with 80 bytes output cannot fail");
        let mut cipher_key = [0u8; 32];
        let mut mac_key = [0u8; 32];
        let mut iv = [0u8; 16];
        cipher_key.copy_from_slice(&derived[..32]);
        mac_key.copy_from_slice(&derived[32..64]);
        iv.copy_from_slice(&derived[64..80]);
        MessageKeys { cipher_key, mac_key, iv, index: self.index }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_advances_index_and_changes_key() {
        let ck = ChainKey::new([7u8; 32], 0);
        let n = ck.next();
        assert_eq!(n.index, 1);
        assert_ne!(n.key, ck.key);
    }

    #[test]
    fn message_keys_size_and_determinism() {
        let ck = ChainKey::new([7u8; 32], 5);
        let m1 = ck.message_keys();
        let m2 = ck.message_keys();
        assert_eq!(m1, m2);
        assert_eq!(m1.cipher_key.len(), 32);
        assert_eq!(m1.mac_key.len(), 32);
        assert_eq!(m1.iv.len(), 16);
        assert_eq!(m1.index, 5);
    }
}
