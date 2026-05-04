//! Outgoing message orchestration — port of `whatsmeow/send.go::SendMessage`.
//!
//! This module ties together the per-recipient Signal session encryption
//! ([`crate::send_encrypt::encrypt_for_recipient`]) and the group sender-key
//! flow ([`crate::send_group::encrypt_for_group`]). It owns:
//!
//! 1. Generating a `WebMessageIDPrefix`-style message ID (mirrors
//!    upstream's `Client.GenerateMessageID`).
//! 2. Marshalling the [`wha_proto::e2e::Message`] body via prost.
//! 3. Branching on the destination JID server: `g.us` → group, otherwise → DM.
//! 4. Wrapping the per-recipient `<enc>` Nodes (and, for groups, the
//!    `<enc type="skmsg">` Node) inside the outer `<message>` envelope.
//! 5. Sending the envelope via [`Client::send_node`] and awaiting the
//!    server's `<ack id="...">` to obtain `t` (timestamp) and `server_id`.
//!
//! The detailed per-device fan-out and SKDM encryption live in their
//! sibling modules; this file is intentionally orchestration-only.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

use wha_binary::{Attrs, Node, Value};
use wha_proto::e2e::Message;
use wha_types::Jid;

use crate::client::{Client, DEFAULT_REQUEST_TIMEOUT};
use crate::error::ClientError;

/// Upstream's `WebMessageIDPrefix` constant.
pub const WEB_MESSAGE_ID_PREFIX: &str = "3EB0";

/// Generate a WhatsApp Web–style message ID. Mirrors
/// `whatsmeow.Client.GenerateMessageID`:
///
/// `"3EB0" + uppercase_hex(sha256(unix_ts_be8 || own_user || "@c.us" || rand16)[..9])`
///
/// When the client has no own JID yet (e.g. immediately post-pair), the
/// `own_user || "@c.us"` block is simply omitted, exactly like upstream.
pub fn generate_message_id(client: &Client) -> String {
    let mut data: Vec<u8> = Vec::with_capacity(8 + 20 + 16);

    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    data.extend_from_slice(&unix.to_be_bytes());

    if let Some(own) = client.device.jid() {
        data.extend_from_slice(own.user.as_bytes());
        data.extend_from_slice(b"@c.us");
    }

    let mut rand_buf = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut rand_buf);
    data.extend_from_slice(&rand_buf);

    let hash = Sha256::digest(&data);
    let mut out = String::with_capacity(WEB_MESSAGE_ID_PREFIX.len() + 18);
    out.push_str(WEB_MESSAGE_ID_PREFIX);
    out.push_str(&hex::encode_upper(&hash[..9]));
    out
}

/// Build the outer `<message>` envelope. Mirrors the attrs on the message
/// node assembled by `whatsmeow/send.go::prepareMessageNode` (id, type, to,
/// plus the unix-time `t` that upstream's server returns and clients echo).
///
/// `enc_nodes` are the per-recipient `<enc>` children (and, for groups,
/// the `<enc type="skmsg">` node + any participant SKDMs).
pub fn build_message_envelope(message_id: &str, to: &Jid, msg_type: &str, enc_nodes: Vec<Node>) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("id".into(), Value::String(message_id.to_owned()));
    attrs.insert("type".into(), Value::String(msg_type.to_owned()));
    attrs.insert("to".into(), Value::Jid(to.clone()));
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    attrs.insert("t".into(), Value::String(t.to_string()));
    Node::new("message", attrs, Some(Value::Nodes(enc_nodes)))
}

/// Best-effort port of upstream's `getTypeFromMessage`. Most "ordinary"
/// payloads (Conversation, ExtendedTextMessage, ProtocolMessage) all map
/// to `"text"`, which is the only branch we depend on for the current
/// fan-out tests; the more exotic categories will be filled in alongside
/// their specific outgoing flows (media, polls, reactions, …).
fn message_type_attr(msg: &Message) -> &'static str {
    if msg.reaction_message.is_some() || msg.enc_reaction_message.is_some() {
        "reaction"
    } else if msg.poll_creation_message.is_some()
        || msg.poll_update_message.is_some()
        || msg.poll_creation_message_v2.is_some()
        || msg.poll_creation_message_v3.is_some()
    {
        "poll"
    } else {
        "text"
    }
}

/// Top-level send entry. Mirrors `Client::SendMessage` in upstream Go,
/// minus the upstream-only knobs that haven't been ported yet (peer-mode
/// retries, inline bot mode, newsletter/broadcast servers, LID migration
/// fetch, debug timing instrumentation).
///
/// On success the returned [`SendResponse`] reports the server's `t`
/// timestamp, the message ID, the optional `server_id` (currently only
/// emitted for newsletter sends — None otherwise), and a
/// [`SendDebugTimings`] populated with zeros until each phase is wired up.
pub async fn send_message(
    client: &Client,
    to: &Jid,
    msg: Message,
) -> Result<SendResponse, ClientError> {
    if !client.is_connected() {
        return Err(ClientError::NotConnected);
    }

    // 1. Generate the message ID upfront — we need it both for the wire
    //    envelope and for the ack-waiter installed below.
    let message_id = generate_message_id(client);

    // 2. Marshal the proto body once.
    let mut plaintext = Vec::with_capacity(64);
    prost::Message::encode(&msg, &mut plaintext)
        .map_err(|e| ClientError::Proto(e.to_string()))?;

    // 3. Encrypt: branch on group vs DM. The siblings already return the
    //    `<enc>` child Nodes shaped for the wire envelope.
    let enc_children: Vec<Node> = if to.is_group() {
        let group = crate::send_group::encrypt_for_group(client, to, &plaintext).await?;
        let mut children: Vec<Node> = group
            .distribution_nodes
            .into_iter()
            .map(|(_, node)| node)
            .collect();
        children.push(group.group_message_node);
        children
    } else {
        crate::send_encrypt::encrypt_for_recipient(client, to, &plaintext).await?
    };

    // 4. Build envelope and install ack waiter before sending so we can't
    //    miss a fast ack.
    let msg_type = message_type_attr(&msg);
    let envelope = build_message_envelope(&message_id, to, msg_type, enc_children);

    let (tx, rx) = oneshot::channel();
    client.install_waiter(message_id.clone(), tx);

    // 5. Send.
    if let Err(e) = client.send_node(&envelope).await {
        // The waiter is now orphaned — best-effort cleanup not exposed by
        // the Client API yet; the channel will simply drop unfulfilled.
        return Err(e);
    }

    // 6. Wait for the `<ack>` (or any node the dispatcher routes to our
    //    waiter) up to the default request timeout.
    let ack = match tokio::time::timeout(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT.as_secs()), rx).await {
        Ok(Ok(node)) => node,
        Ok(Err(_)) => return Err(ClientError::IqDisconnected),
        Err(_) => return Err(ClientError::IqTimedOut),
    };

    // 7. Decode the ack attrs into a SendResponse.
    let mut ag = ack.attr_getter();
    let timestamp = ag.optional_i64("t").unwrap_or(0);
    let server_id = ag.optional_i64("server_id");

    Ok(SendResponse {
        timestamp,
        message_id,
        server_id,
        debug_timings: SendDebugTimings::default(),
    })
}

/// Result of a successful send. Mirrors upstream's `SendResponse` (minus
/// the `Sender` field — the orchestrator can derive it from the device).
#[derive(Debug, Clone)]
pub struct SendResponse {
    pub timestamp: i64,
    pub message_id: String,
    pub server_id: Option<i64>,
    pub debug_timings: SendDebugTimings,
}

impl SendResponse {
    /// Convenience constructor for callers that synthesise a response
    /// (tests, retry paths) without going through the full send pipeline.
    pub fn new(message_id: impl Into<String>, timestamp: i64) -> Self {
        Self {
            timestamp,
            message_id: message_id.into(),
            server_id: None,
            debug_timings: SendDebugTimings::default(),
        }
    }
}

/// Per-phase timings, used purely for debug logging upstream. We keep the
/// field shape so callers can swap the `Default::default()` in
/// [`SendResponse`] for instrumented values without breaking the public API.
#[derive(Debug, Clone, Default)]
pub struct SendDebugTimings {
    pub queue_us: u64,
    pub fanout_us: u64,
    pub encrypt_us: u64,
    pub network_us: u64,
}

impl SendDebugTimings {
    pub fn zero() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use wha_store::MemoryStore;

    #[test]
    fn build_message_envelope_has_required_attrs() {
        let to: Jid = "1234@s.whatsapp.net".parse().unwrap();
        let env = build_message_envelope("3EB0DEADBEEF", &to, "text", vec![]);
        assert_eq!(env.tag, "message");
        assert_eq!(env.get_attr_str("id"), Some("3EB0DEADBEEF"));
        assert_eq!(env.get_attr_str("type"), Some("text"));
        assert_eq!(env.get_attr_jid("to"), Some(&to));
        // `t` must be present and parse as a non-zero unix timestamp.
        let t_str = env.get_attr_str("t").expect("t attr present");
        let t: i64 = t_str.parse().expect("t parses as integer");
        assert!(t > 0, "t should be a recent unix timestamp, got {t}");
    }

    #[test]
    fn build_message_envelope_carries_enc_children() {
        let to: Jid = "1234@s.whatsapp.net".parse().unwrap();
        let mut enc_attrs = Attrs::new();
        enc_attrs.insert("v".into(), Value::String("2".into()));
        enc_attrs.insert("type".into(), Value::String("msg".into()));
        let enc1 = Node::new("enc", enc_attrs.clone(), Some(Value::Bytes(vec![1, 2, 3])));
        let enc2 = Node::new("enc", enc_attrs, Some(Value::Bytes(vec![4, 5, 6])));
        let env = build_message_envelope("3EB0AAAA", &to, "text", vec![enc1, enc2]);
        let kids = env.children();
        assert_eq!(kids.len(), 2);
        assert!(kids.iter().all(|c| c.tag == "enc"));
        assert_eq!(kids[0].content.as_bytes(), Some(&[1u8, 2, 3][..]));
        assert_eq!(kids[1].content.as_bytes(), Some(&[4u8, 5, 6][..]));
    }

    #[tokio::test]
    async fn send_message_to_uninitialised_client_errors() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let to: Jid = "1234@s.whatsapp.net".parse().unwrap();
        let msg = Message::default();
        let r = send_message(&client, &to, msg).await;
        assert!(matches!(r, Err(ClientError::NotConnected)), "got {r:?}");
    }

    #[test]
    fn generate_message_id_has_prefix_and_length() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let id = generate_message_id(&client);
        assert!(id.starts_with(WEB_MESSAGE_ID_PREFIX));
        // 4-char prefix + 9 bytes hex (= 18 chars) = 22 chars total.
        assert_eq!(id.len(), WEB_MESSAGE_ID_PREFIX.len() + 18);
        // The hex portion must be uppercase hex.
        assert!(id[WEB_MESSAGE_ID_PREFIX.len()..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && (!c.is_alphabetic() || c.is_uppercase())));
    }

    #[test]
    fn generate_message_id_is_unique() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let a = generate_message_id(&client);
        let b = generate_message_id(&client);
        assert_ne!(a, b);
    }
}
