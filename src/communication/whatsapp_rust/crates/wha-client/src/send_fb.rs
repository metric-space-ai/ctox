//! Facebook / Messenger-protocol send variant — port of
//! `_upstream/whatsmeow/sendfb.go::SendFBMessage`.
//!
//! ## Wire shape
//!
//! Messenger E2EE rides on top of the **Signal session** transport — same
//! per-device fan-out, same identity material, same prekey bundles — but
//! with a different inner envelope:
//!
//! 1. The user-supplied [`EncryptedMessage`] (alias for `MessageApplication`)
//!    is prost-marshalled to a byte buffer.
//! 2. That buffer is wrapped in a [`MessageTransport`] envelope (the
//!    `payload.application_payload` is the serialised
//!    `MessageApplication` from step 1, version `2` to match upstream's
//!    `FBMessageApplicationVersion`).
//! 3. The transport bytes are Signal-encrypted via the existing
//!    per-recipient session pipeline ([`crate::send_encrypt`]).
//! 4. The ciphertext goes into a single `<enc v="3" type="msmsg|skmsg">`
//!    child of the outer `<message>` envelope, addressed to the Messenger
//!    server (`to.server == "msgr"`).
//!
//! Group fan-out (`sendGroupV3`), franking + participant-list-hash, and
//! multi-device deviation across the `sendDMV3` path are intentionally
//! **not** part of this port — the foundation port targets the DM /
//! single-recipient case which is the only path exercised by the current
//! integration tests. The wire-shape primitives here are correct for the
//! group case too; only the SKDM construction is missing.
//!
//! ## Differences from `sendFBMessage` upstream
//!
//! * Upstream walks `cli.GetUserDevices(participants)` to fan out across
//!   every device of the recipient. We send a single `<to jid="…">` for
//!   the recipient as supplied; multi-device fan-out lands with the same
//!   `getDevicesForJID` follow-up that the regular `send_message` path
//!   awaits.
//! * Franking (`waMsgApplication.MessageApplication.Metadata.FrankingKey`
//!   + HMAC tag emitted as the `<franking>` child) is a Messenger-only
//!   integrity feature. The minimum-viable wire format does not require
//!   it for delivery; we leave it `None` and add the `<franking>` child
//!   in a follow-up. Note in the comment is the same parity caveat the
//!   regular `send.rs` carries for `<franking>`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use prost::Message as ProstMessage;
use tokio::sync::oneshot;

use wha_binary::{Attrs, Node, Value};
use wha_proto::messenger::transport::message_transport::{
    protocol::{Ancillary as TransportAncillary, Integral as TransportIntegral},
    Payload as TransportPayload, Protocol as TransportProtocol,
};
use wha_proto::messenger::transport::MessageTransport;
use wha_proto::messenger::EncryptedMessage;
use wha_proto::common::{FutureProofBehavior, SubProtocol};
use wha_types::{Jid, Server};

use crate::client::{Client, DEFAULT_REQUEST_TIMEOUT};
use crate::error::ClientError;
use crate::send::{generate_message_id, SendDebugTimings, SendResponse};

/// Upstream's `FBMessageVersion` — the `v=` attribute on the outgoing
/// `<enc>` node. Mirrors `_upstream/whatsmeow/sendfb.go::FBMessageVersion`.
pub const FB_MESSAGE_VERSION: &str = "3";

/// Upstream's `FBMessageApplicationVersion` — the inner `version` field on
/// the [`SubProtocol`] wrapper for the application payload. Mirrors
/// `_upstream/whatsmeow/sendfb.go::FBMessageApplicationVersion`.
pub const FB_MESSAGE_APPLICATION_VERSION: i32 = 2;

/// Whether `jid` should be sent via the Messenger interop path. Mirrors
/// the `case types.MessengerServer:` arm in
/// `_upstream/whatsmeow/sendfb.go::SendFBMessage`.
pub fn is_fb_recipient(jid: &Jid) -> bool {
    jid.server == Server::MESSENGER
}

/// Build the outer `<message>` envelope for a Messenger-protocol send.
///
/// Differs from [`crate::send::build_message_envelope`] only in the `to`
/// attribute — upstream always rewrites the destination server to
/// Messenger — and in the `type` attribute, which upstream's
/// `getAttrsFromFBMessage` defaults to `"text"` when no specific
/// armadillo/consumer subtype is recognised.
///
/// `meta_node` and `enc_nodes` are inserted as children in that order.
/// `meta_node` mirrors upstream's `<meta/>` — for now an empty placeholder
/// since the foundation port doesn't compute polltype/decrypt-fail attrs.
pub fn build_fb_message_envelope(
    message_id: &str,
    to: &Jid,
    meta_node: Node,
    enc_nodes: Vec<Node>,
) -> Node {
    // Force the destination server to MESSENGER. Callers may legitimately
    // pass `@s.whatsapp.net` JIDs here (rare upstream branch). We preserve
    // the user/device fields and only rewrite the server.
    let mut routed = to.clone();
    routed.server = Server::MESSENGER.to_owned();

    let mut attrs = Attrs::new();
    attrs.insert("id".into(), Value::String(message_id.to_owned()));
    attrs.insert("type".into(), Value::String("text".into()));
    attrs.insert("to".into(), Value::Jid(routed));
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    attrs.insert("t".into(), Value::String(t.to_string()));

    let mut children = Vec::with_capacity(1 + enc_nodes.len());
    children.push(meta_node);
    children.extend(enc_nodes);
    Node::new("message", attrs, Some(Value::Nodes(children)))
}

/// Wrap a serialised [`EncryptedMessage`] (i.e. `MessageApplication`) in a
/// [`MessageTransport`] and serialise it back to bytes.
///
/// This is the "transport-encode" step in upstream's `prepareFBMessage` /
/// `encryptMessageForDeviceV3`: the `MessageApplication` bytes go in
/// `payload.application_payload`, with version
/// [`FB_MESSAGE_APPLICATION_VERSION`].
///
/// Note: per-device padding + the `DeviceSentMessage` integral marker are
/// intentionally left empty here. Upstream sets them for the multi-device
/// fan-out (so the sender's other devices know the destination JID). Once
/// device fan-out lands, this helper grows a `dsm: Option<DSM>` parameter
/// and the transport gets per-device DSM stamping.
pub fn wrap_in_transport(message_app_bytes: Vec<u8>) -> Result<Vec<u8>, ClientError> {
    let payload = TransportPayload {
        application_payload: Some(SubProtocol {
            payload: Some(message_app_bytes),
            version: Some(FB_MESSAGE_APPLICATION_VERSION),
        }),
        future_proof: Some(FutureProofBehavior::Placeholder as i32),
    };
    let protocol = TransportProtocol {
        integral: Some(TransportIntegral {
            // Empty padding — matches the foundation-port stance in the
            // regular send path. `padMessage(nil)` upstream returns a tiny
            // randomised buffer; we elide it to keep the outgoing bytes
            // deterministic for tests.
            padding: None,
            dsm: None,
        }),
        ancillary: Some(TransportAncillary::default()),
    };
    let transport = MessageTransport {
        payload: Some(payload),
        protocol: Some(protocol),
    };
    let mut out = Vec::with_capacity(message_app_bytes_len_hint(&transport));
    transport
        .encode(&mut out)
        .map_err(|e| ClientError::Proto(e.to_string()))?;
    Ok(out)
}

fn message_app_bytes_len_hint(t: &MessageTransport) -> usize {
    // Cheap upper bound — caller pre-allocates.
    prost::Message::encoded_len(t)
}

/// Build the Messenger-style `<enc>` node carrying the per-device
/// ciphertext. Mirrors `encryptMessageForDeviceV3`'s wire output:
/// `<enc v="3" type="msmsg|pkmsg">…ciphertext…</enc>`.
///
/// `enc_type` is `"msmsg"` for established Signal sessions and `"pkmsg"`
/// for first-flight messages where the recipient still needs the prekey
/// bundle.
pub fn build_fb_enc_node(enc_type: &str, ciphertext: Vec<u8>) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("v".into(), Value::String(FB_MESSAGE_VERSION.into()));
    attrs.insert("type".into(), Value::String(enc_type.to_owned()));
    Node::new("enc", attrs, Some(Value::Bytes(ciphertext)))
}

impl Client {
    /// Send a Messenger-protocol (Facebook E2EE) message.
    ///
    /// The `msg` parameter is a typed [`EncryptedMessage`] (alias for
    /// `MessageApplication`). The function:
    ///
    /// 1. Marshals `msg` via prost.
    /// 2. Wraps it in a `MessageTransport` envelope ([`wrap_in_transport`]).
    /// 3. Encrypts the transport bytes through the existing Signal-session
    ///    pipeline ([`crate::send_encrypt::encrypt_for_recipient`]).
    /// 4. Wraps the resulting `<enc>` node in a Messenger-shaped
    ///    `<message>` envelope ([`build_fb_message_envelope`]) and sends it.
    ///
    /// Returns the message ID. Mirrors the return shape of upstream's
    /// `SendFBMessage` (which returns `SendResponse`); we follow the
    /// project's convention of returning the message ID directly so call
    /// sites that only need the ID don't have to destructure the struct.
    pub async fn send_fb_message(
        &self,
        to: &Jid,
        msg: &EncryptedMessage,
    ) -> Result<String, ClientError> {
        if !self.is_connected() {
            return Err(ClientError::NotConnected);
        }
        // Upstream's `SendFBMessage` requires `getOwnID` non-empty. We
        // surface the same precondition — Signal-encrypt needs an
        // identity_key on the device to derive the local Signal address.
        if self.device.id.is_none() {
            return Err(ClientError::NotLoggedIn);
        }

        let message_id = generate_message_id(self);

        // 1. Marshal the typed MessageApplication.
        let mut app_bytes = Vec::with_capacity(64);
        msg.encode(&mut app_bytes)
            .map_err(|e| ClientError::Proto(e.to_string()))?;

        // 2. Wrap in MessageTransport.
        let transport_bytes = wrap_in_transport(app_bytes)?;

        // 3. Signal-encrypt via the same per-recipient pipeline used for
        //    the WhatsApp-protocol path. The session is identified by the
        //    recipient's JID through `device.identity_store` /
        //    `device.sessions` — same as `send_message`.
        let enc_children =
            crate::send_encrypt::encrypt_for_recipient(self, to, &transport_bytes).await?;

        // The `send_encrypt` helper emits `<enc v="2" type="…">`. For the
        // FB path we need `v="3"` and `type="msmsg"|"pkmsg"`. Rewrite the
        // attrs in-place; the body bytes are unchanged.
        let enc_children: Vec<Node> = enc_children
            .into_iter()
            .map(|n| {
                let typ = n.get_attr_str("type").unwrap_or("msg");
                let new_typ = match typ {
                    "msg" => "msmsg",
                    "pkmsg" => "pkmsg",
                    other => other, // pass-through for unknown types
                };
                let bytes = n.content.as_bytes().map(<[u8]>::to_vec).unwrap_or_default();
                build_fb_enc_node(new_typ, bytes)
            })
            .collect();

        // 4. Build envelope and ship it. `<meta/>` carries no attrs in the
        //    foundation port — see module-level note on franking/polltype.
        let meta = Node::new("meta", Attrs::new(), None);
        let envelope = build_fb_message_envelope(&message_id, to, meta, enc_children);

        let (tx, rx) = oneshot::channel();
        self.install_waiter(message_id.clone(), tx);

        if let Err(e) = self.send_node(&envelope).await {
            return Err(e);
        }

        // Await the `<ack>`. We discard the ack contents — the message ID
        // is what the FB API contract returns.
        match tokio::time::timeout(
            Duration::from_secs(DEFAULT_REQUEST_TIMEOUT.as_secs()),
            rx,
        )
        .await
        {
            Ok(Ok(_node)) => Ok(message_id),
            Ok(Err(_)) => Err(ClientError::IqDisconnected),
            Err(_) => Err(ClientError::IqTimedOut),
        }
    }

    /// Pre-encoded variant. Kept as a thin wrapper for callers that have
    /// already done their own `prost::encode` (e.g. when interop-testing
    /// against a captured upstream wire dump). The body bytes are
    /// forwarded verbatim — no transport wrap, no Signal encrypt — and
    /// shipped under `<enc v="3" type="msmsg">`.
    ///
    /// Returns a [`SendResponse`] like the original API to keep wire-replay
    /// tests source-compatible.
    pub async fn send_fb_message_raw(
        &self,
        to: &Jid,
        armadillo_payload: Vec<u8>,
    ) -> Result<SendResponse, ClientError> {
        if !self.is_connected() {
            return Err(ClientError::NotConnected);
        }
        let message_id = generate_message_id(self);
        let enc = build_fb_enc_node("msmsg", armadillo_payload);
        let meta = Node::new("meta", Attrs::new(), None);
        let envelope = build_fb_message_envelope(&message_id, to, meta, vec![enc]);

        let (tx, rx) = oneshot::channel();
        self.install_waiter(message_id.clone(), tx);

        if let Err(e) = self.send_node(&envelope).await {
            return Err(e);
        }

        let ack = match tokio::time::timeout(
            Duration::from_secs(DEFAULT_REQUEST_TIMEOUT.as_secs()),
            rx,
        )
        .await
        {
            Ok(Ok(node)) => node,
            Ok(Err(_)) => return Err(ClientError::IqDisconnected),
            Err(_) => return Err(ClientError::IqTimedOut),
        };

        let mut ag = ack.attr_getter();
        let timestamp = ag.optional_i64("t").unwrap_or(0);
        let server_id = ag.optional_i64("server_id");

        Ok(SendResponse {
            timestamp,
            message_id,
            server_id,
            debug_timings: SendDebugTimings::zero(),
        })
    }
}

/// Free-function form of [`Client::send_fb_message`] — matches the public
/// API surface specified in the porting brief.
pub async fn send_fb_message(
    client: &Client,
    to: &Jid,
    msg: &EncryptedMessage,
) -> Result<String, ClientError> {
    client.send_fb_message(to, msg).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use wha_store::MemoryStore;

    #[test]
    fn is_fb_recipient_classifies_msgr_jids() {
        let msgr: Jid = "1234@msgr".parse().unwrap();
        let wa: Jid = "1234@s.whatsapp.net".parse().unwrap();
        let group: Jid = "abc-123@g.us".parse().unwrap();
        let lid: Jid = "1234@lid".parse().unwrap();

        assert!(is_fb_recipient(&msgr), "msgr server should route to FB path");
        assert!(!is_fb_recipient(&wa));
        assert!(!is_fb_recipient(&group));
        assert!(!is_fb_recipient(&lid));
    }

    #[test]
    fn build_fb_message_envelope_has_required_attrs() {
        // Even when given a non-`@msgr` JID, the envelope's `to` attr must
        // be rewritten to address the Messenger server. The id, type=text,
        // and unix-time `t` must all be set, and the meta+enc children
        // appear in order.
        let to: Jid = "5550001@s.whatsapp.net".parse().unwrap();
        let meta = Node::new("meta", Attrs::new(), None);
        let env = build_fb_message_envelope("3EB0FBCAFE01", &to, meta, vec![]);

        assert_eq!(env.tag, "message");
        assert_eq!(env.get_attr_str("id"), Some("3EB0FBCAFE01"));
        assert_eq!(env.get_attr_str("type"), Some("text"));

        let routed = env.get_attr_jid("to").expect("to attr present");
        assert_eq!(routed.user, "5550001", "user is preserved");
        assert_eq!(routed.server, Server::MESSENGER, "server rewritten to msgr");

        let t_str = env.get_attr_str("t").expect("t attr present");
        let t: i64 = t_str.parse().expect("t parses as integer");
        assert!(t > 0, "t should be a recent unix timestamp, got {t}");

        // <meta/> only — no <enc> children passed in.
        let kids = env.children();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].tag, "meta");
    }

    #[test]
    fn build_fb_message_envelope_carries_meta_then_enc_children() {
        let to: Jid = "5550002@msgr".parse().unwrap();
        let enc = build_fb_enc_node("msmsg", vec![0xAA, 0xBB]);
        let meta = Node::new("meta", Attrs::new(), None);
        let env = build_fb_message_envelope("3EB0FBCAFE02", &to, meta, vec![enc]);

        let kids = env.children();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].tag, "meta");
        assert_eq!(kids[1].tag, "enc");
        assert_eq!(kids[1].get_attr_str("v"), Some("3"));
        assert_eq!(kids[1].get_attr_str("type"), Some("msmsg"));
        assert_eq!(kids[1].content.as_bytes(), Some(&[0xAAu8, 0xBB][..]));
    }

    /// `wrap_in_transport` round-trips: the bytes we put in
    /// `application_payload.payload` come back unchanged after a
    /// prost::decode of the produced transport buffer.
    #[test]
    fn wrap_in_transport_round_trips_application_bytes() {
        let app_bytes = b"hello messenger".to_vec();
        let bytes = wrap_in_transport(app_bytes.clone()).expect("wrap");
        // Decode the transport back.
        let decoded =
            <MessageTransport as ProstMessage>::decode(bytes.as_slice()).expect("decode");
        let payload = decoded.payload.expect("payload present");
        let app = payload.application_payload.expect("app payload present");
        assert_eq!(app.payload, Some(app_bytes), "app bytes round-trip");
        assert_eq!(
            app.version,
            Some(FB_MESSAGE_APPLICATION_VERSION),
            "version pinned"
        );
        // FutureProofBehavior::Placeholder is the only value we set.
        assert_eq!(
            payload.future_proof,
            Some(FutureProofBehavior::Placeholder as i32),
        );
    }

    /// Disconnected clients short-circuit with `NotConnected` without
    /// running the encrypt pipeline — mirrors the early-return upstream.
    #[tokio::test]
    async fn send_fb_to_disconnected_client_errors() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let to: Jid = "5550003@msgr".parse().unwrap();
        let msg = EncryptedMessage::default();
        let r = client.send_fb_message(&to, &msg).await;
        assert!(matches!(r, Err(ClientError::NotConnected)), "got {r:?}");
    }

    /// `<enc v="3" type="msmsg">` shape pinned for the wire format
    /// upstream's `encryptMessageForDeviceV3` produces. Tests both the
    /// established-session (`msmsg`) and first-flight (`pkmsg`) types.
    #[test]
    fn build_fb_enc_node_carries_v3_and_type() {
        for typ in ["msmsg", "pkmsg"] {
            let n = build_fb_enc_node(typ, vec![1, 2, 3]);
            assert_eq!(n.tag, "enc");
            assert_eq!(n.get_attr_str("v"), Some("3"));
            assert_eq!(n.get_attr_str("type"), Some(typ));
            assert_eq!(n.content.as_bytes(), Some(&[1u8, 2, 3][..]));
        }
    }
}
