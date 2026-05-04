//! Sender-key chain key derivation. The full Sender-Key state machine
//! involves message-key generation, ratcheting, and signing — the parts
//! wired up here are the deterministic key derivations every group member
//! does locally; the rest plugs in alongside the message pipeline.
//!
//! Faithfully ported from `go.mau.fi/libsignal/groups/ratchet/SenderChainKey.go`
//! and `groups/ratchet/SenderMessageKey.go`. The chain-key derivation is the
//! same HMAC-with-seed-byte trick as the per-pair `ChainKey`, but the
//! message-key expansion is HKDF-SHA256 with the literal info string
//! `"WhisperGroup"`, producing a 48-byte block split into a 16-byte IV and
//! a 32-byte cipher key.

use std::collections::HashMap;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use wha_crypto::{hkdf_sha256, KeyPair};

type HmacSha256 = Hmac<Sha256>;

const MESSAGE_KEY_SEED: u8 = 0x01;
const CHAIN_KEY_SEED: u8 = 0x02;

/// libsignal's `info` parameter when expanding sender message keys.
const KDF_INFO_GROUP: &[u8] = b"WhisperGroup";

/// libsignal's per-state cap on cached skipped message keys.
pub const MAX_MESSAGE_KEYS: usize = 2000;

/// Per-iteration chain key. Each call to [`Self::next`] advances the chain
/// deterministically; each call to [`Self::message_key`] derives a one-shot
/// 32-byte key for a single message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderChainKey {
    pub iteration: u32,
    pub seed: [u8; 32],
}

impl SenderChainKey {
    pub fn new(iteration: u32, seed: [u8; 32]) -> Self {
        Self { iteration, seed }
    }

    /// HMAC-SHA256(self.seed, [seed_byte]) — libsignal's chain key trick.
    fn derivative(&self, seed_byte: u8) -> [u8; 32] {
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.seed).unwrap();
        mac.update(&[seed_byte]);
        let mut out = [0u8; 32];
        out.copy_from_slice(&mac.finalize().into_bytes());
        out
    }

    /// Derive the 32-byte message-key seed for this iteration.
    pub fn message_key(&self) -> [u8; 32] {
        self.derivative(MESSAGE_KEY_SEED)
    }

    /// Advance the chain deterministically.
    pub fn next(&self) -> SenderChainKey {
        SenderChainKey { iteration: self.iteration + 1, seed: self.derivative(CHAIN_KEY_SEED) }
    }

    /// Derive the libsignal-compatible [`SenderMessageKey`] for this
    /// iteration. The 32-byte chain-key derivative is fed through
    /// `HKDF-SHA256(info="WhisperGroup")` and split into a 16-byte IV and
    /// a 32-byte cipher key.
    pub fn sender_message_key(&self) -> SenderMessageKey {
        SenderMessageKey::derive(self.iteration, self.message_key())
    }
}

/// Per-message AES-CBC inputs for a sender-key chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderMessageKey {
    pub iteration: u32,
    /// 32-byte chain-key derivative — kept around for libsignal-compatible
    /// serialisation and for caching skipped keys.
    pub seed: [u8; 32],
    pub iv: [u8; 16],
    pub cipher_key: [u8; 32],
}

impl SenderMessageKey {
    /// Derive a [`SenderMessageKey`] from the 32-byte chain-key derivative.
    /// This is the byte-for-byte port of libsignal's `NewSenderMessageKey`.
    pub fn derive(iteration: u32, seed: [u8; 32]) -> Self {
        // HKDF-SHA256(IKM=seed, salt=empty, info="WhisperGroup", len=48).
        let derived = hkdf_sha256(&seed, &[], KDF_INFO_GROUP, 48)
            .expect("HKDF expansion of 48 bytes cannot fail");
        let mut iv = [0u8; 16];
        let mut cipher_key = [0u8; 32];
        iv.copy_from_slice(&derived[..16]);
        cipher_key.copy_from_slice(&derived[16..48]);
        Self { iteration, seed, iv, cipher_key }
    }
}

/// Per-sender state in a group session. Mirrors libsignal's `SenderKeyState`.
///
/// `signing_key_private` is `Some` when this peer owns the key (i.e. it
/// generated the distribution message); `None` when the state was
/// reconstructed from a remote peer's distribution message and we only have
/// the public half for signature verification. `KeyPair::from_private`
/// re-derives the public key on demand for signing.
#[derive(Debug, Clone)]
pub struct SenderKeyState {
    pub key_id: u32,
    pub chain_key: SenderChainKey,
    pub signing_key_public: [u8; 32],
    pub signing_key_private: Option<[u8; 32]>,
    /// Cache of skipped (out-of-order) message keys, keyed by iteration.
    /// Capped at [`MAX_MESSAGE_KEYS`] entries — oldest-by-insertion-order
    /// dropped first.
    pub skipped_message_keys: HashMap<u32, SenderMessageKey>,
    /// Insertion-order tracker for FIFO eviction of `skipped_message_keys`.
    pub skipped_order: Vec<u32>,
}

impl SenderKeyState {
    /// Construct a state owned by *this* peer (with private signing key).
    pub fn new_own(key_id: u32, iteration: u32, chain_key: [u8; 32], signing: KeyPair) -> Self {
        Self {
            key_id,
            chain_key: SenderChainKey::new(iteration, chain_key),
            signing_key_public: signing.public,
            signing_key_private: Some(signing.private),
            skipped_message_keys: HashMap::new(),
            skipped_order: Vec::new(),
        }
    }

    /// Construct a state for a *remote* peer (public key only).
    pub fn new_remote(
        key_id: u32,
        iteration: u32,
        chain_key: [u8; 32],
        signing_public: [u8; 32],
    ) -> Self {
        Self {
            key_id,
            chain_key: SenderChainKey::new(iteration, chain_key),
            signing_key_public: signing_public,
            signing_key_private: None,
            skipped_message_keys: HashMap::new(),
            skipped_order: Vec::new(),
        }
    }

    /// `KeyPair` reconstructed from the private signing key, if owned.
    pub fn signing_key_pair(&self) -> Option<KeyPair> {
        self.signing_key_private.map(KeyPair::from_private)
    }

    /// Whether a skipped message key for `iteration` is cached.
    pub fn has_sender_message_key(&self, iteration: u32) -> bool {
        self.skipped_message_keys.contains_key(&iteration)
    }

    /// Cache a skipped message key. Drops the oldest-inserted entry once
    /// the table reaches [`MAX_MESSAGE_KEYS`].
    pub fn add_sender_message_key(&mut self, key: SenderMessageKey) {
        let iteration = key.iteration;
        if self.skipped_message_keys.insert(iteration, key).is_none() {
            self.skipped_order.push(iteration);
        }
        while self.skipped_message_keys.len() > MAX_MESSAGE_KEYS {
            if let Some(oldest) = self.skipped_order.first().copied() {
                self.skipped_order.remove(0);
                self.skipped_message_keys.remove(&oldest);
            } else {
                break;
            }
        }
    }

    /// Pop a cached skipped message key for `iteration`.
    pub fn remove_sender_message_key(&mut self, iteration: u32) -> Option<SenderMessageKey> {
        let v = self.skipped_message_keys.remove(&iteration)?;
        if let Some(pos) = self.skipped_order.iter().position(|&x| x == iteration) {
            self.skipped_order.remove(pos);
        }
        Some(v)
    }

    /// Replace the chain key. Used by the cipher path to advance after every
    /// encrypt/decrypt.
    pub fn set_chain_key(&mut self, chain_key: SenderChainKey) {
        self.chain_key = chain_key;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_key_advances_deterministically() {
        let a = SenderChainKey::new(0, [7u8; 32]);
        let b = a.next();
        let c = a.next();
        assert_eq!(b, c, "next() is deterministic");
        assert_eq!(b.iteration, 1);
        assert_ne!(a.seed, b.seed);
    }

    #[test]
    fn message_key_differs_from_chain_seed() {
        let ck = SenderChainKey::new(0, [7u8; 32]);
        assert_ne!(ck.message_key(), ck.seed);
        // Message key from same chain key is deterministic.
        assert_eq!(ck.message_key(), ck.message_key());
    }

    #[test]
    fn sender_message_key_split_is_iv16_cipher32() {
        let ck = SenderChainKey::new(0, [7u8; 32]);
        let mk = ck.sender_message_key();
        assert_eq!(mk.iv.len(), 16);
        assert_eq!(mk.cipher_key.len(), 32);
        assert_eq!(mk.iteration, 0);
        // Re-derivation matches.
        let again = SenderMessageKey::derive(0, ck.message_key());
        assert_eq!(mk, again);
    }

    #[test]
    fn skipped_keys_capped_at_max() {
        let chain_key = [9u8; 32];
        let kp = KeyPair::from_private([1u8; 32]);
        let mut st = SenderKeyState::new_own(1, 0, chain_key, kp);
        // Insert MAX + 5 skipped keys; expect MAX retained, oldest dropped.
        for i in 0..(MAX_MESSAGE_KEYS as u32 + 5) {
            let mk = SenderMessageKey::derive(i, [i as u8; 32]);
            st.add_sender_message_key(mk);
        }
        assert_eq!(st.skipped_message_keys.len(), MAX_MESSAGE_KEYS);
        // The first 5 iterations should be evicted.
        for i in 0..5u32 {
            assert!(!st.has_sender_message_key(i));
        }
        // The most recent kept ones should still be there.
        assert!(st.has_sender_message_key(MAX_MESSAGE_KEYS as u32 + 4));
    }
}
