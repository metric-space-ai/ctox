//! Mock-server integration test for the pairing state machine.
//!
//! Exercises the pairing flow without ever opening a websocket. The two
//! halves we care about live in `whatsapp::client::pair`:
//!
//!   1. `handle_pair_device` — turns a server `<iq><pair-device>` into an
//!      ack + a QR-emitter task. Because the function writes the ack
//!      through the live noise socket, it cannot be driven against a
//!      `Client` that hasn't completed `connect()`. We therefore drive the
//!      QR-string production end-to-end via the public `make_qr_string`
//!      helper plus the same node-walking the function performs
//!      internally.
//!
//!   2. `handle_pair_success` — same socket-write coupling for the
//!      acknowledgement, but the *parser* half is exposed as
//!      `parse_pair_success`, which is the path we exercise here. We
//!      hand-build an ADV-signed device identity using a synthetic
//!      account-signing keypair (the foundation port doesn't carry the
//!      real WA cert chain, so we sign with our own key and hand the
//!      parser the matching public half). The test then applies the
//!      parsed result to the device in the same shape
//!      `handle_pair_success` does internally and confirms `device.id`
//!      ends up populated.
//!
//! The integration test does NOT pull in `prost` directly so we keep the
//! crate's dependency surface unchanged. Instead we hand-encode the three
//! ADV protobuf messages — they are small (`AdvDeviceIdentity`,
//! `AdvSignedDeviceIdentity`, `AdvSignedDeviceIdentityHmac`) and the wire
//! format is stable.

use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as B64, Engine};

use whatsapp::binary::{Attrs, Node, Value};
use whatsapp::client::pair::{
    account_sign_for_tests, make_qr_string, parse_pair_success,
};
use whatsapp::client::{Client, Event};
use whatsapp::crypto::{hmac_sha256, KeyPair};
use whatsapp::store::MemoryStore;
use whatsapp::types::{jid::server, Jid};

// ---------------------------------------------------------------------------
// Tiny protobuf encoder — we only need length-delimited (wire type 2) and
// varint (wire type 0) for the three ADV messages we synthesise below.
// ---------------------------------------------------------------------------

fn pb_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn pb_field_bytes(out: &mut Vec<u8>, field: u32, value: &[u8]) {
    pb_varint(out, ((field as u64) << 3) | 2);
    pb_varint(out, value.len() as u64);
    out.extend_from_slice(value);
}

fn pb_field_varint(out: &mut Vec<u8>, field: u32, value: u64) {
    pb_varint(out, ((field as u64) << 3) | 0);
    pb_varint(out, value);
}

/// Encode an `AdvDeviceIdentity`:
///   1: raw_id (uint32), 2: timestamp (uint64), 3: key_index (uint32),
///   4: account_type (enum), 5: device_type (enum). All optional.
fn encode_adv_device_identity(
    raw_id: u32,
    timestamp: u64,
    key_index: u32,
    account_type: u32,
    device_type: u32,
) -> Vec<u8> {
    let mut out = Vec::new();
    pb_field_varint(&mut out, 1, raw_id as u64);
    pb_field_varint(&mut out, 2, timestamp);
    pb_field_varint(&mut out, 3, key_index as u64);
    pb_field_varint(&mut out, 4, account_type as u64);
    pb_field_varint(&mut out, 5, device_type as u64);
    out
}

/// Encode an `AdvSignedDeviceIdentity`:
///   1: details (bytes), 2: account_signature_key (bytes),
///   3: account_signature (bytes), 4: device_signature (bytes, optional).
fn encode_adv_signed_device_identity(
    details: &[u8],
    account_sig_key: &[u8],
    account_sig: &[u8],
    device_sig: Option<&[u8]>,
) -> Vec<u8> {
    let mut out = Vec::new();
    pb_field_bytes(&mut out, 1, details);
    pb_field_bytes(&mut out, 2, account_sig_key);
    pb_field_bytes(&mut out, 3, account_sig);
    if let Some(s) = device_sig {
        pb_field_bytes(&mut out, 4, s);
    }
    out
}

/// Encode an `AdvSignedDeviceIdentityHmac`:
///   1: details (bytes), 2: hmac (bytes), 3: account_type (enum).
fn encode_adv_hmac_container(details: &[u8], mac: &[u8], account_type: u32) -> Vec<u8> {
    let mut out = Vec::new();
    pb_field_bytes(&mut out, 1, details);
    pb_field_bytes(&mut out, 2, mac);
    pb_field_varint(&mut out, 3, account_type as u64);
    out
}

// ---------------------------------------------------------------------------
// Node builders.
// ---------------------------------------------------------------------------

fn synth_pair_device_iq(refs: &[&str], iq_id: &str) -> Node {
    let ref_nodes: Vec<Node> = refs
        .iter()
        .map(|r| {
            Node::new(
                "ref",
                Attrs::new(),
                Some(Value::Bytes(r.as_bytes().to_vec())),
            )
        })
        .collect();
    let pair_device = Node::new(
        "pair-device",
        Attrs::new(),
        Some(Value::Nodes(ref_nodes)),
    );
    let mut iq_attrs = Attrs::new();
    iq_attrs.insert("from".into(), Value::String(server::DEFAULT_USER.into()));
    iq_attrs.insert("id".into(), Value::String(iq_id.into()));
    iq_attrs.insert("type".into(), Value::String("set".into()));
    Node::new("iq", iq_attrs, Some(Value::Nodes(vec![pair_device])))
}

/// Mirrors `pair::collect_pair_refs` (which is `pub(crate)` and so not
/// reachable from an integration test).
fn extract_refs(iq: &Node) -> Vec<String> {
    let pair_device = iq
        .child_by_tag(&["pair-device"])
        .expect("iq must contain <pair-device>");
    let mut out = Vec::new();
    for child in pair_device.children() {
        if child.tag != "ref" {
            continue;
        }
        match &child.content {
            Value::Bytes(b) => out.push(String::from_utf8_lossy(b).into_owned()),
            Value::String(s) => out.push(s.clone()),
            _ => {}
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn synth_pair_success_iq(
    paired_jid: &Jid,
    paired_lid: &Jid,
    business: &str,
    platform: &str,
    iq_id: &str,
    adv_secret_key: &[u8; 32],
    identity_pub: &[u8; 32],
    account_kp: &KeyPair,
    key_index: u32,
) -> Node {
    // 1. Inner AdvDeviceIdentity bytes.
    let inner_bytes = encode_adv_device_identity(
        0xdead_beef, // raw_id
        1_700_000_000,
        key_index,
        0, // E2EE
        0, // E2EE
    );

    // 2. AdvSignedDeviceIdentity with the synthetic account signature.
    let sig = account_sign_for_tests(account_kp, &inner_bytes, identity_pub, false);
    let signed_identity_bytes = encode_adv_signed_device_identity(
        &inner_bytes,
        account_kp.public.as_ref(),
        &sig,
        None,
    );

    // 3. AdvSignedDeviceIdentityHmac. The non-hosted path does NOT prefix
    //    the details with anything (parse_pair_success only adds the
    //    hosted prefix when account_type == Hosted).
    let mac = hmac_sha256(adv_secret_key, &signed_identity_bytes);
    let hmac_bytes = encode_adv_hmac_container(&signed_identity_bytes, &mac, 0); // E2ee

    // 4. XML node tree.
    let device_identity_node = Node::new(
        "device-identity",
        Attrs::new(),
        Some(Value::Bytes(hmac_bytes)),
    );
    let mut device_attrs = Attrs::new();
    device_attrs.insert("jid".into(), Value::Jid(paired_jid.clone()));
    device_attrs.insert("lid".into(), Value::Jid(paired_lid.clone()));
    let device_node = Node::new("device", device_attrs, None);

    let mut biz_attrs = Attrs::new();
    biz_attrs.insert("name".into(), Value::String(business.into()));
    let biz_node = Node::new("biz", biz_attrs, None);

    let mut platform_attrs = Attrs::new();
    platform_attrs.insert("name".into(), Value::String(platform.into()));
    let platform_node = Node::new("platform", platform_attrs, None);

    let pair_success = Node::new(
        "pair-success",
        Attrs::new(),
        Some(Value::Nodes(vec![
            device_identity_node,
            device_node,
            biz_node,
            platform_node,
        ])),
    );

    let mut iq_attrs = Attrs::new();
    iq_attrs.insert("from".into(), Value::String(server::DEFAULT_USER.into()));
    iq_attrs.insert("id".into(), Value::String(iq_id.into()));
    iq_attrs.insert("type".into(), Value::String("set".into()));
    Node::new("iq", iq_attrs, Some(Value::Nodes(vec![pair_success])))
}

// ---------------------------------------------------------------------------
// 1. <pair-device> path: refs round-trip into wire-format QR strings.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pair_device_iq_yields_well_formed_qr_strings() {
    let store = Arc::new(MemoryStore::new());
    let device = store.new_device();
    // We don't drive Client through a real socket here, but we DO confirm
    // Client::new accepts the device the same shape as the example does,
    // so the wiring stays honest.
    let (_client, mut events) = Client::new(device.clone());

    let iq = synth_pair_device_iq(&["ref-A", "ref-B", "ref-C"], "iq-pair-1");
    let refs = extract_refs(&iq);
    assert_eq!(refs, vec!["ref-A".to_owned(), "ref-B".into(), "ref-C".into()]);

    // Build the QR strings in the same shape `handle_pair_device` does.
    let qrs: Vec<String> = refs
        .iter()
        .map(|r| {
            make_qr_string(
                &device.noise_key.public,
                &device.identity_key.public,
                &device.adv_secret_key,
                r,
            )
        })
        .collect();
    assert_eq!(qrs.len(), 3);

    for (i, qr) in qrs.iter().enumerate() {
        let parts: Vec<&str> = qr.split(',').collect();
        assert_eq!(parts.len(), 4, "QR #{i} has wrong field count: {qr}");
        assert_eq!(parts[0], refs[i], "QR #{i} ref mismatch");

        let dec_noise = B64.decode(parts[1]).expect("noise base64");
        assert_eq!(
            dec_noise, device.noise_key.public,
            "QR #{i} noise pubkey mismatch"
        );
        let dec_identity = B64.decode(parts[2]).expect("identity base64");
        assert_eq!(
            dec_identity, device.identity_key.public,
            "QR #{i} identity pubkey mismatch"
        );
        let dec_adv = B64.decode(parts[3]).expect("adv base64");
        assert_eq!(
            dec_adv, device.adv_secret_key,
            "QR #{i} adv secret mismatch"
        );
    }

    // The Client we created should not have synthesised events on its own.
    assert!(events.try_recv().is_err(), "no event expected before any traffic");
}

// ---------------------------------------------------------------------------
// 2. <pair-success> parse path.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pair_success_parse_then_apply_to_device() {
    use rand::rngs::OsRng;

    let store = Arc::new(MemoryStore::new());
    let device = store.new_device();
    let (mut client, mut events) = Client::new(device);

    // Synthetic intermediate-account keypair — stands in for WA's
    // account-signing key. The parser only needs the SAME public half
    // we attach inside `account_signature_key`, so this round-trips even
    // though it's not the real cert chain.
    let account_kp = KeyPair::generate(&mut OsRng);

    let paired_jid = Jid::new_ad("15551231234", 0, 1);
    let paired_lid = Jid::new("99999", "lid");
    let key_index = 7u32;

    let iq = synth_pair_success_iq(
        &paired_jid,
        &paired_lid,
        "Test Business",
        "android",
        "iq-pair-success-1",
        &client.device.adv_secret_key,
        &client.device.identity_key.public,
        &account_kp,
        key_index,
    );

    // Parse-only path — the public helper that does HMAC verify + ADV
    // signature verify but does NOT write anything to the socket.
    let result = parse_pair_success(
        &iq,
        &client.device.adv_secret_key,
        &client.device.identity_key,
    )
    .expect("parse_pair_success should succeed for a well-formed synthetic IQ");

    assert_eq!(result.jid, paired_jid, "jid mismatch");
    assert_eq!(result.lid, paired_lid, "lid mismatch");
    assert_eq!(result.business_name, "Test Business");
    assert_eq!(result.platform, "android");
    assert_eq!(result.req_id, "iq-pair-success-1");
    assert_eq!(result.key_index, key_index);
    assert!(
        !result.self_signed_device_identity.is_empty(),
        "self-signed identity bytes must be populated"
    );

    // Apply the result to the device in the same shape
    // `handle_pair_success` does internally (the parts after the ack
    // send). This is the half the integration test is asserting against.
    assert!(client.device.id.is_none(), "device.id starts unset");
    client.device.id = Some(result.jid.clone());
    client.device.lid = Some(result.lid.clone());
    client.device.platform = result.platform.clone();
    client.device.business_name = result.business_name.clone();
    client.device.initialized = true;

    assert_eq!(client.device.id.as_ref(), Some(&paired_jid));
    assert_eq!(client.device.lid.as_ref(), Some(&paired_lid));
    assert_eq!(client.device.platform, "android");
    assert_eq!(client.device.business_name, "Test Business");
    assert!(client.device.initialized);

    // Surface a synthetic PairSuccess event so we cover the dispatch_event
    // half (handle_pair_success does this on its own — without a socket
    // we have to do it ourselves).
    client.dispatch_event(Event::PairSuccess {
        id: result.jid.clone(),
    });
    match events.try_recv() {
        Ok(Event::PairSuccess { id }) => assert_eq!(id, paired_jid),
        other => panic!("expected PairSuccess event, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 3. parse_pair_success rejects a tampered HMAC.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pair_success_parse_rejects_tampered_hmac() {
    use rand::rngs::OsRng;

    let store = Arc::new(MemoryStore::new());
    let device = store.new_device();
    let account_kp = KeyPair::generate(&mut OsRng);

    let paired_jid = Jid::new_ad("15551231234", 0, 1);
    let paired_lid = Jid::new("99999", "lid");

    // Build a legitimate IQ first.
    let iq = synth_pair_success_iq(
        &paired_jid,
        &paired_lid,
        "Biz",
        "android",
        "iq-tamper",
        &device.adv_secret_key,
        &device.identity_key.public,
        &account_kp,
        3,
    );

    // Walk: iq -> children -> pair-success -> children -> device-identity,
    // tamper the device-identity bytes, and rebuild the tree. The
    // `wha-binary` API does not yet expose mutable child accessors so we
    // reconstruct from the ground up.
    let pair_success_old = iq
        .child_by_tag(&["pair-success"])
        .expect("pair-success node present")
        .clone();
    let mut new_pair_success_children = Vec::new();
    for child in pair_success_old.children() {
        if child.tag == "device-identity" {
            let mut bytes = match &child.content {
                Value::Bytes(b) => b.clone(),
                _ => panic!("device-identity has non-bytes content"),
            };
            let idx = bytes.len() / 2;
            bytes[idx] ^= 0x01;
            new_pair_success_children.push(Node::new(
                "device-identity",
                child.attrs.clone(),
                Some(Value::Bytes(bytes)),
            ));
        } else {
            new_pair_success_children.push(child.clone());
        }
    }
    let new_pair_success = Node::new(
        "pair-success",
        pair_success_old.attrs.clone(),
        Some(Value::Nodes(new_pair_success_children)),
    );
    let mut iq_attrs = iq.attrs.clone();
    iq_attrs.insert("type".into(), Value::String("set".into()));
    let tampered_iq = Node::new("iq", iq_attrs, Some(Value::Nodes(vec![new_pair_success])));

    let r = parse_pair_success(&tampered_iq, &device.adv_secret_key, &device.identity_key);
    assert!(
        r.is_err(),
        "tampered device-identity must NOT parse cleanly; got Ok({:?})",
        r.ok()
    );
}
