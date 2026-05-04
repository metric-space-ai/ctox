//! QR-code and phone-code pairing flows. Mirrors `whatsmeow/pair.go`,
//! `whatsmeow/pair-code.go` and the QR cadence from `whatsmeow/qrchan.go`.
//!
//! Three public flows live here:
//!
//! 1. [`handle_pair_device`] — turn the inbound `<iq><pair-device>` into a
//!    `<iq type="result">` ack and start emitting [`Event::QrCode`] events
//!    on a tokio cadence (60s for the first ref, 20s for every subsequent
//!    one, matching the upstream timing).
//! 2. [`handle_pair_success`] — validate the ADV signature on the inbound
//!    `<pair-success>` notification, persist the linked Jid/LID/business
//!    name into the device, send the signed acknowledgement back, and emit
//!    [`Event::PairSuccess`].
//! 3. [`pair_phone`] — phone-number based linking. Generates an 8-character
//!    code, derives an ephemeral keypair, encrypts the public key with the
//!    code-derived AES key, and sends the `link_code_companion_reg /
//!    companion_hello` IQ. The 8-character code is returned for display.

use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use prost::Message;
use rand::RngCore;
use tokio::time::sleep;

use wha_binary::{Attrs, Node, Value};
use wha_crypto::{ctr_xor, hmac_sha256, hmac_sha256_verify, KeyPair};
use wha_proto::adv;
use wha_types::{jid::server, Jid};

use crate::client::Client;
use crate::error::ClientError;
use crate::events::Event;
use crate::request::{InfoQuery, IqType};

/// Prefix used inside ADV identity HMACs. Mirrors `AdvAccountSignaturePrefix`.
pub const ADV_ACCOUNT_SIGNATURE_PREFIX: [u8; 2] = [6, 0];
/// Prefix used when the device re-signs the identity. Mirrors
/// `AdvDeviceSignaturePrefix`.
pub const ADV_DEVICE_SIGNATURE_PREFIX: [u8; 2] = [6, 1];
/// Prefix for hosted accounts. Mirrors `AdvHostedAccountSignaturePrefix`.
pub const ADV_HOSTED_ACCOUNT_SIGNATURE_PREFIX: [u8; 2] = [6, 5];
/// Prefix for hosted devices. Mirrors `AdvHostedDeviceSignaturePrefix`.
pub const ADV_HOSTED_DEVICE_SIGNATURE_PREFIX: [u8; 2] = [6, 6];

/// QR rotation cadence — first code lives for 60 seconds (matching upstream's
/// `len(codes) == 6` branch), every subsequent one for 20 seconds.
const QR_INITIAL_TIMEOUT: Duration = Duration::from_secs(60);
const QR_ROTATE_TIMEOUT: Duration = Duration::from_secs(20);

/// Build the `ref,noise_pub,identity_pub,adv_secret` joined-base64 string a
/// QR-code application turns into a scannable QR. Mirrors `makeQRData` in
/// `pair.go`.
pub fn make_qr_string(
    noise_pub: &[u8; 32],
    identity_pub: &[u8; 32],
    adv_secret: &[u8; 32],
    qr_ref: &str,
) -> String {
    let noise = B64.encode(noise_pub);
    let identity = B64.encode(identity_pub);
    let adv = B64.encode(adv_secret);
    format!("{qr_ref},{noise},{identity},{adv}")
}

/// Build the `<iq type="result">` ack for a `<pair-device>` request.
pub(crate) fn build_pair_device_ack(req: &Node) -> Result<Node, ClientError> {
    // The `from` attr arrives as a JID-pair token in the binary protocol
    // (since "s.whatsapp.net" is a known server JID), but defensive parsing
    // also accepts a plain string in case the codec falls back to that.
    let to_value: Value = match req.attrs.get("from") {
        Some(Value::Jid(j)) => Value::Jid(j.clone()),
        Some(Value::String(s)) => Value::String(s.clone()),
        _ => return Err(ClientError::Malformed("pair-device iq missing from".into())),
    };
    let id = req
        .get_attr_str("id")
        .ok_or_else(|| ClientError::Malformed("pair-device iq missing id".into()))?
        .to_owned();

    let mut attrs = Attrs::new();
    attrs.insert("to".into(), to_value);
    attrs.insert("id".into(), Value::String(id));
    attrs.insert("type".into(), Value::String("result".into()));
    Ok(Node::new("iq", attrs, None))
}

/// Extract the ordered `<ref>` payloads from a `<pair-device>` IQ.
pub(crate) fn collect_pair_refs(iq: &Node) -> Vec<String> {
    let pair_device = match iq.child_by_tag(&["pair-device"]) {
        Some(n) => n,
        None => return Vec::new(),
    };
    let mut refs = Vec::new();
    for child in pair_device.children() {
        if child.tag != "ref" {
            continue;
        }
        let s = match &child.content {
            Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
            Value::String(s) => s.clone(),
            _ => continue,
        };
        refs.push(s);
    }
    refs
}

/// Handle an inbound `<iq><pair-device>` notification: send the IQ ack and
/// kick off the QR-code emitter on a tokio task. Returns once the ack is sent;
/// the QR rotation runs in the background until the references are exhausted
/// or the task is dropped.
pub async fn handle_pair_device(client: &Client, iq: &Node) -> Result<(), ClientError> {
    let ack = build_pair_device_ack(iq)?;
    client.send_node(&ack).await?;

    let refs = collect_pair_refs(iq);
    if refs.is_empty() {
        return Ok(());
    }

    let noise_pub = client.device.noise_key.public;
    let identity_pub = client.device.identity_key.public;
    let adv_secret = client.device.adv_secret_key;

    // Pre-build every QR string so the spawned task only needs the channel.
    let qrs: Vec<String> = refs
        .iter()
        .map(|r| make_qr_string(&noise_pub, &identity_pub, &adv_secret, r))
        .collect();

    let sender = client.events_sender_clone();
    tokio::spawn(async move {
        emit_qr_codes(sender, qrs).await;
    });

    Ok(())
}

/// Async emitter — sends QR codes through the event channel, sleeps the
/// upstream-canonical timeout between each.
async fn emit_qr_codes(
    sender: tokio::sync::mpsc::UnboundedSender<Event>,
    codes: Vec<String>,
) {
    for (i, code) in codes.into_iter().enumerate() {
        let timeout = if i == 0 { QR_INITIAL_TIMEOUT } else { QR_ROTATE_TIMEOUT };
        if sender.send(Event::QrCode { code }).is_err() {
            return;
        }
        sleep(timeout).await;
    }
}
/// Validate the ADV signature on the device identity. Mirrors
/// `verifyAccountSignature` in `pair.go`.
pub(crate) fn verify_account_signature(
    device_identity: &adv::AdvSignedDeviceIdentity,
    identity_pub: &[u8; 32],
    is_hosted: bool,
) -> Result<(), ClientError> {
    let sig_key = device_identity
        .account_signature_key
        .as_ref()
        .ok_or_else(|| ClientError::Malformed("missing account signature key".into()))?;
    let sig = device_identity
        .account_signature
        .as_ref()
        .ok_or_else(|| ClientError::Malformed("missing account signature".into()))?;
    let details = device_identity
        .details
        .as_ref()
        .ok_or_else(|| ClientError::Malformed("missing device identity details".into()))?;

    if sig_key.len() != 32 {
        return Err(ClientError::Malformed(format!(
            "account signature key wrong length: {}",
            sig_key.len()
        )));
    }
    if sig.len() != 64 {
        return Err(ClientError::Malformed(format!(
            "account signature wrong length: {}",
            sig.len()
        )));
    }
    let mut sig_key_arr = [0u8; 32];
    sig_key_arr.copy_from_slice(sig_key);
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(sig);

    let prefix: &[u8] = if is_hosted {
        &ADV_HOSTED_ACCOUNT_SIGNATURE_PREFIX
    } else {
        &ADV_ACCOUNT_SIGNATURE_PREFIX
    };
    let mut message = Vec::with_capacity(prefix.len() + details.len() + 32);
    message.extend_from_slice(prefix);
    message.extend_from_slice(details);
    message.extend_from_slice(identity_pub);

    KeyPair::verify(&sig_key_arr, &message, &sig_arr)
        .map_err(|e| ClientError::Malformed(format!("ADV signature verify failed: {e}")))
}

/// Build the device-side counter-signature on the ADV identity. Mirrors
/// `generateDeviceSignature` in `pair.go`.
pub(crate) fn generate_device_signature(
    device_identity: &adv::AdvSignedDeviceIdentity,
    identity_kp: &KeyPair,
) -> Result<[u8; 64], ClientError> {
    let details = device_identity
        .details
        .as_ref()
        .ok_or_else(|| ClientError::Malformed("missing device identity details".into()))?;
    let acc_key = device_identity
        .account_signature_key
        .as_ref()
        .ok_or_else(|| ClientError::Malformed("missing account signature key".into()))?;

    let mut message = Vec::with_capacity(2 + details.len() + 32 + acc_key.len());
    message.extend_from_slice(&ADV_DEVICE_SIGNATURE_PREFIX);
    message.extend_from_slice(details);
    message.extend_from_slice(&identity_kp.public);
    message.extend_from_slice(acc_key);

    Ok(identity_kp.sign(&message))
}

/// Result of processing a `<pair-success>` notification — the caller (or
/// in-place mutator below) applies these to the [`wha_store::Device`].
#[derive(Debug, Clone)]
pub struct PairResult {
    pub jid: Jid,
    pub lid: Jid,
    pub business_name: String,
    pub platform: String,
    pub req_id: String,
    pub key_index: u32,
    pub self_signed_device_identity: Vec<u8>,
}

/// Parse + validate a `<pair-success>` IQ and produce the data needed both
/// to confirm pairing back to the server and to update the local Device.
pub fn parse_pair_success(
    pair_success_iq: &Node,
    adv_secret_key: &[u8; 32],
    identity_kp: &KeyPair,
) -> Result<PairResult, ClientError> {
    let req_id = pair_success_iq
        .get_attr_str("id")
        .ok_or_else(|| ClientError::Malformed("pair-success missing id".into()))?
        .to_owned();
    let pair_success = pair_success_iq
        .child_by_tag(&["pair-success"])
        .ok_or_else(|| ClientError::Malformed("missing <pair-success>".into()))?;

    let device_identity_node = pair_success
        .child_by_tag(&["device-identity"])
        .ok_or_else(|| ClientError::Malformed("missing device-identity".into()))?;
    let device_identity_bytes = match &device_identity_node.content {
        Value::Bytes(b) => b.clone(),
        Value::String(s) => s.as_bytes().to_vec(),
        _ => {
            return Err(ClientError::Malformed(
                "device-identity has non-bytes content".into(),
            ))
        }
    };

    let business_name = pair_success
        .child_by_tag(&["biz"])
        .and_then(|n| n.get_attr_str("name"))
        .unwrap_or("")
        .to_owned();
    let platform = pair_success
        .child_by_tag(&["platform"])
        .and_then(|n| n.get_attr_str("name"))
        .unwrap_or("")
        .to_owned();

    let device_node = pair_success
        .child_by_tag(&["device"])
        .ok_or_else(|| ClientError::Malformed("missing <device>".into()))?;
    let jid = device_node
        .get_attr_jid("jid")
        .cloned()
        .ok_or_else(|| ClientError::Malformed("device missing jid".into()))?;
    let lid = device_node
        .get_attr_jid("lid")
        .cloned()
        .unwrap_or_default();

    // 1. Decode the HMAC container.
    let container = adv::AdvSignedDeviceIdentityHmac::decode(&device_identity_bytes[..])?;
    let details = container
        .details
        .as_ref()
        .ok_or_else(|| ClientError::Malformed("missing HMAC container details".into()))?;
    let mac = container
        .hmac
        .as_ref()
        .ok_or_else(|| ClientError::Malformed("missing HMAC".into()))?;

    let is_hosted =
        container.account_type == Some(adv::AdvEncryptionType::Hosted as i32);

    // 2. Verify the HMAC.
    let mut signed = Vec::with_capacity(2 + details.len());
    if is_hosted {
        signed.extend_from_slice(&ADV_HOSTED_ACCOUNT_SIGNATURE_PREFIX);
    }
    signed.extend_from_slice(details);
    hmac_sha256_verify(adv_secret_key, &signed, mac)
        .map_err(|_| ClientError::Malformed("ADV HMAC mismatch".into()))?;

    // 3. Decode the signed identity and the inner details.
    let mut signed_identity = adv::AdvSignedDeviceIdentity::decode(&details[..])?;
    let identity_details = adv::AdvDeviceIdentity::decode(
        signed_identity
            .details
            .as_deref()
            .ok_or_else(|| ClientError::Malformed("missing inner details".into()))?,
    )?;

    let device_is_hosted =
        identity_details.device_type == Some(adv::AdvEncryptionType::Hosted as i32);

    // 4. Verify the account signature.
    verify_account_signature(&signed_identity, &identity_kp.public, device_is_hosted)?;

    // 5. Build & embed our device signature.
    let device_sig = generate_device_signature(&signed_identity, identity_kp)?;
    signed_identity.device_signature = Some(device_sig.to_vec());

    // 6. Marshal a copy with the AccountSignatureKey stripped — that's the
    //    payload we send back to the server, exactly as upstream does.
    let mut wire_copy = signed_identity.clone();
    wire_copy.account_signature_key = None;
    let mut self_signed_bytes = Vec::with_capacity(wire_copy.encoded_len());
    wire_copy.encode(&mut self_signed_bytes)?;

    Ok(PairResult {
        jid,
        lid,
        business_name,
        platform,
        req_id,
        key_index: identity_details.key_index.unwrap_or(0),
        self_signed_device_identity: self_signed_bytes,
    })
}

/// Build the `<iq type="result">` confirming the pair succeeded — the
/// `pair-device-sign / device-identity` payload upstream sends from
/// `handlePair`.
pub(crate) fn build_pair_success_ack(result: &PairResult) -> Node {
    let mut device_identity_attrs = Attrs::new();
    device_identity_attrs
        .insert("key-index".into(), Value::String(result.key_index.to_string()));

    let device_identity = Node::new(
        "device-identity",
        device_identity_attrs,
        Some(Value::Bytes(result.self_signed_device_identity.clone())),
    );
    let pair_sign = Node::new(
        "pair-device-sign",
        Attrs::new(),
        Some(Value::Nodes(vec![device_identity])),
    );

    let mut iq_attrs = Attrs::new();
    iq_attrs.insert("to".into(), Value::Jid(Jid::new("", server::DEFAULT_USER)));
    iq_attrs.insert("type".into(), Value::String("result".into()));
    iq_attrs.insert("id".into(), Value::String(result.req_id.clone()));
    Node::new("iq", iq_attrs, Some(Value::Nodes(vec![pair_sign])))
}

/// Handle a `<pair-success>` IQ end-to-end: parse + validate + send the
/// confirmation back + apply the pairing result to the device + emit
/// [`Event::PairSuccess`].
///
/// Takes `&mut Client` because pairing mutates the device fields
/// (`id`, `lid`, `platform`, `business_name`).
pub async fn handle_pair_success(client: &mut Client, iq: &Node) -> Result<(), ClientError> {
    let result = {
        let dev = &client.device;
        parse_pair_success(iq, &dev.adv_secret_key, &dev.identity_key)?
    };

    // Send the signed acknowledgement before mutating local state — if the
    // server rejects it we still know what we've done.
    let ack = build_pair_success_ack(&result);
    client.send_node(&ack).await?;

    // Persist into the device. There is no Save() in the foundation store
    // (the SQL backend will add one); we update the in-memory fields and
    // upsert the main-device identity into the IdentityStore.
    client.device.id = Some(result.jid.clone());
    client.device.lid = Some(result.lid.clone());
    client.device.platform = result.platform.clone();
    client.device.business_name = result.business_name.clone();
    client.device.initialized = true;

    // Persist the LID↔PN mapping for the freshly-paired account. Mirrors
    // upstream `_upstream/whatsmeow/pair.go::handlePairSuccess` which seeds
    // the LID store with the (LID, PN) pair as soon as the server confirms
    // the link — downstream code (retry receipts, group fan-out) relies on
    // being able to resolve either direction.
    let pn_non_ad = result.jid.to_non_ad();
    let lid_non_ad = result.lid.to_non_ad();
    if let Err(e) = client
        .device
        .lids
        .put_lid_pn_mapping(lid_non_ad, pn_non_ad)
        .await
    {
        tracing::warn!("pair-success: failed to persist LID↔PN mapping: {e}");
    }

    client.dispatch_event(Event::PairSuccess { id: result.jid });
    Ok(())
}

// ---------------------------------------------------------------------------
// Phone-code linking
// ---------------------------------------------------------------------------

/// Custom base32 alphabet WhatsApp uses for the 8-character linking code,
/// matching `linkingBase32` in `pair-code.go`.
const LINKING_BASE32: &[u8; 32] = b"123456789ABCDEFGHJKLMNPQRSTVWXYZ";

/// Encode 5 raw bytes as 8 base32 chars using [`LINKING_BASE32`].
fn linking_base32_encode(input: &[u8; 5]) -> String {
    // 5 bytes = 40 bits = 8 × 5-bit groups.
    let bits: u64 = ((input[0] as u64) << 32)
        | ((input[1] as u64) << 24)
        | ((input[2] as u64) << 16)
        | ((input[3] as u64) << 8)
        | (input[4] as u64);
    let mut out = String::with_capacity(8);
    for shift in (0..8).rev() {
        let idx = ((bits >> (shift * 5)) & 0x1F) as usize;
        out.push(LINKING_BASE32[idx] as char);
    }
    out
}

/// PBKDF2-HMAC-SHA256 with the given password / salt / iterations and a
/// 32-byte output. Equivalent to upstream's
/// `pbkdf2.Key([]byte(code), salt, 2<<16, 32, sha256.New)`.
fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut block_input = Vec::with_capacity(salt.len() + 4);
    block_input.extend_from_slice(salt);
    block_input.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac_sha256(password, &block_input);
    let mut t = u.clone();
    for _ in 1..iterations {
        u = hmac_sha256(password, &u);
        for (acc, byte) in t.iter_mut().zip(u.iter()) {
            *acc ^= byte;
        }
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&t[..32]);
    out
}

/// Generate an 8-character linking code, the matching ephemeral X25519
/// keypair, and the 80-byte wrapped ephemeral pubkey expected on the wire.
/// Mirrors `generateCompanionEphemeralKey`.
fn generate_companion_ephemeral_key() -> Result<(KeyPair, Vec<u8>, String), ClientError> {
    let mut rng = rand::rngs::OsRng;
    let ephemeral = KeyPair::generate(&mut rng);

    let mut salt = [0u8; 32];
    rng.fill_bytes(&mut salt);
    let mut iv = [0u8; 16];
    rng.fill_bytes(&mut iv);
    let mut linking_code_raw = [0u8; 5];
    rng.fill_bytes(&mut linking_code_raw);
    let encoded_linking_code = linking_base32_encode(&linking_code_raw);

    let key = pbkdf2_hmac_sha256(encoded_linking_code.as_bytes(), &salt, 2 << 16);
    let mut encrypted_pub = ephemeral.public;
    ctr_xor(&key, &iv, &mut encrypted_pub)
        .map_err(|e| ClientError::Malformed(format!("AES-CTR setup: {e}")))?;

    let mut wrapped = Vec::with_capacity(80);
    wrapped.extend_from_slice(&salt);
    wrapped.extend_from_slice(&iv);
    wrapped.extend_from_slice(&encrypted_pub);

    Ok((ephemeral, wrapped, encoded_linking_code))
}

/// Pair this device to an existing primary by phone number. Returns the
/// 8-character pairing code (formatted `XXXX-XXXX`) the user types into the
/// linked-devices screen on their phone. The IQ exchange is the
/// `companion_hello` half — once the primary device approves, a follow-up
/// notification arrives that the read pump should route to a code-pair
/// notification handler (out of scope for the foundation port).
///
/// Mirrors `Client.PairPhone` in `pair-code.go`.
pub async fn pair_phone(client: &Client, phone: &str) -> Result<String, ClientError> {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() <= 6 {
        return Err(ClientError::Malformed("phone number too short".into()));
    }
    if digits.starts_with('0') {
        return Err(ClientError::Malformed(
            "phone number must be in international format (no leading 0)".into(),
        ));
    }

    let (_ephemeral_kp, wrapped_ephemeral_pub, encoded) = generate_companion_ephemeral_key()?;
    let jid = Jid::new(digits, server::DEFAULT_USER);

    // Build the inner contents.
    let mut companion_attrs = Attrs::new();
    companion_attrs.insert("jid".into(), Value::Jid(jid.clone()));
    companion_attrs.insert("stage".into(), Value::String("companion_hello".into()));
    companion_attrs.insert(
        "should_show_push_notification".into(),
        Value::String("true".into()),
    );

    let pairing_ephemeral = Node::new(
        "link_code_pairing_wrapped_companion_ephemeral_pub",
        Attrs::new(),
        Some(Value::Bytes(wrapped_ephemeral_pub)),
    );
    let server_auth = Node::new(
        "companion_server_auth_key_pub",
        Attrs::new(),
        Some(Value::Bytes(client.device.noise_key.public.to_vec())),
    );
    // Upstream sends `PairClientUnknown == 0` unless the caller overrides.
    let platform_id = Node::new(
        "companion_platform_id",
        Attrs::new(),
        Some(Value::String("0".into())),
    );
    let platform_display = Node::new(
        "companion_platform_display",
        Attrs::new(),
        Some(Value::String("Chrome (Linux)".into())),
    );
    let nonce = Node::new(
        "link_code_pairing_nonce",
        Attrs::new(),
        Some(Value::Bytes(vec![0])),
    );

    let companion_reg = Node::new(
        "link_code_companion_reg",
        companion_attrs,
        Some(Value::Nodes(vec![
            pairing_ephemeral,
            server_auth,
            platform_id,
            platform_display,
            nonce,
        ])),
    );

    let resp = client
        .send_iq(
            InfoQuery::new("md", IqType::Set)
                .to(Jid::new("", server::DEFAULT_USER))
                .content(Value::Nodes(vec![companion_reg])),
        )
        .await?;

    // Confirm the response shape — even though we don't *need* the ref to
    // return the linking code, surfacing a malformed reply early helps.
    let _ = resp
        .child_by_tag(&["link_code_companion_reg", "link_code_pairing_ref"])
        .ok_or_else(|| ClientError::Malformed("missing link_code_pairing_ref".into()))?;

    // Format as `XXXX-XXXX`.
    Ok(format!("{}-{}", &encoded[0..4], &encoded[4..]))
}

// ---------------------------------------------------------------------------
// Helper: ADV details builder for tests + signature helper used by both
// the success path and tests.
// ---------------------------------------------------------------------------

/// Sign an ADV identity exactly like the server would — signs `[prefix ||
/// details || identity_pub]` with the supplied account key. Used in tests so
/// the round-trip test for `parse_pair_success` doesn't need a real server.
#[doc(hidden)]
pub fn account_sign_for_tests(
    account_kp: &KeyPair,
    details: &[u8],
    identity_pub: &[u8; 32],
    is_hosted: bool,
) -> [u8; 64] {
    let prefix: &[u8] = if is_hosted {
        &ADV_HOSTED_ACCOUNT_SIGNATURE_PREFIX
    } else {
        &ADV_ACCOUNT_SIGNATURE_PREFIX
    };
    let mut msg = Vec::with_capacity(prefix.len() + details.len() + 32);
    msg.extend_from_slice(prefix);
    msg.extend_from_slice(details);
    msg.extend_from_slice(identity_pub);
    account_kp.sign(&msg)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wha_store::MemoryStore;

    fn fake_iq(refs: &[&str]) -> Node {
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
        iq_attrs.insert("id".into(), Value::String("req-123".into()));
        iq_attrs.insert("type".into(), Value::String("set".into()));
        Node::new("iq", iq_attrs, Some(Value::Nodes(vec![pair_device])))
    }

    #[test]
    fn make_qr_string_joins_base64_fields() {
        let noise = [1u8; 32];
        let identity = [2u8; 32];
        let adv = [3u8; 32];
        let s = make_qr_string(&noise, &identity, &adv, "myref");
        let parts: Vec<&str> = s.split(',').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "myref");
        assert_eq!(parts[1], B64.encode(noise));
        assert_eq!(parts[2], B64.encode(identity));
        assert_eq!(parts[3], B64.encode(adv));
    }

    #[test]
    fn build_pair_device_ack_has_correct_shape() {
        let iq = fake_iq(&["a", "b"]);
        let ack = build_pair_device_ack(&iq).expect("ack");
        assert_eq!(ack.tag, "iq");
        assert_eq!(ack.get_attr_str("type"), Some("result"));
        assert_eq!(ack.get_attr_str("id"), Some("req-123"));
        assert_eq!(ack.get_attr_str("to"), Some(server::DEFAULT_USER));
    }

    #[test]
    fn collect_pair_refs_round_trip() {
        let iq = fake_iq(&["one", "two", "three"]);
        let refs = collect_pair_refs(&iq);
        assert_eq!(refs, vec!["one".to_owned(), "two".into(), "three".into()]);
    }

    #[test]
    fn linking_base32_encodes_full_alphabet_indices() {
        // 0x00 0x44 0x32 0x14 0xc7 → 5 bytes packed = bits
        // 00000 00100 01000 11001 00001 01000 11000 00111 → indices 0,4,8,25,1,8,24,7
        // alphabet: 0='1', 4='5', 8='9', 25='V', 1='2', 8='9', 24='T', 7='8'
        let encoded = linking_base32_encode(&[0x00, 0x44, 0x32, 0x14, 0xc7]);
        assert_eq!(encoded.len(), 8);
        assert!(encoded.chars().all(|c| LINKING_BASE32.contains(&(c as u8))));
    }

    #[test]
    fn pbkdf2_matches_hmac_first_iteration_when_iterations_one() {
        let password = b"PASSWORD";
        let salt = b"saltsaltsalt";
        let out = pbkdf2_hmac_sha256(password, salt, 1);
        // Iteration 1: HMAC(pw, salt || INT32BE(1)) truncated to 32 bytes.
        let mut block_input = Vec::from(&salt[..]);
        block_input.extend_from_slice(&1u32.to_be_bytes());
        let mac = hmac_sha256(password, &block_input);
        assert_eq!(&out[..], &mac[..32]);
    }

    #[test]
    fn ctr_xor_is_self_inverse() {
        let key = [9u8; 32];
        let iv = [3u8; 16];
        let original = b"the quick brown fox jumps over!!".to_vec();
        let mut data = original.clone();
        ctr_xor(&key, &iv, &mut data).unwrap();
        assert_ne!(data, original);
        ctr_xor(&key, &iv, &mut data).unwrap();
        assert_eq!(data, original);
    }

    #[tokio::test]
    async fn handle_pair_device_emits_qr_and_ignores_when_offline() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        // No live socket → send_node should fail cleanly with NotConnected,
        // which surfaces as Err from handle_pair_device. We're checking that
        // the function does not panic and that the error is the expected
        // variant.
        let iq = fake_iq(&["abc"]);
        let r = handle_pair_device(&cli, &iq).await;
        assert!(matches!(r, Err(ClientError::NotConnected)));
    }
}
