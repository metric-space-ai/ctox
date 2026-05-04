//! Long-term identity key for a Signal participant.
//!
//! Mirrors `go.mau.fi/libsignal/keys/identity.IdentityKeyPair`. We wrap
//! [`wha_crypto::KeyPair`] (X25519 + XEdDSA) and add the convenience helper
//! used during pre-key registration: signing a `PreKey`'s public point with
//! the canonical libsignal prefix `0x05 || pub`.

use wha_crypto::{KeyPair, PreKey};

use crate::SignalProtocolError;

/// Long-term identity key. The X25519 keypair is reused for XEdDSA signing.
#[derive(Debug, Clone)]
pub struct IdentityKeyPair {
    pub key_pair: KeyPair,
}

impl IdentityKeyPair {
    pub fn new(key_pair: KeyPair) -> Self {
        Self { key_pair }
    }

    pub fn from_private(private: [u8; 32]) -> Self {
        Self { key_pair: KeyPair::from_private(private) }
    }

    pub fn public(&self) -> [u8; 32] {
        self.key_pair.public
    }

    pub fn private(&self) -> [u8; 32] {
        self.key_pair.private
    }

    /// Sign a pre-key's public point with this identity key. The signed
    /// payload is `0x05 || pub`, matching libsignal's `DjbECPublicKey.Serialize`.
    pub fn sign_pre_key(&self, prekey: &PreKey) -> [u8; 64] {
        let mut to_sign = [0u8; 33];
        to_sign[0] = 0x05;
        to_sign[1..].copy_from_slice(&prekey.key_pair.public);
        self.key_pair.sign(&to_sign)
    }

    /// Verify a pre-key signature using a remote identity public key.
    /// Helper used while ingesting a `PreKeyBundle` from the server.
    pub fn verify_pre_key_signature(
        identity_pub: &[u8; 32],
        prekey_pub: &[u8; 32],
        signature: &[u8; 64],
    ) -> Result<(), SignalProtocolError> {
        let mut signed = [0u8; 33];
        signed[0] = 0x05;
        signed[1..].copy_from_slice(prekey_pub);
        KeyPair::verify(identity_pub, &signed, signature)
            .map_err(|_| SignalProtocolError::BadSignature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn sign_and_verify_round_trip() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(11);
        let identity = IdentityKeyPair::new(KeyPair::generate(&mut rng));
        let prekey = PreKey::new(7, KeyPair::generate(&mut rng));
        let sig = identity.sign_pre_key(&prekey);
        IdentityKeyPair::verify_pre_key_signature(
            &identity.public(),
            &prekey.key_pair.public,
            &sig,
        )
        .expect("verify");
    }
}
