//! PreKey upload + refresh.
//!
//! Mirrors `whatsmeow/prekeys.go`: we pre-generate one-time pre-keys and
//! upload them in batches; once consumed we refill. The signed pre-key is
//! built once at device-creation time (see `MemoryStore::new_device`) and
//! re-attached to every upload IQ. Upload is via:
//!
//! ```xml
//! <iq xmlns="encrypt" type="set" to="s.whatsapp.net" id="...">
//!   <registration>...4-byte big-endian registration_id...</registration>
//!   <type>...0x05 (DjbType)...</type>
//!   <identity>...32-byte identity pubkey...</identity>
//!   <list>
//!     <key><id>...3-byte big-endian id...</id><value>...32-byte pubkey...</value></key>
//!     ...
//!   </list>
//!   <skey>
//!     <id>...3-byte big-endian id...</id>
//!     <value>...32-byte pubkey...</value>
//!     <signature>...64-byte signature...</signature>
//!   </skey>
//! </iq>
//! ```

use std::collections::HashMap;

use wha_binary::{Attrs, Node, Value};
use wha_crypto::PreKey;
use wha_signal::PreKeyBundle;
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

/// Mirrors upstream `MinPreKeyCount`: when the server's count of usable
/// uploaded prekeys drops below this, we refill.
pub const MIN_PRE_KEY_COUNT: u32 = 5;

/// Mirrors upstream `WantedPreKeyCount`: the target batch size we upload
/// each refill.
pub const WANTED_PRE_KEY_COUNT: u32 = 50;

/// X25519 / "DjbType" key-format byte that wraps the identity public key in
/// the upload IQ. Mirrors `ecc.DjbType` in libsignal.
const DJB_TYPE: u8 = 0x05;

/// Upload a fresh batch of one-time pre-keys plus our signed pre-key to the
/// server. Mirrors `Client.uploadPreKeys` in `whatsmeow/prekeys.go`.
///
/// We ask the store for `WANTED_PRE_KEY_COUNT - already_uploaded` new keys,
/// build the IQ, send it, and on success mark every prekey up to and
/// including the highest id we just uploaded as "uploaded".
pub async fn upload_pre_keys(client: &Client) -> Result<(), ClientError> {
    let already = client.device.pre_keys.uploaded_pre_key_count().await? as u32;
    let needed = WANTED_PRE_KEY_COUNT.saturating_sub(already);
    if needed == 0 {
        // Nothing to do — the store already has WANTED prekeys uploaded.
        return Ok(());
    }

    let pre_keys = client.device.pre_keys.get_or_gen_pre_keys(needed).await?;
    if pre_keys.is_empty() {
        // Defensive: if the store handed us nothing, there's nothing to upload.
        return Ok(());
    }

    let highest_id = pre_keys.iter().map(|k| k.key_id).max().unwrap_or(0);

    let identity_pub = client.device.identity_key.public;
    let registration_id = client.device.registration_id;
    let signed = &client.device.signed_pre_key;

    let node = build_upload_pre_keys_iq(registration_id, &identity_pub, &pre_keys, signed);

    // The IQ may fail with a `<iq type="error">…</iq>` reply. `Client::send_iq`
    // hands back the raw node, so we have to inspect `type` and extract the
    // error code/text ourselves.
    let resp = client
        .send_iq(
            InfoQuery::new("encrypt", IqType::Set)
                .to(Jid::new("", wha_types::Server::DEFAULT_USER))
                .content(node.content.clone()),
        )
        .await?;

    if let Some(err) = iq_error_from_response(&resp) {
        return Err(err);
    }

    client.device.pre_keys.mark_pre_keys_uploaded(highest_id).await?;
    Ok(())
}

/// Refresh the prekey batch on the server iff the locally-tracked
/// uploaded-count has dropped below [`MIN_PRE_KEY_COUNT`]. Mirrors the
/// `getServerPreKeyCount` + conditional `uploadPreKeys` flow upstream
/// (we use the locally tracked count as a proxy for "what the server has",
/// matching the store contract documented in `traits::PreKeyStore`).
pub async fn refresh_pre_keys_if_low(client: &Client) -> Result<(), ClientError> {
    let count = client.device.pre_keys.uploaded_pre_key_count().await? as u32;
    if count < MIN_PRE_KEY_COUNT {
        upload_pre_keys(client).await?;
    }
    Ok(())
}

/// Build the full `<iq xmlns="encrypt" type="set" to="s.whatsapp.net">` Node
/// that uploads `one_time_keys` plus `signed_prekey`. Returns a Node ready to
/// hand to [`wha_binary::marshal`].
///
/// This is exposed so tests can inspect the wire shape without needing a live
/// client / socket.
pub fn build_upload_pre_keys_iq(
    registration_id: u32,
    identity_pub: &[u8; 32],
    one_time_keys: &[PreKey],
    signed_prekey: &PreKey,
) -> Node {
    let registration_bytes = registration_id_to_bytes(registration_id);

    let registration = Node::new(
        "registration",
        Attrs::new(),
        Some(Value::Bytes(registration_bytes.to_vec())),
    );
    let key_type = Node::new("type", Attrs::new(), Some(Value::Bytes(vec![DJB_TYPE])));
    let identity = Node::new(
        "identity",
        Attrs::new(),
        Some(Value::Bytes(identity_pub.to_vec())),
    );

    let key_nodes: Vec<Node> = one_time_keys.iter().map(prekey_to_node).collect();
    let list = Node::new("list", Attrs::new(), Some(Value::Nodes(key_nodes)));

    let skey = signed_prekey_to_node(signed_prekey);

    let mut attrs = Attrs::new();
    attrs.insert("xmlns".into(), Value::String("encrypt".into()));
    attrs.insert("type".into(), Value::String("set".into()));
    attrs.insert(
        "to".into(),
        Value::Jid(Jid::new("", wha_types::Server::DEFAULT_USER)),
    );
    Node::new(
        "iq",
        attrs,
        Some(Value::Nodes(vec![registration, key_type, identity, list, skey])),
    )
}

/// 4-byte big-endian encoding of `id`. Mirrors `binary.BigEndian.PutUint32`.
pub fn registration_id_to_bytes(id: u32) -> [u8; 4] {
    id.to_be_bytes()
}

/// 3-byte big-endian encoding of a prekey id. Upstream packs the id as
/// `binary.BigEndian.PutUint32` then drops the leading byte (`keyID[1:]`),
/// which is exactly the low 24 bits of the u32 in big-endian order.
pub fn prekey_id_to_bytes(id: u32) -> [u8; 3] {
    let full = id.to_be_bytes();
    [full[1], full[2], full[3]]
}

/// Build the `<key><id/><value/></key>` Node for a one-time prekey.
fn prekey_to_node(key: &PreKey) -> Node {
    let id_bytes = prekey_id_to_bytes(key.key_id);
    let id = Node::new("id", Attrs::new(), Some(Value::Bytes(id_bytes.to_vec())));
    let value = Node::new(
        "value",
        Attrs::new(),
        Some(Value::Bytes(key.key_pair.public.to_vec())),
    );
    Node::new("key", Attrs::new(), Some(Value::Nodes(vec![id, value])))
}

/// Build the `<skey><id/><value/><signature/></skey>` Node for a signed
/// prekey. Panics nowhere — if the signature is absent (which should never
/// happen for a key created via `MemoryStore::new_device`) we emit an empty
/// `<signature>` so the shape is still well-formed and the bug is caught
/// loudly by the server / by tests.
fn signed_prekey_to_node(key: &PreKey) -> Node {
    let id_bytes = prekey_id_to_bytes(key.key_id);
    let id = Node::new("id", Attrs::new(), Some(Value::Bytes(id_bytes.to_vec())));
    let value = Node::new(
        "value",
        Attrs::new(),
        Some(Value::Bytes(key.key_pair.public.to_vec())),
    );
    let sig_bytes = key.signature.map(|s| s.to_vec()).unwrap_or_default();
    let signature = Node::new("signature", Attrs::new(), Some(Value::Bytes(sig_bytes)));
    Node::new(
        "skey",
        Attrs::new(),
        Some(Value::Nodes(vec![id, value, signature])),
    )
}

/// Fetch one [`PreKeyBundle`] per device JID in `jids`. Mirrors
/// `whatsmeow/prekeys.go::fetchPreKeys` — a single bulk
/// `<iq xmlns="encrypt" type="get">` carrying a `<key>` body with one
/// `<user jid="..." reason="identity"/>` per target.
///
/// The returned map preserves the JIDs that the SERVER attached to each
/// `<user>` child of `<list>` (which is the AD-form JID, including device).
/// Devices that the server reports a per-user `<error>` on are silently
/// dropped from the map — caller decides how to handle the missing entry.
///
/// Reuses [`crate::send_message::parse_user_node_to_bundle`] so the
/// per-user parsing logic stays in exactly one place.
pub async fn fetch_pre_keys(
    client: &Client,
    jids: &[Jid],
) -> Result<HashMap<Jid, PreKeyBundle>, ClientError> {
    if !client.is_connected() {
        return Err(ClientError::NotConnected);
    }
    if jids.is_empty() {
        return Ok(HashMap::new());
    }

    // Build one <user jid="..." reason="identity"/> per target device.
    let user_nodes: Vec<Node> = jids
        .iter()
        .map(|j| {
            let mut a = Attrs::new();
            a.insert("jid".into(), Value::Jid(j.clone()));
            a.insert("reason".into(), Value::String("identity".into()));
            Node::new("user", a, None)
        })
        .collect();
    let key = Node::new("key", Attrs::new(), Some(Value::Nodes(user_nodes)));

    let resp = client
        .send_iq(
            InfoQuery::new("encrypt", IqType::Get)
                .to(Jid::new("", wha_types::Server::DEFAULT_USER))
                .content(Value::Nodes(vec![key])),
        )
        .await?;

    if let Some(err) = iq_error_from_response(&resp) {
        return Err(err);
    }

    let list = resp
        .child_by_tag(&["list"])
        .ok_or_else(|| ClientError::Malformed("prekey response missing <list>".into()))?;

    let mut out = HashMap::with_capacity(jids.len());
    for child in list.children() {
        if child.tag != "user" {
            continue;
        }
        let jid = match child.get_attr_jid("jid") {
            Some(j) => j.clone(),
            None => continue,
        };
        // Per-user errors are reported via a `<error>` child on the user
        // node; mirror upstream's "warn-and-skip" semantics here so a single
        // unreachable device doesn't fail the whole fanout.
        match crate::send_message::parse_user_node_to_bundle(&jid, child) {
            Ok(bundle) => {
                out.insert(jid, bundle);
            }
            Err(_) => continue,
        }
    }
    Ok(out)
}

/// If `resp` is an `<iq type="error">…<error code="..." text="..."/></iq>`,
/// return the matching [`ClientError::Iq`]. Returns `None` for normal
/// success responses.
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
    use std::sync::Arc;
    use wha_crypto::KeyPair;
    use wha_store::{MemoryStore, PreKeyStore};

    fn fixed_prekey(id: u32) -> PreKey {
        // Build a deterministic key pair from a fixed private byte.
        // We don't care about the public bytes for shape tests, just the id.
        let private = [7u8; 32];
        PreKey::new(id, KeyPair::from_private(private))
    }

    #[test]
    fn build_upload_iq_has_required_children() {
        let identity_pub = [9u8; 32];
        let pks = vec![fixed_prekey(1), fixed_prekey(2)];
        let signed = fixed_prekey(99).signed_by(&KeyPair::from_private([5u8; 32])).unwrap();

        let node = build_upload_pre_keys_iq(0xCAFEBABE, &identity_pub, &pks, &signed);

        assert_eq!(node.tag, "iq");
        assert_eq!(node.get_attr_str("xmlns"), Some("encrypt"));
        assert_eq!(node.get_attr_str("type"), Some("set"));

        // Each top-level child must be present.
        assert!(node.child_by_tag(&["registration"]).is_some(), "registration missing");
        assert!(node.child_by_tag(&["type"]).is_some(), "<type> missing");
        assert!(node.child_by_tag(&["identity"]).is_some(), "identity missing");
        assert!(node.child_by_tag(&["list"]).is_some(), "list missing");
        assert!(node.child_by_tag(&["skey"]).is_some(), "skey missing");

        // <list> contains one <key> per prekey.
        let list = node.child_by_tag(&["list"]).unwrap();
        assert_eq!(list.children().len(), 2);
        for child in list.children() {
            assert_eq!(child.tag, "key");
            assert!(child.child_by_tag(&["id"]).is_some());
            assert!(child.child_by_tag(&["value"]).is_some());
        }

        // <skey> contains id + value + signature.
        let skey = node.child_by_tag(&["skey"]).unwrap();
        assert!(skey.child_by_tag(&["id"]).is_some());
        assert!(skey.child_by_tag(&["value"]).is_some());
        let sig = skey
            .child_by_tag(&["signature"])
            .expect("skey has signature");
        assert_eq!(sig.content.as_bytes().unwrap().len(), 64);

        // <type> body is the single DjbType byte.
        let kt = node.child_by_tag(&["type"]).unwrap();
        assert_eq!(kt.content.as_bytes(), Some(&[DJB_TYPE][..]));

        // <identity> body is the 32-byte pubkey.
        let ident = node.child_by_tag(&["identity"]).unwrap();
        assert_eq!(ident.content.as_bytes(), Some(&identity_pub[..]));
    }

    #[test]
    fn prekey_id_is_3_byte_big_endian() {
        // 0x010203 => [0x01, 0x02, 0x03]
        assert_eq!(prekey_id_to_bytes(0x010203), [0x01, 0x02, 0x03]);
        // The top byte of a u32 is dropped — anything above 24 bits is lost.
        assert_eq!(prekey_id_to_bytes(0xFF010203), [0x01, 0x02, 0x03]);
        // id=1 packs as [0,0,1].
        assert_eq!(prekey_id_to_bytes(1), [0x00, 0x00, 0x01]);

        // Verify the encoded shape inside a built node, too.
        let pk = fixed_prekey(0xABCDEF);
        let n = prekey_to_node(&pk);
        let id_node = n.child_by_tag(&["id"]).unwrap();
        assert_eq!(
            id_node.content.as_bytes(),
            Some(&[0xAB, 0xCD, 0xEF][..]),
        );
    }

    #[test]
    fn registration_id_is_4_byte_big_endian() {
        assert_eq!(registration_id_to_bytes(0x01020304), [0x01, 0x02, 0x03, 0x04]);
        assert_eq!(registration_id_to_bytes(0), [0, 0, 0, 0]);
        assert_eq!(registration_id_to_bytes(u32::MAX), [0xFF, 0xFF, 0xFF, 0xFF]);

        // And inside the built IQ.
        let identity_pub = [9u8; 32];
        let pks = vec![fixed_prekey(1)];
        let signed = fixed_prekey(99).signed_by(&KeyPair::from_private([5u8; 32])).unwrap();
        let node = build_upload_pre_keys_iq(0xDEADBEEF, &identity_pub, &pks, &signed);
        let reg = node.child_by_tag(&["registration"]).unwrap();
        assert_eq!(
            reg.content.as_bytes(),
            Some(&[0xDE, 0xAD, 0xBE, 0xEF][..]),
        );
    }

    #[tokio::test]
    async fn refresh_does_nothing_when_count_above_min() {
        // Pre-populate the store with > MIN_PRE_KEY_COUNT prekeys and mark
        // them all uploaded. Then refresh_pre_keys_if_low must NOT call
        // upload (which would require a live socket and panic / error here).
        let store = Arc::new(MemoryStore::new());
        let n = (MIN_PRE_KEY_COUNT + 5) as usize;
        let pks = store
            .get_or_gen_pre_keys(n as u32)
            .await
            .unwrap();
        let highest = pks.iter().map(|k| k.key_id).max().unwrap();
        store.mark_pre_keys_uploaded(highest).await.unwrap();

        // Sanity: count is what we expect.
        assert!(store.uploaded_pre_key_count().await.unwrap() >= MIN_PRE_KEY_COUNT as usize);

        let device = store.new_device();
        // Replace the new_device's pre_keys handle with our pre-populated one.
        let device = wha_store::Device {
            pre_keys: store.clone(),
            ..device
        };
        let (cli, _evt) = Client::new(device);

        // Without a connection, upload_pre_keys would fail at send_node →
        // NotConnected. So if refresh_pre_keys_if_low returns Ok(()), it
        // proves it short-circuited (i.e. didn't try to upload).
        refresh_pre_keys_if_low(&cli)
            .await
            .expect("refresh should be a no-op above MIN");
    }
}
