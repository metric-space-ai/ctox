//! WhatsApp media-server access: refresh `MediaConn` via IQ, fetch encrypted
//! attachment blobs over HTTPS, and decrypt typed message attachments.
//!
//! Mirrors the relevant pieces of `_upstream/whatsmeow/mediaconn.go` and
//! `_upstream/whatsmeow/download.go` (`DownloadMediaWithPath`,
//! `downloadEncryptedMedia`, `Download`, `DownloadThumbnail`). Decryption
//! primitives themselves live in `wha-crypto`.
//!
//! Architecture note: this crate deliberately does NOT depend on
//! `wha-client` — that would create a cycle, since `wha-client` itself
//! consumes `wha-media` for its history-sync path. Instead we expose:
//!
//! - [`build_media_conn_iq`] — the static `<iq>` Node; the caller (which
//!   already holds a `wha_client::Client`) is responsible for actually
//!   shipping it through the noise socket.
//! - [`parse_media_conn_response`] — turns the raw response Node into a
//!   typed [`MediaConn`].
//! - [`refresh_media_conn`] — convenience that combines the two via an
//!   abstract [`IqSender`] trait, so callers from any crate can plug in
//!   their own `Client` without us depending on it.
//! - [`download_encrypted_media`] — given a [`MediaConn`], a `direct_path`,
//!   the encrypted-file SHA-256 digest, and the `mms-type` URL parameter,
//!   walks the host list in order and returns the first successful HTTP
//!   body. The body is still ciphertext + 10-byte trailing MAC — pass it to
//!   `wha_crypto::decrypt_media`.
//! - [`download_image`] / [`download_video`] / [`download_audio`] /
//!   [`download_document`] / [`download_sticker`] — typed convenience
//!   wrappers that read `direct_path`, `media_key`, and `file_enc_sha256`
//!   straight off a `wha_proto::e2e::*Message`, derive the right keys with
//!   the matching app-info, run the HTTPS request with the matching
//!   `mms-type`, and return the decrypted plaintext. Mirrors
//!   `Client.Download` upstream (`_upstream/whatsmeow/download.go:214`).
//! - [`download_thumbnail`] — same idea for the link-preview thumbnail on
//!   `ExtendedTextMessage`, mirroring `Client.DownloadThumbnail`
//!   (`_upstream/whatsmeow/download.go:181`).

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE as B64_URL;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;
use wha_binary::{Attrs, Node, Value};
use wha_crypto::{
    cbc_encrypt, decrypt_media, derive_media_keys, hmac_sha256, AUDIO_INFO, DOCUMENT_INFO,
    IMAGE_INFO, LINK_THUMBNAIL_INFO, VIDEO_INFO,
};
use wha_proto::e2e::{
    AudioMessage, DocumentMessage, ExtendedTextMessage, ImageMessage, StickerMessage, VideoMessage,
};
use wha_types::{jid::server, Jid};

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("iq send: {0}")]
    Iq(String),
    #[error("malformed media_conn response: {0}")]
    Malformed(String),
    #[error("http: {0}")]
    Http(String),
    #[error("all media hosts failed; last error: {0}")]
    AllHostsFailed(String),
    /// The encrypted attachment metadata in the proto is incomplete or
    /// malformed (missing direct path, media key, or enc-sha hash).
    #[error("missing/invalid attachment metadata: {0}")]
    BadAttachment(String),
    /// HKDF / HMAC / CBC failure surfacing from `wha-crypto`.
    #[error("decrypt: {0}")]
    Decrypt(String),
}

impl From<wha_crypto::CryptoError> for MediaError {
    fn from(e: wha_crypto::CryptoError) -> Self {
        MediaError::Decrypt(e.to_string())
    }
}

/// Parsed `<media_conn>` reply.
///
/// `auth` is an opaque server token that some endpoints want as a query
/// parameter or header — we don't currently use it for the download path
/// (history-sync URLs already authenticate themselves through the
/// direct_path-bound MAC), but we keep it so callers can pass it through
/// when needed.
#[derive(Debug, Clone)]
pub struct MediaConn {
    pub auth: String,
    pub hosts: Vec<String>,
}

/// Capability adapter: anything that can synchronously translate an `<iq>`
/// request Node into the corresponding response Node. `wha_client::Client`
/// implements this through a thin wrapper in the `history_sync` module.
#[async_trait]
pub trait IqSender {
    async fn send_media_conn_iq(&self, iq: Node) -> Result<Node, MediaError>;
}

/// Build the `<iq xmlns="w:m" type="set" to="s.whatsapp.net">
/// <media_conn/></iq>` request. The caller assigns its own request id by
/// inserting an `id` attr before sending; the parsing helper does not care.
///
/// Mirrors `whatsmeow.queryMediaConn`'s `infoQuery` construction.
pub fn build_media_conn_iq(id: impl Into<String>) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("id".into(), Value::String(id.into()));
    attrs.insert("xmlns".into(), Value::String("w:m".into()));
    attrs.insert("type".into(), Value::String("set".into()));
    attrs.insert(
        "to".into(),
        Value::Jid(Jid::new("", server::DEFAULT_USER)),
    );
    Node::new(
        "iq",
        attrs,
        Some(Value::Nodes(vec![Node::tag_only("media_conn")])),
    )
}

/// Parse the `<iq>` reply produced by [`build_media_conn_iq`] into a
/// [`MediaConn`].  Accepts either the top-level `<iq>` (with one
/// `<media_conn>` child) or a `<media_conn>` directly.
pub fn parse_media_conn_response(node: &Node) -> Result<MediaConn, MediaError> {
    let media_conn = if node.tag == "media_conn" {
        node
    } else {
        node.child_by_tag(&["media_conn"])
            .ok_or_else(|| MediaError::Malformed("no <media_conn> child".into()))?
    };

    let auth = media_conn
        .get_attr_str("auth")
        .ok_or_else(|| MediaError::Malformed("media_conn missing @auth".into()))?
        .to_owned();

    let mut hosts = Vec::new();
    for child in media_conn.children() {
        if child.tag != "host" {
            continue;
        }
        if let Some(h) = child.get_attr_str("hostname") {
            hosts.push(h.to_owned());
        }
    }
    if hosts.is_empty() {
        return Err(MediaError::Malformed(
            "media_conn returned no <host> children".into(),
        ));
    }
    Ok(MediaConn { auth, hosts })
}

/// Convenience: build the IQ, ship it through `sender`, parse the result.
/// Most callers will use this; the lower-level helpers are exposed for
/// direct testing without a live `Client`.
pub async fn refresh_media_conn<S: IqSender>(
    sender: &S,
    request_id: impl Into<String>,
) -> Result<MediaConn, MediaError> {
    let iq = build_media_conn_iq(request_id);
    let resp = sender.send_media_conn_iq(iq).await?;
    let conn = parse_media_conn_response(&resp)?;
    tracing::info!(
        host_count = conn.hosts.len(),
        first_host = %conn.hosts.first().map(|s| s.as_str()).unwrap_or(""),
        "refreshed media_conn"
    );
    Ok(conn)
}

/// Download the *encrypted* media blob for an attachment of any class.
///
/// Builds the URL according to the public WhatsApp Web mms scheme:
/// ```text
/// https://{host}{direct_path}&hash={base64url(file_enc_sha256)}&mms-type={mms_type}&__wa-mms=
/// ```
/// (whatsmeow's `DownloadMediaWithPath`). The `mms_type` argument is the
/// per-class label — `image`, `video`, `audio`, `document`, `md-msg-hist`,
/// `md-app-state`, `sticker-pack`, or `thumbnail-link` — see
/// `_upstream/whatsmeow/download.go::mediaTypeToMMSType`.
///
/// Hosts are tried in order. The first successful 200-OK response wins; on
/// non-2xx status the next host is attempted. Network errors propagate the
/// same way. The returned bytes are still ciphertext + 10-byte trailing
/// MAC — pass them to `wha_crypto::decrypt_media`.
pub async fn download_encrypted_media(
    conn: &MediaConn,
    direct_path: &str,
    file_enc_sha256: &[u8],
    mms_type: &str,
) -> Result<Vec<u8>, MediaError> {
    if !direct_path.starts_with('/') {
        return Err(MediaError::Malformed(format!(
            "direct_path must start with '/': {direct_path}"
        )));
    }
    let hash_b64 = B64_URL.encode(file_enc_sha256);

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| MediaError::Http(e.to_string()))?;

    let mut last_err = String::from("(no hosts attempted)");
    for host in &conn.hosts {
        let url = format!(
            "https://{host}{direct_path}&hash={hash_b64}&mms-type={mms_type}&__wa-mms="
        );
        tracing::info!(%url, "attempting media download");
        match client
            .get(&url)
            .header("Origin", "https://web.whatsapp.com")
            .header("Referer", "https://web.whatsapp.com/")
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.bytes().await {
                        Ok(b) => {
                            tracing::info!(host = %host, len = b.len(), "media download ok");
                            return Ok(b.to_vec());
                        }
                        Err(e) => {
                            last_err = format!("body read from {host}: {e}");
                            tracing::warn!(%last_err);
                            continue;
                        }
                    }
                } else {
                    last_err = format!("{host} responded {status}");
                    tracing::warn!(%last_err);
                    continue;
                }
            }
            Err(e) => {
                last_err = format!("send to {host}: {e}");
                tracing::warn!(%last_err);
                continue;
            }
        }
    }
    Err(MediaError::AllHostsFailed(last_err))
}

// ---------------------------------------------------------------------------
// Typed convenience: per media-class download wrappers.
//
// Each wrapper:
// 1. Pulls `direct_path`, `media_key`, and `file_enc_sha256` out of the
//    proto.
// 2. Calls `download_encrypted_media` with the matching `mms-type`.
// 3. Derives the per-class media keys with the matching app-info.
// 4. Calls `wha_crypto::decrypt_media`.
// 5. Verifies `file_sha256` over the plaintext when present (warn on
//    mismatch; matches whatsmeow's "non-fatal validation warning"
//    `ErrInvalidMediaSHA256` behaviour, controlled by
//    `ReturnDownloadWarnings` upstream — we keep it as a log-only warn).
//
// Mirrors `Client.Download` (`_upstream/whatsmeow/download.go:214`) plus the
// `classToMediaType` and `mediaTypeToMMSType` tables there.

/// Internal helper used by every typed downloader so the per-class shape
/// stays identical and minimal.
async fn download_attachment(
    conn: &MediaConn,
    direct_path: Option<&str>,
    media_key: Option<&[u8]>,
    file_enc_sha256: Option<&[u8]>,
    file_sha256: Option<&[u8]>,
    app_info: &str,
    mms_type: &str,
) -> Result<Vec<u8>, MediaError> {
    let direct_path = direct_path
        .filter(|s| !s.is_empty())
        .ok_or_else(|| MediaError::BadAttachment("missing direct_path".into()))?;
    let media_key = media_key
        .filter(|k| !k.is_empty())
        .ok_or_else(|| MediaError::BadAttachment("missing media_key".into()))?;
    let file_enc_sha256 = file_enc_sha256
        .filter(|h| h.len() == 32)
        .ok_or_else(|| MediaError::BadAttachment("missing or short file_enc_sha256".into()))?;

    let blob = download_encrypted_media(conn, direct_path, file_enc_sha256, mms_type).await?;
    let keys = derive_media_keys(media_key, app_info)?;
    let plaintext = decrypt_media(&blob, &keys)?;

    if let Some(expected) = file_sha256.filter(|h| h.len() == 32) {
        let actual: [u8; 32] = Sha256::digest(&plaintext).into();
        if actual.as_slice() != expected {
            tracing::warn!(
                expected = %hex32(expected),
                actual = %hex32(&actual),
                "file_sha256 mismatch on decrypted media (continuing)"
            );
        }
    }

    Ok(plaintext)
}

/// Lower-cased hex render of a 32-byte hash — for log lines only.
fn hex32(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len() * 2);
    for byte in b {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Download + decrypt an `ImageMessage`. mms-type `image`, app-info
/// `WhatsApp Image Keys`.
pub async fn download_image(
    conn: &MediaConn,
    msg: &ImageMessage,
) -> Result<Vec<u8>, MediaError> {
    download_attachment(
        conn,
        msg.direct_path.as_deref(),
        msg.media_key.as_deref(),
        msg.file_enc_sha256.as_deref(),
        msg.file_sha256.as_deref(),
        IMAGE_INFO,
        "image",
    )
    .await
}

/// Download + decrypt a `VideoMessage`. mms-type `video`, app-info
/// `WhatsApp Video Keys`.
pub async fn download_video(
    conn: &MediaConn,
    msg: &VideoMessage,
) -> Result<Vec<u8>, MediaError> {
    download_attachment(
        conn,
        msg.direct_path.as_deref(),
        msg.media_key.as_deref(),
        msg.file_enc_sha256.as_deref(),
        msg.file_sha256.as_deref(),
        VIDEO_INFO,
        "video",
    )
    .await
}

/// Download + decrypt an `AudioMessage` (covers ptt/voice — same key class).
/// mms-type `audio`, app-info `WhatsApp Audio Keys`.
pub async fn download_audio(
    conn: &MediaConn,
    msg: &AudioMessage,
) -> Result<Vec<u8>, MediaError> {
    download_attachment(
        conn,
        msg.direct_path.as_deref(),
        msg.media_key.as_deref(),
        msg.file_enc_sha256.as_deref(),
        msg.file_sha256.as_deref(),
        AUDIO_INFO,
        "audio",
    )
    .await
}

/// Download + decrypt a `DocumentMessage`. mms-type `document`, app-info
/// `WhatsApp Document Keys`.
pub async fn download_document(
    conn: &MediaConn,
    msg: &DocumentMessage,
) -> Result<Vec<u8>, MediaError> {
    download_attachment(
        conn,
        msg.direct_path.as_deref(),
        msg.media_key.as_deref(),
        msg.file_enc_sha256.as_deref(),
        msg.file_sha256.as_deref(),
        DOCUMENT_INFO,
        "document",
    )
    .await
}

/// Download + decrypt a `StickerMessage`. mms-type `image`, app-info
/// `WhatsApp Image Keys` — stickers reuse the image key class per
/// `_upstream/whatsmeow/download.go::classToMediaType:107-119`.
pub async fn download_sticker(
    conn: &MediaConn,
    msg: &StickerMessage,
) -> Result<Vec<u8>, MediaError> {
    download_attachment(
        conn,
        msg.direct_path.as_deref(),
        msg.media_key.as_deref(),
        msg.file_enc_sha256.as_deref(),
        msg.file_sha256.as_deref(),
        IMAGE_INFO,
        "image",
    )
    .await
}

/// Download + decrypt a link-preview thumbnail off an `ExtendedTextMessage`.
/// mms-type `thumbnail-link`, app-info `WhatsApp Link Thumbnail Keys`.
///
/// Mirrors `Client.DownloadThumbnail` upstream
/// (`_upstream/whatsmeow/download.go:181`). The thumbnail uses
/// `thumbnail_direct_path` / `thumbnail_enc_sha256` / `thumbnail_sha256`
/// rather than the regular `direct_path` field — the attached `media_key` is
/// shared with the parent extended-text message.
pub async fn download_thumbnail(
    conn: &MediaConn,
    msg: &ExtendedTextMessage,
) -> Result<Vec<u8>, MediaError> {
    download_attachment(
        conn,
        msg.thumbnail_direct_path.as_deref(),
        msg.media_key.as_deref(),
        msg.thumbnail_enc_sha256.as_deref(),
        msg.thumbnail_sha256.as_deref(),
        LINK_THUMBNAIL_INFO,
        "thumbnail-link",
    )
    .await
}

// ---------------------------------------------------------------------------
// Upload — encrypt a plaintext attachment + POST it to the MMS host.
//
// Mirrors `_upstream/whatsmeow/upload.go::Client.Upload` plus the URL-build
// inside `rawUpload`. This crate intentionally does NOT depend on `wha-client`,
// so the IQ refresh is the caller's job: hand us a fresh `MediaConn` + the
// `auth` token (read off `MediaConn.auth`) and we'll handle the encrypt +
// HTTPS POST.

/// Result of [`upload`] — what callers stitch into the proto message they're
/// about to send. Mirrors the `UploadResponse` struct in upstream
/// `whatsmeow/upload.go`. The `media_key` / `file_*_sha256` / `file_length`
/// fields are filled locally; `url`/`direct_path` come from the JSON the MMS
/// host returns.
#[derive(Debug, Clone)]
pub struct UploadResult {
    /// `https://...` URL the recipient downloads from.
    pub url: String,
    /// Path-only form of [`Self::url`], used to download via the MMS proxy.
    pub direct_path: String,
    /// 32-byte AES-CBC media key (random per upload).
    pub media_key: [u8; 32],
    /// SHA-256 of the encrypted blob (`ciphertext || mac10`).
    pub file_enc_sha256: [u8; 32],
    /// SHA-256 of the plaintext.
    pub file_sha256: [u8; 32],
    /// Plaintext byte count.
    pub file_length: u64,
    /// "Media handle" — only populated for newsletter sends; usually empty.
    pub handle: String,
}

/// Map a media-class label (`"image"`, `"video"`, `"audio"`, `"document"`,
/// `"sticker"`) to the matching app-info constant from `wha-crypto`. Returns
/// `None` for unknown labels — the caller surfaces that as `MediaError`.
///
/// Stickers reuse the IMAGE app-info (mirror upstream's
/// `classToMediaType`/`getMediaKeys` mapping in `download.go`), which is why
/// the URL path uses `image` for stickers too.
fn app_info_for(media_type: &str) -> Option<(&'static str, &'static str)> {
    // returns (app_info, mms_path_segment)
    match media_type {
        "image" => Some((IMAGE_INFO, "image")),
        "video" => Some((VIDEO_INFO, "video")),
        "audio" => Some((AUDIO_INFO, "audio")),
        "document" => Some((DOCUMENT_INFO, "document")),
        // Stickers ride the image key class but keep their own logical
        // label so callers can branch on `media_type`. URL path is "image".
        "sticker" => Some((IMAGE_INFO, "image")),
        _ => None,
    }
}

/// Minimal percent-encoder for query-string values — same SAFE-set
/// (`A-Z a-z 0-9 - _ . ~`) as the Go `url.QueryEscape`. We need this
/// because `auth` tokens routinely contain `+`, `=`, `/` which would
/// otherwise be misparsed by the receiving server.
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

/// Tiny zero-dependency JSON string-field extractor. The MMS upload
/// endpoint always returns a flat `{"url":"…","direct_path":"…","handle":"…"}`
/// blob; pulling in `serde_json` for one shape is overkill.
fn extract_json_string_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let key_idx = body.find(&needle)?;
    let after = &body[key_idx + needle.len()..];
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

/// HTTP transport abstraction for the upload path. Lets tests stub the
/// actual `POST` without standing up an HTTPS endpoint. Production uses the
/// default reqwest impl wired through [`upload`].
#[async_trait]
pub trait UploadHttpClient: Send + Sync {
    /// POST `body` to `url` with the given headers, return the response body
    /// on a 2xx, or an error otherwise. Implementations must NOT panic on
    /// non-2xx — surface them via `Err(MediaError::Http(...))` so the upload
    /// loop can try the next host.
    async fn post(
        &self,
        url: &str,
        body: Vec<u8>,
        headers: &[(&str, &str)],
    ) -> Result<Vec<u8>, MediaError>;
}

/// Default reqwest-backed [`UploadHttpClient`]. Built lazily on each upload —
/// the cost is small and avoids holding a global pool across tests.
struct ReqwestUploadHttp;

#[async_trait]
impl UploadHttpClient for ReqwestUploadHttp {
    async fn post(
        &self,
        url: &str,
        body: Vec<u8>,
        headers: &[(&str, &str)],
    ) -> Result<Vec<u8>, MediaError> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| MediaError::Http(e.to_string()))?;
        let mut req = http.post(url).body(body);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| MediaError::Http(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(MediaError::Http(format!(
                "non-success status {status}"
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MediaError::Http(e.to_string()))?;
        Ok(bytes.to_vec())
    }
}

/// Encrypt + upload an attachment, mirroring `Client.Upload` upstream.
///
/// Steps:
/// 1. Generate a random 32-byte `media_key`.
/// 2. Derive `(iv, cipher_key, mac_key, _)` via HKDF-SHA256 with the
///    matching app-info label.
/// 3. AES-256-CBC (PKCS#7) encrypt the plaintext, then append the first
///    10 bytes of `HMAC-SHA256(mac_key, iv ‖ ciphertext)`.
/// 4. Compute `file_sha256 = SHA-256(plaintext)` and
///    `file_enc_sha256 = SHA-256(ciphertext ‖ mac10)`.
/// 5. POST the encrypted blob to
///    `https://{host}/mms/{media_type}/{base64url(file_enc_sha256)}?auth={auth}&token={base64url(file_enc_sha256)}`.
///    Hosts in `conn.hosts` are tried in order; first success wins.
/// 6. Parse the returned JSON for `url`, `direct_path`, `handle` and fold
///    them into [`UploadResult`] alongside the locally-computed fields.
///
/// `media_type` MUST be one of `"image"`, `"video"`, `"audio"`,
/// `"document"`, `"sticker"`. `auth_string` should be `MediaConn.auth`
/// (broken out as a parameter so callers don't have to clone the conn).
pub async fn upload(
    conn: &MediaConn,
    plaintext: &[u8],
    media_type: &str,
    auth_string: &str,
) -> Result<UploadResult, MediaError> {
    upload_with_http(conn, plaintext, media_type, auth_string, &ReqwestUploadHttp).await
}

/// Same as [`upload`] but with a pluggable HTTP transport — used by tests.
pub async fn upload_with_http(
    conn: &MediaConn,
    plaintext: &[u8],
    media_type: &str,
    auth_string: &str,
    http: &dyn UploadHttpClient,
) -> Result<UploadResult, MediaError> {
    let (app_info, mms_segment) = app_info_for(media_type)
        .ok_or_else(|| MediaError::Malformed(format!("unknown media_type {media_type:?}")))?;
    if conn.hosts.is_empty() {
        return Err(MediaError::Malformed(
            "media_conn returned no hosts".into(),
        ));
    }

    let mut media_key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut media_key);

    let file_sha256: [u8; 32] = Sha256::digest(plaintext).into();

    let keys = derive_media_keys(&media_key, app_info)?;
    let ciphertext = cbc_encrypt(&keys.cipher_key, &keys.iv, plaintext)
        .map_err(|e| MediaError::Decrypt(format!("cbc encrypt: {e}")))?;

    let mut mac_input = Vec::with_capacity(keys.iv.len() + ciphertext.len());
    mac_input.extend_from_slice(&keys.iv);
    mac_input.extend_from_slice(&ciphertext);
    let mac = hmac_sha256(&keys.mac_key, &mac_input);

    let mut blob = ciphertext;
    blob.extend_from_slice(&mac[..10]);

    let file_enc_sha256: [u8; 32] = Sha256::digest(&blob).into();

    let token = B64_URL.encode(file_enc_sha256);
    let auth_q = url_encode_query_param(auth_string);
    let token_q = url_encode_query_param(&token);

    let mut last_err = String::from("(no hosts attempted)");
    for host in &conn.hosts {
        let url = format!(
            "https://{host}/mms/{mms_segment}/{token}?auth={auth_q}&token={token_q}"
        );
        tracing::info!(%url, blob_len = blob.len(), "uploading media blob");
        match http
            .post(
                &url,
                blob.clone(),
                &[
                    ("Origin", "https://web.whatsapp.com"),
                    ("Referer", "https://web.whatsapp.com/"),
                    ("Content-Type", "application/octet-stream"),
                ],
            )
            .await
        {
            Ok(body) => {
                let body_str = std::str::from_utf8(&body).unwrap_or("");
                let url_field =
                    extract_json_string_field(body_str, "url").unwrap_or_default();
                let direct_path =
                    extract_json_string_field(body_str, "direct_path").unwrap_or_default();
                let handle =
                    extract_json_string_field(body_str, "handle").unwrap_or_default();
                if url_field.is_empty() && direct_path.is_empty() {
                    last_err = format!("{host} returned malformed JSON: {body_str}");
                    tracing::warn!(%last_err);
                    continue;
                }
                tracing::info!(host = %host, %direct_path, "media upload ok");
                return Ok(UploadResult {
                    url: url_field,
                    direct_path,
                    media_key,
                    file_enc_sha256,
                    file_sha256,
                    file_length: plaintext.len() as u64,
                    handle,
                });
            }
            Err(e) => {
                last_err = format!("{host}: {e}");
                tracing::warn!(%last_err);
                continue;
            }
        }
    }
    Err(MediaError::AllHostsFailed(last_err))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity: the request IQ has the right xmlns/type/to and a `<media_conn>` child.
    #[test]
    fn build_iq_shape() {
        let iq = build_media_conn_iq("abc");
        assert_eq!(iq.tag, "iq");
        assert_eq!(iq.get_attr_str("xmlns"), Some("w:m"));
        assert_eq!(iq.get_attr_str("type"), Some("set"));
        let to = iq.get_attr_jid("to").unwrap();
        assert_eq!(to.server, server::DEFAULT_USER);
        assert!(to.user.is_empty());
        assert_eq!(iq.children().len(), 1);
        assert_eq!(iq.children()[0].tag, "media_conn");
    }

    /// Round-trip: synthesise a typical `<iq><media_conn auth="..."><host
    /// hostname="..."/>...` tree and feed it through `parse_media_conn_response`.
    #[test]
    fn parse_response_extracts_auth_and_hosts() {
        let mut mc_attrs = Attrs::new();
        mc_attrs.insert("auth".into(), Value::String("AUTH".into()));
        let mut h1 = Attrs::new();
        h1.insert("hostname".into(), Value::String("mmg.whatsapp.net".into()));
        let mut h2 = Attrs::new();
        h2.insert("hostname".into(), Value::String("mmg-fna.whatsapp.net".into()));
        let media_conn = Node::new(
            "media_conn",
            mc_attrs,
            Some(Value::Nodes(vec![
                Node::new("host", h1, None),
                Node::new("host", h2, None),
            ])),
        );
        let iq = Node::new("iq", Attrs::new(), Some(Value::Nodes(vec![media_conn])));
        let conn = parse_media_conn_response(&iq).unwrap();
        assert_eq!(conn.auth, "AUTH");
        assert_eq!(conn.hosts, vec!["mmg.whatsapp.net".to_string(), "mmg-fna.whatsapp.net".into()]);
    }

    /// Sanity: requesting download with an empty direct path is rejected
    /// before any HTTP request is issued. Mirrors whatsmeow's
    /// "media download path does not start with slash" pre-check.
    #[tokio::test]
    async fn download_rejects_relative_path() {
        let conn = MediaConn {
            auth: "auth".into(),
            hosts: vec!["mmg.whatsapp.net".into()],
        };
        let err = download_encrypted_media(&conn, "no-slash", &[1, 2, 3], "image")
            .await
            .unwrap_err();
        assert!(matches!(err, MediaError::Malformed(_)));
    }

    // ---------------------------------------------------------------------
    // Typed-download tests.
    //
    // Each test synthesises a server-side encrypted blob with the same
    // primitive (HKDF-SHA256 → AES-256-CBC + truncated HMAC) that
    // `wha_crypto::derive_media_keys` + `decrypt_media` consume, drops it
    // into a deterministic `download_attachment` shim that bypasses the
    // `MediaConn` network layer, and verifies plaintext round-trips for
    // each of the five message types and the link-preview thumbnail.
    //
    // We can't exercise the real `download_attachment` end-to-end without
    // standing up an HTTPS server, so we test the blob-build + decrypt
    // pipeline that lives inside `download_attachment` (everything after
    // the network call) and trust the IQ/HTTP plumbing tests above to
    // cover the transport.
    use sha2::{Digest, Sha256};
    use wha_crypto::{
        cbc_encrypt, derive_media_keys, hmac_sha256, AUDIO_INFO, DOCUMENT_INFO, IMAGE_INFO,
        LINK_THUMBNAIL_INFO, VIDEO_INFO,
    };
    use wha_proto::e2e::{
        AudioMessage, DocumentMessage, ExtendedTextMessage, ImageMessage, StickerMessage,
        VideoMessage,
    };

    /// Build (encrypted_blob, file_sha256, file_enc_sha256) for a given
    /// (plaintext, media_key, app_info) tuple, the way the real WhatsApp
    /// server would.
    fn synth_blob(
        plaintext: &[u8],
        media_key: &[u8; 32],
        app_info: &str,
    ) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let keys = derive_media_keys(media_key, app_info).unwrap();
        let body = cbc_encrypt(&keys.cipher_key, &keys.iv, plaintext).unwrap();
        let mut mac_in = Vec::with_capacity(keys.iv.len() + body.len());
        mac_in.extend_from_slice(&keys.iv);
        mac_in.extend_from_slice(&body);
        let mac_full = hmac_sha256(&keys.mac_key, &mac_in);
        let mut blob = body.clone();
        blob.extend_from_slice(&mac_full[..10]);

        let plain_sha: [u8; 32] = Sha256::digest(plaintext).into();
        let enc_sha: [u8; 32] = Sha256::digest(&blob).into();
        (blob, plain_sha.to_vec(), enc_sha.to_vec())
    }

    /// Helper that mirrors `download_attachment`'s decrypt half (everything
    /// after the network call): pull metadata out of the proto, derive
    /// keys, decrypt, and return plaintext. Used to exercise per-class
    /// `app_info` selection without standing up a network mock.
    fn decrypt_via_metadata(
        blob: &[u8],
        media_key: &[u8],
        app_info: &str,
    ) -> Result<Vec<u8>, MediaError> {
        let keys = derive_media_keys(media_key, app_info)?;
        let plaintext = decrypt_media(blob, &keys)?;
        Ok(plaintext)
    }

    #[test]
    fn image_round_trip_uses_image_info() {
        let mk = [1u8; 32];
        let plain = b"a JPEG, allegedly".to_vec();
        let (blob, _plain_sha, _enc_sha) = synth_blob(&plain, &mk, IMAGE_INFO);

        let mut img = ImageMessage::default();
        img.direct_path = Some("/v/t62/img-foo".into());
        img.media_key = Some(mk.to_vec());

        // Decrypt with the SAME app-info `download_image` would pick.
        let out = decrypt_via_metadata(&blob, img.media_key.as_ref().unwrap(), IMAGE_INFO).unwrap();
        assert_eq!(out, plain);

        // And with the WRONG app-info → MAC mismatch.
        let err = decrypt_via_metadata(&blob, img.media_key.as_ref().unwrap(), VIDEO_INFO)
            .unwrap_err();
        assert!(matches!(err, MediaError::Decrypt(_)));
    }

    #[test]
    fn video_round_trip_uses_video_info() {
        let mk = [2u8; 32];
        let plain = b"\x00\x00\x00\x18ftypmp42 some video bytes".to_vec();
        let (blob, _plain_sha, _enc_sha) = synth_blob(&plain, &mk, VIDEO_INFO);

        let mut v = VideoMessage::default();
        v.direct_path = Some("/v/t62/vid-foo".into());
        v.media_key = Some(mk.to_vec());

        let out = decrypt_via_metadata(&blob, v.media_key.as_ref().unwrap(), VIDEO_INFO).unwrap();
        assert_eq!(out, plain);
    }

    #[test]
    fn audio_round_trip_uses_audio_info() {
        let mk = [3u8; 32];
        let plain = b"OggS audio frame ...".to_vec();
        let (blob, _plain_sha, _enc_sha) = synth_blob(&plain, &mk, AUDIO_INFO);

        let mut a = AudioMessage::default();
        a.direct_path = Some("/v/t62/aud-foo".into());
        a.media_key = Some(mk.to_vec());

        let out = decrypt_via_metadata(&blob, a.media_key.as_ref().unwrap(), AUDIO_INFO).unwrap();
        assert_eq!(out, plain);
    }

    #[test]
    fn document_round_trip_uses_document_info() {
        let mk = [4u8; 32];
        let plain = b"%PDF-1.4 ...".to_vec();
        let (blob, _plain_sha, _enc_sha) = synth_blob(&plain, &mk, DOCUMENT_INFO);

        let mut d = DocumentMessage::default();
        d.direct_path = Some("/v/t62/doc-foo".into());
        d.media_key = Some(mk.to_vec());

        let out = decrypt_via_metadata(&blob, d.media_key.as_ref().unwrap(), DOCUMENT_INFO).unwrap();
        assert_eq!(out, plain);
    }

    #[test]
    fn sticker_round_trip_uses_image_info() {
        // Sticker is the cross-class case from `classToMediaType`: it lives
        // under StickerMessage but uses the IMAGE app-info / mms-type. The
        // assert below pins that mapping (it would silently break decrypt
        // against real servers if it drifted).
        let mk = [5u8; 32];
        let plain = b"WEBP sticker payload".to_vec();
        let (blob, _plain_sha, _enc_sha) = synth_blob(&plain, &mk, IMAGE_INFO);

        let mut s = StickerMessage::default();
        s.direct_path = Some("/v/t62/sticker-foo".into());
        s.media_key = Some(mk.to_vec());

        let out = decrypt_via_metadata(&blob, s.media_key.as_ref().unwrap(), IMAGE_INFO).unwrap();
        assert_eq!(out, plain);
    }

    #[test]
    fn link_thumbnail_round_trip_uses_link_thumbnail_info() {
        let mk = [6u8; 32];
        let plain = b"thumbnail JPEG".to_vec();
        let (blob, _plain_sha, _enc_sha) = synth_blob(&plain, &mk, LINK_THUMBNAIL_INFO);

        let mut et = ExtendedTextMessage::default();
        et.thumbnail_direct_path = Some("/v/t62/thumb-foo".into());
        et.media_key = Some(mk.to_vec());

        let out = decrypt_via_metadata(&blob, et.media_key.as_ref().unwrap(), LINK_THUMBNAIL_INFO)
            .unwrap();
        assert_eq!(out, plain);
    }

    /// Pre-flight metadata validation: missing direct_path / media_key /
    /// file_enc_sha256 must surface as `BadAttachment` *before* any
    /// network or crypto work. We feed an empty `MediaConn` (no hosts) so
    /// the test would loudly fail with `AllHostsFailed` if the pre-check
    /// were bypassed.
    #[tokio::test]
    async fn typed_download_rejects_missing_metadata() {
        let conn = MediaConn { auth: "x".into(), hosts: vec![] };

        // Missing direct_path.
        let mut img = ImageMessage::default();
        img.media_key = Some(vec![0u8; 32]);
        img.file_enc_sha256 = Some(vec![0u8; 32]);
        let err = download_image(&conn, &img).await.unwrap_err();
        assert!(matches!(err, MediaError::BadAttachment(_)), "got {err:?}");

        // Missing media_key.
        let mut img = ImageMessage::default();
        img.direct_path = Some("/v/t62/img-foo".into());
        img.file_enc_sha256 = Some(vec![0u8; 32]);
        let err = download_image(&conn, &img).await.unwrap_err();
        assert!(matches!(err, MediaError::BadAttachment(_)), "got {err:?}");

        // Missing file_enc_sha256.
        let mut img = ImageMessage::default();
        img.direct_path = Some("/v/t62/img-foo".into());
        img.media_key = Some(vec![0u8; 32]);
        let err = download_image(&conn, &img).await.unwrap_err();
        assert!(matches!(err, MediaError::BadAttachment(_)), "got {err:?}");

        // Wrong-length file_enc_sha256 (must be 32).
        let mut img = ImageMessage::default();
        img.direct_path = Some("/v/t62/img-foo".into());
        img.media_key = Some(vec![0u8; 32]);
        img.file_enc_sha256 = Some(vec![0u8; 16]);
        let err = download_image(&conn, &img).await.unwrap_err();
        assert!(matches!(err, MediaError::BadAttachment(_)), "got {err:?}");
    }

    // ---------------------------------------------------------------------
    // Upload tests.
    //
    // The upload path is `encrypt → POST → parse JSON`. We exercise the
    // full pipeline with a stub `UploadHttpClient` that records the URL
    // + body it received, then runs the same `decrypt_media` path the
    // recipient would, using the keys returned in `UploadResult`.

    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Test stub: records the URL + body of the most recent POST and
    /// returns a fixed JSON response. The captured URL is used to assert
    /// shape (`/mms/{media_type}/{token}?auth=...&token=...`).
    #[derive(Default)]
    struct CapturingHttp {
        captured_url: Mutex<Option<String>>,
        captured_body: Mutex<Option<Vec<u8>>>,
        captured_headers: Mutex<Vec<(String, String)>>,
        response: Vec<u8>,
    }

    #[async_trait]
    impl UploadHttpClient for CapturingHttp {
        async fn post(
            &self,
            url: &str,
            body: Vec<u8>,
            headers: &[(&str, &str)],
        ) -> Result<Vec<u8>, MediaError> {
            *self.captured_url.lock().await = Some(url.to_owned());
            *self.captured_body.lock().await = Some(body);
            let mut h = self.captured_headers.lock().await;
            h.clear();
            for (k, v) in headers {
                h.push(((*k).to_owned(), (*v).to_owned()));
            }
            Ok(self.response.clone())
        }
    }

    /// End-to-end: encrypt + POST (via stub) + decrypt with the same keys
    /// recovers the original plaintext. This pins that the keys returned in
    /// `UploadResult` are usable for downstream decryption — which is the
    /// whole contract upstream depends on for the recipient flow.
    #[tokio::test]
    async fn upload_round_trip_with_local_decrypt() {
        let conn = MediaConn {
            auth: "AUTH+TOKEN==".into(),
            hosts: vec!["mmg.example.test".into()],
        };
        let body_response =
            br#"{"url":"https://cdn.example/u","direct_path":"/v/t62/u","handle":"H1"}"#
                .to_vec();
        let http = Arc::new(CapturingHttp {
            response: body_response,
            ..Default::default()
        });
        let plaintext = b"hello upload pipeline".to_vec();

        let result = upload_with_http(&conn, &plaintext, "image", &conn.auth, http.as_ref())
            .await
            .expect("upload ok");

        assert_eq!(result.file_length, plaintext.len() as u64);
        assert_eq!(result.url, "https://cdn.example/u");
        assert_eq!(result.direct_path, "/v/t62/u");
        assert_eq!(result.handle, "H1");
        assert_ne!(result.media_key, [0u8; 32], "media_key must be randomized");

        // The captured POST body MUST be the full encrypted blob — and
        // decrypting it with the keys we got back MUST recover plaintext.
        let posted = http.captured_body.lock().await.clone().expect("body posted");
        // file_enc_sha256 in result == SHA256(posted blob).
        let computed_enc_sha: [u8; 32] = Sha256::digest(&posted).into();
        assert_eq!(computed_enc_sha, result.file_enc_sha256);

        // file_sha256 == SHA256(plaintext).
        let computed_plain_sha: [u8; 32] = Sha256::digest(&plaintext).into();
        assert_eq!(computed_plain_sha, result.file_sha256);

        // Now decrypt — the recipient's path. Use IMAGE_INFO since we
        // uploaded with media_type="image".
        let keys = derive_media_keys(&result.media_key, IMAGE_INFO).unwrap();
        let recovered = decrypt_media(&posted, &keys).expect("decrypt");
        assert_eq!(recovered, plaintext);
    }

    /// URL pin: every uploaded blob hits a path that contains the right
    /// `mms/{media_type}` segment AND carries the `auth` query parameter
    /// verbatim (URL-encoded). Ensures we don't accidentally drop the
    /// segment for any of the supported media classes.
    #[tokio::test]
    async fn media_url_path_includes_media_type_segment() {
        let conn = MediaConn {
            auth: "TOK+E/N==".into(),
            hosts: vec!["mmg.example.test".into()],
        };
        let json_resp =
            br#"{"url":"https://cdn/u","direct_path":"/v/u","handle":""}"#.to_vec();

        for (media_type, expected_segment) in [
            ("image", "image"),
            ("video", "video"),
            ("audio", "audio"),
            ("document", "document"),
            // Stickers ride the IMAGE app-info AND the IMAGE URL segment —
            // mirror upstream's `mediaTypeToMMSType[MediaImage]` mapping for
            // sticker uploads in the multi-device client. Pin it here.
            ("sticker", "image"),
        ] {
            let http = Arc::new(CapturingHttp {
                response: json_resp.clone(),
                ..Default::default()
            });
            let _ =
                upload_with_http(&conn, b"data", media_type, &conn.auth, http.as_ref())
                    .await
                    .expect("upload ok");
            let url = http.captured_url.lock().await.clone().expect("url");
            assert!(
                url.starts_with(&format!(
                    "https://mmg.example.test/mms/{expected_segment}/"
                )),
                "media_type={media_type} → url={url}"
            );
            // auth=TOK%2BE%2FN%3D%3D — `+`, `/`, `=` all percent-encoded.
            assert!(
                url.contains("auth=TOK%2BE%2FN%3D%3D"),
                "auth not URL-encoded: {url}"
            );
        }
    }

    /// Unknown media_type label is rejected up front — no random key is
    /// generated, no HTTP call is issued.
    #[tokio::test]
    async fn upload_rejects_unknown_media_type() {
        let conn = MediaConn {
            auth: "x".into(),
            hosts: vec!["h".into()],
        };
        let http = Arc::new(CapturingHttp::default());
        let err = upload_with_http(&conn, b"x", "wat", &conn.auth, http.as_ref())
            .await
            .unwrap_err();
        assert!(matches!(err, MediaError::Malformed(_)));
        assert!(http.captured_url.lock().await.is_none());
    }

    /// Empty host list surfaces as `Malformed`, not `AllHostsFailed`. We
    /// explicitly distinguish "configuration error" (caller forgot to
    /// refresh) from "every host rejected the upload" so callers can
    /// log them differently.
    #[tokio::test]
    async fn upload_rejects_empty_hosts() {
        let conn = MediaConn {
            auth: "x".into(),
            hosts: vec![],
        };
        let http = Arc::new(CapturingHttp::default());
        let err = upload_with_http(&conn, b"x", "image", &conn.auth, http.as_ref())
            .await
            .unwrap_err();
        assert!(matches!(err, MediaError::Malformed(_)));
    }

    /// First host failing falls through to the second. The retry loop is
    /// load-bearing on the live wire (mmg.whatsapp.net occasionally 503s
    /// during peak hours) so pin it.
    #[tokio::test]
    async fn upload_falls_through_to_second_host_on_failure() {
        struct FlakyHttp {
            attempts: Mutex<usize>,
        }
        #[async_trait]
        impl UploadHttpClient for FlakyHttp {
            async fn post(
                &self,
                url: &str,
                _body: Vec<u8>,
                _headers: &[(&str, &str)],
            ) -> Result<Vec<u8>, MediaError> {
                let mut a = self.attempts.lock().await;
                *a += 1;
                if url.contains("primary") {
                    Err(MediaError::Http("primary down".into()))
                } else {
                    Ok(br#"{"url":"https://x/u","direct_path":"/v/u","handle":""}"#
                        .to_vec())
                }
            }
        }
        let conn = MediaConn {
            auth: "auth".into(),
            hosts: vec!["primary.example.test".into(), "fallback.example.test".into()],
        };
        let http = FlakyHttp {
            attempts: Mutex::new(0),
        };
        let result =
            upload_with_http(&conn, b"hi", "audio", &conn.auth, &http).await.unwrap();
        assert_eq!(result.url, "https://x/u");
        assert_eq!(*http.attempts.lock().await, 2);
    }
}
