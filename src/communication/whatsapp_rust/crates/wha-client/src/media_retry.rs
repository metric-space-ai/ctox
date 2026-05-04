//! Media retry receipts.
//!
//! Mirrors `_upstream/whatsmeow/mediaretry.go` (185 LOC). When a media
//! download fails (typically with HTTP 404/410 after a history sync), the
//! companion device asks the primary phone to re-upload the asset by sending a
//! `<receipt type="server-error">` carrying a "retry" payload. The phone then
//! responds with a fresh direct path inside an `<encrypt>` element.
//!
//! Two shapes appear on the wire:
//!
//! 1. **Outbound retry request** (this module's [`send_media_retry_receipt`]):
//!    the simpler shape used by media-retry pings — a `<retry id="..." count
//!    t="..." v="1"/>` child plus a top-level `<registration>` carrying our
//!    4-BE `registration_id`. Upstream's `SendMediaRetryReceipt` uses a
//!    richer shape with an `<encrypt>` element wrapping the encrypted retry
//!    plaintext (the `WhatsApp Media Retry Notification`-keyed AES-GCM
//!    container) and an `<rmr>` element carrying the JID/from_me/participant
//!    metadata. We keep the simpler shape on the wire here because the public
//!    API explicitly takes only `(msg_id, chat, sender, retry_id_data)` —
//!    callers that want the encrypted-RMR shape should use the upstream
//!    pipeline (out of scope; see TODO at end of module).
//!
//! 2. **Inbound `<encrypt>` retry response**: the server may also push back a
//!    `<encrypt type="..."><retry id="..."/></encrypt>` notification asking
//!    the companion to re-encrypt the original message with a refreshed prekey
//!    bundle. [`parse_inbound_encrypt_retry`] decodes that envelope; the
//!    actual resend ties into `crate::send_encrypt` and is currently
//!    `NotImplemented` like the rest of the retry-cache wiring (see
//!    `crate::retry::TODO(retry-cache)`).

use tracing::debug;

use wha_binary::{Attrs, Node, Value};
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;

/// Build a `<receipt type="server-error">` requesting media retry from the
/// phone. Pure: no I/O. Mirrors the receipt shape called out in the public
/// API of this module (see file-level docs).
///
/// ```xml
/// <receipt id="MSG-X" type="server-error" to="<chat>" participant="<sender>">
///   <retry id="MSG-X" count="1" t="<unix>" v="1"/>
///   <registration>...4 BE bytes...</registration>
/// </receipt>
/// ```
///
/// `participant` is omitted entirely when the retry targets a 1:1 chat (the
/// `sender` argument is then ignored — upstream documents that group retries
/// always have `chat ≠ sender`).
pub(crate) fn build_media_retry_receipt_node(
    msg_id: &str,
    chat: &Jid,
    sender: &Jid,
    retry_id_data: &[u8],
    registration_id: u32,
    timestamp: i64,
) -> Node {
    let mut receipt_attrs = Attrs::new();
    receipt_attrs.insert("id".into(), Value::String(msg_id.to_owned()));
    receipt_attrs.insert("type".into(), Value::String("server-error".into()));
    receipt_attrs.insert("to".into(), Value::Jid(chat.clone()));
    // Only include `participant` when this is a group retry — i.e. the chat
    // and sender are distinct JIDs. Upstream's `SendMediaRetryReceipt` only
    // sets `rmr.participant` when `message.IsGroup` is true; this is the same
    // distinction at the receipt-attribute level.
    if sender != chat && !sender.is_empty() {
        receipt_attrs.insert("participant".into(), Value::Jid(sender.clone()));
    }

    // <retry id=".." count="1" t=".." v="1"/>
    let mut retry_attrs = Attrs::new();
    retry_attrs.insert("id".into(), Value::String(msg_id.to_owned()));
    retry_attrs.insert("count".into(), Value::String("1".into()));
    retry_attrs.insert("t".into(), Value::String(timestamp.to_string()));
    retry_attrs.insert("v".into(), Value::String("1".into()));
    // The `retry_id_data` blob is what upstream's encryption pipeline uses to
    // bind the retry to the original message. We attach it as the inner
    // content of <retry> when non-empty so callers that *do* compute it (out
    // of scope; see file docs) can carry it through. Empty => content omitted.
    let retry = if retry_id_data.is_empty() {
        Node::new("retry", retry_attrs, None)
    } else {
        Node::new(
            "retry",
            retry_attrs,
            Some(Value::Bytes(retry_id_data.to_vec())),
        )
    };

    // <registration>...4 BE bytes...</registration>
    let registration = Node::new(
        "registration",
        Attrs::new(),
        Some(Value::Bytes(registration_id.to_be_bytes().to_vec())),
    );

    Node::new(
        "receipt",
        receipt_attrs,
        Some(Value::Nodes(vec![retry, registration])),
    )
}

/// Send a media-retry request to the phone for `msg_id` in `chat`.
///
/// Mirrors `Client.SendMediaRetryReceipt` in `mediaretry.go` minus the
/// AES-GCM-encrypted RMR wrapping (see file-level docs for the simpler
/// receipt shape this function builds).
///
/// # Arguments
/// - `client`: connected, logged-in client (uses `device.registration_id`
///   for the `<registration>` payload).
/// - `msg_id`: the message id of the asset that failed to download.
/// - `chat`: the chat JID where the message was sent.
/// - `sender`: the sender JID — for 1:1 chats pass the same value as `chat`
///   (or an empty JID) and the `participant=` attribute is omitted.
/// - `retry_id_data`: optional opaque data supplied by the caller — usually
///   computed from the original message's `Info.ID` and the media key. When
///   empty, the `<retry/>` element is sent without any inner content.
pub async fn send_media_retry_receipt(
    client: &Client,
    msg_id: &str,
    chat: &Jid,
    sender: &Jid,
    retry_id_data: &[u8],
) -> Result<(), ClientError> {
    if client.device.id.is_none() {
        return Err(ClientError::NotLoggedIn);
    }
    let registration_id = client.device.registration_id;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let receipt = build_media_retry_receipt_node(
        msg_id,
        chat,
        sender,
        retry_id_data,
        registration_id,
        timestamp,
    );
    client.send_node(&receipt).await
}

// ---------------------------------------------------------------------------
// Inbound `<encrypt>` retry response handler
// ---------------------------------------------------------------------------

/// Parsed view of an inbound `<encrypt><retry/></encrypt>` envelope.
///
/// Surface-level only: just the `id` and `count` upstream's media-retry path
/// surfaces in `events.MediaRetry`. The actual retry response decoding (with
/// its AES-GCM "WhatsApp Media Retry Notification" payload) lives in
/// `whatsmeow/mediaretry.go::DecryptMediaRetryNotification` — porting that
/// fully needs the media-retry proto messages and is left out of scope here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundEncryptRetry {
    /// Server-assigned ID of the retry round-trip (echoed back from our
    /// `<receipt>`).
    pub id: String,
    /// `type` attribute on the outer `<encrypt>` (e.g. `"pkmsg"`, `"msg"`,
    /// or `"mediaretry"` depending on what the server is asking for).
    pub encrypt_type: Option<String>,
}

/// Decode a server-pushed `<encrypt type="..."><retry id="..."/></encrypt>`
/// envelope. Pure: no I/O.
///
/// The server uses this shape to ask the companion to re-encrypt with a fresh
/// bundle (separate from the receipt-based retry handled in
/// [`crate::retry::handle_retry_receipt`]).
pub fn parse_inbound_encrypt_retry(node: &Node) -> Result<InboundEncryptRetry, ClientError> {
    if node.tag != "encrypt" {
        return Err(ClientError::Malformed(format!(
            "expected <encrypt>, got <{}>",
            node.tag
        )));
    }
    let retry = node
        .child_by_tag(&["retry"])
        .ok_or_else(|| ClientError::Malformed("encrypt envelope missing <retry>".into()))?;
    let id = retry
        .get_attr_str("id")
        .ok_or_else(|| ClientError::Malformed("encrypt/retry missing id".into()))?
        .to_owned();
    let encrypt_type = node.get_attr_str("type").map(|s| s.to_owned());
    Ok(InboundEncryptRetry { id, encrypt_type })
}

/// Top-level dispatcher for an inbound `<encrypt>` retry envelope.
///
/// Currently logs the parsed envelope and returns `Ok(())` so the receive
/// pump stays healthy. The actual resend (re-encrypt with fresh bundle, send
/// the message again) requires the recent-outgoing-message cache that
/// `crate::retry::handle_retry_receipt` is also blocked on — see the
/// `TODO(retry-cache)` in `crate::retry`.
pub async fn handle_inbound_encrypt_retry(
    _client: &Client,
    node: &Node,
) -> Result<(), ClientError> {
    let parsed = parse_inbound_encrypt_retry(node)?;
    debug!(
        retry_id = %parsed.id,
        encrypt_type = ?parsed.encrypt_type,
        "parsed inbound encrypt-retry envelope (resend path not implemented; see TODO(retry-cache))",
    );
    // TODO(retry-cache): look up the original plaintext keyed by the retry
    // id, re-encrypt under a fresh signal session, and send a new <message>
    // back to the server.
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wha_store::MemoryStore;
    use wha_types::jid::server;

    #[test]
    fn build_media_retry_receipt_node_one_to_one_omits_participant() {
        let chat = Jid::new("4915211111111", server::DEFAULT_USER);
        // 1:1 chat — sender == chat ⇒ no participant attribute.
        let receipt = build_media_retry_receipt_node(
            "MSG-AAA",
            &chat,
            &chat,
            &[],
            0xDEADBEEF,
            1714521600,
        );

        assert_eq!(receipt.tag, "receipt");
        assert_eq!(receipt.get_attr_str("id"), Some("MSG-AAA"));
        assert_eq!(receipt.get_attr_str("type"), Some("server-error"));
        assert_eq!(receipt.get_attr_jid("to"), Some(&chat));
        assert!(receipt.attrs.get("participant").is_none());

        // Children: <retry/> then <registration>.
        let kids = receipt.children();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].tag, "retry");
        assert_eq!(kids[0].get_attr_str("id"), Some("MSG-AAA"));
        assert_eq!(kids[0].get_attr_str("count"), Some("1"));
        assert_eq!(kids[0].get_attr_str("v"), Some("1"));
        assert_eq!(kids[0].get_attr_str("t"), Some("1714521600"));
        // No retry_id_data ⇒ <retry/> has no content.
        assert_eq!(kids[0].content, Value::None);

        // <registration> carries 4 BE bytes of the registration id.
        assert_eq!(kids[1].tag, "registration");
        assert_eq!(
            kids[1].content,
            Value::Bytes(0xDEADBEEFu32.to_be_bytes().to_vec())
        );
    }

    #[test]
    fn build_media_retry_receipt_node_group_includes_participant_and_data() {
        let chat = Jid::new("12345", server::GROUP);
        let sender = Jid::new("9999", server::DEFAULT_USER);
        let retry_data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let receipt = build_media_retry_receipt_node(
            "MSG-GROUP",
            &chat,
            &sender,
            &retry_data,
            0x11223344,
            1714000000,
        );

        // Group → participant set to the sender.
        let participant = receipt
            .get_attr_jid("participant")
            .expect("group retry must carry participant=");
        assert_eq!(participant, &sender);

        // <retry> now wraps retry_id_data as bytes.
        let retry = &receipt.children()[0];
        assert_eq!(retry.tag, "retry");
        assert_eq!(retry.content, Value::Bytes(retry_data));
    }

    #[tokio::test]
    async fn send_media_retry_receipt_when_logged_out_errors_cleanly() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        // No `device.id` set ⇒ NotLoggedIn before we even hit the socket.
        let (cli, _evt) = Client::new(device);
        let chat = Jid::new("123", server::DEFAULT_USER);
        let r = send_media_retry_receipt(&cli, "MSG-NEVER", &chat, &chat, &[]).await;
        assert!(matches!(r, Err(ClientError::NotLoggedIn)));
    }

    #[tokio::test]
    async fn send_media_retry_receipt_logged_in_but_disconnected_returns_not_connected() {
        let store = Arc::new(MemoryStore::new());
        let mut device = store.new_device();
        // Mark the device as "logged in" (has a JID) but with no live socket
        // — the call should reach send_node and fail with NotConnected.
        device.id = Some(Jid::new("4915200000000", server::DEFAULT_USER));
        let (cli, _evt) = Client::new(device);
        let chat = Jid::new("123", server::DEFAULT_USER);
        let r = send_media_retry_receipt(&cli, "MSG-OFFLINE", &chat, &chat, &[]).await;
        assert!(matches!(r, Err(ClientError::NotConnected)));
    }

    #[test]
    fn parse_inbound_encrypt_retry_extracts_id_and_type() {
        // <encrypt type="pkmsg"><retry id="ABC"/></encrypt>
        let mut retry_attrs = Attrs::new();
        retry_attrs.insert("id".into(), Value::String("ABC".into()));
        let retry = Node::new("retry", retry_attrs, None);

        let mut envelope_attrs = Attrs::new();
        envelope_attrs.insert("type".into(), Value::String("pkmsg".into()));
        let envelope = Node::new("encrypt", envelope_attrs, Some(Value::Nodes(vec![retry])));

        let parsed = parse_inbound_encrypt_retry(&envelope).expect("parse");
        assert_eq!(parsed.id, "ABC");
        assert_eq!(parsed.encrypt_type.as_deref(), Some("pkmsg"));
    }

    #[test]
    fn parse_inbound_encrypt_retry_rejects_wrong_tag() {
        let n = Node::tag_only("notification");
        let r = parse_inbound_encrypt_retry(&n);
        assert!(matches!(r, Err(ClientError::Malformed(_))));
    }

    #[test]
    fn parse_inbound_encrypt_retry_requires_inner_retry_with_id() {
        // <encrypt><retry/></encrypt> — missing the id attribute.
        let retry = Node::new("retry", Attrs::new(), None);
        let envelope = Node::new("encrypt", Attrs::new(), Some(Value::Nodes(vec![retry])));
        let r = parse_inbound_encrypt_retry(&envelope);
        assert!(matches!(r, Err(ClientError::Malformed(_))));
    }

    #[tokio::test]
    async fn handle_inbound_encrypt_retry_swallows_until_cache_lands() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);

        let mut retry_attrs = Attrs::new();
        retry_attrs.insert("id".into(), Value::String("X-1".into()));
        let retry = Node::new("retry", retry_attrs, None);
        let envelope = Node::new("encrypt", Attrs::new(), Some(Value::Nodes(vec![retry])));

        // Without the recent-messages cache the handler should still return
        // Ok so the receive loop stays healthy.
        handle_inbound_encrypt_retry(&cli, &envelope)
            .await
            .expect("handler should swallow until cache lands");
    }
}

// ---------------------------------------------------------------------------
// TODO: AES-GCM encrypted RMR receipt
// ---------------------------------------------------------------------------
//
// Upstream's `Client.SendMediaRetryReceipt` (mediaretry.go:77) builds a
// richer receipt shape that we deliberately do *not* produce here:
//
//   <receipt id="..." type="server-error" to="<own_jid>">
//     <encrypt><enc_p>...</enc_p><enc_iv>...</enc_iv></encrypt>
//     <rmr jid="<chat>" from_me="<bool>" participant="<sender if group>"/>
//   </receipt>
//
// The encrypted payload is `gcmutil.Encrypt(key, iv, marshalled_proto, msg_id)`
// where:
//   - `key = HKDF-SHA256(media_key, info="WhatsApp Media Retry Notification", L=32)`
//   - `marshalled_proto = waMmsRetry.ServerErrorReceipt{ stanza_id: msg_id }`
//
// Porting it requires:
//   - the `waMmsRetry` proto definitions (not yet in `wha-proto`),
//   - `wha_crypto::hkdf_sha256` (already available) + `gcm_encrypt` (already
//     available),
//   - threading the original message's media_key through the public API.
//
// The current `send_media_retry_receipt` signature takes only an opaque
// `retry_id_data` blob, which is enough for the simpler shape callers in this
// repo build today. When the proto + media-key plumbing lands, an
// `send_media_retry_receipt_encrypted` overload should live next to this
// helper.
