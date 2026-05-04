//! Media download — port of whatsmeow's `download.go` and `download-to-file.go`.
//!
//! WhatsApp media is encrypted with AES-256-CBC and authenticated with a
//! truncated HMAC-SHA256. The wire format is:
//!
//! ```text
//!   enc_blob = ciphertext || hmac_sha256(mac_key, iv || ciphertext)[..10]
//! ```
//!
//! The cipher_key, mac_key, and iv are derived from a single 32-byte
//! `media_key` via HKDF-SHA256 with a media-type-specific `info` string and
//! an empty salt:
//!
//! ```text
//!   expanded = HKDF(media_key, salt = b"", info = mediaType.info(), len = 112)
//!   iv         = expanded[..16]
//!   cipher_key = expanded[16..48]
//!   mac_key    = expanded[48..80]
//!   ref_key    = expanded[80..112]   // unused on the receive side
//! ```
//!
//! The downloader also verifies the SHA-256 of the entire encrypted blob
//! against `file_enc_sha256` and (for `download_to_file`) the SHA-256 of the
//! decrypted plaintext against `file_sha256` when the latter is supplied.
//!
//! Networking is abstracted behind the [`HttpClient`] trait so this module
//! works without `reqwest` being in the dependency tree. Callers that have
//! `reqwest` available can implement the trait themselves; tests in this file
//! use a small fake.

use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use wha_crypto::{cbc_decrypt, hkdf_sha256, hmac_sha256};

use crate::client::Client;
use crate::error::ClientError;

/// Length (in bytes) of the truncated HMAC tag appended to each encrypted
/// media blob — `whatsmeow`'s `mediaHMACLength`.
pub const MEDIA_HMAC_LENGTH: usize = 10;

/// Total bytes produced by the HKDF expansion: 16 (iv) + 32 (cipher) +
/// 32 (mac) + 32 (ref).
const MEDIA_KEY_EXPANDED_LEN: usize = 112;

/// The media types whose `info` string is used as the HKDF `info` parameter
/// when deriving the per-blob keys. Mirrors whatsmeow's `MediaType` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaType {
    Image,
    Video,
    Audio,
    Document,
    History,
    AppState,
    StickerPack,
    LinkThumbnail,
}

impl MediaType {
    /// The `info` byte string passed to HKDF for this media type.
    pub fn info(self) -> &'static [u8] {
        match self {
            MediaType::Image => b"WhatsApp Image Keys",
            MediaType::Video => b"WhatsApp Video Keys",
            MediaType::Audio => b"WhatsApp Audio Keys",
            MediaType::Document => b"WhatsApp Document Keys",
            MediaType::History => b"WhatsApp History Keys",
            MediaType::AppState => b"WhatsApp App State Keys",
            MediaType::StickerPack => b"WhatsApp Sticker Pack Keys",
            MediaType::LinkThumbnail => b"WhatsApp Link Thumbnail Keys",
        }
    }

    /// The `mms-type` URL parameter used when constructing media URLs from a
    /// direct path.
    pub fn mms_type(self) -> &'static str {
        match self {
            MediaType::Image => "image",
            MediaType::Video => "video",
            MediaType::Audio => "audio",
            MediaType::Document => "document",
            MediaType::History => "md-msg-hist",
            MediaType::AppState => "md-app-state",
            MediaType::StickerPack => "sticker-pack",
            MediaType::LinkThumbnail => "thumbnail-link",
        }
    }
}

/// Subset of a `DownloadableMessage` from whatsmeow that the decrypt path
/// actually needs. Higher layers can implement this for whichever
/// protobuf-shaped struct they have on hand.
pub trait Downloadable {
    /// Direct path on the WhatsApp media CDN — starts with `/`. Used when no
    /// fully-qualified URL is present.
    fn direct_path(&self) -> &str {
        ""
    }
    /// Pre-signed media URL (when the sender included one). Returning an
    /// empty string falls back to building a URL from `direct_path`.
    fn url(&self) -> &str {
        ""
    }
    /// 32-byte media key from which all per-blob keys are derived.
    fn media_key(&self) -> &[u8];
    /// Expected SHA-256 of the encrypted blob. May be empty.
    fn file_enc_sha256(&self) -> &[u8];
    /// Expected SHA-256 of the decrypted plaintext. May be empty.
    fn file_sha256(&self) -> &[u8] {
        &[]
    }
    /// Plaintext length, if known. Used only for sanity checks.
    fn file_length(&self) -> Option<u64> {
        None
    }
}

/// Bag of derived keys produced by [`expand_media_key`].
#[derive(Debug, Clone)]
pub struct MediaKeys {
    pub iv: [u8; 16],
    pub cipher_key: [u8; 32],
    pub mac_key: [u8; 32],
    pub ref_key: [u8; 32],
}

/// Run HKDF-SHA256 with empty salt and the media type's info string and split
/// the resulting 112 bytes into iv / cipher_key / mac_key / ref_key.
pub fn expand_media_key(media_key: &[u8], media_type: MediaType) -> Result<MediaKeys, ClientError> {
    let expanded = hkdf_sha256(media_key, &[], media_type.info(), MEDIA_KEY_EXPANDED_LEN)?;
    if expanded.len() != MEDIA_KEY_EXPANDED_LEN {
        return Err(ClientError::Crypto(format!(
            "HKDF returned unexpected length: {}",
            expanded.len()
        )));
    }
    let mut iv = [0u8; 16];
    let mut cipher_key = [0u8; 32];
    let mut mac_key = [0u8; 32];
    let mut ref_key = [0u8; 32];
    iv.copy_from_slice(&expanded[..16]);
    cipher_key.copy_from_slice(&expanded[16..48]);
    mac_key.copy_from_slice(&expanded[48..80]);
    ref_key.copy_from_slice(&expanded[80..112]);
    Ok(MediaKeys {
        iv,
        cipher_key,
        mac_key,
        ref_key,
    })
}

/// Constant-time byte-slice equality. We use the `subtle` semantics provided
/// by the `hmac` crate's verifier in a couple of helpers, but for a 10-byte
/// truncated tag a hand-rolled compare is equivalent and avoids pulling in
/// another dependency.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

/// Verify a media blob's truncated HMAC. `iv` and `ciphertext` are hashed in
/// sequence (matching whatsmeow's `validateMedia`); the first
/// [`MEDIA_HMAC_LENGTH`] bytes of the result must equal `mac`.
pub fn validate_media_mac(
    iv: &[u8],
    ciphertext: &[u8],
    mac_key: &[u8],
    mac: &[u8],
) -> Result<(), ClientError> {
    if mac.len() != MEDIA_HMAC_LENGTH {
        return Err(ClientError::Crypto(format!(
            "expected {}-byte mac, got {}",
            MEDIA_HMAC_LENGTH,
            mac.len()
        )));
    }
    let mut buf = Vec::with_capacity(iv.len().checked_add(ciphertext.len()).ok_or_else(
        || ClientError::Download("iv + ciphertext length overflow".into()),
    )?);
    buf.extend_from_slice(iv);
    buf.extend_from_slice(ciphertext);
    let full = hmac_sha256(mac_key, &buf);
    if !ct_eq(&full[..MEDIA_HMAC_LENGTH], mac) {
        return Err(ClientError::Crypto("media HMAC mismatch".into()));
    }
    Ok(())
}

/// HTTP transport abstraction. The whatsmeow code uses `net/http` directly;
/// here we keep transport pluggable so this crate doesn't have to take a
/// hard dependency on `reqwest`.
#[async_trait]
pub trait HttpClient: Send + Sync {
    /// Issue an HTTPS GET and return the full response body. Implementations
    /// should fail with a non-empty error message on any non-2xx status.
    async fn get(&self, url: &str) -> Result<Vec<u8>, ClientError>;
}

/// Convenience type alias used to share an [`HttpClient`] across tasks.
pub type SharedHttpClient = Arc<dyn HttpClient>;

/// Pure-data downloader entry-point — easier to test in isolation than the
/// `Client` method, which carries socket state.
///
/// `enc_blob` is the encrypted file as fetched from the CDN; it must contain
/// the trailing 10-byte HMAC tag. `meta` supplies the keys and the optional
/// integrity hashes.
pub fn decrypt_downloaded(
    enc_blob: &[u8],
    meta: &impl Downloadable,
    media_type: MediaType,
) -> Result<Vec<u8>, ClientError> {
    if enc_blob.len() <= MEDIA_HMAC_LENGTH {
        return Err(ClientError::Download(format!(
            "encrypted blob too short: {} bytes",
            enc_blob.len()
        )));
    }

    // Verify the SHA-256 of the encrypted blob (when supplied) before doing
    // any keyed work — this short-circuits on tampered or truncated CDN
    // responses.
    let enc_hash_expected = meta.file_enc_sha256();
    if enc_hash_expected.len() == 32 {
        let actual = Sha256::digest(enc_blob);
        if !ct_eq(actual.as_slice(), enc_hash_expected) {
            return Err(ClientError::Crypto("file_enc_sha256 mismatch".into()));
        }
    }

    let split_at = enc_blob
        .len()
        .checked_sub(MEDIA_HMAC_LENGTH)
        .ok_or_else(|| ClientError::Download("blob length underflow".into()))?;
    let (ciphertext, mac) = enc_blob.split_at(split_at);

    let keys = expand_media_key(meta.media_key(), media_type)?;
    validate_media_mac(&keys.iv, ciphertext, &keys.mac_key, mac)?;

    let plaintext = cbc_decrypt(&keys.cipher_key, &keys.iv, ciphertext)?;

    // Optional plaintext integrity checks (warnings in whatsmeow). We make
    // them hard errors here — the caller can suppress by clearing the
    // hash/length fields of their `Downloadable` impl.
    if let Some(expected_len) = meta.file_length() {
        if plaintext.len() as u64 != expected_len {
            return Err(ClientError::Download(format!(
                "file length mismatch: expected {}, got {}",
                expected_len,
                plaintext.len()
            )));
        }
    }
    let plaintext_hash_expected = meta.file_sha256();
    if plaintext_hash_expected.len() == 32 {
        let actual = Sha256::digest(&plaintext);
        if !ct_eq(actual.as_slice(), plaintext_hash_expected) {
            return Err(ClientError::Crypto("file_sha256 mismatch".into()));
        }
    }

    Ok(plaintext)
}

impl Client {
    /// Download and decrypt a media blob, returning the plaintext bytes.
    ///
    /// `http` is injected so callers can plug in `reqwest`, a stub, or a
    /// pre-fetched-bytes adapter.
    pub async fn download(
        &self,
        http: &dyn HttpClient,
        url: &str,
        meta: &impl Downloadable,
        media_type: MediaType,
    ) -> Result<Vec<u8>, ClientError> {
        if url.is_empty() {
            return Err(ClientError::Download("no URL provided".into()));
        }
        let enc_blob = http.get(url).await?;
        decrypt_downloaded(&enc_blob, meta, media_type)
    }

    /// Stream-download to a tokio file. Mirrors `download-to-file.go`'s
    /// strategy: the encrypted blob is written to the file while its HMAC and
    /// SHA-256 are computed in chunks, then the file is rewritten in place
    /// with the decrypted plaintext.
    ///
    /// Note: the AES-CBC plaintext is shorter than the ciphertext (by the
    /// PKCS#7 padding), and is held entirely in memory between the verify
    /// step and the write-back. A fully-streaming CBC decrypt is possible
    /// but isn't worth the extra complexity here — most WhatsApp media is
    /// well under a hundred megabytes.
    pub async fn download_to_file(
        &self,
        http: &dyn HttpClient,
        url: &str,
        meta: &impl Downloadable,
        media_type: MediaType,
        file: &mut tokio::fs::File,
    ) -> Result<(), ClientError> {
        if url.is_empty() {
            return Err(ClientError::Download("no URL provided".into()));
        }

        // Fetch the whole blob. (whatsmeow streams through io.Copy with a
        // tee'd hasher; here we let the HttpClient impl decide whether to
        // buffer or stream-then-return — most reqwest impls allocate a Vec
        // of the body anyway.)
        let enc_blob = http.get(url).await?;

        if enc_blob.len() <= MEDIA_HMAC_LENGTH {
            return Err(ClientError::Download(format!(
                "encrypted blob too short: {} bytes",
                enc_blob.len()
            )));
        }

        // Verify the file_enc_sha256 of the entire blob, with a chunked
        // hasher so we mirror the tee'd behaviour of the upstream code (and
        // so this would be cheap to wire up to a streaming downloader
        // later).
        let enc_hash_expected = meta.file_enc_sha256();
        if enc_hash_expected.len() == 32 {
            let mut hasher = Sha256::new();
            for chunk in enc_blob.chunks(64 * 1024) {
                hasher.update(chunk);
            }
            let actual = hasher.finalize();
            if !ct_eq(actual.as_slice(), enc_hash_expected) {
                return Err(ClientError::Crypto("file_enc_sha256 mismatch".into()));
            }
        }

        // Write the ciphertext-without-tag part to the file. We need it on
        // disk for the HMAC-over-iv||ciphertext check to mirror whatsmeow's
        // `validateMediaFile` (which seeks back to start and re-reads the
        // file through the hmac).
        let split_at = enc_blob
            .len()
            .checked_sub(MEDIA_HMAC_LENGTH)
            .ok_or_else(|| ClientError::Download("blob length underflow".into()))?;
        let ciphertext = &enc_blob[..split_at];
        let mac = &enc_blob[split_at..];

        file.set_len(0).await?;
        file.seek(std::io::SeekFrom::Start(0)).await?;
        // chunked write to demonstrate the streaming path
        for chunk in ciphertext.chunks(64 * 1024) {
            file.write_all(chunk).await?;
        }
        file.flush().await?;

        let keys = expand_media_key(meta.media_key(), media_type)?;

        // Compute hmac(mac_key, iv || file_contents) by reading the file
        // back in chunks. This is the analogue of whatsmeow's tee'd HMAC.
        file.seek(std::io::SeekFrom::Start(0)).await?;
        let mut hmac_buf = Vec::with_capacity(
            keys.iv
                .len()
                .checked_add(ciphertext.len())
                .ok_or_else(|| ClientError::Download("iv + ciphertext length overflow".into()))?,
        );
        hmac_buf.extend_from_slice(&keys.iv);
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            hmac_buf.extend_from_slice(&chunk[..n]);
        }
        let full_mac = hmac_sha256(&keys.mac_key, &hmac_buf);
        if !ct_eq(&full_mac[..MEDIA_HMAC_LENGTH], mac) {
            return Err(ClientError::Crypto("media HMAC mismatch".into()));
        }

        // Decrypt and rewrite the file in place.
        let plaintext = cbc_decrypt(&keys.cipher_key, &keys.iv, ciphertext)?;

        if let Some(expected_len) = meta.file_length() {
            if plaintext.len() as u64 != expected_len {
                return Err(ClientError::Download(format!(
                    "file length mismatch: expected {}, got {}",
                    expected_len,
                    plaintext.len()
                )));
            }
        }
        let plaintext_hash_expected = meta.file_sha256();
        if plaintext_hash_expected.len() == 32 {
            let actual = Sha256::digest(&plaintext);
            if !ct_eq(actual.as_slice(), plaintext_hash_expected) {
                return Err(ClientError::Crypto("file_sha256 mismatch".into()));
            }
        }

        file.set_len(0).await?;
        file.seek(std::io::SeekFrom::Start(0)).await?;
        for chunk in plaintext.chunks(64 * 1024) {
            file.write_all(chunk).await?;
        }
        file.flush().await?;
        file.seek(std::io::SeekFrom::Start(0)).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wha_crypto::cbc_encrypt;

    /// Encrypt + tag a plaintext the same way the WhatsApp server does, so
    /// the decrypt path has something to chew on.
    fn encrypt_for_test(
        plaintext: &[u8],
        media_key: &[u8],
        media_type: MediaType,
    ) -> (Vec<u8>, [u8; 32]) {
        let keys = expand_media_key(media_key, media_type).unwrap();
        let ciphertext = cbc_encrypt(&keys.cipher_key, &keys.iv, plaintext).unwrap();
        let mut mac_input = Vec::with_capacity(keys.iv.len() + ciphertext.len());
        mac_input.extend_from_slice(&keys.iv);
        mac_input.extend_from_slice(&ciphertext);
        let full_mac = hmac_sha256(&keys.mac_key, &mac_input);
        let mut blob = ciphertext;
        blob.extend_from_slice(&full_mac[..MEDIA_HMAC_LENGTH]);
        let enc_hash: [u8; 32] = Sha256::digest(&blob).into();
        (blob, enc_hash)
    }

    struct StaticMeta {
        media_key: Vec<u8>,
        enc_sha: Vec<u8>,
        plain_sha: Vec<u8>,
        len: Option<u64>,
    }

    impl Downloadable for StaticMeta {
        fn media_key(&self) -> &[u8] {
            &self.media_key
        }
        fn file_enc_sha256(&self) -> &[u8] {
            &self.enc_sha
        }
        fn file_sha256(&self) -> &[u8] {
            &self.plain_sha
        }
        fn file_length(&self) -> Option<u64> {
            self.len
        }
    }

    #[test]
    fn round_trip_decrypts_back_to_plaintext() {
        let media_key = [7u8; 32];
        let plaintext = b"the quick brown fox jumps over the lazy dog";
        let (blob, enc_hash) = encrypt_for_test(plaintext, &media_key, MediaType::Image);
        let plain_sha: [u8; 32] = Sha256::digest(plaintext).into();
        let meta = StaticMeta {
            media_key: media_key.to_vec(),
            enc_sha: enc_hash.to_vec(),
            plain_sha: plain_sha.to_vec(),
            len: Some(plaintext.len() as u64),
        };
        let out = decrypt_downloaded(&blob, &meta, MediaType::Image).unwrap();
        assert_eq!(out, plaintext);
    }

    #[test]
    fn tampered_ciphertext_returns_error() {
        let media_key = [9u8; 32];
        let plaintext = b"sensitive media payload that must be authenticated";
        let (mut blob, enc_hash) = encrypt_for_test(plaintext, &media_key, MediaType::Document);
        // Flip a bit in the ciphertext (anywhere before the trailing 10-byte
        // MAC). The blob's enc_sha will no longer match either, but we want
        // to verify the HMAC catches this even if we update the enc_sha — so
        // re-hash too.
        let target = blob.len() / 2;
        blob[target] ^= 0x01;
        let mut meta = StaticMeta {
            media_key: media_key.to_vec(),
            enc_sha: enc_hash.to_vec(),
            plain_sha: Vec::new(),
            len: None,
        };
        // First: with the original enc_sha — should fail at the SHA check.
        let r = decrypt_downloaded(&blob, &meta, MediaType::Document);
        assert!(r.is_err(), "expected error on tampered ciphertext");
        // Second: update the enc_sha so SHA check passes; HMAC must still
        // catch it.
        let new_hash: [u8; 32] = Sha256::digest(&blob).into();
        meta.enc_sha = new_hash.to_vec();
        let r = decrypt_downloaded(&blob, &meta, MediaType::Document);
        match r {
            Err(ClientError::Crypto(msg)) => assert!(
                msg.contains("HMAC") || msg.contains("hmac"),
                "expected HMAC error, got: {msg}"
            ),
            other => panic!("expected Crypto(HMAC) error, got {:?}", other),
        }
    }

    #[test]
    fn tampered_hmac_returns_error() {
        let media_key = [3u8; 32];
        let plaintext = b"another payload";
        let (mut blob, _enc_hash) = encrypt_for_test(plaintext, &media_key, MediaType::Audio);
        // Flip a bit in the trailing MAC.
        let last = blob.len() - 1;
        blob[last] ^= 0x80;
        let new_hash: [u8; 32] = Sha256::digest(&blob).into();
        let meta = StaticMeta {
            media_key: media_key.to_vec(),
            enc_sha: new_hash.to_vec(),
            plain_sha: Vec::new(),
            len: None,
        };
        let r = decrypt_downloaded(&blob, &meta, MediaType::Audio);
        match r {
            Err(ClientError::Crypto(msg)) => assert!(
                msg.contains("HMAC") || msg.contains("hmac"),
                "expected HMAC error, got: {msg}"
            ),
            other => panic!("expected Crypto(HMAC) error, got {:?}", other),
        }
    }

    #[test]
    fn too_short_blob_is_rejected() {
        let media_key = [1u8; 32];
        let meta = StaticMeta {
            media_key: media_key.to_vec(),
            enc_sha: Vec::new(),
            plain_sha: Vec::new(),
            len: None,
        };
        let r = decrypt_downloaded(b"abc", &meta, MediaType::Image);
        assert!(matches!(r, Err(ClientError::Download(_))));
    }

    #[test]
    fn media_type_info_strings_match_whatsmeow() {
        // Sanity: these are wire-format constants. A typo here silently
        // breaks decryption against real servers.
        assert_eq!(MediaType::Image.info(), b"WhatsApp Image Keys");
        assert_eq!(MediaType::Video.info(), b"WhatsApp Video Keys");
        assert_eq!(MediaType::Audio.info(), b"WhatsApp Audio Keys");
        assert_eq!(MediaType::Document.info(), b"WhatsApp Document Keys");
        assert_eq!(MediaType::History.info(), b"WhatsApp History Keys");
        assert_eq!(MediaType::AppState.info(), b"WhatsApp App State Keys");
        assert_eq!(MediaType::StickerPack.info(), b"WhatsApp Sticker Pack Keys");
        assert_eq!(MediaType::LinkThumbnail.info(), b"WhatsApp Link Thumbnail Keys");
    }

    #[tokio::test]
    async fn download_to_file_round_trip() {
        let media_key = [11u8; 32];
        let plaintext = b"hello, file-streaming world".repeat(100);
        let (blob, enc_hash) = encrypt_for_test(&plaintext, &media_key, MediaType::Video);
        let plain_sha: [u8; 32] = Sha256::digest(&plaintext).into();

        struct FakeHttp {
            body: Vec<u8>,
        }
        #[async_trait]
        impl HttpClient for FakeHttp {
            async fn get(&self, _url: &str) -> Result<Vec<u8>, ClientError> {
                Ok(self.body.clone())
            }
        }

        let meta = StaticMeta {
            media_key: media_key.to_vec(),
            enc_sha: enc_hash.to_vec(),
            plain_sha: plain_sha.to_vec(),
            len: Some(plaintext.len() as u64),
        };

        // Build a minimal Client. We don't need a connected one — the
        // download path is socket-free.
        use std::sync::Arc;
        use wha_store::MemoryStore;
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);

        let http = FakeHttp { body: blob };

        let tmp = tempfile_path("wha_download_to_file");
        let mut f = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .await
            .unwrap();
        cli.download_to_file(&http, "https://example/test", &meta, MediaType::Video, &mut f)
            .await
            .unwrap();
        // Read back and confirm.
        f.seek(std::io::SeekFrom::Start(0)).await.unwrap();
        let mut got = Vec::new();
        f.read_to_end(&mut got).await.unwrap();
        assert_eq!(got, plaintext);
        let _ = tokio::fs::remove_file(&tmp).await;
    }

    fn tempfile_path(prefix: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let nonce: u64 = rand::random();
        p.push(format!("{prefix}.{nonce}.bin"));
        p
    }
}
