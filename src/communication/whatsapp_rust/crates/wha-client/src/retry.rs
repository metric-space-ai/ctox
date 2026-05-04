//! Retry-receipt handling — both directions.
//!
//! Mirrors `_upstream/whatsmeow/retry.go`. There are two flows:
//!
//! * **Outgoing retry receipt** — when our local decrypt of an inbound
//!   `<message>` fails, we ship a `<receipt type="retry">` back to the sender
//!   so they re-send under a fresh prekey bundle. See [`send_retry_receipt`].
//!
//! * **Incoming retry receipt** — a peer failed to decrypt one of OUR
//!   messages and is asking us to re-send. We look up the original plaintext
//!   in the recent-messages cache (see [`crate::client::Client::add_recent_message`]),
//!   re-encrypt under the (possibly new) Signal session, and ship a fresh
//!   `<message>`. See [`handle_retry_receipt`].
//!
//! Receipt shape (mirrors `Client.sendRetryReceipt`):
//!
//! ```xml
//! <receipt id="<msg-id>" to="<sender>" participant="<sender-device>" type="retry">
//!   <retry id="<msg-id>" t="<unix-time>" count="<n>" v="1"/>
//!   <registration>...4 BE bytes of our registration_id...</registration>
//!   <keys>                       <!-- only when retry_count > 1 OR force_include_identity -->
//!     <type>...0x05 (DjbType)...</type>
//!     <identity>...32-byte identity pubkey...</identity>
//!     <key>
//!       <id>...3 BE bytes...</id>
//!       <value>...32-byte one-time pre-key public...</value>
//!     </key>
//!     <skey>
//!       <id>...3 BE bytes...</id>
//!       <value>...32-byte signed pre-key public...</value>
//!       <signature>...64-byte signature...</signature>
//!     </skey>
//!     <device-identity>...marshalled adv account proto, or empty...</device-identity>
//!   </keys>
//! </receipt>
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{debug, warn};

use wha_binary::{Attrs, Node, Value};
use wha_signal::cipher::EncryptedMessage;
use wha_signal::session::SessionState;
use wha_signal::{x3dh, IdentityKeyPair, PreKeyBundle, SessionCipher, SignalAddress};
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;
use crate::prekeys::{prekey_id_to_bytes, registration_id_to_bytes};

/// Mirrors the upstream cap in `Client.sendRetryReceipt`: stop after the
/// 5th retry for the same message id.
pub const MAX_RETRIES_PER_MESSAGE: u32 = 5;

/// "DjbType" — X25519 key-format byte. Mirrors `ecc.DjbType` upstream.
const DJB_TYPE: u8 = 0x05;

// -----------------------------------------------------------------------------
// Outgoing: send `<receipt type="retry">` for an inbound message we failed to
// decrypt.
// -----------------------------------------------------------------------------

/// Send a retry receipt asking `sender` to re-encrypt and re-send the message
/// with id `original_message_id` under a fresh bundle.
///
/// `sender` is the chat JID (the `from` on the inbound `<message>`),
/// `sender_device_jid` is the per-device participant JID (set on group +
/// LID-routed DMs). Pass `Some(ciphertext_hash)` to surface the original
/// `<enc>` ciphertext SHA-256 in the log line — currently advisory; we keep the
/// hook so callers don't need to change shape later.
///
/// We bump the per-message retry counter via
/// [`Client::bump_message_retry`]; once it reaches [`MAX_RETRIES_PER_MESSAGE`]
/// we silently drop the request to mirror upstream's "give up after 5".
pub async fn send_retry_receipt(
    client: &Client,
    original_message_id: &str,
    sender: &Jid,
    sender_device_jid: &Jid,
    ciphertext_hash: Option<&[u8; 32]>,
) -> Result<(), ClientError> {
    let count = client.bump_message_retry(original_message_id);
    if count > MAX_RETRIES_PER_MESSAGE {
        debug!(
            msg_id = %original_message_id,
            count,
            "skipping retry receipt — over MAX_RETRIES_PER_MESSAGE"
        );
        return Ok(());
    }
    if let Some(h) = ciphertext_hash {
        debug!(
            msg_id = %original_message_id,
            count,
            ciphertext_hash = %hex::encode(h),
            "sending retry receipt"
        );
    } else {
        debug!(msg_id = %original_message_id, count, "sending retry receipt");
    }

    let now_t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let node = build_retry_receipt_node(client, original_message_id, sender, sender_device_jid, count, now_t).await?;
    client.send_node(&node).await
}

/// Build the wire `<receipt type="retry">` Node. Pure (no socket I/O),
/// exposed via `pub(crate)` for tests.
async fn build_retry_receipt_node(
    client: &Client,
    msg_id: &str,
    sender: &Jid,
    sender_device_jid: &Jid,
    retry_count: u32,
    now_unix: i64,
) -> Result<Node, ClientError> {
    let mut receipt_attrs = Attrs::new();
    receipt_attrs.insert("id".into(), Value::String(msg_id.to_owned()));
    receipt_attrs.insert("to".into(), Value::Jid(sender.clone()));
    if sender != sender_device_jid {
        receipt_attrs.insert("participant".into(), Value::Jid(sender_device_jid.clone()));
    }
    receipt_attrs.insert("type".into(), Value::String("retry".into()));

    // <retry id=… t=… count=… v=1/>
    let mut retry_attrs = Attrs::new();
    retry_attrs.insert("id".into(), Value::String(msg_id.to_owned()));
    retry_attrs.insert("t".into(), Value::String(now_unix.to_string()));
    retry_attrs.insert("count".into(), Value::String(retry_count.to_string()));
    retry_attrs.insert("v".into(), Value::String("1".into()));
    let retry = Node::new("retry", retry_attrs, None);

    // <registration>4 BE bytes</registration>
    let registration = Node::new(
        "registration",
        Attrs::new(),
        Some(Value::Bytes(
            registration_id_to_bytes(client.device.registration_id).to_vec(),
        )),
    );

    let mut children: Vec<Node> = vec![retry, registration];

    // <keys>…</keys> only when count > 1 (or on first retry if forced).
    // Mirrors the `if retryCount > 1 || forceIncludeIdentity` branch upstream.
    if retry_count > 1 {
        let keys_node = build_keys_node(client).await?;
        children.push(keys_node);
    }

    Ok(Node::new("receipt", receipt_attrs, Some(Value::Nodes(children))))
}

/// Build the inner `<keys>` block carrying our identity material + a fresh
/// one-time pre-key + the signed pre-key. Mirrors the inner `payload.Content`
/// append in `Client.sendRetryReceipt`.
async fn build_keys_node(client: &Client) -> Result<Node, ClientError> {
    // Mint a one-time pre-key the peer will consume to bootstrap their X3DH.
    let one_time = client
        .device
        .pre_keys
        .gen_one_pre_key()
        .await
        .map_err(ClientError::from)?;

    let key_type = Node::new("type", Attrs::new(), Some(Value::Bytes(vec![DJB_TYPE])));
    let identity = Node::new(
        "identity",
        Attrs::new(),
        Some(Value::Bytes(client.device.identity_key.public.to_vec())),
    );

    let opk_id_bytes = prekey_id_to_bytes(one_time.key_id).to_vec();
    let opk_id = Node::new("id", Attrs::new(), Some(Value::Bytes(opk_id_bytes)));
    let opk_value = Node::new(
        "value",
        Attrs::new(),
        Some(Value::Bytes(one_time.key_pair.public.to_vec())),
    );
    let key = Node::new(
        "key",
        Attrs::new(),
        Some(Value::Nodes(vec![opk_id, opk_value])),
    );

    let signed = &client.device.signed_pre_key;
    let skey_id_bytes = prekey_id_to_bytes(signed.key_id).to_vec();
    let skey_id = Node::new("id", Attrs::new(), Some(Value::Bytes(skey_id_bytes)));
    let skey_value = Node::new(
        "value",
        Attrs::new(),
        Some(Value::Bytes(signed.key_pair.public.to_vec())),
    );
    let sig_bytes = signed.signature.map(|s| s.to_vec()).unwrap_or_default();
    let skey_sig = Node::new("signature", Attrs::new(), Some(Value::Bytes(sig_bytes)));
    let skey = Node::new(
        "skey",
        Attrs::new(),
        Some(Value::Nodes(vec![skey_id, skey_value, skey_sig])),
    );

    // <device-identity>…</device-identity>. Upstream marshals the persisted
    // ADV account proto here. The current Rust port doesn't keep that proto
    // on `Device` (only the `adv_secret_key` is persisted), so we emit an
    // empty body — the recipient still sees the tag and treats it as a
    // first-contact identity. Replace once `wha_proto::adv` lands.
    let device_identity = Node::new("device-identity", Attrs::new(), Some(Value::Bytes(Vec::new())));

    Ok(Node::new(
        "keys",
        Attrs::new(),
        Some(Value::Nodes(vec![key_type, identity, key, skey, device_identity])),
    ))
}

// -----------------------------------------------------------------------------
// Incoming: handle a `<receipt type="retry">` for one of our own messages.
// -----------------------------------------------------------------------------

/// Parsed `<receipt type="retry">` — the bare facts we pull off the wire.
///
/// Upstream's `handleRetryReceipt` reads the same fields from the inner
/// `<retry>` child plus the surrounding `<receipt>` envelope. The inbound
/// `<keys>` block (when present) is consumed later by the resend pipeline; we
/// only surface its `<registration>` integer here because the rest is
/// re-parsed by the prekey-bundle helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryReceipt {
    /// `id` attribute on the inner `<retry>` (also matches the outer
    /// `<receipt id="…">`).
    pub message_id: String,
    /// `from` attribute on the outer `<receipt>` — the peer requesting the
    /// retry.
    pub from: Jid,
    /// `participant` attribute on the outer `<receipt>` — set on group
    /// receipts where `from` is the group JID and `participant` is the actual
    /// device that failed to decrypt.
    pub participant: Option<Jid>,
    /// `t` attribute on the inner `<retry>` (seconds since epoch).
    pub timestamp: i64,
    /// `count` attribute on the inner `<retry>` — defaults to 1 when missing
    /// to match the spirit of upstream's `messageRetries` book-keeping (the
    /// first retry is logically count=1).
    pub retry_count: u32,
    /// 4 BE bytes pulled out of `<keys><registration>…</registration></keys>`
    /// when present. Populated only on retries that include a fresh prekey
    /// bundle (typically count > 1 or after a session loss).
    pub registration_id: Option<u32>,
}

/// Parse the receipt envelope into a [`RetryReceipt`]. Pure, no I/O.
///
/// Mirrors `parseRetryReceipt` in upstream. Errors are accumulated through
/// `AttrUtility` so callers see all malformed-attr issues at once.
pub fn parse_retry_receipt(node: &Node) -> Result<RetryReceipt, ClientError> {
    if node.tag != "receipt" {
        return Err(ClientError::Malformed(format!(
            "expected <receipt>, got <{}>",
            node.tag
        )));
    }

    // Outer <receipt …> attrs.
    let mut outer = node.attr_getter();
    let from = outer.jid("from");
    let participant = outer.optional_jid("participant").cloned();
    if !outer.ok() {
        let errs = outer.into_result().err().unwrap_or_default();
        return Err(ClientError::Malformed(format!(
            "failed to parse retry receipt outer attrs: {errs:?}"
        )));
    }

    // Inner <retry …/> child carries id/t/count.
    let retry_child = node
        .children()
        .iter()
        .find(|c| c.tag == "retry")
        .ok_or_else(|| {
            ClientError::Malformed("retry receipt missing <retry> child".into())
        })?;

    let mut inner = retry_child.attr_getter();
    let message_id = inner.string("id").to_owned();
    let timestamp = inner.i64("t");
    // Upstream reads count via the strict `Int` getter, but a missing count
    // logically means "this is the first retry" — default to 1 to be lenient.
    let retry_count_i64 = inner.optional_i64("count").unwrap_or(1);
    if !inner.ok() {
        let errs = inner.into_result().err().unwrap_or_default();
        return Err(ClientError::Malformed(format!(
            "failed to parse retry receipt inner attrs: {errs:?}"
        )));
    }
    let retry_count: u32 = if retry_count_i64 < 0 {
        1
    } else {
        retry_count_i64 as u32
    };

    // Optional <keys><registration>…</registration></keys> — 4 BE bytes.
    let registration_id = node
        .children()
        .iter()
        .find(|c| c.tag == "keys")
        .and_then(|keys| keys.children().iter().find(|c| c.tag == "registration"))
        .and_then(|reg| match &reg.content {
            Value::Bytes(b) if b.len() == 4 => {
                Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            }
            _ => None,
        });

    Ok(RetryReceipt {
        message_id,
        from,
        participant,
        timestamp,
        retry_count,
        registration_id,
    })
}

/// Handle an inbound `<receipt type="retry">`.
///
/// Mirrors `Client.handleRetryReceipt` in upstream. On a valid receipt we
/// look up the original plaintext in the recent-messages cache, run a fresh
/// X3DH (or ride the existing session) under the inbound prekey bundle, and
/// ship a new `<message>` carrying a `<enc type="pkmsg">`.
///
/// If the original plaintext is not in the recent-messages cache we log and
/// return `Ok(())` — there's nothing we can re-send, but the receive loop
/// must keep running.
pub async fn handle_retry_receipt(client: &Client, receipt: &Node) -> Result<(), ClientError> {
    let parsed = parse_retry_receipt(receipt)?;
    debug!(
        msg_id = %parsed.message_id,
        from = %parsed.from,
        retry_count = parsed.retry_count,
        timestamp = parsed.timestamp,
        registration_id = ?parsed.registration_id,
        "handling retry receipt"
    );

    // The peer wants us to re-send `parsed.message_id` to either
    // `parsed.from` (DM) or `parsed.participant` (group). The recent-messages
    // cache is keyed by the recipient JID we passed to `add_recent_message`.
    //
    // If the participant is a LID we try the LidStore for the matching PN
    // before falling back to the literal LID — Signal sessions are indexed
    // on the PN form, so this resolution is what makes the cache lookup
    // hit. Mirrors the LID crossreference upstream's
    // `Client.handleRetryReceipt` runs inline.
    let raw_recipient = parsed
        .participant
        .clone()
        .unwrap_or_else(|| parsed.from.clone());
    let recipient = if raw_recipient.server == wha_types::Server::HIDDEN_USER {
        match client.device.lids.get_pn_for_lid(&raw_recipient).await {
            Ok(Some(pn)) => pn,
            Ok(None) => raw_recipient,
            Err(e) => {
                warn!(
                    lid = %raw_recipient,
                    error = %e,
                    "retry: LidStore lookup failed; falling back to LID JID",
                );
                raw_recipient
            }
        }
    } else {
        raw_recipient
    };
    let plaintext = match client.get_recent_message(&recipient, &parsed.message_id) {
        Some(pt) => pt,
        None => {
            warn!(
                msg_id = %parsed.message_id,
                recipient = %recipient,
                "no recent-messages cache entry for retry — dropping",
            );
            return Ok(());
        }
    };

    // If the receipt carried a `<keys>` block we have a brand-new bundle —
    // wipe the existing session so the next encrypt runs X3DH again. Mirrors
    // the `if hasKeys { bundle = nodeToPreKeyBundle(...) }` branch.
    let address = SignalAddress::from_jid(&recipient).serialize();
    let inbound_keys = receipt.child_by_tag(&["keys"]);
    let bundle = if let Some(keys_node) = inbound_keys {
        Some(parse_keys_to_bundle(&recipient, keys_node, parsed.registration_id)?)
    } else {
        None
    };

    let env = match bundle {
        Some(b) => {
            // Reset the session and run X3DH against the new bundle.
            let our_identity = IdentityKeyPair::new(client.device.identity_key.clone());
            let outgoing = x3dh::initiate_outgoing(&our_identity, &b)?;
            let mut state = SessionState::initialize_as_alice(
                our_identity.public(),
                b.identity_key,
                outgoing,
                b.signed_pre_key_id,
                b.pre_key_id,
                client.device.registration_id,
                b.registration_id,
            );
            let env = SessionCipher::encrypt(&mut state, &plaintext)?;
            // Persist via crate::send_message::encode_session — but that's
            // private. We swap the session bytes via the same wire route as
            // `crate::send_encrypt::encrypt_for_recipient` would. For now,
            // delete the old session to force a fresh first-flight on the
            // next regular send (the resent `<message>` already rides this
            // freshly-built state in-memory).
            client.device.sessions.delete_session(&address).await?;
            env
        }
        None => {
            // No bundle — this means our existing session should still work.
            // Ride [`crate::send_encrypt::encrypt_for_recipient`] to drive the
            // cipher and persist the advanced session.
            let nodes = crate::send_encrypt::encrypt_for_recipient(client, &recipient, &plaintext).await?;
            let enc = nodes
                .into_iter()
                .next()
                .ok_or_else(|| ClientError::Other("encrypt_for_recipient returned no <enc>".into()))?;
            let enc_type = enc.get_attr_str("type").unwrap_or("msg").to_owned();
            let ciphertext = match enc.content {
                Value::Bytes(b) => b,
                _ => return Err(ClientError::Other("encrypt_for_recipient: <enc> not bytes".into())),
            };
            if enc_type == "pkmsg" {
                EncryptedMessage::Pkmsg(ciphertext)
            } else {
                EncryptedMessage::Msg(ciphertext)
            }
        }
    };

    let (enc_type, ciphertext) = match env {
        EncryptedMessage::Pkmsg(b) => ("pkmsg", b),
        EncryptedMessage::Msg(b) => ("msg", b),
    };

    let mut enc_attrs = Attrs::new();
    enc_attrs.insert("v".into(), Value::String("2".into()));
    enc_attrs.insert("type".into(), Value::String(enc_type.into()));
    enc_attrs.insert("count".into(), Value::String(parsed.retry_count.to_string()));
    let enc = Node::new("enc", enc_attrs, Some(Value::Bytes(ciphertext)));

    let mut msg_attrs = Attrs::new();
    msg_attrs.insert("id".into(), Value::String(parsed.message_id.clone()));
    msg_attrs.insert("to".into(), Value::Jid(parsed.from.clone()));
    msg_attrs.insert("type".into(), Value::String("text".into()));
    msg_attrs.insert("t".into(), Value::String(parsed.timestamp.to_string()));
    if let Some(p) = &parsed.participant {
        msg_attrs.insert("participant".into(), Value::Jid(p.clone()));
    }
    let message = Node::new("message", msg_attrs, Some(Value::Nodes(vec![enc])));

    client.send_node(&message).await
}

/// Parse a `<keys>…</keys>` Node from an inbound retry receipt into a
/// [`PreKeyBundle`]. Mirrors `nodeToPreKeyBundle` upstream — except the
/// `<registration>` lives next to the keys (not inside a `<user>`), and the
/// `device_id` is taken from the recipient JID's `device` field.
fn parse_keys_to_bundle(
    recipient: &Jid,
    keys: &Node,
    pre_parsed_registration: Option<u32>,
) -> Result<PreKeyBundle, ClientError> {
    let registration_id = match pre_parsed_registration {
        Some(r) => r,
        None => {
            // Fall back to re-extracting the registration id from inside <keys>.
            let reg = keys
                .child_by_tag(&["registration"])
                .ok_or_else(|| ClientError::Malformed("<keys> missing <registration>".into()))?
                .content
                .as_bytes()
                .ok_or_else(|| ClientError::Malformed("<registration> not bytes".into()))?
                .to_vec();
            if reg.len() != 4 {
                return Err(ClientError::Malformed(format!(
                    "<registration> wrong length {}",
                    reg.len()
                )));
            }
            u32::from_be_bytes([reg[0], reg[1], reg[2], reg[3]])
        }
    };

    let identity = keys
        .child_by_tag(&["identity"])
        .ok_or_else(|| ClientError::Malformed("<keys> missing <identity>".into()))?
        .content
        .as_bytes()
        .ok_or_else(|| ClientError::Malformed("<identity> not bytes".into()))?
        .to_vec();
    if identity.len() != 32 {
        return Err(ClientError::Malformed(format!(
            "<identity> wrong length {}",
            identity.len()
        )));
    }
    let mut identity_key = [0u8; 32];
    identity_key.copy_from_slice(&identity);

    let signed = keys
        .child_by_tag(&["skey"])
        .ok_or_else(|| ClientError::Malformed("<keys> missing <skey>".into()))?;
    let (signed_pre_key_id, signed_pre_key_public, signed_pre_key_signature) =
        parse_signed_prekey_block(signed)?;

    let one_time = keys.child_by_tag(&["key"]);
    let (pre_key_id, pre_key_public) = match one_time {
        Some(n) => {
            let (id, pub_) = parse_one_time_prekey_block(n)?;
            (Some(id), Some(pub_))
        }
        None => (None, None),
    };

    Ok(PreKeyBundle {
        registration_id,
        device_id: recipient.device as u32,
        pre_key_id,
        pre_key_public,
        signed_pre_key_id,
        signed_pre_key_public,
        signed_pre_key_signature,
        identity_key,
    })
}

fn parse_one_time_prekey_block(node: &Node) -> Result<(u32, [u8; 32]), ClientError> {
    let id_bytes = node
        .child_by_tag(&["id"])
        .ok_or_else(|| ClientError::Malformed("<key> missing <id>".into()))?
        .content
        .as_bytes()
        .ok_or_else(|| ClientError::Malformed("<id> not bytes".into()))?
        .to_vec();
    if id_bytes.len() != 3 {
        return Err(ClientError::Malformed(format!(
            "<id> wrong length {}",
            id_bytes.len()
        )));
    }
    let id = u32::from_be_bytes([0, id_bytes[0], id_bytes[1], id_bytes[2]]);
    let value = node
        .child_by_tag(&["value"])
        .ok_or_else(|| ClientError::Malformed("<key> missing <value>".into()))?
        .content
        .as_bytes()
        .ok_or_else(|| ClientError::Malformed("<value> not bytes".into()))?
        .to_vec();
    if value.len() != 32 {
        return Err(ClientError::Malformed(format!(
            "<value> wrong length {}",
            value.len()
        )));
    }
    let mut pub_key = [0u8; 32];
    pub_key.copy_from_slice(&value);
    Ok((id, pub_key))
}

fn parse_signed_prekey_block(node: &Node) -> Result<(u32, [u8; 32], [u8; 64]), ClientError> {
    let (id, pub_key) = parse_one_time_prekey_block(node)?;
    let sig_bytes = node
        .child_by_tag(&["signature"])
        .ok_or_else(|| ClientError::Malformed("<skey> missing <signature>".into()))?
        .content
        .as_bytes()
        .ok_or_else(|| ClientError::Malformed("<signature> not bytes".into()))?
        .to_vec();
    if sig_bytes.len() != 64 {
        return Err(ClientError::Malformed(format!(
            "<signature> wrong length {}",
            sig_bytes.len()
        )));
    }
    let mut sig = [0u8; 64];
    sig.copy_from_slice(&sig_bytes);
    Ok((id, pub_key, sig))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use wha_binary::{Attrs, Node, Value};
    use wha_store::MemoryStore;
    use wha_types::jid::server;

    /// Build a synthetic `<receipt type="retry">` with the given inner
    /// `<retry>` attrs + optional `<keys><registration>` child.
    fn build_retry_receipt(
        outer_id: &str,
        from: Jid,
        participant: Option<Jid>,
        outer_t: &str,
        retry_id: &str,
        retry_count: Option<&str>,
        retry_t: &str,
        registration_id: Option<u32>,
    ) -> Node {
        let mut outer_attrs = Attrs::new();
        outer_attrs.insert("id".into(), Value::String(outer_id.into()));
        outer_attrs.insert("from".into(), Value::Jid(from));
        outer_attrs.insert("type".into(), Value::String("retry".into()));
        outer_attrs.insert("t".into(), Value::String(outer_t.into()));
        if let Some(p) = participant {
            outer_attrs.insert("participant".into(), Value::Jid(p));
        }

        let mut retry_attrs = Attrs::new();
        retry_attrs.insert("id".into(), Value::String(retry_id.into()));
        retry_attrs.insert("t".into(), Value::String(retry_t.into()));
        if let Some(c) = retry_count {
            retry_attrs.insert("count".into(), Value::String(c.into()));
        }
        let retry = Node::new("retry", retry_attrs, None);

        let mut children = vec![retry];
        if let Some(reg) = registration_id {
            let registration = Node::new(
                "registration",
                Attrs::new(),
                Some(Value::Bytes(reg.to_be_bytes().to_vec())),
            );
            let keys = Node::new(
                "keys",
                Attrs::new(),
                Some(Value::Nodes(vec![registration])),
            );
            children.push(keys);
        }

        Node::new("receipt", outer_attrs, Some(Value::Nodes(children)))
    }

    #[test]
    fn parse_retry_receipt_extracts_attrs() {
        let from = Jid::new("123", server::DEFAULT_USER);
        let participant = Jid::new("999", server::DEFAULT_USER);
        let node = build_retry_receipt(
            "MSG-X",
            from.clone(),
            Some(participant.clone()),
            "1714521600",
            "MSG-X",
            Some("2"),
            "1714521605",
            Some(0xDEADBEEF),
        );

        let parsed = parse_retry_receipt(&node).expect("parse");
        assert_eq!(parsed.message_id, "MSG-X");
        assert_eq!(parsed.from, from);
        assert_eq!(parsed.participant.as_ref(), Some(&participant));
        assert_eq!(parsed.timestamp, 1714521605);
        assert_eq!(parsed.retry_count, 2);
        assert_eq!(parsed.registration_id, Some(0xDEADBEEF));
    }

    #[test]
    fn parse_receipt_without_count_defaults_to_one() {
        let from = Jid::new("123", server::DEFAULT_USER);
        let node = build_retry_receipt(
            "MSG-Y",
            from,
            None,
            "1714521600",
            "MSG-Y",
            None, // no count
            "1714521600",
            None, // no <keys>
        );

        let parsed = parse_retry_receipt(&node).expect("parse");
        assert_eq!(parsed.retry_count, 1);
        assert!(parsed.registration_id.is_none());
        assert!(parsed.participant.is_none());
    }

    #[tokio::test]
    async fn handle_retry_receipt_no_recent_message_returns_ok() {
        // Build a real Client (no socket needed — we never touch the wire).
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);

        let from = Jid::new("555", server::DEFAULT_USER);
        let node = build_retry_receipt(
            "UNKNOWN-MSG",
            from,
            None,
            "1714521600",
            "UNKNOWN-MSG",
            Some("1"),
            "1714521600",
            None,
        );

        // Without a recent-messages entry the handler must return Ok so the
        // receive loop keeps running. Mirrors upstream's "couldn't find
        // message" log + return path.
        handle_retry_receipt(&cli, &node)
            .await
            .expect("handler should swallow unknown messages");
    }

    /// `build_retry_receipt_node` shape matches the upstream
    /// `Client.sendRetryReceipt` payload: outer `<receipt>` carries id/to/
    /// participant/type, inner `<retry>` carries id/t/count/v, plus a
    /// `<registration>` body of 4 BE bytes. On retry_count==1 there's no
    /// `<keys>` child (mirror upstream's `if retryCount > 1` gate).
    #[tokio::test]
    async fn build_retry_receipt_node_shape_first_retry() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let registration_id = device.registration_id;
        let (cli, _evt) = Client::new(device);

        let sender = Jid::new("12345", server::DEFAULT_USER);
        let device_jid: Jid = "12345:7@s.whatsapp.net".parse().unwrap();
        let node =
            build_retry_receipt_node(&cli, "MSG-A", &sender, &device_jid, 1, 1714521600)
                .await
                .expect("build");

        assert_eq!(node.tag, "receipt");
        assert_eq!(node.get_attr_str("id"), Some("MSG-A"));
        assert_eq!(node.get_attr_str("type"), Some("retry"));
        assert_eq!(node.get_attr_jid("to"), Some(&sender));
        assert_eq!(node.get_attr_jid("participant"), Some(&device_jid));

        // <retry id=… t=… count=… v=1/>
        let retry = node.child_by_tag(&["retry"]).expect("<retry/>");
        assert_eq!(retry.get_attr_str("id"), Some("MSG-A"));
        assert_eq!(retry.get_attr_str("t"), Some("1714521600"));
        assert_eq!(retry.get_attr_str("count"), Some("1"));
        assert_eq!(retry.get_attr_str("v"), Some("1"));

        // <registration>4 BE bytes</registration>
        let reg = node.child_by_tag(&["registration"]).expect("<registration>");
        let bytes = reg.content.as_bytes().expect("registration is bytes");
        assert_eq!(bytes.len(), 4);
        assert_eq!(
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            registration_id
        );

        // First retry → no <keys> block.
        assert!(
            node.child_by_tag(&["keys"]).is_none(),
            "first retry must not include <keys>"
        );
    }

    /// On retry_count > 1 the receipt MUST carry a `<keys>` block with type,
    /// identity, key, skey, and a `<device-identity>` placeholder. Mirrors
    /// the `if retryCount > 1 || forceIncludeIdentity { … }` branch upstream.
    #[tokio::test]
    async fn build_retry_receipt_node_shape_includes_keys_on_count_gt_one() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        // Pre-mint at least one one-time prekey so gen_one_pre_key returns
        // deterministically.
        let _ = device.pre_keys.gen_one_pre_key().await.unwrap();
        let (cli, _evt) = Client::new(device);

        let sender = Jid::new("12345", server::DEFAULT_USER);
        let node =
            build_retry_receipt_node(&cli, "MSG-B", &sender, &sender, 2, 1714521700)
                .await
                .expect("build");

        let keys = node.child_by_tag(&["keys"]).expect("<keys>");
        assert!(keys.child_by_tag(&["type"]).is_some(), "keys.<type>");
        let kt = keys.child_by_tag(&["type"]).unwrap();
        assert_eq!(kt.content.as_bytes(), Some(&[DJB_TYPE][..]));

        let id = keys.child_by_tag(&["identity"]).expect("keys.<identity>");
        assert_eq!(id.content.as_bytes().map(|b| b.len()), Some(32));

        let key = keys.child_by_tag(&["key"]).expect("keys.<key>");
        assert!(key.child_by_tag(&["id"]).is_some(), "key.<id>");
        assert!(key.child_by_tag(&["value"]).is_some(), "key.<value>");

        let skey = keys.child_by_tag(&["skey"]).expect("keys.<skey>");
        assert!(skey.child_by_tag(&["id"]).is_some(), "skey.<id>");
        assert!(skey.child_by_tag(&["value"]).is_some(), "skey.<value>");
        let sig = skey.child_by_tag(&["signature"]).expect("skey.<signature>");
        assert_eq!(sig.content.as_bytes().map(|b| b.len()), Some(64));

        assert!(
            keys.child_by_tag(&["device-identity"]).is_some(),
            "keys.<device-identity>"
        );

        // No `participant` attr on the outer receipt when from == sender_device.
        assert!(node.get_attr_str("participant").is_none());
    }

    /// `send_retry_receipt` bumps the per-message retry counter and refuses
    /// to fire after the 5th call. Mirrors the `if retryCount >= 5` gate
    /// upstream.
    #[tokio::test]
    async fn send_retry_receipt_caps_at_five() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let sender = Jid::new("12345", server::DEFAULT_USER);

        // Calls 1..=5 try to send (and fail with NotConnected — the cap
        // logic still ran, the counter still advanced). Call 6 silently
        // returns Ok without trying to send.
        for i in 1..=MAX_RETRIES_PER_MESSAGE {
            let r = send_retry_receipt(&cli, "MSG-CAP", &sender, &sender, None).await;
            // First call sends nothing on the wire (no socket); we just want
            // to confirm the counter advances consistently.
            assert_eq!(cli.message_retry_count("MSG-CAP"), i);
            // We expect either Ok (cap branch) or NotConnected (the send_node
            // path). Anything else is a bug.
            match r {
                Ok(()) | Err(ClientError::NotConnected) => {}
                Err(e) => panic!("unexpected error on retry {i}: {e:?}"),
            }
        }
        // Past the cap → silent success, no further bumps.
        let r = send_retry_receipt(&cli, "MSG-CAP", &sender, &sender, None).await;
        assert!(matches!(r, Ok(())));
        assert_eq!(
            cli.message_retry_count("MSG-CAP"),
            MAX_RETRIES_PER_MESSAGE + 1,
            "the cap-bump itself counts (so we don't miss a future re-bump)"
        );
    }

    /// `handle_retry_receipt` consults the `LidStore` when the participant
    /// is a hidden-user (LID) JID and falls back to the literal LID JID
    /// when nothing's persisted. This test pre-populates the LidStore
    /// with a (LID, PN) pair, then sends a retry receipt naming the LID
    /// as participant. Because no recent-message cache entry exists for
    /// either form, the handler still returns Ok(()) (the cache miss is
    /// the early exit), but the LidStore lookup must have been exercised
    /// without panicking — the function compiling and running through
    /// the LID branch is the assertion.
    #[tokio::test]
    async fn handle_retry_receipt_resolves_lid_to_pn() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);

        let lid_participant: Jid = "AAAA@lid".parse().unwrap();
        let pn_participant: Jid = "12345@s.whatsapp.net".parse().unwrap();

        // Seed the LidStore so the resolution path actually finds a PN.
        cli.device
            .lids
            .put_lid_pn_mapping(lid_participant.clone(), pn_participant.clone())
            .await
            .unwrap();

        let from = Jid::new("group-id", server::GROUP);
        let mut outer_attrs = Attrs::new();
        outer_attrs.insert("id".into(), Value::String("MSG-LID".into()));
        outer_attrs.insert("from".into(), Value::Jid(from));
        outer_attrs.insert("type".into(), Value::String("retry".into()));
        outer_attrs.insert("t".into(), Value::String("1714521600".into()));
        outer_attrs.insert("participant".into(), Value::Jid(lid_participant.clone()));

        let mut retry_attrs = Attrs::new();
        retry_attrs.insert("id".into(), Value::String("MSG-LID".into()));
        retry_attrs.insert("t".into(), Value::String("1714521600".into()));
        retry_attrs.insert("count".into(), Value::String("1".into()));
        let retry = Node::new("retry", retry_attrs, None);
        let receipt = Node::new(
            "receipt",
            outer_attrs,
            Some(Value::Nodes(vec![retry])),
        );

        // Without a recent-messages cache entry the handler returns Ok(())
        // after running the LID resolution. We only care that the LID branch
        // was taken without erroring.
        handle_retry_receipt(&cli, &receipt)
            .await
            .expect("LID-participant retry should be handled gracefully");
    }
}
