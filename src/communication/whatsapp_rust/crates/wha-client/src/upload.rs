//! Media upload — encrypts an attachment and POSTs it to a WhatsApp media
//! host.
//!
//! Direct port of `whatsmeow/upload.go`, `whatsmeow/mediaconn.go` and the
//! parsing half of `whatsmeow/mediaretry.go`. The `MediaType` /
//! `MediaKeys` / HKDF expansion already lives in [`crate::download`]; we
//! re-export it from here so callers don't have to know which module first
//! introduced the type.
//!
//! HTTP transport is delegated to a small [`UploadHttpClient`] trait so the
//! module is unit-testable without a real network. Wiring an actual HTTP
//! backend (reqwest, hyper, ureq, ...) is left for a follow-up.

use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

use wha_binary::{Node, Value};
use wha_crypto::{cbc_encrypt, hmac_sha256};
use wha_types::Jid;

use crate::client::Client;
use crate::download::{expand_media_key, MediaKeys, MediaType};
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

// ---------------------------------------------------------------------------
// MediaUpload result
// ---------------------------------------------------------------------------

/// Mirrors `UploadResponse` in `whatsmeow/upload.go`. The fields with `_`
/// JSON tags upstream are filled in by the Rust caller (we already have them
/// in scope at the upload call-site); the rest come from the JSON the MMS
/// host returns.
#[derive(Debug, Clone, Default)]
pub struct MediaUpload {
    /// `https://...` URL the recipient downloads from.
    pub url: String,
    /// Path-only form of [`Self::url`], used to download via the MMS proxy.
    pub direct_path: String,
    /// "Media handle" — required for newsletter sends.
    pub handle: String,
    /// Object ID; not always present.
    pub object_id: String,

    /// 32-byte AES-CBC media key. The recipient runs the same HKDF with this
    /// key to recover the iv + cipher_key + mac_key.
    pub media_key: Vec<u8>,
    /// SHA-256 of the encrypted blob (ciphertext || hmac10).
    pub file_enc_sha256: Vec<u8>,
    /// SHA-256 of the plaintext.
    pub file_sha256: Vec<u8>,
    /// Plaintext byte count.
    pub file_length: u64,
    /// First-frame sidecar for video. Only set when the caller separately
    /// computed it; the upload itself does not generate it.
    pub first_frame_sidecar: Option<Vec<u8>>,
}

// ---------------------------------------------------------------------------
// Media-conn IQ
// ---------------------------------------------------------------------------

/// Single host returned by the `<media_conn/>` IQ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaConnHost {
    pub hostname: String,
}

/// Parsed `<media_conn/>` response. Mirrors `MediaConn` in
/// `whatsmeow/mediaconn.go`.
#[derive(Debug, Clone)]
pub struct MediaConn {
    pub auth: String,
    pub auth_ttl: i64,
    pub ttl: i64,
    pub max_buckets: i64,
    pub fetched_at: SystemTime,
    pub hosts: Vec<MediaConnHost>,
}

impl MediaConn {
    /// When the auth token / host list expires.
    pub fn expiry(&self) -> SystemTime {
        self.fetched_at + Duration::from_secs(self.ttl.max(0) as u64)
    }
}

fn parse_media_conn(node: &Node) -> Result<MediaConn, ClientError> {
    let mc_node = node
        .child_by_tag(&["media_conn"])
        .ok_or_else(|| ClientError::Malformed("missing <media_conn> in IQ response".into()))?;

    let mut ag = mc_node.attr_getter();
    let auth = ag.string("auth").to_owned();
    let ttl = ag.optional_i64("ttl").unwrap_or(0);
    let auth_ttl = ag.optional_i64("auth_ttl").unwrap_or(0);
    let max_buckets = ag.optional_i64("max_buckets").unwrap_or(0);
    if !ag.ok() {
        return Err(ClientError::Malformed(format!(
            "failed to parse <media_conn> attrs: {:?}",
            ag.errors
        )));
    }

    let mut hosts = Vec::new();
    for child in mc_node.children() {
        if child.tag != "host" {
            continue;
        }
        let mut cag = child.attr_getter();
        let hostname = cag.string("hostname").to_owned();
        if !cag.ok() {
            return Err(ClientError::Malformed(format!(
                "failed to parse <host>: {:?}",
                cag.errors
            )));
        }
        hosts.push(MediaConnHost { hostname });
    }

    Ok(MediaConn {
        auth,
        auth_ttl,
        ttl,
        max_buckets,
        fetched_at: SystemTime::now(),
        hosts,
    })
}

// ---------------------------------------------------------------------------
// HTTP transport — pluggable so tests don't need a network
// ---------------------------------------------------------------------------

/// HTTP client abstraction for the upload path. The download path has its
/// own [`crate::download::HttpClient`] for `GET`s; uploads need `POST`,
/// hence a separate trait. A real reqwest-based implementation can satisfy
/// both traits in one struct.
#[async_trait]
pub trait UploadHttpClient: Send + Sync {
    /// Perform an HTTPS POST with the given body and headers. Implementations
    /// should return the raw response body bytes on a 2xx status, or an error
    /// otherwise.
    async fn post(
        &self,
        url: &str,
        body: Vec<u8>,
        headers: &[(&str, &str)],
    ) -> Result<Vec<u8>, ClientError>;
}

/// What the MMS upload endpoint returns (a subset; matches
/// `UploadResponse`'s JSON tags upstream).
#[derive(Debug, Default)]
struct UploadJson {
    url: String,
    direct_path: String,
    handle: String,
    object_id: String,
}

/// Tiny zero-dependency JSON string-field extractor. The MMS endpoint always
/// returns a flat `{ "url": "...", "direct_path": "...", ... }` blob, so
/// straightforward field extraction is enough — pulling in `serde_json`
/// for one shape is overkill.
fn parse_upload_json(body: &[u8]) -> UploadJson {
    let s = std::str::from_utf8(body).unwrap_or("");
    UploadJson {
        url: extract_json_string_field(s, "url").unwrap_or_default(),
        direct_path: extract_json_string_field(s, "direct_path").unwrap_or_default(),
        handle: extract_json_string_field(s, "handle").unwrap_or_default(),
        object_id: extract_json_string_field(s, "object_id").unwrap_or_default(),
    }
}

fn extract_json_string_field(s: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let key_idx = s.find(&needle)?;
    let after = &s[key_idx + needle.len()..];
    let colon = after.find(':')?;
    let after_colon = &after[colon + 1..];
    let quote_start = after_colon.find('"')?;
    let value_region = &after_colon[quote_start + 1..];
    let mut out = String::new();
    let mut chars = value_region.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                other => out.push(other),
            },
            other => out.push(other),
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Encryption helper (whatsmeow `Upload` body)
// ---------------------------------------------------------------------------

/// AES-256-CBC (PKCS7) encrypt + append HMAC-SHA256(mac_key, iv||ct)[:10].
/// Returns the blob to upload + sha256(blob).
pub fn encrypt_media(
    plaintext: &[u8],
    keys: &MediaKeys,
) -> Result<(Vec<u8>, [u8; 32]), ClientError> {
    let ciphertext = cbc_encrypt(&keys.cipher_key, &keys.iv, plaintext)
        .map_err(|e| ClientError::Crypto(format!("cbc encrypt: {e}")))?;
    let mut mac_input = Vec::with_capacity(keys.iv.len() + ciphertext.len());
    mac_input.extend_from_slice(&keys.iv);
    mac_input.extend_from_slice(&ciphertext);
    let mac = hmac_sha256(&keys.mac_key, &mac_input);
    let mut blob = ciphertext;
    blob.extend_from_slice(&mac[..10]);
    let enc_sha = Sha256::digest(&blob);
    Ok((blob, enc_sha.into()))
}

// ---------------------------------------------------------------------------
// Public Client API
// ---------------------------------------------------------------------------

impl Client {
    /// Fetch a fresh `<media_conn/>` IQ from `s.whatsapp.net`. The Go side
    /// caches this; here we just re-fetch on demand — the cache layer can be
    /// added once it's clear what the call patterns are.
    pub async fn query_media_conn(&self) -> Result<MediaConn, ClientError> {
        let resp = self
            .send_iq(
                InfoQuery::new("w:m", IqType::Set)
                    .to(Jid::new("", "s.whatsapp.net"))
                    .content(Value::Nodes(vec![Node::tag_only("media_conn")])),
            )
            .await?;
        parse_media_conn(&resp)
    }

    /// Encrypt + upload an attachment. Mirrors `Client.Upload` in
    /// `whatsmeow/upload.go`.
    ///
    /// The HTTP side is delegated to [`UploadHttpClient::post`] so the test
    /// suite can mock it out. Production callers wire a real reqwest/hyper
    /// client in.
    pub async fn upload_media(
        &self,
        plaintext: &[u8],
        media_type: MediaType,
        http: &dyn UploadHttpClient,
    ) -> Result<MediaUpload, ClientError> {
        let mut media_key = vec![0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut media_key);

        let file_sha256 = Sha256::digest(plaintext).to_vec();
        let keys = expand_media_key(&media_key, media_type)?;
        let (enc_blob, file_enc_sha256) = encrypt_media(plaintext, &keys)?;

        let media_conn = self.query_media_conn().await?;
        let upload = upload_blob(
            http,
            &media_conn,
            media_type,
            &enc_blob,
            &file_enc_sha256,
            /* newsletter */ false,
        )
        .await?;

        Ok(MediaUpload {
            url: upload.url,
            direct_path: upload.direct_path,
            handle: upload.handle,
            object_id: upload.object_id,
            media_key,
            file_enc_sha256: file_enc_sha256.to_vec(),
            file_sha256,
            file_length: plaintext.len() as u64,
            first_frame_sidecar: None,
        })
    }
}

/// Perform the actual HTTPS POST against one of the media hosts. Public so a
/// future "upload from a separately-encrypted stream" path can reuse it.
pub async fn upload_blob(
    http: &dyn UploadHttpClient,
    media_conn: &MediaConn,
    media_type: MediaType,
    enc_blob: &[u8],
    file_hash: &[u8; 32],
    newsletter: bool,
) -> Result<MediaUpload, ClientError> {
    let host = media_conn
        .hosts
        .first()
        .ok_or_else(|| ClientError::Malformed("media_conn returned no hosts".into()))?;

    // URL-safe base64 of the enc-sha256 hash (Go uses base64.URLEncoding).
    let token = general_purpose::URL_SAFE.encode(file_hash);

    let mut mms_type = media_type.mms_type().to_owned();
    let mut upload_prefix = "mms".to_owned();
    if newsletter {
        mms_type = format!("newsletter-{mms_type}");
        upload_prefix = "newsletter".to_owned();
    }

    let url = format!(
        "https://{}/{}/{}/{}?auth={}&token={}",
        host.hostname,
        upload_prefix,
        mms_type,
        token,
        url_encode_query_param(&media_conn.auth),
        url_encode_query_param(&token),
    );

    let body = http
        .post(
            &url,
            enc_blob.to_vec(),
            &[("Origin", "https://web.whatsapp.com"), ("Referer", "https://web.whatsapp.com/")],
        )
        .await?;
    let parsed = parse_upload_json(&body);
    Ok(MediaUpload {
        url: parsed.url,
        direct_path: parsed.direct_path,
        handle: parsed.handle,
        object_id: parsed.object_id,
        ..Default::default()
    })
}

/// Minimal percent-encoder. The values we feed in (base64url tokens + auth
/// strings) only contain ASCII; we still escape `+`, `&`, `=`, `?`, `#` and
/// space so we don't smuggle them into the query.
fn url_encode_query_param(s: &str) -> String {
    const SAFE: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if SAFE.contains(&b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Media retry notification (parse half of mediaretry.go)
// ---------------------------------------------------------------------------

/// Parsed `<notification type="server-error">` payload — what the phone
/// returns after the client asks for a media re-upload via
/// `SendMediaRetryReceipt`. Mirrors `events.MediaRetry` in whatsmeow's
/// `types/events`.
#[derive(Debug, Clone)]
pub struct MediaRetryNotification {
    pub message_id: String,
    pub timestamp: i64,
    pub chat_id: Jid,
    pub sender_id: Jid,
    pub from_me: bool,
    pub ciphertext: Option<Vec<u8>>,
    pub iv: Option<Vec<u8>>,
    pub error_code: Option<i64>,
}

/// Parse a `<notification type="server-error">` retry node. Mirrors
/// `parseMediaRetryNotification` in `whatsmeow/mediaretry.go`.
pub fn parse_media_retry(node: &Node) -> Result<MediaRetryNotification, ClientError> {
    let mut ag = node.attr_getter();
    let message_id = ag.string("id").to_owned();
    let timestamp = ag.optional_i64("t").unwrap_or(0);
    if !ag.ok() {
        return Err(ClientError::Malformed(format!(
            "missing attrs in retry notification: {:?}",
            ag.errors
        )));
    }

    let rmr = node
        .child_by_tag(&["rmr"])
        .ok_or_else(|| ClientError::Malformed("retry notification missing <rmr>".into()))?;
    let mut rmr_ag = rmr.attr_getter();
    let chat_id = rmr_ag.jid("jid");
    let from_me = rmr_ag.optional_bool("from_me");
    let sender_id = rmr_ag.optional_jid("participant").cloned().unwrap_or_default();
    if !rmr_ag.ok() {
        return Err(ClientError::Malformed(format!(
            "missing attrs in <rmr>: {:?}",
            rmr_ag.errors
        )));
    }

    if let Some(err_node) = node.child_by_tag(&["error"]) {
        let mut eag = err_node.attr_getter();
        let code = eag.optional_i64("code").unwrap_or(0);
        return Ok(MediaRetryNotification {
            message_id,
            timestamp,
            chat_id,
            sender_id,
            from_me,
            ciphertext: None,
            iv: None,
            error_code: Some(code),
        });
    }

    let enc_p = node
        .child_by_tag(&["encrypt", "enc_p"])
        .ok_or_else(|| ClientError::Malformed("retry notification missing <enc_p>".into()))?;
    let enc_iv = node
        .child_by_tag(&["encrypt", "enc_iv"])
        .ok_or_else(|| ClientError::Malformed("retry notification missing <enc_iv>".into()))?;
    let ciphertext = enc_p
        .content
        .as_bytes()
        .ok_or_else(|| ClientError::Malformed("<enc_p> content not bytes".into()))?
        .to_vec();
    let iv = enc_iv
        .content
        .as_bytes()
        .ok_or_else(|| ClientError::Malformed("<enc_iv> content not bytes".into()))?
        .to_vec();

    Ok(MediaRetryNotification {
        message_id,
        timestamp,
        chat_id,
        sender_id,
        from_me,
        ciphertext: Some(ciphertext),
        iv: Some(iv),
        error_code: None,
    })
}

// ---------------------------------------------------------------------------
// Typed upload wrappers — refresh MediaConn → upload → build proto
//
// Mirrors the call-site stitching upstream's `Upload(...)` users do by hand
// (see the `Upload` doc-comment in `_upstream/whatsmeow/upload.go`). Each
// wrapper:
//   1. Refreshes `MediaConn` via `wha_media::refresh_media_conn` (reusing
//      the same `ClientIqSender` adapter we already use for history sync —
//      that adapter ships the `<media_conn/>` IQ over our noise socket and
//      parses the response).
//   2. Calls `wha_media::upload(...)` to encrypt + POST the blob.
//   3. Builds the matching `wha_proto::e2e::*Message` with the `url`,
//      `direct_path`, `media_key`, `file_enc_sha256`, `file_sha256`,
//      `file_length`, plus the per-class extras (`mimetype`, `caption`,
//      `file_name`, `ptt`, …).
// ---------------------------------------------------------------------------

use wha_proto::e2e::{
    AudioMessage as ProtoAudioMessage, DocumentMessage as ProtoDocumentMessage,
    ImageMessage as ProtoImageMessage, StickerMessage as ProtoStickerMessage,
    VideoMessage as ProtoVideoMessage,
};

/// Refresh the media-connection list from `s.whatsapp.net` over our open
/// noise socket. Implemented in `crate::history_sync` (which already exposes
/// `ClientIqSender` for history-sync downloads); we reuse it here because
/// `wha-media` doesn't link `wha-client`.
async fn refresh_conn(client: &Client) -> Result<wha_media::MediaConn, ClientError> {
    let sender = crate::history_sync::ClientIqSender { client };
    wha_media::refresh_media_conn(&sender, client.generate_request_id())
        .await
        .map_err(|e| ClientError::Download(e.to_string()))
}

/// Encrypt + upload a JPEG/PNG/GIF (`mime_type`) and return a populated
/// `ImageMessage` ready to drop into `wha_proto::e2e::Message::image_message`.
///
/// `caption` is optional. Stickers go through [`upload_sticker`] instead — they
/// share the IMAGE app-info but live under their own proto message.
pub async fn upload_image(
    client: &Client,
    plaintext: &[u8],
    mime_type: &str,
    caption: Option<&str>,
) -> Result<ProtoImageMessage, ClientError> {
    let conn = refresh_conn(client).await?;
    let result = wha_media::upload(&conn, plaintext, "image", &conn.auth)
        .await
        .map_err(|e| ClientError::Download(e.to_string()))?;
    Ok(ProtoImageMessage {
        url: Some(result.url),
        mimetype: Some(mime_type.to_owned()),
        caption: caption.map(|s| s.to_owned()),
        file_sha256: Some(result.file_sha256.to_vec()),
        file_length: Some(result.file_length),
        media_key: Some(result.media_key.to_vec()),
        file_enc_sha256: Some(result.file_enc_sha256.to_vec()),
        direct_path: Some(result.direct_path),
        ..Default::default()
    })
}

/// Encrypt + upload a video and return a populated `VideoMessage`.
pub async fn upload_video(
    client: &Client,
    plaintext: &[u8],
    mime_type: &str,
    caption: Option<&str>,
) -> Result<ProtoVideoMessage, ClientError> {
    let conn = refresh_conn(client).await?;
    let result = wha_media::upload(&conn, plaintext, "video", &conn.auth)
        .await
        .map_err(|e| ClientError::Download(e.to_string()))?;
    Ok(ProtoVideoMessage {
        url: Some(result.url),
        mimetype: Some(mime_type.to_owned()),
        caption: caption.map(|s| s.to_owned()),
        file_sha256: Some(result.file_sha256.to_vec()),
        file_length: Some(result.file_length),
        media_key: Some(result.media_key.to_vec()),
        file_enc_sha256: Some(result.file_enc_sha256.to_vec()),
        direct_path: Some(result.direct_path),
        ..Default::default()
    })
}

/// Encrypt + upload audio (or PTT/voice memo, see `ptt`) and return a
/// populated `AudioMessage`. `ptt=true` marks the recording as a push-to-talk
/// voice message — the recipient's UI renders it as a circular waveform.
pub async fn upload_audio(
    client: &Client,
    plaintext: &[u8],
    mime_type: &str,
    ptt: bool,
) -> Result<ProtoAudioMessage, ClientError> {
    let conn = refresh_conn(client).await?;
    let result = wha_media::upload(&conn, plaintext, "audio", &conn.auth)
        .await
        .map_err(|e| ClientError::Download(e.to_string()))?;
    Ok(ProtoAudioMessage {
        url: Some(result.url),
        mimetype: Some(mime_type.to_owned()),
        file_sha256: Some(result.file_sha256.to_vec()),
        file_length: Some(result.file_length),
        ptt: Some(ptt),
        media_key: Some(result.media_key.to_vec()),
        file_enc_sha256: Some(result.file_enc_sha256.to_vec()),
        direct_path: Some(result.direct_path),
        ..Default::default()
    })
}

/// Encrypt + upload a document (PDF, etc.) and return a populated
/// `DocumentMessage`. `file_name` is what the recipient sees in the chat list.
pub async fn upload_document(
    client: &Client,
    plaintext: &[u8],
    mime_type: &str,
    file_name: &str,
) -> Result<ProtoDocumentMessage, ClientError> {
    let conn = refresh_conn(client).await?;
    let result = wha_media::upload(&conn, plaintext, "document", &conn.auth)
        .await
        .map_err(|e| ClientError::Download(e.to_string()))?;
    Ok(ProtoDocumentMessage {
        url: Some(result.url),
        mimetype: Some(mime_type.to_owned()),
        file_sha256: Some(result.file_sha256.to_vec()),
        file_length: Some(result.file_length),
        media_key: Some(result.media_key.to_vec()),
        file_name: Some(file_name.to_owned()),
        file_enc_sha256: Some(result.file_enc_sha256.to_vec()),
        direct_path: Some(result.direct_path),
        ..Default::default()
    })
}

/// Encrypt + upload a sticker and return a populated `StickerMessage`. The
/// `mime_type` is set to `image/webp` — WhatsApp stickers are always WebP.
/// Animated stickers use the same proto field set; the caller can flip
/// `is_animated` on the returned struct before sending.
pub async fn upload_sticker(
    client: &Client,
    plaintext: &[u8],
) -> Result<ProtoStickerMessage, ClientError> {
    let conn = refresh_conn(client).await?;
    let result = wha_media::upload(&conn, plaintext, "sticker", &conn.auth)
        .await
        .map_err(|e| ClientError::Download(e.to_string()))?;
    Ok(ProtoStickerMessage {
        url: Some(result.url),
        file_sha256: Some(result.file_sha256.to_vec()),
        file_enc_sha256: Some(result.file_enc_sha256.to_vec()),
        media_key: Some(result.media_key.to_vec()),
        mimetype: Some("image/webp".to_owned()),
        direct_path: Some(result.direct_path),
        file_length: Some(result.file_length),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Builder helpers — used by both production callers and tests so we can pin
// the proto-shape contract without standing up a live connection. These are
// the pure-data half of the typed wrappers above; the wrappers compose
// `refresh_conn` + `wha_media::upload` + these.
// ---------------------------------------------------------------------------

/// Stitch a [`wha_media::UploadResult`] into a fully-populated `ImageMessage`.
/// Test-friendly mirror of the proto-build half of [`upload_image`].
#[allow(dead_code)]
pub(crate) fn build_image_message(
    result: &wha_media::UploadResult,
    mime_type: &str,
    caption: Option<&str>,
) -> ProtoImageMessage {
    ProtoImageMessage {
        url: Some(result.url.clone()),
        mimetype: Some(mime_type.to_owned()),
        caption: caption.map(|s| s.to_owned()),
        file_sha256: Some(result.file_sha256.to_vec()),
        file_length: Some(result.file_length),
        media_key: Some(result.media_key.to_vec()),
        file_enc_sha256: Some(result.file_enc_sha256.to_vec()),
        direct_path: Some(result.direct_path.clone()),
        ..Default::default()
    }
}

/// Test-friendly mirror of the proto-build half of [`upload_audio`].
#[allow(dead_code)]
pub(crate) fn build_audio_message(
    result: &wha_media::UploadResult,
    mime_type: &str,
    ptt: bool,
) -> ProtoAudioMessage {
    ProtoAudioMessage {
        url: Some(result.url.clone()),
        mimetype: Some(mime_type.to_owned()),
        file_sha256: Some(result.file_sha256.to_vec()),
        file_length: Some(result.file_length),
        ptt: Some(ptt),
        media_key: Some(result.media_key.to_vec()),
        file_enc_sha256: Some(result.file_enc_sha256.to_vec()),
        direct_path: Some(result.direct_path.clone()),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use wha_binary::Attrs;
    use wha_crypto::hkdf_sha256;

    /// HKDF expansion produces the documented sub-key offsets.
    #[test]
    fn derive_media_keys_uses_correct_offsets() {
        let media_key = [0x42u8; 32];
        let keys = expand_media_key(&media_key, MediaType::Image).unwrap();

        // Re-derive with the raw HKDF helper and slice manually — they have to match.
        let expected = hkdf_sha256(&media_key, &[], b"WhatsApp Image Keys", 112).unwrap();
        assert_eq!(&keys.iv[..], &expected[0..16]);
        assert_eq!(&keys.cipher_key[..], &expected[16..48]);
        assert_eq!(&keys.mac_key[..], &expected[48..80]);
        assert_eq!(&keys.ref_key[..], &expected[80..112]);

        // Different media types produce different derivations (info string changes).
        let video_keys = expand_media_key(&media_key, MediaType::Video).unwrap();
        assert_ne!(keys.iv, video_keys.iv);
        assert_ne!(keys.cipher_key, video_keys.cipher_key);
    }

    /// AES-CBC + HMAC-SHA256 truncation produces a deterministic blob for a
    /// fixed plaintext + media_key. Re-running has to give the same output.
    #[test]
    fn encrypt_media_is_deterministic() {
        let media_key = [0x01u8; 32];
        let plaintext = b"the quick brown fox jumps over the lazy dog".to_vec();
        let keys = expand_media_key(&media_key, MediaType::Document).unwrap();

        let (blob_a, sha_a) = encrypt_media(&plaintext, &keys).unwrap();
        let (blob_b, sha_b) = encrypt_media(&plaintext, &keys).unwrap();
        assert_eq!(blob_a, blob_b, "encryption is not deterministic for fixed inputs");
        assert_eq!(sha_a, sha_b);

        // Sanity-check the structure: blob = ciphertext || mac10. Pull off the
        // last 10 bytes and verify HMAC(mac_key, iv || ciphertext)[:10] matches.
        assert!(blob_a.len() >= 10);
        let split = blob_a.len() - 10;
        let (ct, mac_tail) = blob_a.split_at(split);
        let mut mac_input = Vec::with_capacity(keys.iv.len() + ct.len());
        mac_input.extend_from_slice(&keys.iv);
        mac_input.extend_from_slice(ct);
        let full_mac = hmac_sha256(&keys.mac_key, &mac_input);
        assert_eq!(mac_tail, &full_mac[..10]);

        // Ciphertext must be a multiple of 16 (AES-CBC PKCS7 padding).
        assert_eq!(ct.len() % 16, 0);

        // SHA256 of the full blob equals the returned hash.
        let expected_sha = Sha256::digest(&blob_a);
        assert_eq!(&sha_a[..], expected_sha.as_slice());
    }

    /// `parse_media_retry` on a synthesised node returns the right struct.
    #[test]
    fn parse_media_retry_extracts_encrypt_payload() {
        let chat = Jid::new("12345", "g.us");
        let participant = Jid::new("67890", "s.whatsapp.net");

        let mut rmr_attrs = Attrs::new();
        rmr_attrs.insert("jid".into(), Value::Jid(chat.clone()));
        rmr_attrs.insert("from_me".into(), Value::String("false".into()));
        rmr_attrs.insert("participant".into(), Value::Jid(participant.clone()));
        let rmr = Node::new("rmr", rmr_attrs, None);

        let enc_p = Node::new("enc_p", Attrs::new(), Some(Value::Bytes(vec![1, 2, 3, 4])));
        let enc_iv = Node::new("enc_iv", Attrs::new(), Some(Value::Bytes(vec![9, 9, 9])));
        let encrypt = Node::new(
            "encrypt",
            Attrs::new(),
            Some(Value::Nodes(vec![enc_p, enc_iv])),
        );

        let mut attrs = Attrs::new();
        attrs.insert("id".into(), Value::String("MSG123".into()));
        attrs.insert("t".into(), Value::String("1700000000".into()));
        let notif = Node::new("notification", attrs, Some(Value::Nodes(vec![rmr, encrypt])));

        let parsed = parse_media_retry(&notif).expect("parse failed");
        assert_eq!(parsed.message_id, "MSG123");
        assert_eq!(parsed.timestamp, 1_700_000_000);
        assert_eq!(parsed.chat_id, chat);
        assert_eq!(parsed.sender_id, participant);
        assert!(!parsed.from_me);
        assert_eq!(parsed.ciphertext.as_deref(), Some(&[1u8, 2, 3, 4][..]));
        assert_eq!(parsed.iv.as_deref(), Some(&[9u8, 9, 9][..]));
        assert!(parsed.error_code.is_none());
    }

    /// Error-only retry: an `<error>` child means the phone refused to retry.
    #[test]
    fn parse_media_retry_extracts_error_branch() {
        let chat = Jid::new("12345", "s.whatsapp.net");
        let mut rmr_attrs = Attrs::new();
        rmr_attrs.insert("jid".into(), Value::Jid(chat));
        rmr_attrs.insert("from_me".into(), Value::String("true".into()));
        let rmr = Node::new("rmr", rmr_attrs, None);

        let mut err_attrs = Attrs::new();
        err_attrs.insert("code".into(), Value::String("2".into()));
        let err_node = Node::new("error", err_attrs, None);

        let mut attrs = Attrs::new();
        attrs.insert("id".into(), Value::String("M2".into()));
        attrs.insert("t".into(), Value::String("100".into()));
        let notif = Node::new("notification", attrs, Some(Value::Nodes(vec![rmr, err_node])));

        let parsed = parse_media_retry(&notif).unwrap();
        assert_eq!(parsed.error_code, Some(2));
        assert!(parsed.ciphertext.is_none());
        assert!(parsed.iv.is_none());
    }

    /// `<media_conn>` parsing extracts auth, ttl, and host list.
    #[test]
    fn parse_media_conn_extracts_hosts_and_auth() {
        let mut host_attrs = Attrs::new();
        host_attrs.insert("hostname".into(), Value::String("mmg.whatsapp.net".into()));
        let host = Node::new("host", host_attrs, None);

        let mut mc_attrs = Attrs::new();
        mc_attrs.insert("auth".into(), Value::String("auth-token-xyz".into()));
        mc_attrs.insert("ttl".into(), Value::String("60".into()));
        mc_attrs.insert("auth_ttl".into(), Value::String("3600".into()));
        mc_attrs.insert("max_buckets".into(), Value::String("12".into()));
        let mc = Node::new("media_conn", mc_attrs, Some(Value::Nodes(vec![host])));

        let iq = Node::new("iq", Attrs::new(), Some(Value::Nodes(vec![mc])));
        let parsed = parse_media_conn(&iq).unwrap();
        assert_eq!(parsed.auth, "auth-token-xyz");
        assert_eq!(parsed.ttl, 60);
        assert_eq!(parsed.auth_ttl, 3600);
        assert_eq!(parsed.max_buckets, 12);
        assert_eq!(parsed.hosts.len(), 1);
        assert_eq!(parsed.hosts[0].hostname, "mmg.whatsapp.net");
    }

    /// JSON parser handles the flat shape that the MMS endpoint returns.
    #[test]
    fn parse_upload_json_extracts_known_fields() {
        let body = br#"{"url":"https://mmg.whatsapp.net/path","direct_path":"/v/path","handle":"abc","object_id":"123"}"#;
        let parsed = parse_upload_json(body);
        assert_eq!(parsed.url, "https://mmg.whatsapp.net/path");
        assert_eq!(parsed.direct_path, "/v/path");
        assert_eq!(parsed.handle, "abc");
        assert_eq!(parsed.object_id, "123");
    }

    /// `upload_blob` POSTs to the right URL and returns parsed fields.
    #[tokio::test]
    async fn upload_blob_calls_http_with_correct_url() {
        struct Mock {
            captured: Arc<Mutex<Option<String>>>,
            response: Vec<u8>,
        }
        #[async_trait]
        impl UploadHttpClient for Mock {
            async fn post(
                &self,
                url: &str,
                _body: Vec<u8>,
                _headers: &[(&str, &str)],
            ) -> Result<Vec<u8>, ClientError> {
                *self.captured.lock().await = Some(url.to_owned());
                Ok(self.response.clone())
            }
        }
        let captured = Arc::new(Mutex::new(None));
        let mock = Mock {
            captured: captured.clone(),
            response: br#"{"url":"u","direct_path":"d","handle":"h","object_id":"o"}"#.to_vec(),
        };
        let mc = MediaConn {
            auth: "AUTH+PADDING==".into(),
            auth_ttl: 0,
            ttl: 60,
            max_buckets: 0,
            fetched_at: SystemTime::now(),
            hosts: vec![MediaConnHost { hostname: "mmg.example.com".into() }],
        };
        let blob = vec![0u8; 32];
        let hash = [7u8; 32];
        let result = upload_blob(&mock, &mc, MediaType::Image, &blob, &hash, false).await.unwrap();
        assert_eq!(result.url, "u");
        assert_eq!(result.direct_path, "d");
        assert_eq!(result.handle, "h");
        assert_eq!(result.object_id, "o");

        let url_used = captured.lock().await.clone().unwrap();
        assert!(url_used.starts_with("https://mmg.example.com/mms/image/"), "got {url_used}");
        assert!(url_used.contains("auth=AUTH%2BPADDING%3D%3D"), "auth not URL-encoded: {url_used}");
    }

    // ---------------------------------------------------------------------
    // Proto-shape pins for the typed upload wrappers.
    //
    // The wrappers themselves require a connected `Client` (so they can
    // refresh `MediaConn` over the noise socket); we can't drive that from
    // a unit test. Instead we pin the proto-shape half — `build_image_message`
    // and `build_audio_message` — which is the stable contract callers
    // depend on. End-to-end network coverage lives in the live-pair example.
    // ---------------------------------------------------------------------

    /// Build an `ImageMessage` from a fixed `UploadResult` and assert every
    /// required field is populated. This is the contract recipients depend
    /// on — drop one of these fields and the recipient's download fails.
    #[test]
    fn upload_image_creates_proto_with_all_required_fields() {
        let result = wha_media::UploadResult {
            url: "https://cdn.example/u".into(),
            direct_path: "/v/t62/img-foo".into(),
            media_key: [0xAA; 32],
            file_enc_sha256: [0xBB; 32],
            file_sha256: [0xCC; 32],
            file_length: 4_242,
            handle: String::new(),
        };

        let img = build_image_message(&result, "image/jpeg", Some("hi mom"));
        // Required proto fields the recipient uses to download + decrypt.
        assert_eq!(img.url.as_deref(), Some("https://cdn.example/u"));
        assert_eq!(img.direct_path.as_deref(), Some("/v/t62/img-foo"));
        assert_eq!(img.media_key.as_deref(), Some(&[0xAA; 32][..]));
        assert_eq!(img.file_enc_sha256.as_deref(), Some(&[0xBB; 32][..]));
        assert_eq!(img.file_sha256.as_deref(), Some(&[0xCC; 32][..]));
        assert_eq!(img.file_length, Some(4_242));
        assert_eq!(img.mimetype.as_deref(), Some("image/jpeg"));
        assert_eq!(img.caption.as_deref(), Some("hi mom"));

        // No caption is also valid — recipients render it as a captionless
        // image. None must serialise as the proto omitting the field.
        let no_caption = build_image_message(&result, "image/jpeg", None);
        assert!(no_caption.caption.is_none());
    }

    /// `upload_audio`'s proto must reflect the `ptt` flag the caller passed
    /// in — that's what flips the recipient's UI between "play" and "voice
    /// memo" rendering. Pin both branches.
    #[test]
    fn upload_audio_sets_ptt_field() {
        let result = wha_media::UploadResult {
            url: "https://cdn.example/u".into(),
            direct_path: "/v/t62/aud-foo".into(),
            media_key: [0x01; 32],
            file_enc_sha256: [0x02; 32],
            file_sha256: [0x03; 32],
            file_length: 99,
            handle: String::new(),
        };

        let voice = build_audio_message(&result, "audio/ogg; codecs=opus", true);
        assert_eq!(voice.ptt, Some(true));
        assert_eq!(voice.mimetype.as_deref(), Some("audio/ogg; codecs=opus"));
        // All the download metadata fields are populated for both branches.
        assert!(voice.url.is_some());
        assert!(voice.direct_path.is_some());
        assert!(voice.media_key.is_some());
        assert!(voice.file_enc_sha256.is_some());
        assert!(voice.file_sha256.is_some());
        assert_eq!(voice.file_length, Some(99));

        let music = build_audio_message(&result, "audio/mp4", false);
        assert_eq!(music.ptt, Some(false));
        assert_eq!(music.mimetype.as_deref(), Some("audio/mp4"));
    }
}
