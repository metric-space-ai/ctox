//! History-sync download handler.
//!
//! When WhatsApp sends a `<message>` whose decrypted body contains a
//! `protocolMessage.historySyncNotification`, that notification points at an
//! externally-stored, end-to-end-encrypted, zlib-compressed blob holding a
//! chunk of the user's chat history. The full pipeline is:
//!
//! 1. **Refresh `MediaConn`** — `<iq xmlns="w:m" type="set"
//!    to="s.whatsapp.net"><media_conn/></iq>` returns a list of HTTPS hosts
//!    that serve attachments (mmg.whatsapp.net, mmg-fna.whatsapp.net, …)
//!    plus an opaque auth token.
//! 2. **Download the blob** — for each host, GET
//!    `https://{host}{direct_path}&hash={base64url(file_enc_sha256)}&mms-type=md-msg-hist&__wa-mms=`.
//! 3. **Verify and decrypt** — the blob is `body || HMAC10`. With
//!    `media_key`, derive `(iv, cipher_key, mac_key, _)` via HKDF-SHA256
//!    over `app_info = "WhatsApp History Keys"`. Constant-time-compare the
//!    trailing 10 MAC bytes against the first 10 of
//!    `HMAC-SHA256(mac_key, iv || body)`, then `cbc_decrypt` the body.
//! 4. **Decompress** — the plaintext is zlib-deflated.
//! 5. **Decode** — the inflated payload is a `wha_proto::history_sync::HistorySync`.
//!
//! Mirrors `_upstream/whatsmeow/Client::handleHistorySyncNotification` in
//! spirit: that function lives at the call-site (not the IQ layer) and
//! similarly chains refresh → download → decrypt → inflate → unmarshal.

use std::io::Read;

use async_trait::async_trait;
use flate2::read::ZlibDecoder;
use prost::Message as _;
use wha_binary::Node;
use wha_media::{IqSender, MediaError};
use wha_proto::e2e::HistorySyncNotification;
use wha_proto::history_sync::HistorySync;

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

impl From<MediaError> for ClientError {
    fn from(e: MediaError) -> Self {
        ClientError::Download(e.to_string())
    }
}

/// Adapter so `wha-media` can fire IQs through our `Client` without a direct
/// dependency the other way (`wha-media` doesn't link `wha-client`). The
/// adapter wraps a `&Client` and forwards through `send_iq`.
pub struct ClientIqSender<'a> {
    pub client: &'a Client,
}

#[async_trait]
impl<'a> IqSender for ClientIqSender<'a> {
    async fn send_media_conn_iq(&self, iq: Node) -> Result<Node, MediaError> {
        // We don't need to use the prebuilt `iq.attrs` directly — `send_iq`
        // assigns its own request id and frames the rest. Just translate the
        // body and the `to` JID into an `InfoQuery`.
        //
        // `wha_media::build_media_conn_iq` always emits xmlns="w:m",
        // type="set", to="s.whatsapp.net", and a single `<media_conn/>` body
        // child. We mirror that here.
        let body = iq.children().to_vec();
        let to = iq
            .get_attr_jid("to")
            .ok_or_else(|| MediaError::Iq("missing @to on media_conn iq".into()))?
            .clone();
        let query = InfoQuery::new("w:m", IqType::Set)
            .to(to)
            .content(wha_binary::Value::Nodes(body));
        let resp = self
            .client
            .send_iq(query)
            .await
            .map_err(|e| MediaError::Iq(e.to_string()))?;
        Ok(resp)
    }
}

/// Resolve a `HistorySyncNotification` into the parsed `HistorySync` proto.
///
/// Steps (each step is a separate failure mode surfaced through
/// [`ClientError`]):
///
/// - Validate that `media_key`, `direct_path`, and `file_enc_sha256` are
///   present on the notification.
/// - Refresh the media connection list.
/// - Download the encrypted blob.
/// - Derive the four keys with `app_info = HISTORY_INFO`, verify the MAC,
///   and AES-CBC decrypt the body.
/// - zlib-inflate the decrypted bytes.
/// - Decode the inflated bytes as `HistorySync`.
pub async fn handle_history_sync_notification(
    client: &Client,
    notif: &HistorySyncNotification,
) -> Result<HistorySync, ClientError> {
    // Some history sync chunks (e.g. INITIAL_BOOTSTRAP marker frames) come
    // with the payload inlined on the notification instead of a download URL.
    // In that case `media_key` / `direct_path` / `file_enc_sha256` are absent
    // and `initial_hist_bootstrap_inline_payload` carries the zlib-compressed
    // protobuf directly.
    if let Some(inline) = notif.initial_hist_bootstrap_inline_payload.as_deref() {
        if !inline.is_empty()
            && notif.media_key.is_none()
            && notif.direct_path.is_none()
        {
            tracing::info!(
                inline_len = inline.len(),
                sync_type = ?notif.sync_type,
                "decoding inline history-sync payload"
            );
            let mut decoder = ZlibDecoder::new(inline);
            let mut inflated = Vec::with_capacity(inline.len() * 2);
            decoder.read_to_end(&mut inflated).map_err(|e| {
                ClientError::Download(format!("inline zlib inflate failed: {e}"))
            })?;
            let parsed = HistorySync::decode(inflated.as_slice()).map_err(|e| {
                ClientError::Proto(format!("HistorySync (inline) decode failed: {e}"))
            })?;
            tracing::info!(
                sync_type = ?parsed.sync_type(),
                conversations = parsed.conversations.len(),
                pushnames = parsed.pushnames.len(),
                "inline history sync decoded"
            );
            return Ok(parsed);
        }
    }

    let media_key = notif
        .media_key
        .as_deref()
        .ok_or_else(|| ClientError::Download("history sync notif missing media_key".into()))?;
    let direct_path = notif
        .direct_path
        .as_deref()
        .ok_or_else(|| ClientError::Download("history sync notif missing direct_path".into()))?;
    let file_enc_sha256 = notif.file_enc_sha256.as_deref().ok_or_else(|| {
        ClientError::Download("history sync notif missing file_enc_sha256".into())
    })?;

    tracing::info!(
        direct_path = %direct_path,
        chunk_order = ?notif.chunk_order,
        sync_type = ?notif.sync_type,
        "starting history-sync download"
    );

    // 1. Refresh MediaConn over our IQ socket.
    let sender = ClientIqSender { client };
    let conn = wha_media::refresh_media_conn(&sender, client.generate_request_id()).await?;

    // 2. Download the encrypted blob. Mirrors upstream's `MediaHistory`
    // mms-type label (`md-msg-hist`, see `_upstream/whatsmeow/download.go`).
    let blob =
        wha_media::download_encrypted_media(&conn, direct_path, file_enc_sha256, "md-msg-hist")
            .await?;

    // 3. Decrypt with the history-keys app-info label.
    let keys = wha_crypto::derive_media_keys(media_key, wha_crypto::HISTORY_INFO)?;
    let decrypted = wha_crypto::decrypt_media(&blob, &keys)?;

    // 4. zlib-inflate.
    let mut decoder = ZlibDecoder::new(decrypted.as_slice());
    let mut inflated = Vec::with_capacity(decrypted.len() * 2);
    decoder
        .read_to_end(&mut inflated)
        .map_err(|e| ClientError::Download(format!("zlib inflate failed: {e}")))?;

    // 5. Decode the HistorySync proto.
    let parsed = HistorySync::decode(inflated.as_slice())
        .map_err(|e| ClientError::Proto(format!("HistorySync decode failed: {e}")))?;

    tracing::info!(
        sync_type = ?parsed.sync_type(),
        conversations = parsed.conversations.len(),
        pushnames = parsed.pushnames.len(),
        progress = ?parsed.progress,
        chunk_order = ?parsed.chunk_order,
        "history sync decoded"
    );

    Ok(parsed)
}

// ---------------------------------------------------------------------------
// Client convenience method.
// ---------------------------------------------------------------------------

impl Client {
    /// Resolve a `HistorySyncNotification` (which arrives inside a decrypted
    /// `<message>`'s `protocol_message`) into the parsed `HistorySync` proto.
    ///
    /// Refreshes the media-conn list, downloads the encrypted blob from the
    /// WhatsApp CDN, validates its truncated HMAC, AES-CBC-decrypts, zlib
    /// inflates, and prost-decodes — or, if the notification is an inline
    /// bootstrap chunk, decodes the inline payload directly.
    pub async fn download_history_sync(
        &self,
        notif: &HistorySyncNotification,
    ) -> Result<HistorySync, ClientError> {
        handle_history_sync_notification(self, notif).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    /// End-to-end: simulate the full encrypt-compress-decrypt-decompress
    /// pipeline against a synthetic `HistorySync` payload, without touching
    /// any network.  This pins that the wha-crypto + flate2 round-trip we
    /// rely on actually works with the proto module wha-proto exposes.
    #[test]
    fn synthetic_round_trip_decodes_history_sync() {
        // Build a tiny HistorySync proto and serialize it.
        let mut hs = HistorySync::default();
        hs.set_sync_type(wha_proto::history_sync::history_sync::HistorySyncType::Recent);
        hs.chunk_order = Some(7);
        hs.progress = Some(99);
        let mut hs_bytes = Vec::new();
        hs.encode(&mut hs_bytes).unwrap();

        // zlib-compress.
        let mut compressed = Vec::new();
        {
            let mut enc = ZlibEncoder::new(&mut compressed, Compression::default());
            enc.write_all(&hs_bytes).unwrap();
            enc.finish().unwrap();
        }

        // Encrypt with derived media keys + append HMAC10 — same shape the
        // server delivers.
        let media_key = [0xAB; 32];
        let keys = wha_crypto::derive_media_keys(&media_key, wha_crypto::HISTORY_INFO).unwrap();
        let body = wha_crypto::cbc_encrypt(&keys.cipher_key, &keys.iv, &compressed).unwrap();
        let mut mac_in = Vec::new();
        mac_in.extend_from_slice(&keys.iv);
        mac_in.extend_from_slice(&body);
        let mac_full = wha_crypto::hmac_sha256(&keys.mac_key, &mac_in);
        let mut blob = body.clone();
        blob.extend_from_slice(&mac_full[..10]);

        // Mirror handle_history_sync_notification's post-download steps:
        // decrypt, inflate, decode.
        let dec = wha_crypto::decrypt_media(&blob, &keys).unwrap();
        let mut decoder = ZlibDecoder::new(dec.as_slice());
        let mut inflated = Vec::new();
        decoder.read_to_end(&mut inflated).unwrap();
        let parsed = HistorySync::decode(inflated.as_slice()).unwrap();
        assert_eq!(
            parsed.sync_type(),
            wha_proto::history_sync::history_sync::HistorySyncType::Recent
        );
        assert_eq!(parsed.chunk_order, Some(7));
        assert_eq!(parsed.progress, Some(99));
    }
}
