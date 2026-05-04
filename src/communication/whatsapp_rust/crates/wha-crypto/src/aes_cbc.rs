//! AES-CBC with PKCS#7 padding. This is the mode WhatsApp uses for media
//! file encryption (`media_message`'s body) — *not* for the noise transport.
//!
//! Also exposes a small AES-256-CTR helper used by the phone-code pairing
//! flow (`whatsmeow/pair-code.go`), which encrypts the companion's ephemeral
//! pubkey under a code-derived key.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit};

use crate::error::CryptoError;

type Encryptor = cbc::Encryptor<aes::Aes256>;
type Decryptor = cbc::Decryptor<aes::Aes256>;

pub fn cbc_encrypt(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if key.len() != 32 {
        return Err(CryptoError::InvalidKeyLength(key.len()));
    }
    if iv.len() != 16 {
        return Err(CryptoError::InvalidIv(iv.len()));
    }
    let enc = Encryptor::new_from_slices(key, iv).map_err(|e| CryptoError::Internal(e.to_string()))?;
    Ok(enc.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

pub fn cbc_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if key.len() != 32 {
        return Err(CryptoError::InvalidKeyLength(key.len()));
    }
    if iv.len() != 16 {
        return Err(CryptoError::InvalidIv(iv.len()));
    }
    let dec = Decryptor::new_from_slices(key, iv).map_err(|e| CryptoError::Internal(e.to_string()))?;
    dec.decrypt_padded_vec_mut::<Pkcs7>(ciphertext).map_err(|_| CryptoError::UnpadFailed)
}

/// AES-256-CTR XOR (encryption and decryption are the same operation). The
/// counter is a big-endian 128-bit integer initialised to `iv`. Used by the
/// phone-code pair flow.
pub fn ctr_xor(key: &[u8], iv: &[u8], data: &mut [u8]) -> Result<(), CryptoError> {
    if key.len() != 32 {
        return Err(CryptoError::InvalidKeyLength(key.len()));
    }
    if iv.len() != 16 {
        return Err(CryptoError::InvalidIv(iv.len()));
    }
    use aes::cipher::BlockEncrypt;
    use aes::cipher::generic_array::GenericArray;

    let cipher = aes::Aes256::new_from_slice(key).map_err(|e| CryptoError::Internal(e.to_string()))?;
    let mut counter = [0u8; 16];
    counter.copy_from_slice(iv);

    for chunk in data.chunks_mut(16) {
        let mut block = GenericArray::clone_from_slice(&counter);
        cipher.encrypt_block(&mut block);
        for (b, k) in chunk.iter_mut().zip(block.iter()) {
            *b ^= *k;
        }
        // Increment the 128-bit big-endian counter.
        for i in (0..16).rev() {
            counter[i] = counter[i].wrapping_add(1);
            if counter[i] != 0 {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let key = [9u8; 32];
        let iv = [1u8; 16];
        let pt = b"hello whatsapp media";
        let ct = cbc_encrypt(&key, &iv, pt).unwrap();
        let back = cbc_decrypt(&key, &iv, &ct).unwrap();
        assert_eq!(back, pt);
    }

    #[test]
    fn tamper_detected_via_padding() {
        let key = [9u8; 32];
        let iv = [1u8; 16];
        let mut ct = cbc_encrypt(&key, &iv, b"abcdefgh").unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0x80;
        assert!(cbc_decrypt(&key, &iv, &ct).is_err());
    }
}
