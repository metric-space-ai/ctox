//! Outgoing/incoming presence, chat-presence, receipt, stream-error and
//! keepalive support — port of:
//!
//! * `whatsmeow/presence.go`
//! * `whatsmeow/receipt.go`
//! * `whatsmeow/connectionevents.go`
//! * `whatsmeow/keepalive.go`
//!
//! The orchestrator wires the parser stubs and the keepalive loop into the
//! main read pump; this module only defines the data types, builders and
//! free-standing parsers, plus the [`Client`] methods that need socket access.
//!
//! Receipts are CRITICAL for delivery reporting. The wire field names match
//! upstream verbatim:
//!
//! * `<receipt id=… type=…|read|read-self|sender|… to=… participant=… t=…>`
//! * `<presence type=available|unavailable name=…/>`
//! * `<chatstate from=… to=…><composing media=audio/></chatstate>`

use std::time::Duration;

use rand::Rng;
use tokio::time::sleep;
use tracing::{debug, warn};

use wha_binary::{Attrs, Node, Value};
use wha_types::jid::server;
use wha_types::{Jid, MessageId};

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

// ---------------------------------------------------------------------------
// Wire enums (mirror of `whatsmeow/types/presence.go`).
// ---------------------------------------------------------------------------

/// Top-level "online/offline" presence — `<presence type="available"/>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceState {
    Available,
    Unavailable,
}

impl PresenceState {
    pub fn as_str(self) -> &'static str {
        match self {
            PresenceState::Available => "available",
            PresenceState::Unavailable => "unavailable",
        }
    }
}

/// "Typing" notification within a chat — `<chatstate><composing/></chatstate>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatPresence {
    Composing,
    Paused,
}

impl ChatPresence {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatPresence::Composing => "composing",
            ChatPresence::Paused => "paused",
        }
    }
}

/// Type of media being recorded while `Composing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatPresenceMedia {
    /// Default — sent without an explicit `media` attribute.
    Text,
    Audio,
}

impl ChatPresenceMedia {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatPresenceMedia::Text => "",
            ChatPresenceMedia::Audio => "audio",
        }
    }
}

/// Mirror of `types.ReceiptType` — see `whatsmeow/types/presence.go`.
///
/// The default empty string ("delivered") is represented by `Delivered`. The
/// rest match the upstream constants verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptType {
    /// Empty string — message was delivered to the device.
    Delivered,
    /// `"sender"` — your other devices got a copy of an outgoing message.
    Sender,
    /// `"retry"` — decryption failed; please resend.
    Retry,
    /// `"read"` — the user opened the chat and saw the message.
    Read,
    /// `"read-self"` — read on another device with read receipts disabled.
    ReadSelf,
    /// `"played"` — view-once media opened.
    Played,
    /// `"played-self"` — view-once opened on another device.
    PlayedSelf,
    /// `"server-error"` — server failed to deliver.
    ServerError,
    /// `"inactive"` — passive delivery receipt.
    Inactive,
    /// `"peer_msg"` — peer message ack.
    PeerMsg,
    /// `"hist_sync"` — history sync notification.
    HistorySync,
    /// Anything we don't have a constant for.
    Other(String),
}

impl ReceiptType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "" => ReceiptType::Delivered,
            "sender" => ReceiptType::Sender,
            "retry" => ReceiptType::Retry,
            "read" => ReceiptType::Read,
            "read-self" => ReceiptType::ReadSelf,
            "played" => ReceiptType::Played,
            "played-self" => ReceiptType::PlayedSelf,
            "server-error" => ReceiptType::ServerError,
            "inactive" => ReceiptType::Inactive,
            "peer_msg" => ReceiptType::PeerMsg,
            "hist_sync" => ReceiptType::HistorySync,
            other => ReceiptType::Other(other.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ReceiptType::Delivered => "",
            ReceiptType::Sender => "sender",
            ReceiptType::Retry => "retry",
            ReceiptType::Read => "read",
            ReceiptType::ReadSelf => "read-self",
            ReceiptType::Played => "played",
            ReceiptType::PlayedSelf => "played-self",
            ReceiptType::ServerError => "server-error",
            ReceiptType::Inactive => "inactive",
            ReceiptType::PeerMsg => "peer_msg",
            ReceiptType::HistorySync => "hist_sync",
            ReceiptType::Other(s) => s.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// Inbound parsed events.
// ---------------------------------------------------------------------------

/// Parsed `<receipt>` node — mirrors `events.Receipt`.
#[derive(Debug, Clone)]
pub struct ReceiptEvent {
    /// `from` attribute — who emitted the receipt.
    pub from: Jid,
    /// `recipient` attribute — set when the message was sent on behalf of
    /// another device.
    pub recipient: Option<Jid>,
    /// `participant` attribute — set on group receipts.
    pub participant: Option<Jid>,
    /// Receipt timestamp (`t` attribute, seconds since epoch).
    pub timestamp: i64,
    /// Receipt classification.
    pub receipt_type: ReceiptType,
    /// All message IDs the receipt covers (always non-empty for non-grouped
    /// receipts; the first ID is the `id` attribute on the root node).
    pub message_ids: Vec<MessageId>,
}

/// Parsed `<stream:error>` node — mirrors the input of upstream's
/// `handleStreamError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamErrorEvent {
    pub code: String,
    /// `<conflict type="…"/>` if present — e.g. "device_removed", "replaced".
    pub conflict_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Outgoing builders + Client methods.
// ---------------------------------------------------------------------------

/// Build a `<presence>` node. Pulled out of [`Client::send_presence`] so unit
/// tests can introspect the shape without a live socket.
pub fn build_presence_node(state: PresenceState, push_name: Option<&str>) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("type".into(), Value::String(state.as_str().to_owned()));
    if let Some(name) = push_name {
        if !name.is_empty() {
            attrs.insert("name".into(), Value::String(name.to_owned()));
        }
    }
    Node::new("presence", attrs, None)
}

/// Build a `<chatstate>` node.
pub fn build_chat_presence_node(
    own_jid: &Jid,
    target: &Jid,
    state: ChatPresence,
    media: ChatPresenceMedia,
) -> Node {
    let mut inner_attrs = Attrs::new();
    if state == ChatPresence::Composing && media == ChatPresenceMedia::Audio {
        inner_attrs.insert("media".into(), Value::String("audio".into()));
    }
    let inner = Node::new(state.as_str().to_owned(), inner_attrs, None);

    let mut attrs = Attrs::new();
    attrs.insert("from".into(), Value::Jid(own_jid.clone()));
    attrs.insert("to".into(), Value::Jid(target.clone()));
    Node::new("chatstate", attrs, Some(Value::Nodes(vec![inner])))
}

/// Build a `<receipt>` ack — mirrors `sendMessageReceipt` upstream.
///
/// `participant` and `recipient` are only attached when present; `t` defaults
/// to the current unix timestamp when callers pass `None`.
pub fn build_message_receipt_node(
    id: &str,
    to: &Jid,
    participant: Option<&Jid>,
    recipient: Option<&Jid>,
    receipt_type: &ReceiptType,
    timestamp: Option<i64>,
) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("id".into(), Value::String(id.to_owned()));
    attrs.insert("to".into(), Value::Jid(to.clone()));
    if let Some(p) = participant {
        attrs.insert("participant".into(), Value::Jid(p.clone()));
    }
    if let Some(r) = recipient {
        attrs.insert("recipient".into(), Value::Jid(r.clone()));
    }
    let typ = receipt_type.as_str();
    if !typ.is_empty() {
        attrs.insert("type".into(), Value::String(typ.to_owned()));
    }
    if let Some(t) = timestamp {
        attrs.insert("t".into(), Value::String(t.to_string()));
    }
    Node::new("receipt", attrs, None)
}

/// Build a `<receipt>` for `MarkRead` / `MarkPlayed` — mirrors `MarkRead` in
/// `whatsmeow/receipt.go`.
///
/// * `chat` is the owning chat (DM user JID or group JID).
/// * `sender` must be set on group receipts (the user who sent the message);
///   it is ignored on direct-message servers (`s.whatsapp.net`, `lid`,
///   `msgr`).
/// * If `message_ids` has more than one element, additional ids go into a
///   single `<list><item id="..."/></list>` child node.
///
/// Panics if `message_ids` is empty.
pub fn build_mark_read_receipt(
    message_ids: &[MessageId],
    timestamp: i64,
    chat: &Jid,
    sender: Option<&Jid>,
    receipt_type: &ReceiptType,
) -> Node {
    assert!(!message_ids.is_empty(), "no message IDs specified");
    let mut attrs = Attrs::new();
    attrs.insert("id".into(), Value::String(message_ids[0].clone()));
    attrs.insert("type".into(), Value::String(receipt_type.as_str().to_owned()));
    attrs.insert("to".into(), Value::Jid(chat.clone()));
    attrs.insert("t".into(), Value::String(timestamp.to_string()));

    // `participant` is only attached for group / broadcast / newsletter
    // chats — the upstream guard skips DM-style servers.
    let chat_server = chat.server.as_str();
    let dm_like = matches!(
        chat_server,
        server::DEFAULT_USER | server::HIDDEN_USER | server::MESSENGER
    );
    if let Some(sender) = sender {
        if !sender.is_empty() && !dm_like {
            attrs.insert("participant".into(), Value::Jid(sender.to_non_ad()));
        }
    }

    let content = if message_ids.len() > 1 {
        let items: Vec<Node> = message_ids[1..]
            .iter()
            .map(|id| {
                let mut a = Attrs::new();
                a.insert("id".into(), Value::String(id.clone()));
                Node::new("item", a, None)
            })
            .collect();
        let list = Node::new("list", Attrs::new(), Some(Value::Nodes(items)));
        Some(Value::Nodes(vec![list]))
    } else {
        None
    };

    Node::new("receipt", attrs, content)
}

/// Build a `<presence type="subscribe" to="..."/>` node — mirrors the body of
/// `SubscribePresence` upstream.
///
/// If `privacy_token` is `Some`, a `<tctoken>` child carrying the token bytes
/// is attached. Mirrors upstream's behaviour where the token (when present in
/// the privacy-token store) is included so the server will deliver presence
/// updates for the contact. Pass `None` when no token is on file.
pub fn build_subscribe_presence_node(jid: &Jid, privacy_token: Option<&[u8]>) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("type".into(), Value::String("subscribe".into()));
    attrs.insert("to".into(), Value::Jid(jid.clone()));
    let content = privacy_token.map(|tok| {
        Value::Nodes(vec![Node::new(
            "tctoken",
            Attrs::new(),
            Some(Value::Bytes(tok.to_vec())),
        )])
    });
    Node::new("presence", attrs, content)
}

impl Client {
    /// Update the user's presence status — `<presence type="available"/>` etc.
    /// Equivalent to `Client.SendPresence` upstream. The `name` attribute is
    /// pulled from the device's pushname when available (whatsmeow refuses to
    /// send without one for non-Messenger configs).
    pub async fn send_presence(&self, state: PresenceState) -> Result<(), ClientError> {
        let push_name = if self.device.push_name.is_empty() {
            None
        } else {
            Some(self.device.push_name.as_str())
        };
        let node = build_presence_node(state, push_name);
        self.send_node(&node).await
    }

    /// Send a `<chatstate>` typing indicator. Requires the device to be logged
    /// in (have a JID) — mirrors upstream's `ErrNotLoggedIn` check.
    pub async fn send_chat_presence(
        &self,
        jid: &Jid,
        state: ChatPresence,
        media: ChatPresenceMedia,
    ) -> Result<(), ClientError> {
        let own = self.device.id.as_ref().ok_or(ClientError::NotLoggedIn)?;
        let node = build_chat_presence_node(own, jid, state, media);
        self.send_node(&node).await
    }

    /// Send a `<receipt>` ack for a single message. The `to`/`participant`
    /// attributes should mirror what was on the inbound `<message>` node.
    pub async fn send_message_receipt(
        &self,
        id: &str,
        to: &Jid,
        participant: Option<&Jid>,
        recipient: Option<&Jid>,
        receipt_type: ReceiptType,
        timestamp: Option<i64>,
    ) -> Result<(), ClientError> {
        let node = build_message_receipt_node(id, to, participant, recipient, &receipt_type, timestamp);
        self.send_node(&node).await
    }

    /// Subscribe to presence updates from `jid` — mirrors
    /// `SubscribePresence` in `whatsmeow/presence.go`. Sends
    /// `<presence type="subscribe" to="<jid>"/>`, attaching the stored
    /// privacy `<tctoken>` if one is on file. Without the token the server
    /// may silently refuse to deliver presence updates from the contact.
    pub async fn subscribe_presence(&self, jid: &Jid) -> Result<(), ClientError> {
        // Look up via the device's PrivacyTokenStore — upstream caches the
        // value via `cli.Store.PrivacyTokens.GetPrivacyToken`. A missing
        // token is non-fatal; we just send the bare `<presence>` node.
        let token_bytes = match self.device.privacy_tokens.get_privacy_token(jid).await {
            Ok(Some((tok, _ts))) => Some(tok),
            Ok(None) => None,
            Err(e) => {
                warn!(?e, %jid, "privacy token lookup failed; sending bare subscribe");
                None
            }
        };
        let node = build_subscribe_presence_node(jid, token_bytes.as_deref());
        self.send_node(&node).await
    }
}

/// Mark up to N messages as read — mirrors `Client.MarkRead` upstream.
///
/// `chat` is the owning chat (user JID for DMs, group JID for groups). For
/// group chats `sender` MUST be the participant who sent the message; for DMs
/// it can be `None` (or `Some(_)`, the helper drops it on DM servers per
/// upstream).
///
/// All message IDs in one call must come from the same sender — the wire
/// format only carries one `participant` attribute, so callers that need to
/// ack messages from different senders must invoke `mark_read` once per
/// sender.
pub async fn mark_read(
    client: &Client,
    message_ids: Vec<String>,
    timestamp: i64,
    chat: &Jid,
    sender: Option<&Jid>,
) -> Result<(), ClientError> {
    if message_ids.is_empty() {
        return Err(ClientError::Malformed(
            "no message IDs specified".into(),
        ));
    }
    let node = build_mark_read_receipt(&message_ids, timestamp, chat, sender, &ReceiptType::Read);
    client.send_node(&node).await
}

/// Mark a voice / view-once message as played — mirrors
/// `Client.MarkRead(ctx, ids, t, chat, sender, ReceiptTypePlayed)` upstream.
///
/// Sends `<receipt id="<msg_id>" type="played" to="<chat>" t="<now>"
/// [participant="<sender>"]/>`.
pub async fn send_played_receipt(
    client: &Client,
    message_id: &str,
    chat: &Jid,
    sender: &Jid,
) -> Result<(), ClientError> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let ids = vec![message_id.to_owned()];
    let node = build_mark_read_receipt(
        &ids,
        timestamp,
        chat,
        Some(sender),
        &ReceiptType::Played,
    );
    client.send_node(&node).await
}

/// Subscribe to presence updates from `jid` — free-function mirror of
/// `Client::subscribe_presence`. Mirrors `SubscribePresence` upstream.
pub async fn subscribe_presence(client: &Client, jid: &Jid) -> Result<(), ClientError> {
    client.subscribe_presence(jid).await
}

/// Send a chat-presence `<chatstate>` — free-function mirror of
/// `Client::send_chat_presence`.
pub async fn send_chat_presence(
    client: &Client,
    jid: &Jid,
    state: ChatPresence,
    media: ChatPresenceMedia,
) -> Result<(), ClientError> {
    client.send_chat_presence(jid, state, media).await
}

// ---------------------------------------------------------------------------
// Inbound parsers.
// ---------------------------------------------------------------------------

/// Parse a `<receipt>` node. Uses `AttrUtility`-style error accumulation so
/// callers that want to log structured errors can introspect the bag.
pub fn parse_receipt(node: &Node) -> Result<ReceiptEvent, ClientError> {
    if node.tag != "receipt" {
        return Err(ClientError::Malformed(format!(
            "expected <receipt>, got <{}>",
            node.tag
        )));
    }

    let mut ag = node.attr_getter();
    let from = ag.jid("from");
    let id = ag.string("id").to_owned();
    let timestamp = ag.optional_i64("t").unwrap_or(0);
    let receipt_type_str = ag.optional_string("type").unwrap_or("").to_owned();
    let recipient = ag.optional_jid("recipient").cloned();
    let participant = ag.optional_jid("participant").cloned();

    if !ag.ok() {
        let errs = ag.into_result().err().unwrap_or_default();
        return Err(ClientError::Malformed(format!(
            "failed to parse receipt attrs: {errs:?}"
        )));
    }

    // Collect message IDs. The root `id` attribute is always the first; if the
    // node carries a `<list>` child each `<item id=...>` adds another.
    let mut message_ids = Vec::with_capacity(1);
    if !id.is_empty() {
        message_ids.push(id);
    }
    if let Some(list) = node.children().iter().find(|c| c.tag == "list") {
        for item in list.children() {
            if item.tag == "item" {
                if let Some(item_id) = item.get_attr_str("id") {
                    message_ids.push(item_id.to_owned());
                }
            }
        }
    }

    Ok(ReceiptEvent {
        from,
        recipient,
        participant,
        timestamp,
        receipt_type: ReceiptType::from_str(&receipt_type_str),
        message_ids,
    })
}

/// Parse a `<stream:error>` node — extract the top-level code and any
/// `<conflict type=…/>` child.
pub fn parse_stream_error(node: &Node) -> StreamErrorEvent {
    let code = node.get_attr_str("code").unwrap_or("").to_owned();
    let conflict_type = node
        .children()
        .iter()
        .find(|c| c.tag == "conflict")
        .and_then(|c| c.get_attr_str("type"))
        .map(|s| s.to_owned());
    StreamErrorEvent { code, conflict_type }
}

// ---------------------------------------------------------------------------
// Connection events: `<success>` and `<failure>` stub handlers.
//
// The full upstream handlers do prekey upload, LID save, etc. Here we just
// surface the connect/disconnect signals to the orchestrator via the existing
// `Event` channel — the heavy lifting is wired in the orchestrator pass.
// ---------------------------------------------------------------------------

/// Decision returned by `handle_connection_node` so the caller knows what to
/// do next (the actual `Event::Connected`/`Disconnected` dispatch lives on
/// `Client`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionNodeKind {
    Success,
    Failure { reason: String },
    Other,
}

/// Stub handler — the orchestrator will replace this with the rich post-connect
/// flow (prekey upload, SetPassive, LID save, …). For now we just classify.
pub fn classify_connection_node(node: &Node) -> ConnectionNodeKind {
    match node.tag.as_str() {
        "success" => ConnectionNodeKind::Success,
        "failure" => {
            let reason = node
                .get_attr_str("reason")
                .map(|s| s.to_owned())
                .unwrap_or_default();
            ConnectionNodeKind::Failure { reason }
        }
        _ => ConnectionNodeKind::Other,
    }
}

// ---------------------------------------------------------------------------
// Keepalive.
// ---------------------------------------------------------------------------

/// Minimum interval between keepalive pings — mirrors upstream
/// `KeepAliveIntervalMin`.
pub const KEEPALIVE_INTERVAL_MIN: Duration = Duration::from_secs(20);
/// Maximum interval — mirrors upstream `KeepAliveIntervalMax`. The actual
/// delay is uniformly drawn from the `[min, max]` range each iteration.
pub const KEEPALIVE_INTERVAL_MAX: Duration = Duration::from_secs(30);
/// Maximum response wait before declaring a keepalive timeout.
pub const KEEPALIVE_RESPONSE_DEADLINE: Duration = Duration::from_secs(10);
/// Maximum cumulative failure window before reporting a disconnect.
pub const KEEPALIVE_MAX_FAIL_TIME: Duration = Duration::from_secs(180);

fn pick_interval() -> Duration {
    let min_ms = KEEPALIVE_INTERVAL_MIN.as_millis() as u64;
    let max_ms = KEEPALIVE_INTERVAL_MAX.as_millis() as u64;
    let span = max_ms.saturating_sub(min_ms);
    let mut rng = rand::thread_rng();
    let extra: u64 = if span == 0 { 0 } else { rng.gen_range(0..=span) };
    Duration::from_millis(min_ms + extra)
}

async fn send_keepalive_once(client: &Client) -> Result<(), ClientError> {
    let q = InfoQuery {
        namespace: "w:p".into(),
        iq_type: IqType::Get,
        to: Some(Jid::new("", wha_types::jid::server::DEFAULT_USER)),
        target: None,
        id: None,
        content: None,
        timeout: Some(KEEPALIVE_RESPONSE_DEADLINE),
        no_retry: true,
    };
    let _ = client.send_iq(q).await?;
    Ok(())
}

/// Background task that sends a `<iq xmlns="urn:xmpp:ping" type="get"/>` every
/// 20–30 seconds (same cadence as upstream). Reports `Err(ClientError)` when
/// pings have failed for longer than [`KEEPALIVE_MAX_FAIL_TIME`] so the caller
/// can trigger a reconnect.
pub async fn keepalive_loop(client: &Client) -> Result<(), ClientError> {
    let mut last_success = std::time::Instant::now();
    let mut error_count: u32 = 0;
    loop {
        sleep(pick_interval()).await;
        if !client.is_connected() {
            return Err(ClientError::NotConnected);
        }
        match send_keepalive_once(client).await {
            Ok(_) => {
                if error_count > 0 {
                    debug!(error_count, "keepalive restored");
                }
                error_count = 0;
                last_success = std::time::Instant::now();
            }
            Err(e) => {
                error_count = error_count.saturating_add(1);
                warn!(?e, error_count, "keepalive failed");
                if last_success.elapsed() > KEEPALIVE_MAX_FAIL_TIME {
                    return Err(ClientError::IqTimedOut);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use wha_binary::{marshal, unmarshal};
    use wha_types::jid::server;

    #[test]
    fn presence_available_round_trip() {
        let n = build_presence_node(PresenceState::Available, Some("Alice"));
        assert_eq!(n.tag, "presence");
        assert_eq!(n.get_attr_str("type"), Some("available"));
        assert_eq!(n.get_attr_str("name"), Some("Alice"));

        // Round-trip through the binary codec.
        let bytes = marshal(&n).expect("marshal");
        let back = unmarshal(&bytes).expect("unmarshal");
        assert_eq!(back.tag, "presence");
        assert_eq!(back.get_attr_str("type"), Some("available"));
        assert_eq!(back.get_attr_str("name"), Some("Alice"));
    }

    #[test]
    fn presence_unavailable_omits_name_when_blank() {
        let n = build_presence_node(PresenceState::Unavailable, None);
        assert_eq!(n.get_attr_str("type"), Some("unavailable"));
        assert!(n.get_attr_str("name").is_none());
    }

    #[test]
    fn chat_presence_composing_audio_carries_media_attr() {
        let me = Jid::new("111", server::DEFAULT_USER);
        let to = Jid::new("222", server::DEFAULT_USER);
        let n = build_chat_presence_node(&me, &to, ChatPresence::Composing, ChatPresenceMedia::Audio);
        assert_eq!(n.tag, "chatstate");
        let kid = &n.children()[0];
        assert_eq!(kid.tag, "composing");
        assert_eq!(kid.get_attr_str("media"), Some("audio"));
    }

    #[test]
    fn chat_presence_paused_drops_media_attr() {
        let me = Jid::new("111", server::DEFAULT_USER);
        let to = Jid::new("222", server::DEFAULT_USER);
        let n = build_chat_presence_node(&me, &to, ChatPresence::Paused, ChatPresenceMedia::Audio);
        let kid = &n.children()[0];
        assert_eq!(kid.tag, "paused");
        assert!(kid.get_attr_str("media").is_none());
    }

    #[test]
    fn parse_receipt_basic_read() {
        let mut attrs = Attrs::new();
        attrs.insert("from".into(), Value::Jid(Jid::new("123", server::DEFAULT_USER)));
        attrs.insert("id".into(), Value::String("MSG1".into()));
        attrs.insert("type".into(), Value::String("read".into()));
        attrs.insert("t".into(), Value::String("1714521600".into()));
        let node = Node::new("receipt", attrs, None);

        let evt = parse_receipt(&node).expect("parse");
        assert_eq!(evt.from.user, "123");
        assert_eq!(evt.timestamp, 1714521600);
        assert!(matches!(evt.receipt_type, ReceiptType::Read));
        assert_eq!(evt.message_ids, vec!["MSG1".to_string()]);
    }

    #[test]
    fn parse_receipt_with_list_collects_all_ids() {
        // <receipt id="MSG1" from="..."><list><item id="MSG2"/><item id="MSG3"/></list></receipt>
        let mut attrs = Attrs::new();
        attrs.insert("from".into(), Value::Jid(Jid::new("123", server::DEFAULT_USER)));
        attrs.insert("id".into(), Value::String("MSG1".into()));

        let mk_item = |id: &str| {
            let mut a = Attrs::new();
            a.insert("id".into(), Value::String(id.into()));
            Node::new("item", a, None)
        };
        let list = Node::new(
            "list",
            Attrs::new(),
            Some(Value::Nodes(vec![mk_item("MSG2"), mk_item("MSG3")])),
        );
        let node = Node::new("receipt", attrs, Some(Value::Nodes(vec![list])));

        let evt = parse_receipt(&node).expect("parse");
        assert_eq!(evt.message_ids, vec!["MSG1", "MSG2", "MSG3"]);
        assert!(matches!(evt.receipt_type, ReceiptType::Delivered));
    }

    #[test]
    fn build_message_receipt_carries_expected_attrs() {
        let to = Jid::new("123", server::DEFAULT_USER);
        let participant = Jid::new("789", server::DEFAULT_USER);
        let n = build_message_receipt_node(
            "MSG-ID-1",
            &to,
            Some(&participant),
            None,
            &ReceiptType::Sender,
            Some(1714521600),
        );
        assert_eq!(n.tag, "receipt");
        assert_eq!(n.get_attr_str("id"), Some("MSG-ID-1"));
        assert_eq!(n.get_attr_str("type"), Some("sender"));
        assert_eq!(n.get_attr_jid("to").unwrap().user, "123");
        assert_eq!(n.get_attr_jid("participant").unwrap().user, "789");
        assert_eq!(n.get_attr_str("t"), Some("1714521600"));
        assert!(n.get_attr_str("recipient").is_none());

        // Empty type (default delivered) should not emit the attribute.
        let n2 = build_message_receipt_node(
            "X",
            &to,
            None,
            None,
            &ReceiptType::Delivered,
            None,
        );
        assert!(n2.get_attr_str("type").is_none());
        assert!(n2.get_attr_str("t").is_none());
    }

    #[test]
    fn parse_stream_error_extracts_conflict_type() {
        // <stream:error code="401"><conflict type="device_removed"/></stream:error>
        let mut attrs = Attrs::new();
        attrs.insert("code".into(), Value::String("401".into()));

        let mut conflict_attrs = Attrs::new();
        conflict_attrs.insert("type".into(), Value::String("device_removed".into()));
        let conflict = Node::new("conflict", conflict_attrs, None);

        let node = Node::new("stream:error", attrs, Some(Value::Nodes(vec![conflict])));
        let evt = parse_stream_error(&node);
        assert_eq!(
            evt,
            StreamErrorEvent {
                code: "401".into(),
                conflict_type: Some("device_removed".into()),
            }
        );

        // Without a conflict child, code only.
        let mut attrs = Attrs::new();
        attrs.insert("code".into(), Value::String("515".into()));
        let bare = Node::new("stream:error", attrs, None);
        let evt = parse_stream_error(&bare);
        assert_eq!(evt.code, "515");
        assert!(evt.conflict_type.is_none());
    }

    #[test]
    fn classify_connection_node_dispatches_correctly() {
        assert_eq!(
            classify_connection_node(&Node::tag_only("success")),
            ConnectionNodeKind::Success
        );
        let mut a = Attrs::new();
        a.insert("reason".into(), Value::String("405".into()));
        let f = Node::new("failure", a, None);
        assert_eq!(
            classify_connection_node(&f),
            ConnectionNodeKind::Failure { reason: "405".into() }
        );
        assert_eq!(
            classify_connection_node(&Node::tag_only("iq")),
            ConnectionNodeKind::Other
        );
    }

    #[test]
    fn pick_interval_within_bounds() {
        for _ in 0..50 {
            let d = pick_interval();
            assert!(d >= KEEPALIVE_INTERVAL_MIN, "{:?} too short", d);
            assert!(d <= KEEPALIVE_INTERVAL_MAX, "{:?} too long", d);
        }
    }

    // -------- mark_read / played -------------------------------------------

    #[test]
    fn mark_read_dm_single_id_drops_participant() {
        // <receipt id="MSG1" type="read" to="123@s.whatsapp.net" t="42"/>
        // — DM server, even if `sender` is supplied it must be skipped.
        let chat = Jid::new("123", server::DEFAULT_USER);
        let sender = Jid::new("999", server::DEFAULT_USER);
        let n = build_mark_read_receipt(
            &["MSG1".to_string()],
            42,
            &chat,
            Some(&sender),
            &ReceiptType::Read,
        );
        assert_eq!(n.tag, "receipt");
        assert_eq!(n.get_attr_str("id"), Some("MSG1"));
        assert_eq!(n.get_attr_str("type"), Some("read"));
        assert_eq!(n.get_attr_str("t"), Some("42"));
        assert_eq!(n.get_attr_jid("to").unwrap().user, "123");
        // DM server → no participant attr.
        assert!(n.get_attr_str("participant").is_none());
        assert!(n.get_attr_jid("participant").is_none());
        // No <list> child for a single id.
        assert_eq!(n.children().len(), 0);
    }

    #[test]
    fn mark_read_group_batch_emits_list_and_participant() {
        // <receipt id="A" type="read" to="120-123@g.us" t="100"
        //          participant="999@s.whatsapp.net">
        //   <list><item id="B"/><item id="C"/></list>
        // </receipt>
        let chat = Jid::new("120-123", server::GROUP);
        let sender = Jid::new_ad("999", 0, 5); // AD JID — must be stripped.
        let ids = vec!["A".into(), "B".into(), "C".into()];
        let n = build_mark_read_receipt(&ids, 100, &chat, Some(&sender), &ReceiptType::Read);

        assert_eq!(n.get_attr_str("id"), Some("A"));
        assert_eq!(n.get_attr_str("type"), Some("read"));
        assert_eq!(n.get_attr_str("t"), Some("100"));
        let to = n.get_attr_jid("to").unwrap();
        assert_eq!(to.user, "120-123");
        assert_eq!(to.server, server::GROUP);
        let part = n.get_attr_jid("participant").expect("participant");
        // ToNonAD: agent + device dropped.
        assert_eq!(part.user, "999");
        assert_eq!(part.server, server::DEFAULT_USER);
        assert_eq!(part.device, 0);
        assert_eq!(part.raw_agent, 0);

        // <list><item id="B"/><item id="C"/></list>
        let kids = n.children();
        assert_eq!(kids.len(), 1);
        let list = &kids[0];
        assert_eq!(list.tag, "list");
        let items = list.children();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].tag, "item");
        assert_eq!(items[0].get_attr_str("id"), Some("B"));
        assert_eq!(items[1].tag, "item");
        assert_eq!(items[1].get_attr_str("id"), Some("C"));
    }

    #[test]
    fn played_receipt_emits_played_type() {
        let chat = Jid::new("120-555", server::GROUP);
        let sender = Jid::new("321", server::DEFAULT_USER);
        let n = build_mark_read_receipt(
            &["VOICE-1".into()],
            7,
            &chat,
            Some(&sender),
            &ReceiptType::Played,
        );
        assert_eq!(n.get_attr_str("type"), Some("played"));
        assert_eq!(n.get_attr_str("id"), Some("VOICE-1"));
        assert_eq!(n.get_attr_jid("participant").unwrap().user, "321");
    }

    // -------- subscribe presence -------------------------------------------

    #[test]
    fn subscribe_presence_node_shape() {
        let to = Jid::new("123", server::DEFAULT_USER);
        let n = build_subscribe_presence_node(&to, None);
        assert_eq!(n.tag, "presence");
        assert_eq!(n.get_attr_str("type"), Some("subscribe"));
        let to_j = n.get_attr_jid("to").unwrap();
        assert_eq!(to_j.user, "123");
        assert_eq!(to_j.server, server::DEFAULT_USER);
        assert!(n.children().is_empty());
    }

    #[test]
    fn subscribe_presence_round_trips_through_codec() {
        let to = Jid::new("777", server::DEFAULT_USER);
        let n = build_subscribe_presence_node(&to, None);
        let bytes = marshal(&n).expect("marshal");
        let back = unmarshal(&bytes).expect("unmarshal");
        assert_eq!(back.tag, "presence");
        assert_eq!(back.get_attr_str("type"), Some("subscribe"));
        assert_eq!(back.get_attr_jid("to").unwrap().user, "777");
    }

    /// When a privacy token is supplied, the `<presence>` node carries a
    /// `<tctoken>` child with the raw token bytes — mirrors upstream's
    /// `SubscribePresence` body building.
    #[test]
    fn subscribe_presence_with_token_attaches_tctoken_child() {
        let to = Jid::new("999", server::DEFAULT_USER);
        let token = b"\x01\x02\x03\x04\x05".to_vec();
        let n = build_subscribe_presence_node(&to, Some(&token));
        assert_eq!(n.tag, "presence");
        assert_eq!(n.get_attr_str("type"), Some("subscribe"));
        let kids = n.children();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].tag, "tctoken");
        assert_eq!(kids[0].content.as_bytes(), Some(token.as_slice()));

        // Round-trips through the binary codec — pre-existing wire test.
        let bytes = marshal(&n).expect("marshal");
        let back = unmarshal(&bytes).expect("unmarshal");
        assert_eq!(back.tag, "presence");
        let kids = back.children();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].tag, "tctoken");
        assert_eq!(kids[0].content.as_bytes(), Some(token.as_slice()));
    }
}
