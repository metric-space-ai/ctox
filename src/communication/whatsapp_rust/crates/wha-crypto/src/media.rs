//! Media key derivation and decryption for WhatsApp encrypted media blobs.
//!
//! Mirrors `_upstream/whatsmeow/download.go::getMediaKeys` and the
//! `validateMedia` + `cbcutil.Decrypt` pair used by `downloadAndDecrypt`.
//!
//! WhatsApp media (images, audio, videos, documents, history-sync blobs,
//! app-state mutations, link-preview thumbnails, sticker packs, …) is
//! end-to-end encrypted with a per-message **mediaKey** (32 bytes) plus a
//! human-readable **app-info** label that names which key class we are
//! deriving. The label disambiguates keys from different attachment classes
//! so a stolen `mediaKey` for one image can never be reused as a video.
//!
//! Derivation:
//!
//! ```text
//! expanded = HKDF-SHA256(mediaKey, salt = "", info = app_info, len = 112)
//! iv         = expanded[ 0..16]
//! cipher_key = expanded[16..48]
//! mac_key    = expanded[48..80]
//! ref_key    = expanded[80..112]
//! ```
//!
//! Decryption: the on-the-wire blob is `body || mac10` where `mac10` is the
//! first 10 bytes of `HMAC-SHA256(mac_key, iv ‖ body)`. After verifying the
//! MAC in constant time, the body is AES-256-CBC-decrypted with `cipher_key`
//! and `iv` (PKCS#7-padded).

use crate::aes_cbc::cbc_decrypt;
use crate::error::CryptoError;
use crate::hkdf::hkdf_sha256;
use crate::hmac::hmac_sha256;

// ---------------------------------------------------------------------------
// app-info constants (one per known media class). These strings are part of
// the protocol — they MUST match WhatsApp's exactly. Mirrors the Go
// `MediaType` constants in `_upstream/whatsmeow/download.go:41-49`.

/// app-info for `ImageMessage` / `StickerMessage` / `StickerMetadata` blobs.
pub const IMAGE_INFO: &str = "WhatsApp Image Keys";
/// app-info for `VideoMessage` blobs.
pub const VIDEO_INFO: &str = "WhatsApp Video Keys";
/// app-info for `AudioMessage` blobs.
pub const AUDIO_INFO: &str = "WhatsApp Audio Keys";
/// app-info for `DocumentMessage` blobs.
pub const DOCUMENT_INFO: &str = "WhatsApp Document Keys";
/// app-info for `HistorySyncNotification` blobs — what we use to pull the
/// historical chat archive after pairing.
pub const HISTORY_INFO: &str = "WhatsApp History Keys";
/// app-info for app-state external blob references.
pub const APP_STATE_INFO: &str = "WhatsApp App State Keys";
/// app-info for sticker-pack blobs.
pub const STICKER_PACK_INFO: &str = "WhatsApp Sticker Pack Keys";
/// app-info for link-preview thumbnails on `ExtendedTextMessage`.
pub const LINK_THUMBNAIL_INFO: &str = "WhatsApp Link Thumbnail Keys";

// ---------------------------------------------------------------------------
// types

/// Output of [`derive_media_keys`]. The four sub-keys are split out from a
/// 112-byte HKDF expansion. `iv` is 16 bytes; `cipher_key`, `mac_key`, and
/// `ref_key` are 32 bytes each. Owned slices (no lifetimes) so callers can
/// keep them across awaits.
#[derive(Debug, Clone)]
pub struct MediaKeys {
    pub iv: [u8; 16],
    pub cipher_key: [u8; 32],
    pub mac_key: [u8; 32],
    pub ref_key: [u8; 32],
}

/// HKDF-expand a `mediaKey` into the 112 bytes of derived material WhatsApp
/// uses for one encrypted attachment, and split it into the four sub-keys.
///
/// `app_info` MUST be one of the `*_INFO` constants in this module — using a
/// different label for the same `media_key` produces a different (and
/// incompatible) derived key set.
pub fn derive_media_keys(media_key: &[u8], app_info: &str) -> Result<MediaKeys, CryptoError> {
    let expanded = hkdf_sha256(media_key, b"", app_info.as_bytes(), 112)?;
    let mut iv = [0u8; 16];
    iv.copy_from_slice(&expanded[0..16]);
    let mut cipher_key = [0u8; 32];
    cipher_key.copy_from_slice(&expanded[16..48]);
    let mut mac_key = [0u8; 32];
    mac_key.copy_from_slice(&expanded[48..80]);
    let mut ref_key = [0u8; 32];
    ref_key.copy_from_slice(&expanded[80..112]);
    Ok(MediaKeys { iv, cipher_key, mac_key, ref_key })
}

/// Decrypt a WhatsApp encrypted-media blob.
///
/// `blob` is `body || mac10` — the trailing 10 bytes are a truncated
/// `HMAC-SHA256(mac_key, iv ‖ body)`. Steps:
///
/// 1. Split `blob` into `body` and `mac10`.
/// 2. Recompute the expected MAC and compare in constant time.
/// 3. AES-256-CBC decrypt `body` with `cipher_key` + `iv`, removing PKCS#7
///    padding, and return the plaintext.
///
/// Mirrors `validateMedia` + `cbcutil.Decrypt` in
/// `_upstream/whatsmeow/download.go:289-318`.
pub fn decrypt_media(blob: &[u8], keys: &MediaKeys) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < 10 {
        return Err(CryptoError::Internal(
            "encrypted media blob shorter than 10-byte MAC".into(),
        ));
    }
    let split = blob.len() - 10;
    let body = &blob[..split];
    let mac = &blob[split..];

    // Expected MAC = first 10 bytes of HMAC-SHA256(mac_key, iv || body).
    let mut hmac_input = Vec::with_capacity(16 + body.len());
    hmac_input.extend_from_slice(&keys.iv);
    hmac_input.extend_from_slice(body);
    let full = hmac_sha256(&keys.mac_key, &hmac_input);
    let expected = &full[..10];

    // Constant-time compare. `mac.len() == expected.len() == 10` here, but we
    // still iterate the full slice via XOR-OR aggregation rather than `==`.
    let mut diff: u8 = 0;
    for (a, b) in mac.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    if diff != 0 {
        return Err(CryptoError::HmacMismatch);
    }

    cbc_decrypt(&keys.cipher_key, &keys.iv, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aes_cbc::cbc_encrypt;

    /// Deterministic vector: with `media_key=[1u8;32]` and the history-sync
    /// app-info, verify the derived sub-keys have the expected layout (right
    /// lengths, no overlap, deterministic across calls).
    #[test]
    fn derive_media_keys_layout() {
        let mk = [1u8; 32];
        let a = derive_media_keys(&mk, HISTORY_INFO).unwrap();
        let b = derive_media_keys(&mk, HISTORY_INFO).unwrap();
        assert_eq!(a.iv, b.iv);
        assert_eq!(a.cipher_key, b.cipher_key);
        assert_eq!(a.mac_key, b.mac_key);
        assert_eq!(a.ref_key, b.ref_key);
        assert_eq!(a.iv.len(), 16);
        assert_eq!(a.cipher_key.len(), 32);
        assert_eq!(a.mac_key.len(), 32);
        assert_eq!(a.ref_key.len(), 32);
        // Sub-keys are slices of HKDF output; they MUST differ unless HKDF
        // is broken — assert a few obvious non-equalities to pin that we
        // didn't accidentally copy the same window twice.
        assert_ne!(&a.iv[..], &a.cipher_key[..16]);
        assert_ne!(&a.cipher_key[..], &a.mac_key[..]);
        assert_ne!(&a.mac_key[..], &a.ref_key[..]);

        // Different app_info → different keys.
        let other = derive_media_keys(&mk, IMAGE_INFO).unwrap();
        assert_ne!(a.iv, other.iv);
    }

    /// Round-trip: encrypt with the derived keys + append HMAC, then call
    /// [`decrypt_media`] and assert we recover the original plaintext.
    #[test]
    fn decrypt_media_round_trip() {
        let mk = [7u8; 32];
        let keys = derive_media_keys(&mk, HISTORY_INFO).unwrap();
        let plaintext = b"hello whatsapp history sync world";

        // body = AES-256-CBC(cipher_key, iv, plaintext) with PKCS#7
        let body = cbc_encrypt(&keys.cipher_key, &keys.iv, plaintext).unwrap();

        // mac10 = first 10 bytes of HMAC-SHA256(mac_key, iv || body)
        let mut mac_in = Vec::new();
        mac_in.extend_from_slice(&keys.iv);
        mac_in.extend_from_slice(&body);
        let mac_full = hmac_sha256(&keys.mac_key, &mac_in);

        let mut blob = body.clone();
        blob.extend_from_slice(&mac_full[..10]);

        let recovered = decrypt_media(&blob, &keys).unwrap();
        assert_eq!(recovered, plaintext);
    }

    /// Tampering the MAC byte must surface as `HmacMismatch`.
    #[test]
    fn decrypt_media_rejects_tampered_mac() {
        let mk = [7u8; 32];
        let keys = derive_media_keys(&mk, HISTORY_INFO).unwrap();
        let body = cbc_encrypt(&keys.cipher_key, &keys.iv, b"x").unwrap();
        let mut mac_in = Vec::new();
        mac_in.extend_from_slice(&keys.iv);
        mac_in.extend_from_slice(&body);
        let mac_full = hmac_sha256(&keys.mac_key, &mac_in);

        let mut blob = body.clone();
        blob.extend_from_slice(&mac_full[..10]);
        // Flip one MAC bit.
        let last = blob.len() - 1;
        blob[last] ^= 0x01;

        let err = decrypt_media(&blob, &keys).unwrap_err();
        assert!(matches!(err, CryptoError::HmacMismatch));
    }

    #[test]
    fn decrypt_media_rejects_short_blob() {
        let keys = derive_media_keys(&[0u8; 32], HISTORY_INFO).unwrap();
        let err = decrypt_media(&[0u8; 9], &keys).unwrap_err();
        assert!(matches!(err, CryptoError::Internal(_)));
    }
}
