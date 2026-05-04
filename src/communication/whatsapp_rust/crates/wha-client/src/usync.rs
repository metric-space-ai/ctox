//! Device-discovery via the `usync` IQ.
//!
//! Mirrors `_upstream/whatsmeow/user.go::usync` + `GetUserDevices` +
//! `parseDeviceList`.
//!
//! On the wire we send:
//!
//! ```xml
//! <iq id="..." to="s.whatsapp.net" type="get" xmlns="usync">
//!   <usync sid="..." mode="query" last="true" index="0" context="message">
//!     <query>
//!       <devices version="2"/>
//!     </query>
//!     <list>
//!       <user jid="<jid>"/>   (one per input jid, stripped to non-AD form)
//!     </list>
//!   </usync>
//! </iq>
//! ```
//!
//! and receive:
//!
//! ```xml
//! <iq type="result"><usync><list>
//!   <user jid="...">
//!     <devices>
//!       <device-list>
//!         <device id="0"/>
//!         <device id="42"/>
//!         ...
//!       </device-list>
//!     </devices>
//!   </user>
//! </list></usync></iq>
//! ```
//!
//! For each `<device id="N"/>` we emit `<user>:N@s.whatsapp.net` (where
//! `N == 0` means the user's primary phone — emitted as plain
//! `<user>@s.whatsapp.net` since `device == 0` is the canonical "no device
//! suffix" form). We deliberately do NOT filter the local device here: the
//! caller (`send_message::send_text`) is in a better position to know which
//! AD-JID matches the running client and excludes it before fanning out.

use wha_binary::{Attrs, Node, Value};
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

/// Fetch the list of linked-device JIDs for each input JID.
///
/// The output flattens all responses: caller hands in `[alice, bob]` and
/// gets back `[alice@…, alice:7@…, bob@…, bob:42@…]`. Order within a single
/// user is the order the server returns; order between users matches the
/// input order (we walk the response `<list>` looking up by JID).
///
/// Bot JIDs and Messenger JIDs are not supported in this minimal port — we
/// fail the iq via `Malformed` if the server reflects something we don't
/// understand. Sufficient for `s.whatsapp.net` user-server JIDs which is
/// the only thing `send_text` (single-recipient DM fan-out) calls us with.
pub async fn fetch_user_devices(
    client: &Client,
    jids: &[Jid],
) -> Result<Vec<Jid>, ClientError> {
    if !client.is_connected() {
        return Err(ClientError::NotConnected);
    }
    if jids.is_empty() {
        return Ok(Vec::new());
    }

    // Build the `<list>` payload: one `<user jid=...>` child per input JID,
    // stripped to non-AD form (mirrors `jid.ToNonAD()` upstream).
    let user_nodes: Vec<Node> = jids
        .iter()
        .map(|j| {
            let mut a = Attrs::new();
            a.insert("jid".into(), Value::Jid(j.to_non_ad()));
            Node::new("user", a, None)
        })
        .collect();

    // <query><devices version="2"/></query>
    let mut dev_attrs = Attrs::new();
    dev_attrs.insert("version".into(), Value::String("2".into()));
    let devices = Node::new("devices", dev_attrs, None);
    let query = Node::new("query", Attrs::new(), Some(Value::Nodes(vec![devices])));
    let list = Node::new("list", Attrs::new(), Some(Value::Nodes(user_nodes)));

    // <usync ...><query/><list/></usync>
    let mut usync_attrs = Attrs::new();
    usync_attrs.insert("sid".into(), Value::String(client.generate_request_id()));
    usync_attrs.insert("mode".into(), Value::String("query".into()));
    usync_attrs.insert("last".into(), Value::String("true".into()));
    usync_attrs.insert("index".into(), Value::String("0".into()));
    usync_attrs.insert("context".into(), Value::String("message".into()));
    let usync = Node::new(
        "usync",
        usync_attrs,
        Some(Value::Nodes(vec![query, list])),
    );

    let resp = client
        .send_iq(
            InfoQuery::new("usync", IqType::Get)
                .to(Jid::new("", wha_types::Server::DEFAULT_USER))
                .content(Value::Nodes(vec![usync])),
        )
        .await?;

    if let Some(err) = iq_error_from_response(&resp) {
        return Err(err);
    }

    // The `mode="query"` device-list reply doesn't carry `<lid>` children, but
    // any `<lid val="…"/>` the server volunteered is worth persisting now so
    // future calls don't re-query. Mirrors the `PutManyLIDMappings` pass
    // upstream's `GetUserInfo` runs after every `usync` reply.
    persist_lid_mappings_from_usync(client, &resp).await;

    parse_usync_response(&resp)
}

/// Walk a usync IQ result and persist every `(user_jid, <lid val="…"/>)` pair
/// it volunteers. `user_jid` is the outer `<user jid="…">` attr — i.e. the PN
/// JID — and the inner `<lid val="…"/>` carries the matching LID. Mirrors
/// upstream's `GetUserInfo` post-processing pass.
///
/// Errors are logged at warn but never propagated: a usync that succeeded but
/// whose LID side-table failed to persist should not fail the caller.
pub async fn persist_lid_mappings_from_usync(client: &Client, resp: &Node) {
    let list = match resp.child_by_tag(&["usync", "list"]) {
        Some(l) => l,
        None => return,
    };
    let mut pairs: Vec<(Jid, Jid)> = Vec::new();
    for user in list.children() {
        if user.tag != "user" {
            continue;
        }
        let pn = match user.get_attr_jid("jid") {
            Some(j) => j.clone(),
            None => continue,
        };
        if let Some(lid_node) = user.child_by_tag(&["lid"]) {
            if let Some(lid) = lid_node.get_attr_jid("val") {
                pairs.push((lid.clone(), pn));
            }
        }
    }
    for (lid, pn) in pairs {
        if let Err(e) = client
            .device
            .lids
            .put_lid_pn_mapping(lid.clone(), pn.clone())
            .await
        {
            tracing::warn!(
                "usync: failed to persist LID↔PN mapping ({lid} ↔ {pn}): {e}"
            );
        }
    }
}

/// Parse the body of a successful `<iq type="result">` carrying the usync
/// device-list reply. Walks `<usync><list>`, then each `<user jid=...>`,
/// then `<devices><device-list>`, emitting one AD-JID per `<device id=...>`.
pub fn parse_usync_response(resp: &Node) -> Result<Vec<Jid>, ClientError> {
    let list = resp
        .child_by_tag(&["usync", "list"])
        .ok_or_else(|| ClientError::Malformed("usync response missing <usync><list>".into()))?;
    let mut out: Vec<Jid> = Vec::new();
    for user in list.children() {
        if user.tag != "user" {
            continue;
        }
        let user_jid = match user.get_attr_jid("jid") {
            Some(j) => j.clone(),
            None => continue,
        };
        let device_list = match user.child_by_tag(&["devices", "device-list"]) {
            Some(dl) => dl,
            None => continue,
        };
        for d in device_list.children() {
            if d.tag != "device" {
                continue;
            }
            let id_str = match d.get_attr_str("id") {
                Some(s) => s,
                None => continue,
            };
            let device_id: u16 = match id_str.parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            // `device == 0` is the primary phone; render that as the bare
            // `<user>@server` form (no device suffix). All non-zero device
            // ids render as `<user>:N@server`.
            let mut ad = user_jid.to_non_ad();
            ad.device = device_id;
            out.push(ad);
        }
    }
    Ok(out)
}

/// `<iq type="error">` extractor. Mirrors the same helper used elsewhere.
fn iq_error_from_response(resp: &Node) -> Option<ClientError> {
    if resp.get_attr_str("type") != Some("error") {
        return None;
    }
    let err = resp.child_by_tag(&["error"])?;
    let code = err
        .get_attr_str("code")
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let text = err.get_attr_str("text").unwrap_or("").to_owned();
    Some(ClientError::Iq { code, text })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-build the `<iq><usync><list><user…>` shape the live server
    /// returns and assert we walk it correctly. Two users, two devices each
    /// (one primary `id=0`, one secondary).
    #[test]
    fn parse_usync_two_users_two_devices_each() {
        let make_device = |id: &str| {
            let mut a = Attrs::new();
            a.insert("id".into(), Value::String(id.into()));
            Node::new("device", a, None)
        };
        let make_user = |user: &str, devs: Vec<&str>| {
            let device_list = Node::new(
                "device-list",
                Attrs::new(),
                Some(Value::Nodes(devs.into_iter().map(make_device).collect())),
            );
            let devices = Node::new(
                "devices",
                Attrs::new(),
                Some(Value::Nodes(vec![device_list])),
            );
            let mut user_attrs = Attrs::new();
            user_attrs.insert(
                "jid".into(),
                Value::Jid(format!("{user}@s.whatsapp.net").parse::<Jid>().unwrap()),
            );
            Node::new("user", user_attrs, Some(Value::Nodes(vec![devices])))
        };
        let alice = make_user("111", vec!["0", "7"]);
        let bob = make_user("222", vec!["0", "42"]);
        let list = Node::new("list", Attrs::new(), Some(Value::Nodes(vec![alice, bob])));
        let usync = Node::new("usync", Attrs::new(), Some(Value::Nodes(vec![list])));
        let iq = Node::new("iq", Attrs::new(), Some(Value::Nodes(vec![usync])));

        let parsed = parse_usync_response(&iq).expect("parse usync");
        assert_eq!(parsed.len(), 4, "got {parsed:?}");
        assert_eq!(parsed[0].to_string(), "111@s.whatsapp.net");
        assert_eq!(parsed[1].to_string(), "111:7@s.whatsapp.net");
        assert_eq!(parsed[2].to_string(), "222@s.whatsapp.net");
        assert_eq!(parsed[3].to_string(), "222:42@s.whatsapp.net");
    }

    /// Missing `<list>` surfaces as `Malformed`.
    #[test]
    fn parse_usync_missing_list_errors() {
        let iq = Node::new("iq", Attrs::new(), None);
        let r = parse_usync_response(&iq);
        assert!(matches!(r, Err(ClientError::Malformed(_))), "got {r:?}");
    }

    /// `<device>` children with non-numeric ids are silently skipped (lenient
    /// parsing — the live server occasionally adds attrs we don't understand).
    #[test]
    fn parse_usync_skips_garbage_device_ids() {
        let mut a = Attrs::new();
        a.insert("id".into(), Value::String("not-a-number".into()));
        let bad = Node::new("device", a, None);

        let mut a2 = Attrs::new();
        a2.insert("id".into(), Value::String("3".into()));
        let good = Node::new("device", a2, None);

        let dl = Node::new(
            "device-list",
            Attrs::new(),
            Some(Value::Nodes(vec![bad, good])),
        );
        let devs = Node::new("devices", Attrs::new(), Some(Value::Nodes(vec![dl])));
        let mut ua = Attrs::new();
        ua.insert(
            "jid".into(),
            Value::Jid("9@s.whatsapp.net".parse::<Jid>().unwrap()),
        );
        let user = Node::new("user", ua, Some(Value::Nodes(vec![devs])));
        let list = Node::new("list", Attrs::new(), Some(Value::Nodes(vec![user])));
        let usync = Node::new("usync", Attrs::new(), Some(Value::Nodes(vec![list])));
        let iq = Node::new("iq", Attrs::new(), Some(Value::Nodes(vec![usync])));

        let parsed = parse_usync_response(&iq).expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].to_string(), "9:3@s.whatsapp.net");
    }

    /// `persist_lid_mappings_from_usync` walks `<usync><list><user>` looking
    /// for inner `<lid val="…"/>` children and persists each `(lid, pn)` pair
    /// it finds into the LidStore. Bare device-list responses (no `<lid>`)
    /// must leave the store empty.
    #[tokio::test]
    async fn persist_lid_mappings_from_usync_writes_pairs_to_store() {
        use std::sync::Arc;
        use wha_store::MemoryStore;

        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);

        // Two users: alice has a <lid>, bob does not.
        let alice_pn: Jid = "111@s.whatsapp.net".parse().unwrap();
        let alice_lid: Jid = "AAAA@lid".parse().unwrap();
        let bob_pn: Jid = "222@s.whatsapp.net".parse().unwrap();

        let mut alice_attrs = Attrs::new();
        alice_attrs.insert("jid".into(), Value::Jid(alice_pn.clone()));
        let mut lid_attrs = Attrs::new();
        lid_attrs.insert("val".into(), Value::Jid(alice_lid.clone()));
        let lid_node = Node::new("lid", lid_attrs, None);
        let alice = Node::new("user", alice_attrs, Some(Value::Nodes(vec![lid_node])));

        let mut bob_attrs = Attrs::new();
        bob_attrs.insert("jid".into(), Value::Jid(bob_pn.clone()));
        let bob = Node::new("user", bob_attrs, None);

        let list = Node::new("list", Attrs::new(), Some(Value::Nodes(vec![alice, bob])));
        let usync = Node::new("usync", Attrs::new(), Some(Value::Nodes(vec![list])));
        let iq = Node::new("iq", Attrs::new(), Some(Value::Nodes(vec![usync])));

        // Pre-condition: store is empty.
        assert!(client
            .device
            .lids
            .get_pn_for_lid(&alice_lid)
            .await
            .unwrap()
            .is_none());

        persist_lid_mappings_from_usync(&client, &iq).await;

        // Alice's mapping was persisted.
        assert_eq!(
            client
                .device
                .lids
                .get_pn_for_lid(&alice_lid)
                .await
                .unwrap(),
            Some(alice_pn.clone())
        );
        assert_eq!(
            client.device.lids.get_lid_for_pn(&alice_pn).await.unwrap(),
            Some(alice_lid)
        );
        // Bob has no LID → nothing to persist for him.
        assert!(client
            .device
            .lids
            .get_lid_for_pn(&bob_pn)
            .await
            .unwrap()
            .is_none());
    }
}
