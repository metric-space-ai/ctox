//! Root key + DH ratchet step.
//!
//! Ported from `go.mau.fi/libsignal/keys/root/RootKey.go`. The asymmetric
//! half of the Double Ratchet: when a peer ratchets, we mix our private
//! ratchet key with their public ratchet key into a fresh root + chain key.

use wha_crypto::{hkdf_sha256, KeyPair};

use crate::chain_key::ChainKey;

/// libsignal's `info` parameter when expanding the root.
const KDF_INFO_RATCHET: &[u8] = b"WhisperRatchet";
const DERIVED_SECRETS_SIZE: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootKey {
    pub key: [u8; 32],
}

impl RootKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }

    /// Mix our ratchet keypair with the peer's ratchet public key into a
    /// fresh root + chain key. This is the canonical "DH ratchet step".
    pub fn create_chain(
        &self,
        their_ratchet_pub: &[u8; 32],
        our_ratchet: &KeyPair,
    ) -> (RootKey, ChainKey) {
        let dh = our_ratchet.shared_secret(their_ratchet_pub);
        // libsignal: HKDF-SHA256(IKM=dh, salt=current_root, info="WhisperRatchet") -> 64 bytes.
        let derived = hkdf_sha256(&dh, &self.key, KDF_INFO_RATCHET, DERIVED_SECRETS_SIZE)
            .expect("HKDF with 64 bytes output cannot fail");
        let mut next_root = [0u8; 32];
        let mut chain = [0u8; 32];
        next_root.copy_from_slice(&derived[..32]);
        chain.copy_from_slice(&derived[32..]);
        (RootKey::new(next_root), ChainKey::new(chain, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn dh_ratchet_step_is_symmetric() {
        // If both sides start with the same root and run the DH ratchet,
        // they should converge — that's the property the Double Ratchet
        // relies on.
        let mut rng = rand::rngs::StdRng::seed_from_u64(123);
        let alice = KeyPair::generate(&mut rng);
        let bob = KeyPair::generate(&mut rng);
        let root = RootKey::new([7u8; 32]);
        let (next_a, chain_a) = root.create_chain(&bob.public, &alice);
        let (next_b, chain_b) = root.create_chain(&alice.public, &bob);
        assert_eq!(next_a, next_b);
        assert_eq!(chain_a, chain_b);
    }
}
