//! Newsletter (WhatsApp Channels) support — port of `whatsmeow/newsletter.go`.
//!
//! Upstream uses two different XML namespaces for newsletter operations:
//!
//!   * `xmlns="newsletter"` — for the live-updates subscription, message
//!     fetching and message-update fetching IQs. These speak XML directly.
//!   * `xmlns="w:mex"`      — for the GraphQL/MEX-style mutations and queries
//!     that drive `getNewsletterInfo`, `FollowNewsletter`, `UnfollowNewsletter`,
//!     `CreateNewsletter`, etc. These wrap a `query_id` plus a small JSON
//!     body in a `<query>` child.
//!
//! This module ports the public-API surface that matters most:
//!
//!   * [`Client::get_newsletter_info_with_invite`] (uses `xmlns="newsletter"`
//!     with a `<live_updates>` query, matching the user-visible function the
//!     binding crate exposes).
//!   * [`Client::follow_newsletter`] / [`Client::unfollow_newsletter`] /
//!     [`Client::create_newsletter`] (use the MEX query namespace).
//!   * [`parse_newsletter_metadata`] for extracting [`NewsletterMetadata`] out
//!     of an XML `<newsletter>` node.
//!
//! The JSON payloads for the MEX mutations are small and fixed-shape, so we
//! synthesise them as bytes here without pulling in `serde_json` (which is not
//! a dependency of this crate). The IQ builders are factored out as
//! free-standing functions so the unit tests can introspect their attrs.

use rand::RngCore;

use wha_binary::{Attrs, Node, Value};
use wha_proto::common::MessageKey;
use wha_proto::e2e::{Message, ReactionMessage};
use wha_types::{jid::server, Jid};

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

/// Prefix on the public WhatsApp channel-link URL: `https://whatsapp.com/channel/<key>`.
pub const NEWSLETTER_LINK_PREFIX: &str = "https://whatsapp.com/channel/";

// ---------------------------------------------------------------------------
// MEX query/mutation IDs (mirror `whatsmeow/newsletter.go` constants).
// ---------------------------------------------------------------------------

pub const QUERY_FETCH_NEWSLETTER: &str = "6563316087068696";
pub const MUTATION_FOLLOW_NEWSLETTER: &str = "9926858900719341";
pub const MUTATION_UNFOLLOW_NEWSLETTER: &str = "6392786840836363";
pub const MUTATION_CREATE_NEWSLETTER: &str = "6234210096708695";

// ---------------------------------------------------------------------------
// Wire enums.
// ---------------------------------------------------------------------------

/// Top-level state of a newsletter — `<state type="active|suspended|geosuspended"/>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewsletterState {
    Active,
    Suspended,
    GeoSuspended,
    Other(String),
}

impl NewsletterState {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "active" => NewsletterState::Active,
            "suspended" => NewsletterState::Suspended,
            "geosuspended" => NewsletterState::GeoSuspended,
            other => NewsletterState::Other(other.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            NewsletterState::Active => "active",
            NewsletterState::Suspended => "suspended",
            NewsletterState::GeoSuspended => "geosuspended",
            NewsletterState::Other(s) => s.as_str(),
        }
    }
}

/// Verification badge — `<verification state="verified|unverified"/>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewsletterVerificationState {
    Verified,
    Unverified,
    Other(String),
}

impl NewsletterVerificationState {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "verified" => NewsletterVerificationState::Verified,
            "unverified" => NewsletterVerificationState::Unverified,
            other => NewsletterVerificationState::Other(other.to_owned()),
        }
    }
}

/// Indicator on the `key` value passed to `getNewsletterInfo` upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewsletterKeyType {
    Jid,
    Invite,
}

impl NewsletterKeyType {
    pub fn as_str(self) -> &'static str {
        match self {
            NewsletterKeyType::Jid => "JID",
            NewsletterKeyType::Invite => "INVITE",
        }
    }
}

// ---------------------------------------------------------------------------
// Metadata struct — minimal subset of upstream's `types.NewsletterMetadata`.
// ---------------------------------------------------------------------------

/// Metadata about a newsletter / WhatsApp channel.
///
/// This is the minimal port of upstream's `types.NewsletterMetadata`. Optional
/// fields are `None` when absent on the wire, matching upstream's "leave the
/// JSON field zero/null" semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsletterMetadata {
    /// Channel JID — `…@newsletter`.
    pub id: Jid,
    /// Top-level activity state.
    pub state: NewsletterState,
    /// Display name.
    pub name: String,
    /// Description (free text), if set.
    pub description: Option<String>,
    /// Picture URL, if the server returned one.
    pub picture_url: Option<String>,
    /// Owner / creator JID, if present.
    pub owner: Option<Jid>,
    /// Subscriber / view count, if reported.
    pub subscriber_count: Option<u64>,
    /// Verification state, if reported.
    pub verification: Option<NewsletterVerificationState>,
    /// Invite-link key, if reported (the `…` part of the channel URL).
    pub invite_code: Option<String>,
}

// ---------------------------------------------------------------------------
// IQ builders. Pulled out so the unit tests can introspect their attrs.
// ---------------------------------------------------------------------------

/// Build the JSON body for a `w:mex` query. Constructed by hand to avoid a
/// `serde_json` dependency — the shapes are small and fixed.
fn json_string_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Build the body of a MEX IQ:
/// `<iq xmlns="w:mex" type="get|set" to="s.whatsapp.net" id="…"><query query_id="…">{json}</query></iq>`.
fn build_mex_iq(query_id: &str, iq_type: IqType, variables_json: &str) -> InfoQuery {
    let payload = format!("{{\"variables\":{variables_json}}}");

    let mut query_attrs = Attrs::new();
    query_attrs.insert("query_id".into(), Value::String(query_id.to_owned()));
    let query_node = Node::new(
        "query",
        query_attrs,
        Some(Value::Bytes(payload.into_bytes())),
    );

    InfoQuery::new("w:mex", iq_type)
        .to(Jid::new("", server::DEFAULT_USER))
        .content(Value::Nodes(vec![query_node]))
}

/// Build the IQ used by [`Client::follow_newsletter`].
pub fn build_follow_newsletter_iq(jid: &Jid) -> InfoQuery {
    let body = format!(
        "{{\"newsletter_id\":{}}}",
        json_string_escape(&jid.to_string())
    );
    build_mex_iq(MUTATION_FOLLOW_NEWSLETTER, IqType::Get, &body)
}

/// Build the IQ used by [`Client::unfollow_newsletter`].
pub fn build_unfollow_newsletter_iq(jid: &Jid) -> InfoQuery {
    let body = format!(
        "{{\"newsletter_id\":{}}}",
        json_string_escape(&jid.to_string())
    );
    build_mex_iq(MUTATION_UNFOLLOW_NEWSLETTER, IqType::Get, &body)
}

/// Build the IQ used by [`Client::create_newsletter`].
pub fn build_create_newsletter_iq(name: &str, description: &str) -> InfoQuery {
    let mut input = format!("\"name\":{}", json_string_escape(name));
    if !description.is_empty() {
        input.push_str(",\"description\":");
        input.push_str(&json_string_escape(description));
    }
    let body = format!("{{\"newsletter_input\":{{{input}}}}}");
    build_mex_iq(MUTATION_CREATE_NEWSLETTER, IqType::Get, &body)
}

/// Build the IQ used by [`Client::get_newsletter_info_with_invite`]. Goes to
/// the `xmlns="newsletter"` namespace with a `<live_updates>` child carrying
/// the invite key (the `…` part of `https://whatsapp.com/channel/…`).
pub fn build_get_newsletter_info_with_invite_iq(invite_key: &str) -> InfoQuery {
    let mut attrs = Attrs::new();
    attrs.insert("key".into(), Value::String(invite_key.to_owned()));
    attrs.insert(
        "type".into(),
        Value::String(NewsletterKeyType::Invite.as_str().to_owned()),
    );
    let live_updates = Node::new("live_updates", attrs, None);

    InfoQuery::new("newsletter", IqType::Get)
        .to(Jid::new("", server::DEFAULT_USER))
        .content(Value::Nodes(vec![live_updates]))
}

// ---------------------------------------------------------------------------
// Parsers.
// ---------------------------------------------------------------------------

/// Parse a `<newsletter>` node into [`NewsletterMetadata`].
///
/// Tolerant of missing optional fields. The expected shape is:
///
/// ```xml
/// <newsletter id="<jid>">
///   <state type="active"/>
///   <thread_metadata>
///     <name>…</name>
///     <description>…</description>
///     <subscribers_count>1234</subscribers_count>
///     <invite>ABCD</invite>
///     <verification state="verified"/>
///     <picture url="https://…"/>
///     <owner jid="…@s.whatsapp.net"/>
///   </thread_metadata>
/// </newsletter>
/// ```
///
/// In practice the server may return name/description/etc. as text content of
/// the child element OR as a `text` attribute (matching upstream's
/// `NewsletterText` JSON shape). Both are accepted.
pub fn parse_newsletter_metadata(node: &Node) -> Result<NewsletterMetadata, ClientError> {
    if node.tag != "newsletter" {
        return Err(ClientError::Malformed(format!(
            "expected <newsletter>, got <{}>",
            node.tag
        )));
    }

    let mut ag = node.attr_getter();
    let id = ag.jid("id");
    if !ag.ok() {
        let errs = ag.into_result().err().unwrap_or_default();
        return Err(ClientError::Malformed(format!(
            "failed to parse newsletter attrs: {errs:?}"
        )));
    }

    // <state type="active"/>
    let state = node
        .child_by_tag(&["state"])
        .and_then(|s| s.get_attr_str("type").map(NewsletterState::from_str))
        .unwrap_or(NewsletterState::Other(String::new()));

    // <thread_metadata>...</thread_metadata>
    let meta = node.child_by_tag(&["thread_metadata"]);

    let extract_text = |tag: &str| -> Option<String> {
        let child = meta?.child_by_tag(&[tag])?;
        // Either as inline text content (Bytes) or a `text` attribute.
        if let Some(b) = child.content.as_bytes() {
            return Some(String::from_utf8_lossy(b).into_owned());
        }
        if let Value::String(s) = &child.content {
            return Some(s.clone());
        }
        child.get_attr_str("text").map(|s| s.to_owned())
    };

    let name = extract_text("name").unwrap_or_default();
    let description = extract_text("description").filter(|s| !s.is_empty());

    let invite_code = extract_text("invite").filter(|s| !s.is_empty());

    let subscriber_count = meta
        .and_then(|m| m.child_by_tag(&["subscribers_count"]))
        .and_then(|c| {
            if let Some(b) = c.content.as_bytes() {
                std::str::from_utf8(b).ok().and_then(|s| s.parse().ok())
            } else if let Value::String(s) = &c.content {
                s.parse().ok()
            } else {
                c.get_attr_str("count").and_then(|s| s.parse().ok())
            }
        });

    let verification = meta
        .and_then(|m| m.child_by_tag(&["verification"]))
        .and_then(|c| c.get_attr_str("state").map(NewsletterVerificationState::from_str));

    let picture_url = meta
        .and_then(|m| m.child_by_tag(&["picture"]))
        .and_then(|c| c.get_attr_str("url").map(|s| s.to_owned()));

    let owner = meta
        .and_then(|m| m.child_by_tag(&["owner"]))
        .and_then(|c| c.get_attr_jid("jid").cloned());

    Ok(NewsletterMetadata {
        id,
        state,
        name,
        description,
        picture_url,
        owner,
        subscriber_count,
        verification,
        invite_code,
    })
}

/// Locate the `<newsletter>` node inside an IQ response. The server wraps the
/// metadata in different containers depending on the operation; this helper
/// digs through the common nesting.
fn find_newsletter_node(resp: &Node) -> Option<&Node> {
    if resp.tag == "newsletter" {
        return Some(resp);
    }
    fn walk<'a>(n: &'a Node) -> Option<&'a Node> {
        for c in n.children() {
            if c.tag == "newsletter" {
                return Some(c);
            }
            if let Some(found) = walk(c) {
                return Some(found);
            }
        }
        None
    }
    walk(resp)
}

// ---------------------------------------------------------------------------
// Client methods.
// ---------------------------------------------------------------------------

impl Client {
    /// Fetch metadata about a newsletter via its invite link.
    ///
    /// Accepts either the full `https://whatsapp.com/channel/<key>` URL or
    /// just the `<key>` part. Mirrors `Client.GetNewsletterInfoWithInvite`.
    pub async fn get_newsletter_info_with_invite(
        &self,
        invite_key: &str,
    ) -> Result<NewsletterMetadata, ClientError> {
        let key = invite_key
            .strip_prefix(NEWSLETTER_LINK_PREFIX)
            .unwrap_or(invite_key);
        let resp = self
            .send_iq(build_get_newsletter_info_with_invite_iq(key))
            .await?;
        let nl = find_newsletter_node(&resp).ok_or_else(|| {
            ClientError::Malformed("no <newsletter> child in invite-info response".into())
        })?;
        parse_newsletter_metadata(nl)
    }

    /// Follow (join) a WhatsApp channel. Mirrors `Client.FollowNewsletter`.
    pub async fn follow_newsletter(&self, jid: &Jid) -> Result<(), ClientError> {
        let _ = self.send_iq(build_follow_newsletter_iq(jid)).await?;
        Ok(())
    }

    /// Unfollow (leave) a WhatsApp channel. Mirrors `Client.UnfollowNewsletter`.
    pub async fn unfollow_newsletter(&self, jid: &Jid) -> Result<(), ClientError> {
        let _ = self.send_iq(build_unfollow_newsletter_iq(jid)).await?;
        Ok(())
    }

    /// Create a new WhatsApp channel. Mirrors `Client.CreateNewsletter` (only
    /// the name + description fields — picture upload is not yet supported).
    pub async fn create_newsletter(
        &self,
        name: &str,
        description: &str,
    ) -> Result<NewsletterMetadata, ClientError> {
        let resp = self
            .send_iq(build_create_newsletter_iq(name, description))
            .await?;
        let nl = find_newsletter_node(&resp).ok_or_else(|| {
            ClientError::Malformed("no <newsletter> child in create-newsletter response".into())
        })?;
        parse_newsletter_metadata(nl)
    }
}

// ---------------------------------------------------------------------------
// Newsletter send path.
//
// Newsletter (= WhatsApp channel) messages are NOT Signal-encrypted: a channel
// is a public broadcast, so anyone subscribed to it must be able to read the
// payload directly. Mirror of `whatsmeow/send.go::sendNewsletter` — the wire
// envelope is just:
//
//   <message id="…" to="<channel-jid>" type="text|media|reaction">
//     <plaintext>…prost-encoded e2e.Message bytes…</plaintext>
//     <meta server_id="…"/>   (reactions / edits — references the upstream id)
//   </message>
//
// The `<meta server_id>` child is what the server uses to route a reaction or
// edit back to the original channel post.
// ---------------------------------------------------------------------------

/// Mint a fresh 16-hex-char uppercase message id. Same shape as the helper in
/// `send_message::send_message_proto` — duplicated here so this module stays
/// self-contained (and to keep the test wire-shape pinning local).
fn mint_message_id() -> String {
    let mut id_bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut id_bytes);
    hex::encode_upper(id_bytes)
}

/// Pick the outer `<message type="…">` attribute for a newsletter message.
/// Mirrors the relevant subset of upstream's `getTypeFromMessage`:
///
/// - `reaction` when `reaction_message` is set,
/// - `media`    when any of the media subtypes (image/video/audio/sticker/
///   document/url-extended) are set,
/// - `text`     otherwise (the bare `conversation`/`extended_text_message`
///   case).
///
/// We deliberately skip the recursive view-once / ephemeral wrappers — those
/// don't apply to channel posts in practice.
pub fn newsletter_message_type(msg: &Message) -> &'static str {
    if msg.reaction_message.is_some() {
        return "reaction";
    }
    if newsletter_media_type(msg).is_some() {
        return "media";
    }
    "text"
}

/// Pick the `<plaintext mediatype="…">` attribute (None means "no
/// mediatype attr"). Mirrors the relevant subset of upstream's
/// `getMediaTypeFromMessage`.
pub fn newsletter_media_type(msg: &Message) -> Option<&'static str> {
    if msg.image_message.is_some() {
        return Some("image");
    }
    if msg.sticker_message.is_some() {
        return Some("sticker");
    }
    if msg.document_message.is_some() {
        return Some("document");
    }
    if let Some(audio) = msg.audio_message.as_ref() {
        return Some(if audio.ptt.unwrap_or(false) { "ptt" } else { "audio" });
    }
    if let Some(video) = msg.video_message.as_ref() {
        return Some(if video.gif_playback.unwrap_or(false) {
            "gif"
        } else {
            "video"
        });
    }
    if let Some(et) = msg.extended_text_message.as_ref() {
        if et.title.is_some() {
            return Some("url");
        }
    }
    None
}

/// Build the `<message>` envelope for a newsletter send. Pulled out so unit
/// tests can introspect the wire shape without needing a connected client.
///
/// `server_id` — when `Some`, the function emits a `<meta server_id="…"/>`
/// child after the `<plaintext>`. Used for reactions and admin-edits, which
/// reference the original channel post by its server-side numeric id.
pub fn build_newsletter_message_node(
    message_id: &str,
    channel: &Jid,
    msg: &Message,
    server_id: Option<&str>,
) -> Result<Node, ClientError> {
    // 1. prost-encode the proto. No padding, no Signal envelope — the
    //    bytes go straight into <plaintext>.
    let mut plaintext = Vec::with_capacity(64);
    prost::Message::encode(msg, &mut plaintext)?;

    // 2. Build the <plaintext mediatype="..."> child.
    let mut pt_attrs = Attrs::new();
    if let Some(mt) = newsletter_media_type(msg) {
        pt_attrs.insert("mediatype".into(), Value::String(mt.to_owned()));
    }
    let plaintext_node = Node::new("plaintext", pt_attrs, Some(Value::Bytes(plaintext)));

    // 3. Optional <meta server_id="..."> child — references the upstream
    //    channel post for reactions / edits.
    let mut content_nodes = vec![plaintext_node];
    if let Some(sid) = server_id {
        let mut meta_attrs = Attrs::new();
        meta_attrs.insert("server_id".into(), Value::String(sid.to_owned()));
        content_nodes.push(Node::new("meta", meta_attrs, None));
    }

    // 4. Outer <message id="…" to="<channel>" type="…">.
    let mut env_attrs = Attrs::new();
    env_attrs.insert("id".into(), Value::String(message_id.to_owned()));
    env_attrs.insert("to".into(), Value::Jid(channel.clone()));
    env_attrs.insert(
        "type".into(),
        Value::String(newsletter_message_type(msg).to_owned()),
    );

    Ok(Node::new(
        "message",
        env_attrs,
        Some(Value::Nodes(content_nodes)),
    ))
}

/// Send an arbitrary `wha_proto::e2e::Message` to a WhatsApp channel
/// (newsletter). Mirrors upstream `Client.sendNewsletter`.
///
/// Returns the assigned 16-hex message id. The `channel` JID must be a
/// `…@newsletter` JID (the user posts as the channel admin; the server
/// routes the broadcast). The `message` proto is **not Signal-encrypted** —
/// channels are a public broadcast surface.
pub async fn send_newsletter_message(
    client: &Client,
    channel: &Jid,
    message: &Message,
) -> Result<String, ClientError> {
    if !client.is_connected() {
        return Err(ClientError::NotConnected);
    }
    let message_id = mint_message_id();
    let node = build_newsletter_message_node(&message_id, channel, message, None)?;
    client.send_node(&node).await?;
    Ok(message_id)
}

/// Send a plain text post to a WhatsApp channel. Convenience wrapper around
/// [`send_newsletter_message`] with `Message { conversation: Some(body), .. }`.
pub async fn send_newsletter_text(
    client: &Client,
    channel: &Jid,
    body: &str,
) -> Result<String, ClientError> {
    let msg = Message {
        conversation: Some(body.to_owned()),
        ..Default::default()
    };
    send_newsletter_message(client, channel, &msg).await
}

/// Send a reaction to a channel message. `server_id` is the numeric upstream
/// id of the channel post being reacted to (it goes into `<meta server_id="…"/>`
/// so the server can route the reaction to the right post). An empty `emoji`
/// removes a previously-sent reaction.
///
/// Mirrors the Rust-port shape: a `Message { reaction_message: …}` proto
/// shipped inside `<plaintext>`, plus a `<meta server_id>` sibling referencing
/// the original. The outer `<message type>` attr is `reaction`, courtesy of
/// [`newsletter_message_type`].
pub async fn send_newsletter_reaction(
    client: &Client,
    channel: &Jid,
    server_id: &str,
    emoji: &str,
) -> Result<String, ClientError> {
    if !client.is_connected() {
        return Err(ClientError::NotConnected);
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    // The reaction's MessageKey references the channel post by id. For
    // newsletter posts the `remote_jid` is the channel JID and `from_me`
    // is false (we're reacting to someone else's post — namely the channel's).
    let key = MessageKey {
        remote_jid: Some(channel.to_string()),
        from_me: Some(false),
        id: Some(server_id.to_owned()),
        participant: None,
    };

    let msg = Message {
        reaction_message: Some(ReactionMessage {
            key: Some(key),
            text: Some(emoji.to_owned()),
            grouping_key: None,
            sender_timestamp_ms: Some(now_ms),
        }),
        ..Default::default()
    };

    let message_id = mint_message_id();
    let node = build_newsletter_message_node(&message_id, channel, &msg, Some(server_id))?;
    client.send_node(&node).await?;
    Ok(message_id)
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn newsletter_jid() -> Jid {
        Jid::new("123456789", server::NEWSLETTER)
    }

    #[test]
    fn build_follow_iq_has_correct_attrs() {
        let jid = newsletter_jid();
        let q = build_follow_newsletter_iq(&jid);
        let n = q.into_node("REQ-1".into());

        assert_eq!(n.tag, "iq");
        assert_eq!(n.get_attr_str("xmlns"), Some("w:mex"));
        assert_eq!(n.get_attr_str("type"), Some("get"));
        assert_eq!(n.get_attr_str("id"), Some("REQ-1"));
        assert_eq!(
            n.get_attr_jid("to").map(|j| j.server.as_str()),
            Some(server::DEFAULT_USER)
        );

        let kids = n.children();
        assert_eq!(kids.len(), 1);
        let query = &kids[0];
        assert_eq!(query.tag, "query");
        assert_eq!(query.get_attr_str("query_id"), Some(MUTATION_FOLLOW_NEWSLETTER));

        let body = query
            .content
            .as_bytes()
            .expect("query body should be raw bytes");
        let body_str = std::str::from_utf8(body).expect("utf8");
        assert!(
            body_str.contains("\"newsletter_id\":\"123456789@newsletter\""),
            "body should embed the JID: {body_str}"
        );
        assert!(body_str.starts_with("{\"variables\":"), "body: {body_str}");
    }

    #[test]
    fn build_create_newsletter_iq_carries_name_and_description() {
        let q = build_create_newsletter_iq("My Channel", "All the news");
        let n = q.into_node("REQ-2".into());

        assert_eq!(n.get_attr_str("xmlns"), Some("w:mex"));

        let query = &n.children()[0];
        assert_eq!(query.get_attr_str("query_id"), Some(MUTATION_CREATE_NEWSLETTER));

        let body = query.content.as_bytes().expect("raw body");
        let body_str = std::str::from_utf8(body).unwrap();
        assert!(body_str.contains("\"name\":\"My Channel\""), "{body_str}");
        assert!(
            body_str.contains("\"description\":\"All the news\""),
            "{body_str}"
        );
        assert!(body_str.contains("\"newsletter_input\""), "{body_str}");

        // Empty description should be omitted entirely.
        let empty = build_create_newsletter_iq("Channel B", "");
        let en = empty.into_node("REQ-3".into());
        let ebody = en.children()[0].content.as_bytes().unwrap();
        let ebody_str = std::str::from_utf8(ebody).unwrap();
        assert!(ebody_str.contains("\"name\":\"Channel B\""));
        assert!(
            !ebody_str.contains("description"),
            "empty description must be omitted: {ebody_str}"
        );
    }

    #[test]
    fn parse_newsletter_metadata_extracts_name_and_owner() {
        // Build:
        // <newsletter id="123456789@newsletter">
        //   <state type="active"/>
        //   <thread_metadata>
        //     <name>Cool Channel</name>
        //     <description>About cool things.</description>
        //     <subscribers_count>4242</subscribers_count>
        //     <invite>ABCDEFG</invite>
        //     <verification state="verified"/>
        //     <picture url="https://example.com/pic.jpg"/>
        //     <owner jid="555@s.whatsapp.net"/>
        //   </thread_metadata>
        // </newsletter>

        let mk_text = |tag: &str, text: &str| {
            Node::new(tag, Attrs::new(), Some(Value::Bytes(text.as_bytes().to_vec())))
        };
        let mk_attr = |tag: &str, key: &str, val: Value| {
            let mut a = Attrs::new();
            a.insert(key.into(), val);
            Node::new(tag, a, None)
        };

        let owner_jid = Jid::new("555", server::DEFAULT_USER);

        let thread = Node::new(
            "thread_metadata",
            Attrs::new(),
            Some(Value::Nodes(vec![
                mk_text("name", "Cool Channel"),
                mk_text("description", "About cool things."),
                mk_text("subscribers_count", "4242"),
                mk_text("invite", "ABCDEFG"),
                mk_attr("verification", "state", Value::String("verified".into())),
                mk_attr(
                    "picture",
                    "url",
                    Value::String("https://example.com/pic.jpg".into()),
                ),
                mk_attr("owner", "jid", Value::Jid(owner_jid.clone())),
            ])),
        );

        let state = mk_attr("state", "type", Value::String("active".into()));

        let mut nl_attrs = Attrs::new();
        nl_attrs.insert("id".into(), Value::Jid(newsletter_jid()));
        let nl = Node::new(
            "newsletter",
            nl_attrs,
            Some(Value::Nodes(vec![state, thread])),
        );

        let md = parse_newsletter_metadata(&nl).expect("parse");
        assert_eq!(md.id, newsletter_jid());
        assert_eq!(md.state, NewsletterState::Active);
        assert_eq!(md.name, "Cool Channel");
        assert_eq!(md.description.as_deref(), Some("About cool things."));
        assert_eq!(md.subscriber_count, Some(4242));
        assert_eq!(md.invite_code.as_deref(), Some("ABCDEFG"));
        assert_eq!(
            md.verification,
            Some(NewsletterVerificationState::Verified)
        );
        assert_eq!(md.picture_url.as_deref(), Some("https://example.com/pic.jpg"));
        assert_eq!(md.owner, Some(owner_jid));
    }

    #[test]
    fn build_get_newsletter_info_with_invite_iq_uses_newsletter_namespace() {
        let q = build_get_newsletter_info_with_invite_iq("ABCDEFG");
        let n = q.into_node("REQ-4".into());
        assert_eq!(n.get_attr_str("xmlns"), Some("newsletter"));
        assert_eq!(n.get_attr_str("type"), Some("get"));
        let live = &n.children()[0];
        assert_eq!(live.tag, "live_updates");
        assert_eq!(live.get_attr_str("key"), Some("ABCDEFG"));
        assert_eq!(live.get_attr_str("type"), Some("INVITE"));
    }

    #[test]
    fn build_unfollow_newsletter_iq_uses_unfollow_query_id() {
        let q = build_unfollow_newsletter_iq(&newsletter_jid());
        let n = q.into_node("REQ-5".into());
        let query = &n.children()[0];
        assert_eq!(
            query.get_attr_str("query_id"),
            Some(MUTATION_UNFOLLOW_NEWSLETTER)
        );
    }

    #[test]
    fn parse_newsletter_metadata_rejects_wrong_tag() {
        let bad = Node::tag_only("notnewsletter");
        let r = parse_newsletter_metadata(&bad);
        assert!(matches!(r, Err(ClientError::Malformed(_))));
    }

    // ---------------------------------------------------------------------
    // send_newsletter_message wire-shape pins.
    // ---------------------------------------------------------------------

    /// `send_newsletter_text` (via `build_newsletter_message_node`) produces
    /// a bare `<plaintext>` envelope — no Signal `<enc>`, no `<participants>`.
    /// The plaintext bytes are the prost-encoded `Message { conversation }`,
    /// and the outer `type` is `text`.
    #[test]
    fn send_newsletter_text_builds_plaintext_envelope() {
        let channel = newsletter_jid();
        let msg = Message {
            conversation: Some("hello channel".to_owned()),
            ..Default::default()
        };

        let node = build_newsletter_message_node("ABCDEF1234567890", &channel, &msg, None)
            .expect("build node");

        // <message id=… to=<channel> type="text">
        assert_eq!(node.tag, "message");
        assert_eq!(node.get_attr_str("id"), Some("ABCDEF1234567890"));
        assert_eq!(node.get_attr_str("type"), Some("text"));
        assert_eq!(node.get_attr_jid("to"), Some(&channel));

        // Exactly ONE child — the <plaintext>. No <participants>, no <enc>.
        let kids = node.children();
        assert_eq!(kids.len(), 1, "kids: {kids:?}");
        let pt = &kids[0];
        assert_eq!(pt.tag, "plaintext");
        // Plain text post — no `mediatype` attribute.
        assert!(
            pt.get_attr_str("mediatype").is_none(),
            "text post must not carry mediatype: {pt:?}"
        );

        // The bytes must be the prost-encoded Message proto (i.e. they
        // round-trip through prost::Message::decode).
        let bytes = pt
            .content
            .as_bytes()
            .expect("plaintext should carry raw bytes");
        let decoded: Message =
            prost::Message::decode(bytes).expect("plaintext should decode as Message");
        assert_eq!(decoded.conversation.as_deref(), Some("hello channel"));
        assert!(decoded.image_message.is_none());
        assert!(decoded.reaction_message.is_none());

        // Crucially: no <enc> or <participants> sibling (these are the
        // Signal-encrypted DM shape — channels don't use them).
        for k in kids {
            assert_ne!(k.tag, "enc", "channel must not carry <enc>");
            assert_ne!(k.tag, "participants", "channel must not carry <participants>");
        }
    }

    /// `send_newsletter_reaction` carries a `<meta server_id>` child after
    /// the `<plaintext>`, the outer `<message type>` is `reaction`, and the
    /// embedded proto carries a `reaction_message` whose key references the
    /// upstream id.
    #[test]
    fn send_newsletter_reaction_includes_meta_server_id() {
        let channel = newsletter_jid();
        let server_id = "12345";
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let key = MessageKey {
            remote_jid: Some(channel.to_string()),
            from_me: Some(false),
            id: Some(server_id.to_owned()),
            participant: None,
        };
        let msg = Message {
            reaction_message: Some(ReactionMessage {
                key: Some(key),
                text: Some("❤".to_owned()),
                grouping_key: None,
                sender_timestamp_ms: Some(now_ms),
            }),
            ..Default::default()
        };

        let node = build_newsletter_message_node(
            "REACT-MSG-ID-000",
            &channel,
            &msg,
            Some(server_id),
        )
        .expect("build node");

        assert_eq!(node.tag, "message");
        assert_eq!(node.get_attr_str("type"), Some("reaction"));

        let kids = node.children();
        // Two children: <plaintext> + <meta server_id="...">.
        assert_eq!(kids.len(), 2, "kids: {kids:?}");
        assert_eq!(kids[0].tag, "plaintext");
        assert_eq!(kids[1].tag, "meta");
        assert_eq!(kids[1].get_attr_str("server_id"), Some(server_id));

        // The proto round-trips and carries the reaction.
        let pt_bytes = kids[0].content.as_bytes().expect("plaintext bytes");
        let decoded: Message =
            prost::Message::decode(pt_bytes).expect("reaction plaintext decodes");
        let r = decoded.reaction_message.as_ref().expect("reaction set");
        assert_eq!(r.text.as_deref(), Some("❤"));
        let k = r.key.as_ref().expect("reaction key");
        assert_eq!(k.id.as_deref(), Some(server_id));
        assert_eq!(k.from_me, Some(false));
        assert_eq!(k.remote_jid.as_deref(), Some("123456789@newsletter"));

        // Empty emoji form — also valid (= remove reaction). The wire-shape
        // is identical except for the proto's `text` field.
        let blank_msg = Message {
            reaction_message: Some(ReactionMessage {
                key: Some(MessageKey {
                    remote_jid: Some(channel.to_string()),
                    from_me: Some(false),
                    id: Some(server_id.to_owned()),
                    participant: None,
                }),
                text: Some(String::new()),
                grouping_key: None,
                sender_timestamp_ms: Some(now_ms),
            }),
            ..Default::default()
        };
        let blank_node = build_newsletter_message_node(
            "REACT-MSG-ID-001",
            &channel,
            &blank_msg,
            Some(server_id),
        )
        .expect("build node");
        assert_eq!(blank_node.get_attr_str("type"), Some("reaction"));
        let blank_kids = blank_node.children();
        assert_eq!(blank_kids.len(), 2);
        assert_eq!(blank_kids[1].get_attr_str("server_id"), Some(server_id));
    }

    /// `send_newsletter_message` with an `image_message` (or any media
    /// subtype) flips the outer `type` to `media` and writes a
    /// `<plaintext mediatype="image">` attr. Mirrors upstream's
    /// `getTypeFromMessage` / `getMediaTypeFromMessage` cascade.
    #[test]
    fn send_newsletter_message_with_media_uses_media_type() {
        let channel = newsletter_jid();
        let msg = Message {
            image_message: Some(Box::new(wha_proto::e2e::ImageMessage {
                url: Some("https://example.com/img.jpg".to_owned()),
                mimetype: Some("image/jpeg".to_owned()),
                ..Default::default()
            })),
            ..Default::default()
        };

        let node = build_newsletter_message_node("MEDIA-MSG-ID-002", &channel, &msg, None)
            .expect("build node");

        // Outer <message type="media">.
        assert_eq!(node.get_attr_str("type"), Some("media"));

        // <plaintext mediatype="image"> with a non-empty body.
        let kids = node.children();
        assert_eq!(kids.len(), 1);
        let pt = &kids[0];
        assert_eq!(pt.tag, "plaintext");
        assert_eq!(pt.get_attr_str("mediatype"), Some("image"));

        let pt_bytes = pt.content.as_bytes().expect("plaintext bytes");
        let decoded: Message =
            prost::Message::decode(pt_bytes).expect("image plaintext decodes");
        assert!(decoded.image_message.is_some());
        assert_eq!(
            decoded
                .image_message
                .as_ref()
                .and_then(|im| im.mimetype.as_deref()),
            Some("image/jpeg")
        );

        // Repeat for a video — switches the mediatype label.
        let video_msg = Message {
            video_message: Some(Box::new(wha_proto::e2e::VideoMessage {
                url: Some("https://example.com/vid.mp4".to_owned()),
                gif_playback: Some(false),
                ..Default::default()
            })),
            ..Default::default()
        };
        let video_node =
            build_newsletter_message_node("MEDIA-MSG-ID-003", &channel, &video_msg, None)
                .expect("build node");
        assert_eq!(video_node.get_attr_str("type"), Some("media"));
        assert_eq!(
            video_node.children()[0].get_attr_str("mediatype"),
            Some("video")
        );

        // GIF playback flips video → gif.
        let gif_msg = Message {
            video_message: Some(Box::new(wha_proto::e2e::VideoMessage {
                url: Some("https://example.com/anim.mp4".to_owned()),
                gif_playback: Some(true),
                ..Default::default()
            })),
            ..Default::default()
        };
        let gif_node = build_newsletter_message_node("MEDIA-MSG-ID-004", &channel, &gif_msg, None)
            .expect("build node");
        assert_eq!(
            gif_node.children()[0].get_attr_str("mediatype"),
            Some("gif")
        );
    }

    /// Pre-flight: `send_newsletter_text` and `send_newsletter_reaction`
    /// surface `NotConnected` before any IO. Same guarantee as the DM
    /// helpers in `send_message`.
    #[tokio::test]
    async fn send_newsletter_helpers_without_connection_error() {
        use std::sync::Arc;
        use wha_store::MemoryStore;

        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let channel = newsletter_jid();

        let r = send_newsletter_text(&client, &channel, "hi").await;
        assert!(matches!(r, Err(ClientError::NotConnected)), "got {r:?}");

        let r = send_newsletter_reaction(&client, &channel, "12345", "❤").await;
        assert!(matches!(r, Err(ClientError::NotConnected)), "got {r:?}");
    }
}
