use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};

use crate::error::CryptoError;

/// AES-GCM encrypt with associated data, exactly mirroring whatsmeow's
/// `gcmutil.Encrypt`. The IV must be 12 bytes; the key 16 or 32.
pub fn gcm_encrypt(key: &[u8], iv: &[u8], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if iv.len() != 12 {
        return Err(CryptoError::InvalidIv(iv.len()));
    }
    let cipher = make_cipher(key)?;
    let nonce = Nonce::from_slice(iv);
    cipher
        .encrypt(nonce, Payload { msg: plaintext, aad })
        .map_err(|_| CryptoError::AeadFailed)
}

/// AES-GCM decrypt mirroring `gcmutil.Decrypt`.
pub fn gcm_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if iv.len() != 12 {
        return Err(CryptoError::InvalidIv(iv.len()));
    }
    let cipher = make_cipher(key)?;
    let nonce = Nonce::from_slice(iv);
    cipher
        .decrypt(nonce, Payload { msg: ciphertext, aad })
        .map_err(|_| CryptoError::AeadFailed)
}

fn make_cipher(key: &[u8]) -> Result<Aes256Gcm, CryptoError> {
    if key.len() != 32 {
        return Err(CryptoError::InvalidKeyLength(key.len()));
    }
    Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::InvalidKeyLength(key.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_aad() {
        let key = [7u8; 32];
        let iv = [3u8; 12];
        let pt = b"the noise handshake greeted you";
        let aad = b"context";
        let ct = gcm_encrypt(&key, &iv, pt, aad).unwrap();
        let back = gcm_decrypt(&key, &iv, &ct, aad).unwrap();
        assert_eq!(back, pt);
    }

    #[test]
    fn aad_mismatch_fails() {
        let key = [7u8; 32];
        let iv = [3u8; 12];
        let ct = gcm_encrypt(&key, &iv, b"x", b"a").unwrap();
        assert!(gcm_decrypt(&key, &iv, &ct, b"b").is_err());
    }
}
