use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha512};

use crate::error::CryptoError;

type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub fn hmac_sha256_verify(key: &[u8], data: &[u8], expected: &[u8]) -> Result<(), CryptoError> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.verify_slice(expected).map_err(|_| CryptoError::HmacMismatch)
}

/// HMAC-SHA256 over a sequence of byte chunks (no allocation of a concatenated buffer).
pub fn hmac_sha256_concat(key: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    for p in parts {
        mac.update(p);
    }
    mac.finalize().into_bytes().to_vec()
}

pub fn hmac_sha512(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <HmacSha512 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// HMAC-SHA512 over a sequence of byte chunks (mirrors Go's `hmac.Write`-loop pattern).
pub fn hmac_sha512_concat(key: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    let mut mac = <HmacSha512 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    for p in parts {
        mac.update(p);
    }
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = hmac_sha256(b"k", b"d");
        let b = hmac_sha256(b"k", b"d");
        assert_eq!(a, b);
    }

    #[test]
    fn verify_round_trip() {
        let mac = hmac_sha256(b"k", b"d");
        hmac_sha256_verify(b"k", b"d", &mac).unwrap();
        assert!(hmac_sha256_verify(b"k", b"X", &mac).is_err());
    }
}
