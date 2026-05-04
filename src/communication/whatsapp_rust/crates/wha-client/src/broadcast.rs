//! Broadcast-list helpers — port of `whatsmeow/broadcast.go`.
//!
//! Whatsmeow's broadcast.go is small: it issues a `<iq xmlns="broadcast">`
//! request to `s.whatsapp.net`, asking for the participant list of a
//! broadcast-list JID, and parses the `<list>` response.  The original Go
//! version forwards the actual data fetch to `getStatusBroadcastRecipients`
//! (a status-privacy lookup) for the special `status@broadcast` JID and
//! returns `ErrBroadcastListUnsupported` for anything else.  The Rust port
//! exposes the network primitive directly: build the IQ, parse the
//! response, and let upper layers compose policy.
//!
//! The wire format used by WhatsApp's broadcast IQ is, in pseudo-XML:
//!
//! ```text
//! <iq xmlns="broadcast" type="get" to="s.whatsapp.net">
//!   <list jid="<broadcast-list-jid>"/>
//! </iq>
//! ```
//!
//! and the response carries:
//!
//! ```text
//! <iq type="result">
//!   <list>
//!     <recipient jid="<participant-jid>"/>
//!     ...
//!   </list>
//! </iq>
//! ```

use wha_binary::{Attrs, Node, Value};
use wha_types::{Jid, Server};

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

/// Information about a WhatsApp broadcast list.
///
/// Mirrors whatsmeow's `BroadcastListInfo`; only the fields that survived the
/// move to the public Web protocol are retained.  The `recipients` field is
/// always populated; `name` is only set when the server echoes a stored
/// label for the list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BroadcastListInfo {
    /// JID of the broadcast list itself (e.g. `<digits>@broadcast`).
    pub jid: Jid,
    /// Optional human-readable label.
    pub name: String,
    /// Recipients (non-AD JIDs) that are members of the list.
    pub recipients: Vec<Jid>,
}

/// Build the `<iq xmlns="broadcast" type="get" to="s.whatsapp.net">
/// <list jid="…"/></iq>` info query for the given broadcast-list JID.
///
/// Kept `pub(crate)` so the unit tests can poke at it without going through
/// the live socket.
pub(crate) fn build_get_broadcast_iq(list: &Jid) -> InfoQuery {
    let mut list_attrs = Attrs::new();
    list_attrs.insert("jid".into(), Value::Jid(list.clone()));
    let list_node = Node::new("list", list_attrs, None);

    InfoQuery::new("broadcast", IqType::Get)
        .to(Jid::new("", Server::DEFAULT_USER))
        .content(Value::Nodes(vec![list_node]))
}

/// Extract participant JIDs from a `<list>` node.
///
/// Accepts either a `<list>` directly or any node that *contains* a `<list>`
/// as a direct child (e.g. the top-level `<iq>` response).  Children are
/// expected to be `<recipient jid="…"/>` elements; anything else is
/// silently skipped.  Both `Value::Jid` and `Value::String` JID attributes
/// are accepted, mirroring how whatsmeow reads heterogeneous node trees.
pub fn parse_broadcast_participants(node: &Node) -> Vec<Jid> {
    let list = if node.tag == "list" {
        node
    } else if let Some(child) = node.child_by_tag(&["list"]) {
        child
    } else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(list.children().len());
    for child in list.children() {
        if child.tag != "recipient" {
            continue;
        }
        if let Some(jid) = child.get_attr_jid("jid") {
            out.push(jid.clone());
            continue;
        }
        if let Some(s) = child.get_attr_str("jid") {
            if let Ok(jid) = Jid::parse(s) {
                out.push(jid);
            }
        }
    }
    out
}

impl Client {
    /// Fetch the participants of a broadcast list from the server.
    ///
    /// Issues the `<iq xmlns="broadcast" type="get">` IQ and returns the
    /// participant JIDs found in the `<list>` reply.  Higher-level callers
    /// are responsible for any additional policy (deduplication against
    /// `status@broadcast`'s privacy settings, appending the device's own
    /// JID, etc.) — this method is a thin wire transport.
    pub async fn get_broadcast_list_participants(
        &self,
        list: &Jid,
    ) -> Result<Vec<Jid>, ClientError> {
        if list.is_empty() {
            return Err(ClientError::Malformed(
                "broadcast list jid must not be empty".into(),
            ));
        }
        let resp = self.send_iq(build_get_broadcast_iq(list)).await?;
        Ok(parse_broadcast_participants(&resp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_get_broadcast_iq_has_xmlns() {
        let list: Jid = "1234567890@broadcast".parse().unwrap();
        let q = build_get_broadcast_iq(&list);
        let node = q.into_node("test-id".into());

        assert_eq!(node.tag, "iq");
        assert_eq!(node.get_attr_str("xmlns"), Some("broadcast"));
        assert_eq!(node.get_attr_str("type"), Some("get"));
        assert_eq!(node.get_attr_str("id"), Some("test-id"));
        // `to` is the WhatsApp server JID (server-only, no user part).
        let to = node
            .get_attr_jid("to")
            .expect("`to` attr must be a JID value");
        assert_eq!(to.server, Server::DEFAULT_USER);
        assert!(to.user.is_empty());

        // Body must be a single <list jid="…"/> node carrying the list JID.
        let children = match &node.content {
            Value::Nodes(n) => n,
            other => panic!("iq content must be Nodes, got {other:?}"),
        };
        assert_eq!(children.len(), 1);
        let list_node = &children[0];
        assert_eq!(list_node.tag, "list");
        let attr_jid = list_node
            .get_attr_jid("jid")
            .expect("list@jid must be Value::Jid");
        assert_eq!(attr_jid.user, "1234567890");
        assert_eq!(attr_jid.server, Server::BROADCAST);
    }

    #[test]
    fn parse_broadcast_participants_extracts_jids() {
        // Synthesise a `<list>` with three `<recipient>` children plus a
        // junk node that must be skipped.
        let mk_recipient = |jid_str: &str| {
            let mut a = Attrs::new();
            a.insert("jid".into(), Value::Jid(jid_str.parse::<Jid>().unwrap()));
            Node::new("recipient", a, None)
        };
        let list = Node::new(
            "list",
            Attrs::new(),
            Some(Value::Nodes(vec![
                mk_recipient("111@s.whatsapp.net"),
                mk_recipient("222@s.whatsapp.net"),
                Node::tag_only("garbage"),
                mk_recipient("333@s.whatsapp.net"),
            ])),
        );

        let got = parse_broadcast_participants(&list);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].user, "111");
        assert_eq!(got[1].user, "222");
        assert_eq!(got[2].user, "333");
        for j in &got {
            assert_eq!(j.server, Server::DEFAULT_USER);
        }

        // Also accept being handed the wrapping <iq> node.
        let iq = Node::new("iq", Attrs::new(), Some(Value::Nodes(vec![list])));
        let via_iq = parse_broadcast_participants(&iq);
        assert_eq!(via_iq.len(), 3);
    }

    #[test]
    fn parse_broadcast_participants_accepts_string_jid_attrs() {
        // The decoder occasionally hands back JID attributes as strings.
        let mut a = Attrs::new();
        a.insert("jid".into(), Value::String("999@s.whatsapp.net".into()));
        let recipient = Node::new("recipient", a, None);
        let list = Node::new(
            "list",
            Attrs::new(),
            Some(Value::Nodes(vec![recipient])),
        );
        let got = parse_broadcast_participants(&list);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].user, "999");
        assert_eq!(got[0].server, Server::DEFAULT_USER);
    }

    #[test]
    fn parse_broadcast_participants_returns_empty_when_no_list() {
        let bare = Node::tag_only("iq");
        assert!(parse_broadcast_participants(&bare).is_empty());
    }

    #[tokio::test]
    async fn get_broadcast_list_participants_rejects_empty_jid() {
        use std::sync::Arc;
        use wha_store::MemoryStore;
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let r = cli.get_broadcast_list_participants(&Jid::default()).await;
        assert!(matches!(r, Err(ClientError::Malformed(_))));
    }
}
