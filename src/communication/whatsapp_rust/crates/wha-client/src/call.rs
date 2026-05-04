//! Call-signaling helpers — port of `whatsmeow/call.go`.
//!
//! Calls in WhatsApp arrive as a top-level `<call>` node with a single child
//! whose tag identifies the signaling phase (`offer`, `accept`, `transport`,
//! `terminate`, …). This module mirrors the upstream parser plus the
//! `RejectCall` outbound builder.
//!
//! The events stay local to this module on purpose: per the foundation port
//! plan we don't expand the global [`Event`] enum yet — callers wanting call
//! support can introspect [`CallEvent`] directly via the future call-stream
//! hook.
//!
//! [`Event`]: crate::events::Event

use wha_binary::{Attrs, Node, Value};
use wha_types::{jid::server, Jid};

use crate::client::Client;
use crate::error::ClientError;
use crate::events::{BasicCallOffer, Event};
use crate::send::generate_message_id;

// ---------------------------------------------------------------------------
// Wire types.
// ---------------------------------------------------------------------------

/// Attributes shared by every call-signaling child node — `<offer>`, `<accept>`,
/// `<terminate>`, …. Mirrors upstream's `types.BasicCallMeta`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicCallMeta {
    /// `from` attribute on the outer `<call>` node — who sent the signal.
    pub from: Jid,
    /// `t` attribute on the outer `<call>` node, in seconds since the epoch.
    pub timestamp: i64,
    /// `call-creator` attribute on the inner child — who initiated the call.
    pub call_creator: Jid,
    /// Alternate JID for the call creator (`caller_pn` when the creator lives
    /// in `lid`, `caller_lid` otherwise). Empty when not present.
    pub call_creator_alt: Option<Jid>,
    /// `call-id` attribute on the inner child.
    pub call_id: String,
    /// Optional `group-jid` attribute on the inner child.
    pub group_jid: Option<Jid>,
}

/// Platform/version metadata pulled off the outer `<call>` node — present on
/// the offer/accept/transport variants. Mirrors `types.CallRemoteMeta`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallRemoteMeta {
    pub remote_platform: String,
    pub remote_version: String,
}

/// One parsed `<call>` signal. Variants line up with the inner-child tag
/// names used upstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallEvent {
    /// `<offer/>` — incoming call invitation.
    Offer {
        meta: BasicCallMeta,
        remote: CallRemoteMeta,
        data: Node,
    },
    /// `<offer_notice/>` — silent offer notice (typically WhatsApp Business).
    OfferNotice {
        meta: BasicCallMeta,
        media: String,
        notice_type: String,
        data: Node,
    },
    /// `<accept/>` — the other side accepted our call.
    Accept {
        meta: BasicCallMeta,
        remote: CallRemoteMeta,
        data: Node,
    },
    /// `<preaccept/>` — partial acceptance ahead of full negotiation.
    PreAccept {
        meta: BasicCallMeta,
        remote: CallRemoteMeta,
        data: Node,
    },
    /// `<transport/>` — ICE / transport negotiation update.
    Transport {
        meta: BasicCallMeta,
        remote: CallRemoteMeta,
        data: Node,
    },
    /// `<reject/>` — peer declined the call.
    Reject {
        meta: BasicCallMeta,
        data: Node,
    },
    /// `<terminate/>` — call ended.
    Terminate {
        meta: BasicCallMeta,
        reason: String,
        data: Node,
    },
    /// `<relaylatency/>` — periodic relay-latency report.
    RelayLatency {
        meta: BasicCallMeta,
        data: Node,
    },
    /// Anything else — preserved verbatim so callers can introspect it.
    Unknown { node: Node },
}

// ---------------------------------------------------------------------------
// Parser.
// ---------------------------------------------------------------------------

/// Parse a top-level `<call>` node into a [`CallEvent`]. Mirrors
/// `whatsmeow/call.go::handleCallEvent`'s switch on the inner child tag.
///
/// A `<call>` node is expected to carry exactly one child; anything else maps
/// to [`CallEvent::Unknown`] (matching upstream's `UnknownCallEvent`).
pub fn parse_call_node(node: &Node) -> Result<CallEvent, ClientError> {
    if node.tag != "call" {
        return Err(ClientError::Malformed(format!(
            "expected <call>, got <{}>",
            node.tag
        )));
    }

    let children = node.children();
    if children.len() != 1 {
        return Ok(CallEvent::Unknown { node: node.clone() });
    }
    let child = &children[0];

    let mut ag = node.attr_getter();
    let from = ag.jid("from");
    let timestamp = ag.optional_i64("t").unwrap_or(0);
    let remote_platform = ag.optional_string("platform").unwrap_or("").to_owned();
    let remote_version = ag.optional_string("version").unwrap_or("").to_owned();
    if !ag.ok() {
        let errs = ag.into_result().err().unwrap_or_default();
        return Err(ClientError::Malformed(format!(
            "failed to parse <call> attrs: {errs:?}"
        )));
    }

    let mut cag = child.attr_getter();
    let call_creator = cag.jid("call-creator");
    let call_id = cag.string("call-id").to_owned();
    let group_jid = cag.optional_jid("group-jid").cloned();
    // Upstream picks `caller_pn` when the creator lives on the hidden-user
    // (lid) server, `caller_lid` otherwise. Both are optional.
    let call_creator_alt = if call_creator.server == server::HIDDEN_USER {
        cag.optional_jid("caller_pn").cloned()
    } else {
        cag.optional_jid("caller_lid").cloned()
    };

    if !cag.ok() {
        let errs = cag.into_result().err().unwrap_or_default();
        return Err(ClientError::Malformed(format!(
            "failed to parse <{}> attrs: {errs:?}",
            child.tag
        )));
    }

    let meta = BasicCallMeta {
        from,
        timestamp,
        call_creator,
        call_creator_alt,
        call_id,
        group_jid,
    };
    let remote = CallRemoteMeta {
        remote_platform,
        remote_version,
    };

    let event = match child.tag.as_str() {
        "offer" => CallEvent::Offer {
            meta,
            remote,
            data: child.clone(),
        },
        "offer_notice" => {
            let mut cag2 = child.attr_getter();
            let media = cag2.optional_string("media").unwrap_or("").to_owned();
            let notice_type = cag2.optional_string("type").unwrap_or("").to_owned();
            CallEvent::OfferNotice {
                meta,
                media,
                notice_type,
                data: child.clone(),
            }
        }
        "accept" => CallEvent::Accept {
            meta,
            remote,
            data: child.clone(),
        },
        "preaccept" => CallEvent::PreAccept {
            meta,
            remote,
            data: child.clone(),
        },
        "transport" => CallEvent::Transport {
            meta,
            remote,
            data: child.clone(),
        },
        "reject" => CallEvent::Reject {
            meta,
            data: child.clone(),
        },
        "terminate" => {
            let mut cag2 = child.attr_getter();
            let reason = cag2.optional_string("reason").unwrap_or("").to_owned();
            CallEvent::Terminate {
                meta,
                reason,
                data: child.clone(),
            }
        }
        "relaylatency" => CallEvent::RelayLatency {
            meta,
            data: child.clone(),
        },
        _ => CallEvent::Unknown { node: node.clone() },
    };

    Ok(event)
}

// ---------------------------------------------------------------------------
// Inbound dispatch.
// ---------------------------------------------------------------------------

/// Top-level handler for an inbound `<call>` stanza. Mirrors
/// `Client.handleCallEvent` upstream:
///
/// - parses the `<call>`'s single child via [`parse_call_node`];
/// - dispatches the matching typed [`Event`] on the client's event channel;
/// - sends the deferred `<ack class="call" type="<child-tag>"/>` so the server
///   doesn't redeliver. Upstream defers the ack via `maybeDeferredAck`; we send
///   it inline at the tail of this function once the application has been
///   notified.
///
/// Errors only on `<ack>` send failure — parsing always produces some event
/// (including [`Event::CallUnknown`] for shapes we don't recognize).
pub async fn handle_call_stanza(client: &Client, node: &Node) -> Result<(), ClientError> {
    let parsed = parse_call_node(node)?;

    // Pull the outer `<call type="audio|video">` once — the offer event needs
    // it to populate `BasicCallOffer.video`. Missing means audio.
    let outer_type = node.get_attr_str("type").unwrap_or("").to_owned();

    let event = match parsed {
        CallEvent::Offer { meta, remote: _, data } => {
            let basic = BasicCallOffer {
                call_id: meta.call_id.clone(),
                call_creator: meta.call_creator.clone(),
                video: outer_type == "video",
            };
            Event::CallOffer {
                call_id: meta.call_id,
                from: meta.from,
                timestamp: meta.timestamp,
                basic_call_offer: basic,
                raw: data,
            }
        }
        CallEvent::OfferNotice {
            meta,
            media,
            notice_type,
            data,
        } => Event::CallOfferNotice {
            call_id: meta.call_id,
            from: meta.from,
            timestamp: meta.timestamp,
            media,
            notice_type,
            raw: data,
        },
        CallEvent::Accept { meta, remote: _, data } => Event::CallAccept {
            call_id: meta.call_id,
            from: meta.from,
            timestamp: meta.timestamp,
            raw: data,
        },
        CallEvent::PreAccept { meta, remote: _, data } => Event::CallPreAccept {
            call_id: meta.call_id,
            from: meta.from,
            timestamp: meta.timestamp,
            raw: data,
        },
        CallEvent::Transport { meta, remote: _, data } => Event::CallTransport {
            call_id: meta.call_id,
            from: meta.from,
            timestamp: meta.timestamp,
            raw: data,
        },
        CallEvent::Terminate { meta, reason, data } => Event::CallTerminate {
            call_id: meta.call_id,
            from: meta.from,
            timestamp: meta.timestamp,
            reason,
            raw: data,
        },
        CallEvent::Reject { meta, data } => {
            // Upstream's `CallReject` doesn't have a `reason` attribute, but
            // some servers do include one — surface it when present, default
            // to an empty string otherwise.
            let reason = data
                .get_attr_str("reason")
                .map(|s| s.to_owned())
                .unwrap_or_default();
            Event::CallReject {
                call_id: meta.call_id,
                from: meta.from,
                timestamp: meta.timestamp,
                reason,
                raw: data,
            }
        }
        CallEvent::RelayLatency { meta, data } => Event::CallRelayLatency {
            call_id: meta.call_id,
            from: meta.from,
            timestamp: meta.timestamp,
            raw: data,
        },
        CallEvent::Unknown { node } => Event::CallUnknown { raw: node },
    };

    client.dispatch_event(event);

    // Send the deferred ack. Best-effort: the dispatch above must not be
    // gated on the socket being writable.
    let own_jid = client
        .device
        .jid()
        .cloned()
        .unwrap_or_else(Jid::default);
    let ack = build_call_ack_node(node, &own_jid);
    client.send_node(&ack).await
}

// ---------------------------------------------------------------------------
// Outbound builders.
// ---------------------------------------------------------------------------

/// Build a `<call>` ack node mirroring an inbound call signal — same id, with
/// the `to`/`from` fields swapped.
///
/// Upstream's `Client.handleCallEvent` defers the standard XMPP ack via
/// `maybeDeferredAck`; this helper produces the matching node so callers can
/// hand it to [`Client::send_node`].
pub fn build_call_ack_node(call_node: &Node, own_jid: &Jid) -> Node {
    let id = call_node
        .get_attr_str("id")
        .map(|s| s.to_owned())
        .unwrap_or_default();
    let from = call_node
        .get_attr_jid("from")
        .cloned()
        .unwrap_or_else(Jid::default);
    let class = call_node
        .children()
        .first()
        .map(|c| c.tag.clone())
        .unwrap_or_else(|| "call".to_owned());

    let mut attrs = Attrs::new();
    attrs.insert("id".into(), Value::String(id));
    attrs.insert("class".into(), Value::String("call".into()));
    attrs.insert("type".into(), Value::String(class));
    attrs.insert("to".into(), Value::Jid(from));
    attrs.insert("from".into(), Value::Jid(own_jid.clone()));
    Node::new("ack", attrs, None)
}

/// Build the `<call><reject .../></call>` envelope used by
/// [`Client::reject_call`]. Pulled out of the async method so unit tests can
/// introspect the shape without a live socket.
pub fn build_reject_call_node(
    own_jid: &Jid,
    call_creator: &Jid,
    call_id: &str,
    request_id: &str,
) -> Node {
    let own_non_ad = own_jid.to_non_ad();
    let creator_non_ad = call_creator.to_non_ad();

    let mut reject_attrs = Attrs::new();
    reject_attrs.insert("call-id".into(), Value::String(call_id.to_owned()));
    reject_attrs.insert("call-creator".into(), Value::Jid(creator_non_ad.clone()));
    reject_attrs.insert("count".into(), Value::String("0".into()));
    let reject = Node::new("reject", reject_attrs, None);

    let mut attrs = Attrs::new();
    attrs.insert("id".into(), Value::String(request_id.to_owned()));
    attrs.insert("from".into(), Value::Jid(own_non_ad));
    attrs.insert("to".into(), Value::Jid(creator_non_ad));
    Node::new("call", attrs, Some(Value::Nodes(vec![reject])))
}

impl Client {
    /// Reject an incoming call. Mirrors `Client.RejectCall` upstream:
    /// sends a `<call from="<own>" to="<creator>"><reject call-id=…
    /// call-creator=… count="0"/></call>` node. Both the outer JIDs and the
    /// inner `call-creator` are normalized to non-AD form.
    pub async fn reject_call(
        &self,
        call_creator: &Jid,
        call_id: &str,
    ) -> Result<(), ClientError> {
        let own = self
            .device
            .id
            .as_ref()
            .ok_or(ClientError::NotLoggedIn)?
            .clone();
        if own.is_empty() {
            return Err(ClientError::NotLoggedIn);
        }
        let request_id = generate_message_id(self);
        let node = build_reject_call_node(&own, call_creator, call_id, &request_id);
        self.send_node(&node).await
    }
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use wha_binary::{Attrs, Node, Value};
    use wha_types::jid::server;

    fn mk_call_node(child_tag: &str, child_attrs: Attrs) -> Node {
        let mut outer = Attrs::new();
        outer.insert(
            "from".into(),
            Value::Jid(Jid::new("100", server::DEFAULT_USER)),
        );
        outer.insert("t".into(), Value::String("1714521600".into()));
        outer.insert("platform".into(), Value::String("android".into()));
        outer.insert("version".into(), Value::String("2.24.0".into()));
        let child = Node::new(child_tag.to_owned(), child_attrs, None);
        Node::new("call", outer, Some(Value::Nodes(vec![child])))
    }

    fn child_attrs(call_id: &str, creator_user: &str, creator_server: &str) -> Attrs {
        let mut a = Attrs::new();
        a.insert("call-id".into(), Value::String(call_id.into()));
        a.insert(
            "call-creator".into(),
            Value::Jid(Jid::new(creator_user, creator_server)),
        );
        a
    }

    #[test]
    fn parse_call_node_offer() {
        let node = mk_call_node("offer", child_attrs("CALL-1", "100", server::DEFAULT_USER));
        let evt = parse_call_node(&node).expect("parse");
        match evt {
            CallEvent::Offer { meta, remote, data } => {
                assert_eq!(meta.call_id, "CALL-1");
                assert_eq!(meta.from.user, "100");
                assert_eq!(meta.timestamp, 1714521600);
                assert_eq!(meta.call_creator.user, "100");
                assert_eq!(remote.remote_platform, "android");
                assert_eq!(remote.remote_version, "2.24.0");
                assert_eq!(data.tag, "offer");
            }
            other => panic!("expected Offer, got {other:?}"),
        }
    }

    #[test]
    fn parse_call_node_terminate() {
        let mut child = child_attrs("CALL-2", "100", server::DEFAULT_USER);
        child.insert("reason".into(), Value::String("timeout".into()));
        let node = mk_call_node("terminate", child);
        let evt = parse_call_node(&node).expect("parse");
        match evt {
            CallEvent::Terminate { meta, reason, data } => {
                assert_eq!(meta.call_id, "CALL-2");
                assert_eq!(reason, "timeout");
                assert_eq!(data.tag, "terminate");
            }
            other => panic!("expected Terminate, got {other:?}"),
        }
    }

    #[test]
    fn build_reject_call_node_has_required_attrs() {
        let own = Jid::new_ad("111", 0, 5); // AD form should be stripped
        let creator = Jid::new_ad("222", 0, 9);
        let n = build_reject_call_node(&own, &creator, "CALL-X", "REQ-1");

        assert_eq!(n.tag, "call");
        assert_eq!(n.get_attr_str("id"), Some("REQ-1"));
        let from = n.get_attr_jid("from").expect("from JID");
        let to = n.get_attr_jid("to").expect("to JID");
        // Both outer JIDs must be non-AD (device stripped).
        assert_eq!(from.user, "111");
        assert_eq!(from.device, 0);
        assert_eq!(to.user, "222");
        assert_eq!(to.device, 0);

        let kids = n.children();
        assert_eq!(kids.len(), 1, "must contain exactly one <reject>");
        let reject = &kids[0];
        assert_eq!(reject.tag, "reject");
        assert_eq!(reject.get_attr_str("call-id"), Some("CALL-X"));
        assert_eq!(reject.get_attr_str("count"), Some("0"));
        let creator_attr = reject.get_attr_jid("call-creator").expect("call-creator");
        assert_eq!(creator_attr.user, "222");
        assert_eq!(creator_attr.device, 0, "call-creator must also be non-AD");
    }

    #[test]
    fn parse_call_node_unknown_when_multiple_children() {
        let mut outer = Attrs::new();
        outer.insert(
            "from".into(),
            Value::Jid(Jid::new("100", server::DEFAULT_USER)),
        );
        let kids = vec![Node::tag_only("offer"), Node::tag_only("terminate")];
        let node = Node::new("call", outer, Some(Value::Nodes(kids)));
        let evt = parse_call_node(&node).expect("parse");
        assert!(matches!(evt, CallEvent::Unknown { .. }));
    }

    #[test]
    fn parse_call_node_lid_creator_picks_caller_pn() {
        let mut child = Attrs::new();
        child.insert("call-id".into(), Value::String("CALL-LID".into()));
        child.insert(
            "call-creator".into(),
            Value::Jid(Jid::new("777", server::HIDDEN_USER)),
        );
        child.insert(
            "caller_pn".into(),
            Value::Jid(Jid::new("888", server::DEFAULT_USER)),
        );
        let node = mk_call_node("offer", child);
        let evt = parse_call_node(&node).expect("parse");
        match evt {
            CallEvent::Offer { meta, .. } => {
                assert_eq!(meta.call_creator.server, server::HIDDEN_USER);
                let alt = meta.call_creator_alt.expect("caller_pn populated");
                assert_eq!(alt.user, "888");
                assert_eq!(alt.server, server::DEFAULT_USER);
            }
            other => panic!("expected Offer, got {other:?}"),
        }
    }

    #[test]
    fn build_call_ack_node_swaps_to_and_from() {
        let mut outer = Attrs::new();
        outer.insert("id".into(), Value::String("REQ-7".into()));
        outer.insert(
            "from".into(),
            Value::Jid(Jid::new("100", server::DEFAULT_USER)),
        );
        let child = Node::tag_only("offer");
        let call = Node::new("call", outer, Some(Value::Nodes(vec![child])));
        let own = Jid::new("999", server::DEFAULT_USER);

        let ack = build_call_ack_node(&call, &own);
        assert_eq!(ack.tag, "ack");
        assert_eq!(ack.get_attr_str("id"), Some("REQ-7"));
        assert_eq!(ack.get_attr_str("class"), Some("call"));
        assert_eq!(ack.get_attr_str("type"), Some("offer"));
        assert_eq!(ack.get_attr_jid("to").unwrap().user, "100");
        assert_eq!(ack.get_attr_jid("from").unwrap().user, "999");
    }

    #[test]
    fn parse_call_node_accept_and_preaccept_and_transport() {
        // accept
        let n = mk_call_node("accept", child_attrs("CALL-A", "100", server::DEFAULT_USER));
        match parse_call_node(&n).expect("parse") {
            CallEvent::Accept { meta, .. } => assert_eq!(meta.call_id, "CALL-A"),
            other => panic!("expected Accept, got {other:?}"),
        }

        // preaccept (note: upstream uses `preaccept`, not `pre_accept`)
        let n = mk_call_node(
            "preaccept",
            child_attrs("CALL-P", "100", server::DEFAULT_USER),
        );
        match parse_call_node(&n).expect("parse") {
            CallEvent::PreAccept { meta, .. } => assert_eq!(meta.call_id, "CALL-P"),
            other => panic!("expected PreAccept, got {other:?}"),
        }

        // transport
        let n = mk_call_node(
            "transport",
            child_attrs("CALL-T", "100", server::DEFAULT_USER),
        );
        match parse_call_node(&n).expect("parse") {
            CallEvent::Transport { meta, .. } => assert_eq!(meta.call_id, "CALL-T"),
            other => panic!("expected Transport, got {other:?}"),
        }

        // reject
        let n = mk_call_node("reject", child_attrs("CALL-R", "100", server::DEFAULT_USER));
        match parse_call_node(&n).expect("parse") {
            CallEvent::Reject { meta, .. } => assert_eq!(meta.call_id, "CALL-R"),
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_call_stanza_dispatches_call_offer_event() {
        use std::sync::Arc;
        use wha_store::MemoryStore;

        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, mut evt_rx) = Client::new(device);

        // <call from=100 t=... type="video"><offer call-id=... call-creator=.../></call>
        let mut outer = Attrs::new();
        outer.insert(
            "from".into(),
            Value::Jid(Jid::new("100", server::DEFAULT_USER)),
        );
        outer.insert("t".into(), Value::String("1714521600".into()));
        outer.insert("type".into(), Value::String("video".into()));
        let mut child = Attrs::new();
        child.insert("call-id".into(), Value::String("CALL-VID".into()));
        child.insert(
            "call-creator".into(),
            Value::Jid(Jid::new("100", server::DEFAULT_USER)),
        );
        let offer = Node::new("offer", child, None);
        let call = Node::new("call", outer, Some(Value::Nodes(vec![offer])));

        // The send_node call after dispatch will fail with NotConnected — that's
        // expected and surfaces as a returned error. The event dispatch above
        // it must still have happened.
        let _ = handle_call_stanza(&cli, &call).await;

        match evt_rx.try_recv() {
            Ok(Event::CallOffer {
                call_id,
                from,
                timestamp,
                basic_call_offer,
                ..
            }) => {
                assert_eq!(call_id, "CALL-VID");
                assert_eq!(from.user, "100");
                assert_eq!(timestamp, 1714521600);
                assert_eq!(basic_call_offer.call_id, "CALL-VID");
                assert_eq!(basic_call_offer.call_creator.user, "100");
                assert!(basic_call_offer.video, "type=video should set video=true");
            }
            other => panic!("expected Event::CallOffer, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_call_stanza_dispatches_terminate_with_reason() {
        use std::sync::Arc;
        use wha_store::MemoryStore;

        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, mut evt_rx) = Client::new(device);

        let mut child = child_attrs("CALL-END", "100", server::DEFAULT_USER);
        child.insert("reason".into(), Value::String("timeout".into()));
        let call = mk_call_node("terminate", child);

        let _ = handle_call_stanza(&cli, &call).await;

        match evt_rx.try_recv() {
            Ok(Event::CallTerminate {
                call_id,
                reason,
                from,
                ..
            }) => {
                assert_eq!(call_id, "CALL-END");
                assert_eq!(reason, "timeout");
                assert_eq!(from.user, "100");
            }
            other => panic!("expected Event::CallTerminate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_call_stanza_dispatches_reject_event() {
        use std::sync::Arc;
        use wha_store::MemoryStore;

        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, mut evt_rx) = Client::new(device);

        let call = mk_call_node("reject", child_attrs("CALL-NO", "100", server::DEFAULT_USER));
        let _ = handle_call_stanza(&cli, &call).await;

        match evt_rx.try_recv() {
            Ok(Event::CallReject { call_id, from, .. }) => {
                assert_eq!(call_id, "CALL-NO");
                assert_eq!(from.user, "100");
            }
            other => panic!("expected Event::CallReject, got {other:?}"),
        }
    }
}
