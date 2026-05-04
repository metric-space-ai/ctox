use ::hkdf::Hkdf;
use sha2::Sha256;

use crate::error::CryptoError;

/// HKDF-SHA256 with optional salt and info, returning `length` bytes. Matches
/// `hkdfutil.SHA256` in whatsmeow.
pub fn hkdf_sha256(key: &[u8], salt: &[u8], info: &[u8], length: usize) -> Result<Vec<u8>, CryptoError> {
    let salt_opt = if salt.is_empty() { None } else { Some(salt) };
    let h = Hkdf::<Sha256>::new(salt_opt, key);
    let mut out = vec![0u8; length];
    h.expand(info, &mut out)
        .map_err(|e| CryptoError::Internal(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_rfc5869_vector_1() {
        // RFC 5869 §A.1, test case 1
        let ikm = hex_decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = hex_decode("000102030405060708090a0b0c");
        let info = hex_decode("f0f1f2f3f4f5f6f7f8f9");
        let okm = hkdf_sha256(&ikm, &salt, &info, 42).unwrap();
        let expect = hex_decode("3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865");
        assert_eq!(okm, expect);
    }

    fn hex_decode(s: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(s.len() / 2);
        let bytes = s.as_bytes();
        for chunk in bytes.chunks(2) {
            let hi = nib(chunk[0]);
            let lo = nib(chunk[1]);
            out.push((hi << 4) | lo);
        }
        out
    }

    fn nib(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => 10 + c - b'a',
            b'A'..=b'F' => 10 + c - b'A',
            _ => 0,
        }
    }
}
