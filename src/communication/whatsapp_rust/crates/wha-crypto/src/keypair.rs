//! X25519 key pair plus an Ed25519-via-Curve25519 signing helper. WhatsApp
//! signs all of its identity material with X25519 keys reinterpreted as
//! Ed25519 (XEdDSA / "DjbECPrivateKey" in libsignal); this module wraps that
//! protocol so the rest of the codebase can ignore the details.

use curve25519_dalek::{edwards::CompressedEdwardsY, scalar::Scalar};
use rand::RngCore;
use sha2::{Digest, Sha512};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::error::CryptoError;

pub const PUBLIC_KEY_LEN: usize = 32;
pub const PRIVATE_KEY_LEN: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyPair {
    pub public: [u8; PUBLIC_KEY_LEN],
    pub private: [u8; PRIVATE_KEY_LEN],
}

impl KeyPair {
    /// Generate a fresh X25519 key pair using the supplied RNG.
    pub fn generate<R: RngCore>(rng: &mut R) -> Self {
        let mut priv_bytes = [0u8; PRIVATE_KEY_LEN];
        rng.fill_bytes(&mut priv_bytes);
        priv_bytes[0] &= 248;
        priv_bytes[31] &= 127;
        priv_bytes[31] |= 64;
        Self::from_private(priv_bytes)
    }

    /// Build a key pair from an existing private key (e.g. one loaded from a
    /// store). The public key is recomputed.
    pub fn from_private(private: [u8; PRIVATE_KEY_LEN]) -> Self {
        let secret = StaticSecret::from(private);
        let public = PublicKey::from(&secret);
        KeyPair { public: *public.as_bytes(), private }
    }

    /// Diffie-Hellman with another party's public key.
    pub fn shared_secret(&self, peer_pub: &[u8; PUBLIC_KEY_LEN]) -> [u8; 32] {
        let secret = StaticSecret::from(self.private);
        let pub_key = PublicKey::from(*peer_pub);
        secret.diffie_hellman(&pub_key).to_bytes()
    }

    /// XEdDSA signature over `message`, per Trevor Perrin's spec
    /// (https://signal.org/docs/specifications/xeddsa/). The X25519 private
    /// scalar is reinterpreted as an Ed25519 key; if the corresponding Edwards
    /// point has a negative `y`-bit, both the scalar and point are negated so
    /// signing can use the canonical Ed25519 algorithm.
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        let mut rng = rand::thread_rng();
        let mut z = [0u8; 64];
        rng.fill_bytes(&mut z);
        self.sign_with_nonce(message, &z)
    }

    fn sign_with_nonce(&self, message: &[u8], z: &[u8; 64]) -> [u8; 64] {
        let mut a = Scalar::from_bytes_mod_order(self.private);
        let mut big_a = curve25519_dalek::constants::ED25519_BASEPOINT_POINT * a;
        // XEdDSA: if the Edwards public key has the high bit set, negate.
        if big_a.compress().as_bytes()[31] & 0x80 != 0 {
            a = -a;
            big_a = -big_a;
        }
        let signing_pub = big_a.compress();

        let mut h = Sha512::new();
        h.update([0xFEu8; 32]);
        h.update(a.to_bytes());
        h.update(message);
        h.update(z);
        let r = Scalar::from_bytes_mod_order_wide(&h.finalize().into());

        let big_r = (curve25519_dalek::constants::ED25519_BASEPOINT_POINT * r).compress();

        let mut hk = Sha512::new();
        hk.update(big_r.as_bytes());
        hk.update(signing_pub.as_bytes());
        hk.update(message);
        let k = Scalar::from_bytes_mod_order_wide(&hk.finalize().into());

        let s = r + k * a;

        let mut sig = [0u8; 64];
        sig[..32].copy_from_slice(big_r.as_bytes());
        sig[32..].copy_from_slice(s.as_bytes());
        sig
    }

    /// XEdDSA verify. `public` is an X25519 public key.
    pub fn verify(public: &[u8; PUBLIC_KEY_LEN], message: &[u8], sig: &[u8; 64]) -> Result<(), CryptoError> {
        // Convert Montgomery → Edwards with sign bit 0 (XEdDSA convention).
        let edwards = montgomery_to_edwards(public)
            .ok_or_else(|| CryptoError::Signature("bad pub".into()))?;

        let big_r_compressed = CompressedEdwardsY::from_slice(&sig[..32])
            .map_err(|e| CryptoError::Signature(e.to_string()))?;
        let _big_r = big_r_compressed
            .decompress()
            .ok_or_else(|| CryptoError::Signature("bad R".into()))?;

        let s_bytes: [u8; 32] = sig[32..].try_into().unwrap();
        let s = Scalar::from_canonical_bytes(s_bytes)
            .into_option()
            .ok_or_else(|| CryptoError::Signature("non-canonical s".into()))?;

        let mut hk = Sha512::new();
        hk.update(&sig[..32]);
        hk.update(edwards.compress().as_bytes());
        hk.update(message);
        let k = Scalar::from_bytes_mod_order_wide(&hk.finalize().into());

        let expected = curve25519_dalek::constants::ED25519_BASEPOINT_POINT * s - edwards * k;
        if expected.compress() == big_r_compressed {
            Ok(())
        } else {
            Err(CryptoError::Signature("verification failed".into()))
        }
    }
}

fn montgomery_to_edwards(public: &[u8; 32]) -> Option<curve25519_dalek::edwards::EdwardsPoint> {
    // XEdDSA verify always interprets the public key as the Edwards point
    // with the high bit cleared.
    let mont = curve25519_dalek::montgomery::MontgomeryPoint(*public);
    mont.to_edwards(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn generate_yields_clamped_private_key() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let kp = KeyPair::generate(&mut rng);
        assert_eq!(kp.private[0] & 7, 0);
        assert_eq!(kp.private[31] & 0xC0, 0x40);
    }

    #[test]
    fn dh_is_symmetric() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let a = KeyPair::generate(&mut rng);
        let b = KeyPair::generate(&mut rng);
        assert_eq!(a.shared_secret(&b.public), b.shared_secret(&a.public));
    }

    #[test]
    fn sign_then_verify() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let kp = KeyPair::generate(&mut rng);
        let msg = b"the quick brown fox";
        let sig = kp.sign(msg);
        KeyPair::verify(&kp.public, msg, &sig).expect("verify");
    }

    #[test]
    fn tampered_message_fails_verify() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let kp = KeyPair::generate(&mut rng);
        let sig = kp.sign(b"original");
        assert!(KeyPair::verify(&kp.public, b"tampered", &sig).is_err());
    }
}
