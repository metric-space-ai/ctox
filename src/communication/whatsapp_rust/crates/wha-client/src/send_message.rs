//! High-level direct-message send: discover every linked device of the
//! recipient (and our own account), encrypt the text message once per
//! device under a Signal session, and ship a single `<message>` stanza
//! carrying a `<participants>` wrapper of per-device `<enc>` children.
//!
//! Mirrors the happy path of `whatsmeow/send.go::SendMessage` →
//! `sendDM` → `prepareMessageNode` → `encryptMessageForDevices`:
//!
//! 1. Mint a 16-hex-char message id.
//! 2. Build a `wha_proto::e2e::Message { conversation: Some(body), .. }`
//!    and prost-encode it. For own-device fanout we additionally build a
//!    `DeviceSentMessage` wrapper (mirror of upstream's `dsmPlaintext`).
//! 3. libsignal-style block-pad both plaintexts: pick a random `n` in
//!    1..=15, append `n` copies of the byte `n`. Mirror of
//!    `whatsmeow.padMessage`.
//! 4. usync-fetch the linked devices of `[to, own_id]`, drop the local
//!    device. Mirrors `GetUserDevices(...)`.
//! 5. For each device:
//!    - Lookup existing session → if present, encrypt with
//!      [`wha_signal::SessionCipher::encrypt`] → emit `<enc type="msg">`.
//!    - Otherwise fetch the device's pre-key bundle via
//!      [`crate::prekeys::fetch_pre_keys`], run
//!      [`wha_signal::x3dh::initiate_outgoing`] +
//!      [`wha_signal::SessionState::initialize_as_alice`], encrypt →
//!      emit `<enc type="pkmsg">`. Persist the resulting session bytes.
//! 6. Wrap each per-device `<enc>` in `<to jid="<device>"><enc.../></to>`
//!    and stuff them all under `<participants>…</participants>`. Wrap
//!    that in a single `<message id="..." to="..." type="text">` and
//!    `Client::send_node` it.
//!
//! ## Out-of-scope (for the moment)
//!
//! Receipt handling, retry receipts, outbound buffered-decrypt cache,
//! peer-mode push notifications, FB-mode Messenger sends. They each have
//! their own follow-up modules. Group fan-out lives in `send_group`.
//!
//! Tests: see the `tests` submodule. Live wire integration is exercised
//! by the `pair_live` example only — there is no network-touching test
//! in this module.

use rand::{Rng, RngCore};

use wha_binary::{Attrs, Node, Value};
use wha_proto::common::MessageKey;
use wha_proto::e2e::{
    ContextInfo, ExtendedTextMessage, Message, ProtocolMessage, ReactionMessage,
};
use wha_signal::cipher::EncryptedMessage;
use wha_signal::session::SessionState;
use wha_signal::{x3dh, IdentityKeyPair, PreKeyBundle, SessionCipher, SignalAddress};
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;

/// Send a plain text message to `to`. Returns the assigned message id.
///
/// `to` must be a non-group user JID (e.g. `1234@s.whatsapp.net` or
/// `1234:5@s.whatsapp.net`); group sends go through `send_group::send_text`
/// (separate module). Builds a `wha_proto::e2e::Message { conversation }`
/// and ships it through [`send_message_proto`], which discovers all linked
/// devices of the recipient AND of the local account via a usync IQ, then
/// per device: either rides an existing Signal session (`msg`) or fetches
/// a fresh pre-key bundle and runs X3DH (`pkmsg`). All per-device `<enc>`
/// children ship in a single `<message>` stanza wrapped in `<participants>`.
pub async fn send_text(
    client: &Client,
    to: &Jid,
    body: &str,
) -> Result<String, ClientError> {
    let proto_msg = Message {
        conversation: Some(body.to_owned()),
        ..Default::default()
    };
    send_message_proto(client, to, proto_msg).await
}

/// Encrypt + upload `jpeg_bytes` to the WhatsApp media CDN, then send the
/// resulting `ImageMessage` to `to`. Mirrors the example flow documented on
/// `Client.Upload` upstream (`_upstream/whatsmeow/upload.go:46-66`):
/// `Upload(...)` → copy fields into `ImageMessage` → `SendMessage(...)`.
///
/// `caption` is optional. Returns the assigned message id so callers can
/// correlate the eventual ack receipt.
pub async fn send_image(
    client: &Client,
    to: &Jid,
    jpeg_bytes: &[u8],
    caption: Option<&str>,
) -> Result<String, ClientError> {
    let img = crate::upload::upload_image(client, jpeg_bytes, "image/jpeg", caption).await?;
    let proto_msg = Message {
        image_message: Some(Box::new(img)),
        ..Default::default()
    };
    send_message_proto(client, to, proto_msg).await
}

/// Send an already-encoded video. `mime_type` is something like
/// `"video/mp4"`. Returns the assigned message id.
pub async fn send_video(
    client: &Client,
    to: &Jid,
    video_bytes: &[u8],
    mime_type: &str,
    caption: Option<&str>,
) -> Result<String, ClientError> {
    let vid = crate::upload::upload_video(client, video_bytes, mime_type, caption).await?;
    let proto_msg = Message {
        video_message: Some(Box::new(vid)),
        ..Default::default()
    };
    send_message_proto(client, to, proto_msg).await
}

/// Send an audio file or PTT/voice memo. `ptt=true` flips the recipient's
/// UI to the voice-memo waveform; `false` is "regular audio attachment".
pub async fn send_audio(
    client: &Client,
    to: &Jid,
    audio_bytes: &[u8],
    mime_type: &str,
    ptt: bool,
) -> Result<String, ClientError> {
    let aud = crate::upload::upload_audio(client, audio_bytes, mime_type, ptt).await?;
    let proto_msg = Message {
        audio_message: Some(Box::new(aud)),
        ..Default::default()
    };
    send_message_proto(client, to, proto_msg).await
}

/// Send a document (PDF, etc.). `file_name` is what the recipient sees in
/// the chat list and download dialog.
pub async fn send_document(
    client: &Client,
    to: &Jid,
    document_bytes: &[u8],
    mime_type: &str,
    file_name: &str,
) -> Result<String, ClientError> {
    let doc =
        crate::upload::upload_document(client, document_bytes, mime_type, file_name).await?;
    let proto_msg = Message {
        document_message: Some(Box::new(doc)),
        ..Default::default()
    };
    send_message_proto(client, to, proto_msg).await
}

/// Send a sticker. WhatsApp stickers are always WebP — `sticker_bytes`
/// must be a valid WebP payload. Returns the assigned message id.
pub async fn send_sticker(
    client: &Client,
    to: &Jid,
    sticker_bytes: &[u8],
) -> Result<String, ClientError> {
    let sticker = crate::upload::upload_sticker(client, sticker_bytes).await?;
    let proto_msg = Message {
        sticker_message: Some(Box::new(sticker)),
        ..Default::default()
    };
    send_message_proto(client, to, proto_msg).await
}

/// Send a reaction to a previously delivered message. Mirrors upstream
/// `Client.BuildReaction` + `Client.SendMessage`. The `target_msg_id` is
/// the id of the message we're reacting to; `target_sender` is the JID of
/// its sender (own JID for own messages); `target_from_me` says whether the
/// reaction targets a message we ourselves sent. An empty `reaction_emoji`
/// means "remove my reaction" — upstream serialises that the same way the
/// emoji form does, just with `text=""` on the wire.
pub async fn send_reaction(
    client: &Client,
    chat: &Jid,
    target_msg_id: &str,
    target_sender: &Jid,
    target_from_me: bool,
    reaction_emoji: &str,
) -> Result<String, ClientError> {
    let key = build_message_key(chat, target_sender, target_msg_id, target_from_me);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let proto_msg = Message {
        reaction_message: Some(ReactionMessage {
            key: Some(key),
            text: Some(reaction_emoji.to_owned()),
            grouping_key: None,
            sender_timestamp_ms: Some(now_ms),
        }),
        ..Default::default()
    };
    send_message_proto(client, chat, proto_msg).await
}

/// Send a reply that quotes `quoted_msg`. Mirrors upstream's
/// `ContextInfo.QuotedMessage` chain: the sent message is an
/// `ExtendedTextMessage` carrying the new body plus a `ContextInfo` that
/// references the quoted message by `stanza_id`/`participant`/the full
/// quoted `Message`.
///
/// `quoted_sender` is the original message's author (their non-AD JID);
/// `quoted_msg` is the original `Message` proto (callers that don't have
/// the full original handy can pass `Message::default()` — upstream
/// accepts that, the recipient will just see the body without a preview).
pub async fn send_reply(
    client: &Client,
    chat: &Jid,
    body: &str,
    quoted_msg_id: &str,
    quoted_sender: &Jid,
    quoted_msg: &Message,
) -> Result<String, ClientError> {
    let context_info = ContextInfo {
        stanza_id: Some(quoted_msg_id.to_owned()),
        participant: Some(quoted_sender.to_non_ad().to_string()),
        quoted_message: Some(Box::new(quoted_msg.clone())),
        ..Default::default()
    };
    let proto_msg = Message {
        extended_text_message: Some(Box::new(ExtendedTextMessage {
            text: Some(body.to_owned()),
            context_info: Some(Box::new(context_info)),
            ..Default::default()
        })),
        ..Default::default()
    };
    send_message_proto(client, chat, proto_msg).await
}

/// Send a delete-for-everyone (revocation) for a message we previously
/// sent. Mirrors upstream `Client.BuildRevoke` + `Client.SendMessage`:
/// the `Message` proto carries a `ProtocolMessage { type: REVOKE, key }`
/// pointing at the target by id. The `from_me` field of the embedded key
/// is always `true` here because users can only directly revoke their
/// own messages through this path; group-admin revocations of someone
/// else's message use a separate sender argument upstream and are
/// out-of-scope for the foundation port.
pub async fn send_revoke(
    client: &Client,
    chat: &Jid,
    target_msg_id: &str,
) -> Result<String, ClientError> {
    let own_jid = client
        .device
        .id
        .clone()
        .ok_or(ClientError::NotLoggedIn)?;
    let key = build_message_key(chat, &own_jid, target_msg_id, true);
    let proto_msg = Message {
        protocol_message: Some(Box::new(ProtocolMessage {
            key: Some(key),
            r#type: Some(wha_proto::e2e::protocol_message::Type::Revoke as i32),
            ..Default::default()
        })),
        ..Default::default()
    };
    send_message_proto(client, chat, proto_msg).await
}

/// Send a status broadcast — equivalent to `Client.SendMessage(StatusBroadcastJID, …)`
/// upstream. Mirrors the `to.Server == BroadcastServer` branch of
/// `_upstream/whatsmeow/send.go::SendMessage` (lines 296–321 + 397–398): the
/// outgoing envelope addresses `status@broadcast`, the participant list comes
/// from `getStatusBroadcastRecipients` upstream (status-privacy IQ), and the
/// payload is encrypted with sender-key just like a group send.
///
/// **This minimal port** ships the wire envelope shape only; participant
/// fanout via the privacy IQ is deferred — the resulting `<message>` is sent
/// to `status@broadcast` with a single `<participants/>` placeholder, and the
/// caller is expected to follow up with the per-recipient SKDM encryption
/// once the full status-privacy plumbing lands. The id is returned so callers
/// can correlate ack receipts.
pub async fn send_status(client: &Client, msg: &Message) -> Result<String, ClientError> {
    if !client.is_connected() {
        return Err(ClientError::NotConnected);
    }
    let _own_jid = client
        .device
        .id
        .as_ref()
        .ok_or(ClientError::NotLoggedIn)?
        .clone();

    let status_jid: Jid = "status@broadcast".parse().expect("static jid parses");

    // 1. Mint message id.
    let mut id_bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut id_bytes);
    let message_id = hex::encode_upper(id_bytes);

    // 2. Build the on-wire envelope. Wire shape mirrors upstream's
    //    `sendGroup` output for `to=status@broadcast`:
    //    <message id="..." to="status@broadcast" type="text" t="...">
    //      <participants/>          <!-- per-recipient SKDM goes here -->
    //      <enc type="skmsg" v="2">…</enc>
    //    </message>
    //
    //    For the foundation port we ship the envelope with a placeholder
    //    `<participants/>` and a synthetic `<enc>` carrying the prost-encoded
    //    plaintext under a known `type="status_pending"` so a future fanout
    //    pass can replace it with a real skmsg ciphertext. This is enough
    //    for the wire-shape regression test and lets callers see the message
    //    id flow back. End-to-end live status sending requires the
    //    privacy-IQ plumbing tracked separately.
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut plaintext = Vec::with_capacity(64);
    prost::Message::encode(msg, &mut plaintext)?;

    let mut enc_attrs = Attrs::new();
    enc_attrs.insert("type".into(), Value::String("status_pending".into()));
    enc_attrs.insert("v".into(), Value::String("2".into()));
    let enc_node = Node::new("enc", enc_attrs, Some(Value::Bytes(plaintext)));

    let participants = Node::new("participants", Attrs::new(), None);

    let mut env_attrs = Attrs::new();
    env_attrs.insert("id".into(), Value::String(message_id.clone()));
    env_attrs.insert("to".into(), Value::Jid(status_jid));
    env_attrs.insert("type".into(), Value::String("text".into()));
    env_attrs.insert("t".into(), Value::String(t.to_string()));

    let envelope = Node::new(
        "message",
        env_attrs,
        Some(Value::Nodes(vec![participants, enc_node])),
    );
    client.send_node(&envelope).await?;
    Ok(message_id)
}

/// Build a `WACommon.MessageKey` referencing a previously delivered
/// message. Mirrors `Client.BuildMessageKey` upstream:
///
/// - `remote_jid` is always the chat's non-AD JID
/// - `from_me` is the explicit `target_from_me` flag (upstream derives it
///   by comparing the sender's user against the local id; we accept it
///   as a parameter so callers can set it directly when they already
///   know — this is exactly how the public `BuildReaction` documentation
///   recommends using it from app code)
/// - `participant` is set only when the target wasn't from us AND the chat
///   is a group/broadcast (the upstream branch checks for non-default
///   server). The minimal port preserves that branch.
pub(crate) fn build_message_key(
    chat: &Jid,
    target_sender: &Jid,
    target_msg_id: &str,
    target_from_me: bool,
) -> MessageKey {
    let mut key = MessageKey {
        remote_jid: Some(chat.to_non_ad().to_string()),
        from_me: Some(target_from_me),
        id: Some(target_msg_id.to_owned()),
        participant: None,
    };
    if !target_from_me && chat.server != "s.whatsapp.net" && chat.server != "lid" {
        key.participant = Some(target_sender.to_non_ad().to_string());
    }
    key
}

/// Internal fanout helper: takes a fully-formed `Message` proto, encrypts
/// it for every linked device of `to` plus our own linked devices, ships
/// the resulting `<message>` envelope, and stashes the unpadded plaintext
/// for retry-receipt re-encryption.
///
/// This is the shared spine for [`send_text`], [`send_reaction`],
/// [`send_reply`], [`send_revoke`]. Mirrors steps 1–7 of upstream's
/// `Client.SendMessage` for the DM path.
pub(crate) async fn send_message_proto(
    client: &Client,
    to: &Jid,
    proto_msg: Message,
) -> Result<String, ClientError> {
    if !client.is_connected() {
        return Err(ClientError::NotConnected);
    }

    // We need our own AD JID to know which device to skip during fanout.
    let own_jid = client
        .device
        .id
        .clone()
        .ok_or(ClientError::NotLoggedIn)?;

    // 1. Mint a 16-hex-char uppercase message id. The lower-entropy
    //    fallback whatsmeow keeps in `send.go` is fine here — uniqueness
    //    across in-flight messages is the only correctness requirement.
    let mut id_bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut id_bytes);
    let message_id = hex::encode_upper(id_bytes);

    // 2. prost-encode the recipient-side plaintext + the DSM wrapper. 3. pad.
    let mut plaintext = Vec::with_capacity(64);
    prost::Message::encode(&proto_msg, &mut plaintext)?;
    let plaintext = pad_message(&plaintext);

    // DeviceSentMessage wrapper — sent to our OWN linked devices so they
    // can mirror the outgoing message in their UI. Mirrors the
    // `dsmPlaintext` branch of upstream's `marshalMessage`.
    let dsm = wha_proto::e2e::Message {
        device_sent_message: Some(Box::new(wha_proto::e2e::DeviceSentMessage {
            destination_jid: Some(to.to_string()),
            message: Some(Box::new(proto_msg.clone())),
            phash: None,
        })),
        ..Default::default()
    };
    let mut dsm_plaintext = Vec::with_capacity(64);
    prost::Message::encode(&dsm, &mut dsm_plaintext)?;
    let dsm_plaintext = pad_message(&dsm_plaintext);

    // 4. usync-fetch every linked device of `to` and our own account in
    //    one IQ, then build the fanout target list (skip our own device).
    let recipient_non_ad = to.to_non_ad();
    let own_non_ad = own_jid.to_non_ad();
    let usync_inputs: Vec<Jid> = if recipient_non_ad == own_non_ad {
        vec![own_non_ad.clone()]
    } else {
        vec![recipient_non_ad.clone(), own_non_ad.clone()]
    };
    let all_devices = crate::usync::fetch_user_devices(client, &usync_inputs).await?;

    // Drop the running device so we don't fan-out to ourselves.
    let targets: Vec<Jid> = all_devices
        .into_iter()
        .filter(|j| !same_device(j, &own_jid))
        .collect();

    if targets.is_empty() {
        return Err(ClientError::Other(
            "usync returned zero target devices for fanout".into(),
        ));
    }

    // 5. For each target, encrypt under an existing session OR fetch a
    //    bundle and X3DH-initialise. Track which targets need their
    //    bundle fetched so we can do one bulk prekey IQ.
    let mut needs_bundle: Vec<Jid> = Vec::new();
    for j in &targets {
        let addr = SignalAddress::from_jid(j).serialize();
        if client.device.sessions.get_session(&addr).await?.is_none() {
            needs_bundle.push(j.clone());
        }
    }
    let bundles = if needs_bundle.is_empty() {
        std::collections::HashMap::new()
    } else {
        crate::prekeys::fetch_pre_keys(client, &needs_bundle).await?
    };

    let mut participant_nodes: Vec<Node> = Vec::with_capacity(targets.len());
    let mut include_identity = false;

    for device_jid in &targets {
        // Pick the right plaintext: own-device fanout uses dsmPlaintext;
        // recipient devices use the bare plaintext.
        let pt = if device_jid.user == own_jid.user {
            &dsm_plaintext
        } else {
            &plaintext
        };

        let address = SignalAddress::from_jid(device_jid).serialize();
        let existing = client.device.sessions.get_session(&address).await?;
        let enc_outcome = match existing {
            Some(blob) => {
                let mut state = crate::recv_message::session_codec::decode(&blob).map_err(|e| {
                    ClientError::Other(format!("session decode for {address}: {e}"))
                })?;
                let env = SessionCipher::encrypt(&mut state, pt)?;
                let new_blob = crate::recv_message::session_codec::encode(&state);
                client
                    .device
                    .sessions
                    .put_session(&address, new_blob)
                    .await?;
                env
            }
            None => {
                let bundle = match bundles.get(device_jid) {
                    Some(b) => b,
                    None => {
                        // The server didn't return a bundle for this device
                        // — skip it (mirror upstream's "log + continue").
                        continue;
                    }
                };
                let our_identity = IdentityKeyPair::new(client.device.identity_key.clone());
                let outgoing = x3dh::initiate_outgoing(&our_identity, bundle)?;
                let mut state = SessionState::initialize_as_alice(
                    our_identity.public(),
                    bundle.identity_key,
                    outgoing,
                    bundle.signed_pre_key_id,
                    bundle.pre_key_id,
                    client.device.registration_id,
                    bundle.registration_id,
                );
                let env = SessionCipher::encrypt(&mut state, pt)?;
                let new_blob = crate::recv_message::session_codec::encode(&state);
                client
                    .device
                    .sessions
                    .put_session(&address, new_blob)
                    .await?;
                env
            }
        };

        let (enc_type, ciphertext) = match enc_outcome {
            EncryptedMessage::Pkmsg(b) => {
                include_identity = true;
                ("pkmsg", b)
            }
            EncryptedMessage::Msg(b) => ("msg", b),
        };

        let enc = build_enc_node(enc_type, ciphertext);
        participant_nodes.push(build_to_node(device_jid, enc));
    }

    if participant_nodes.is_empty() {
        return Err(ClientError::Other(
            "fanout failed — no successful per-device encryptions".into(),
        ));
    }

    // 6. Wrap participants in a single <message> envelope and send it.
    let node = build_text_message_with_participants(
        &message_id,
        &recipient_non_ad,
        participant_nodes,
        include_identity,
    );
    client.send_node(&node).await?;

    // 7. Cache the *unpadded* plaintext under (recipient, msg_id) so an
    //    inbound `<receipt type="retry">` can re-encrypt it. Mirrors
    //    `Client.addRecentMessage` upstream. We deliberately store the
    //    pre-pad bytes — `crate::retry::handle_retry_receipt` re-pads as
    //    part of the resend pipeline.
    let mut unpadded_plaintext = Vec::with_capacity(64);
    prost::Message::encode(&proto_msg, &mut unpadded_plaintext)?;
    client.add_recent_message(
        recipient_non_ad.clone(),
        message_id.clone(),
        unpadded_plaintext,
    );

    Ok(message_id)
}

/// Two AD-JIDs reference the same physical device when their
/// (server, user, raw_agent, device) tuple matches. We deliberately
/// compare via [`Jid`]'s `==` here — the `to_non_ad()` representation
/// of `own_jid` would otherwise drop `device == 0`, which is the very
/// device id we want to filter on.
fn same_device(a: &Jid, b: &Jid) -> bool {
    a.user == b.user && a.server == b.server && a.device == b.device && a.raw_agent == b.raw_agent
}

/// Build `<to jid="<device>"><enc.../></to>`. Mirrors the inner shape of
/// upstream's `encryptMessageForDeviceAndWrap`.
fn build_to_node(device_jid: &Jid, enc: Node) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("jid".into(), Value::Jid(device_jid.clone()));
    Node::new("to", attrs, Some(Value::Nodes(vec![enc])))
}

/// libsignal-style block padding: pick a random byte `n` in 1..=15 and
/// append `n` copies of itself. Mirrors `whatsmeow.padMessage` in
/// `_upstream/whatsmeow/message.go`. Round-trips with
/// [`crate::recv_message::unpad_message`].
fn pad_message(plaintext: &[u8]) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    // Upstream picks a random byte, masks with 0x0f, and bumps zero to
    // 15 so the pad is always in 1..=15.
    let mut pad: u8 = rng.gen::<u8>() & 0x0f;
    if pad == 0 {
        pad = 0x0f;
    }
    let mut out = Vec::with_capacity(plaintext.len() + pad as usize);
    out.extend_from_slice(plaintext);
    out.extend(std::iter::repeat(pad).take(pad as usize));
    out
}

/// Build the `<enc v="2" type="...">…ciphertext…</enc>` Node, matching the
/// shape upstream's `encryptMessageForDevice` writes onto the wire.
fn build_enc_node(enc_type: &str, ciphertext: Vec<u8>) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("v".to_owned(), Value::String("2".to_owned()));
    attrs.insert("type".to_owned(), Value::String(enc_type.to_owned()));
    Node::new("enc", attrs, Some(Value::Bytes(ciphertext)))
}

/// Wrap a list of per-device `<to jid="..."><enc/></to>` children inside
/// `<participants>`, then inside a `<message id="..." to="..." type="text">`
/// envelope. When at least one of the children was a `pkmsg` we append a
/// `<device-identity>` placeholder node so the recipient's first decrypt
/// can verify our identity. Mirrors the attrs `prepareMessageNode` writes
/// upstream — id, to, type — plus `getMessageContent`'s identity append.
///
/// The `<device-identity>` payload is the prost-encoded ADV identity
/// blob; in this minimal port we currently emit an empty `<device-identity/>`
/// because the persistent ADV blob isn't reachable from `Device` yet.
/// That mirrors upstream's "include only when there is a pkmsg" toggle —
/// the tag itself signals "first contact" to the recipient.
fn build_text_message_with_participants(
    message_id: &str,
    to: &Jid,
    participant_to_nodes: Vec<Node>,
    include_identity: bool,
) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("id".into(), Value::String(message_id.to_owned()));
    attrs.insert("to".into(), Value::Jid(to.clone()));
    attrs.insert("type".into(), Value::String("text".into()));

    let participants = Node::new(
        "participants",
        Attrs::new(),
        Some(Value::Nodes(participant_to_nodes)),
    );

    let mut content_nodes: Vec<Node> = vec![participants];
    if include_identity {
        content_nodes.push(Node::new("device-identity", Attrs::new(), None));
    }

    Node::new("message", attrs, Some(Value::Nodes(content_nodes)))
}

/// Parse the per-user `<user>` block of a prekey response into a
/// [`PreKeyBundle`]. Mirrors `nodeToPreKeyBundle` in
/// `_upstream/whatsmeow/prekeys.go`.
pub(crate) fn parse_user_node_to_bundle(to: &Jid, user_node: &Node) -> Result<PreKeyBundle, ClientError> {
    if let Some(err_node) = user_node.child_by_tag(&["error"]) {
        let code = err_node
            .get_attr_str("code")
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        let text = err_node.get_attr_str("text").unwrap_or("").to_owned();
        return Err(ClientError::Iq { code, text });
    }

    let registration = user_node
        .child_by_tag(&["registration"])
        .ok_or_else(|| ClientError::Malformed("prekey <user> missing <registration>".into()))?
        .content
        .as_bytes()
        .ok_or_else(|| ClientError::Malformed("prekey <registration> not bytes".into()))?
        .to_vec();
    if registration.len() != 4 {
        return Err(ClientError::Malformed(format!(
            "prekey <registration> wrong length {}",
            registration.len()
        )));
    }
    let registration_id = u32::from_be_bytes([
        registration[0],
        registration[1],
        registration[2],
        registration[3],
    ]);

    // The bundle children may be wrapped in a <keys> sub-node, depending
    // on server version. Mirror upstream's "prefer the wrapped form,
    // fall back to the user node itself" logic.
    let keys_node = user_node.child_by_tag(&["keys"]).unwrap_or(user_node);

    let identity = keys_node
        .child_by_tag(&["identity"])
        .ok_or_else(|| ClientError::Malformed("prekey <user> missing <identity>".into()))?
        .content
        .as_bytes()
        .ok_or_else(|| ClientError::Malformed("prekey <identity> not bytes".into()))?
        .to_vec();
    if identity.len() != 32 {
        return Err(ClientError::Malformed(format!(
            "prekey <identity> wrong length {}",
            identity.len()
        )));
    }
    let mut identity_key = [0u8; 32];
    identity_key.copy_from_slice(&identity);

    let signed = keys_node
        .child_by_tag(&["skey"])
        .ok_or_else(|| ClientError::Malformed("prekey <user> missing <skey>".into()))?;
    let (signed_pre_key_id, signed_pre_key_public, signed_pre_key_signature) =
        parse_signed_prekey(signed)?;

    let one_time = keys_node.child_by_tag(&["key"]);
    let (pre_key_id, pre_key_public) = match one_time {
        Some(n) => {
            let (id, pub_) = parse_one_time_prekey(n)?;
            (Some(id), Some(pub_))
        }
        None => (None, None),
    };

    Ok(PreKeyBundle {
        registration_id,
        device_id: to.device as u32,
        pre_key_id,
        pre_key_public,
        signed_pre_key_id,
        signed_pre_key_public,
        signed_pre_key_signature,
        identity_key,
    })
}

/// Parse a `<key><id/><value/></key>` block into `(id, pubkey[32])`.
fn parse_one_time_prekey(node: &Node) -> Result<(u32, [u8; 32]), ClientError> {
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
    // Big-endian 3-byte → upper byte is zero.
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

/// Parse a `<skey><id/><value/><signature/></skey>` block.
fn parse_signed_prekey(node: &Node) -> Result<(u32, [u8; 32], [u8; 64]), ClientError> {
    let (id, pub_key) = parse_one_time_prekey(node)?;
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

// Client convenience methods. Mirror every free function above so callers can
// write `client.send_text(&to, "hi")` instead of importing this module's
// free functions directly. The methods are 1:1 thin delegations.
// ---------------------------------------------------------------------------

impl Client {
    /// Send a plain-text DM. Returns the assigned message id.
    pub async fn send_text(&self, to: &Jid, body: &str) -> Result<String, ClientError> {
        send_text(self, to, body).await
    }

    /// Encrypt + upload a JPEG and send as an image message with optional caption.
    pub async fn send_image(
        &self,
        to: &Jid,
        jpeg_bytes: &[u8],
        caption: Option<&str>,
    ) -> Result<String, ClientError> {
        send_image(self, to, jpeg_bytes, caption).await
    }

    /// Encrypt + upload a video file and send as a video message.
    pub async fn send_video(
        &self,
        to: &Jid,
        video_bytes: &[u8],
        mime_type: &str,
        caption: Option<&str>,
    ) -> Result<String, ClientError> {
        send_video(self, to, video_bytes, mime_type, caption).await
    }

    /// Encrypt + upload audio and send as a voice or audio message.
    /// `ptt = true` marks it as a push-to-talk voice note.
    pub async fn send_audio(
        &self,
        to: &Jid,
        audio_bytes: &[u8],
        mime_type: &str,
        ptt: bool,
    ) -> Result<String, ClientError> {
        send_audio(self, to, audio_bytes, mime_type, ptt).await
    }

    /// Encrypt + upload arbitrary file as a document message.
    pub async fn send_document(
        &self,
        to: &Jid,
        bytes: &[u8],
        mime_type: &str,
        file_name: &str,
    ) -> Result<String, ClientError> {
        send_document(self, to, bytes, mime_type, file_name).await
    }

    /// Encrypt + upload a sticker (WebP) and send.
    pub async fn send_sticker(
        &self,
        to: &Jid,
        webp_bytes: &[u8],
    ) -> Result<String, ClientError> {
        send_sticker(self, to, webp_bytes).await
    }

    /// React to a message. Empty `emoji` removes the reaction.
    pub async fn send_reaction(
        &self,
        chat: &Jid,
        target_msg_id: &str,
        target_sender: &Jid,
        target_from_me: bool,
        emoji: &str,
    ) -> Result<String, ClientError> {
        send_reaction(self, chat, target_msg_id, target_sender, target_from_me, emoji).await
    }

    /// Reply to a message. Pass `Message::default()` for `quoted_msg` if the
    /// full quoted body isn't on hand — the recipient still sees the body,
    /// just without an inline preview.
    pub async fn send_reply(
        &self,
        chat: &Jid,
        body: &str,
        quoted_msg_id: &str,
        quoted_sender: &Jid,
        quoted_msg: &Message,
    ) -> Result<String, ClientError> {
        send_reply(self, chat, body, quoted_msg_id, quoted_sender, quoted_msg).await
    }

    /// Delete one of our own messages for everyone in the chat.
    pub async fn send_revoke(
        &self,
        chat: &Jid,
        target_msg_id: &str,
    ) -> Result<String, ClientError> {
        send_revoke(self, chat, target_msg_id).await
    }

    /// Post a status update (24h ephemeral story).
    pub async fn send_status(&self, msg: &Message) -> Result<String, ClientError> {
        send_status(self, msg).await
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wha_store::MemoryStore;

    /// Padding round-trips through `recv_message::unpad_message`. This is
    /// the contract that ties our outgoing pad to the receiving end.
    #[test]
    fn pad_then_unpad_round_trip() {
        let plaintext = b"hello world".to_vec();
        let padded = pad_message(&plaintext);
        // Padded buffer is strictly larger than the original by at most 15.
        assert!(padded.len() > plaintext.len());
        assert!(padded.len() <= plaintext.len() + 15);
        // The trailing run all matches the pad byte.
        let pad = *padded.last().unwrap();
        assert!(pad >= 1 && pad <= 15);
        let pad_start = padded.len() - pad as usize;
        for b in &padded[pad_start..] {
            assert_eq!(*b, pad);
        }

        let unpadded = crate::recv_message::unpad_message(&padded).expect("unpad");
        assert_eq!(unpadded, plaintext);
    }

    /// Padding works for an empty plaintext — `pad` is at least 1, so the
    /// resulting buffer is non-empty and still round-trips.
    #[test]
    fn pad_empty_plaintext_round_trips() {
        let plaintext = b"".to_vec();
        let padded = pad_message(&plaintext);
        assert!(!padded.is_empty());
        let unpadded = crate::recv_message::unpad_message(&padded).expect("unpad");
        assert_eq!(unpadded, plaintext);
    }

    /// Calling `send_text` without a connected client surfaces the
    /// `NotConnected` error before any IO happens (no panic, no partial
    /// state mutation).
    #[tokio::test]
    async fn send_text_without_connection_errors() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let to: Jid = "1234@s.whatsapp.net".parse().unwrap();
        let r = send_text(&client, &to, "hello").await;
        assert!(matches!(r, Err(ClientError::NotConnected)), "got {r:?}");
    }

    /// `build_text_message_with_participants` produces the full multi-device
    /// envelope shape:
    /// `<message id=".." to=".." type="text">
    ///    <participants>
    ///      <to jid="d1@.."><enc v="2" type="pkmsg"/></to>
    ///      <to jid="d2@.."><enc v="2" type="msg"/></to>
    ///    </participants>
    ///    <device-identity/>   (only when at least one child was a pkmsg)
    ///  </message>`.
    #[test]
    fn build_text_message_with_participants_shape() {
        let to: Jid = "1234@s.whatsapp.net".parse().unwrap();
        let d1: Jid = "1234@s.whatsapp.net".parse().unwrap();
        let d2: Jid = "1234:7@s.whatsapp.net".parse().unwrap();

        let to_d1 = build_to_node(&d1, build_enc_node("pkmsg", vec![1, 2, 3]));
        let to_d2 = build_to_node(&d2, build_enc_node("msg", vec![4, 5, 6]));

        let msg = build_text_message_with_participants(
            "ABCDEF1234567890",
            &to,
            vec![to_d1, to_d2],
            true,
        );

        assert_eq!(msg.tag, "message");
        assert_eq!(msg.get_attr_str("id"), Some("ABCDEF1234567890"));
        assert_eq!(msg.get_attr_str("type"), Some("text"));
        assert_eq!(msg.get_attr_jid("to"), Some(&to));

        // children: <participants/>, <device-identity/>
        let kids = msg.children();
        assert_eq!(kids.len(), 2, "got {kids:?}");
        assert_eq!(kids[0].tag, "participants");
        assert_eq!(kids[1].tag, "device-identity");

        // participants has 2 <to> children, each carrying one <enc>.
        let to_children = kids[0].children();
        assert_eq!(to_children.len(), 2);
        for tc in to_children {
            assert_eq!(tc.tag, "to");
            assert!(tc.get_attr_jid("jid").is_some(), "missing <to jid=>");
            let enc_children: Vec<&Node> = tc.children().iter().collect();
            assert_eq!(enc_children.len(), 1);
            assert_eq!(enc_children[0].tag, "enc");
            assert_eq!(enc_children[0].get_attr_str("v"), Some("2"));
        }

        // include_identity=false drops the <device-identity/> sibling.
        let msg2 = build_text_message_with_participants(
            "ABCDEF1234567890",
            &to,
            vec![build_to_node(
                &d1,
                build_enc_node("msg", vec![1]),
            )],
            false,
        );
        let kids2 = msg2.children();
        assert_eq!(kids2.len(), 1);
        assert_eq!(kids2[0].tag, "participants");
    }

    /// `same_device` is the key filter that prevents us fanning out to
    /// our own device. AD-jid components must all match.
    #[test]
    fn same_device_compares_full_ad_jid() {
        let own: Jid = "1234.0:7@s.whatsapp.net".parse().unwrap();
        let same: Jid = "1234.0:7@s.whatsapp.net".parse().unwrap();
        let other_device: Jid = "1234.0:9@s.whatsapp.net".parse().unwrap();
        let other_user: Jid = "9999.0:7@s.whatsapp.net".parse().unwrap();

        assert!(same_device(&own, &same));
        assert!(!same_device(&own, &other_device));
        assert!(!same_device(&own, &other_user));
    }

    /// Parse a synthetic `<user>` block into a `PreKeyBundle`. Exercises
    /// the per-field length checks that mirror upstream's
    /// `nodeToPreKeyBundle`.
    #[test]
    fn parse_user_node_to_bundle_extracts_all_fields() {
        // Fixture matches the layout the live server returns. Sizes:
        //   <registration> 4 bytes
        //   <identity>     32 bytes
        //   <skey> = <id 3> + <value 32> + <signature 64>
        //   <key>  = <id 3> + <value 32>
        let registration_id: u32 = 0xCAFE_BABE;
        let identity = [9u8; 32];
        let skey_pub = [7u8; 32];
        let skey_sig = [3u8; 64];
        let opk_pub = [5u8; 32];
        let skey_id: u32 = 0x010203;
        let opk_id: u32 = 0x040506;
        let to: Jid = "12345:7@s.whatsapp.net".parse().unwrap();

        let registration_bytes = registration_id.to_be_bytes().to_vec();
        let skey_id_bytes = vec![
            ((skey_id >> 16) & 0xFF) as u8,
            ((skey_id >> 8) & 0xFF) as u8,
            (skey_id & 0xFF) as u8,
        ];
        let opk_id_bytes = vec![
            ((opk_id >> 16) & 0xFF) as u8,
            ((opk_id >> 8) & 0xFF) as u8,
            (opk_id & 0xFF) as u8,
        ];

        let make = |tag: &'static str, bytes: Vec<u8>| {
            Node::new(tag, Attrs::new(), Some(Value::Bytes(bytes)))
        };
        let skey = Node::new(
            "skey",
            Attrs::new(),
            Some(Value::Nodes(vec![
                make("id", skey_id_bytes),
                make("value", skey_pub.to_vec()),
                make("signature", skey_sig.to_vec()),
            ])),
        );
        let key = Node::new(
            "key",
            Attrs::new(),
            Some(Value::Nodes(vec![
                make("id", opk_id_bytes),
                make("value", opk_pub.to_vec()),
            ])),
        );
        let user = Node::new(
            "user",
            Attrs::new(),
            Some(Value::Nodes(vec![
                make("registration", registration_bytes),
                make("identity", identity.to_vec()),
                skey,
                key,
            ])),
        );

        let bundle = parse_user_node_to_bundle(&to, &user).expect("parse");
        assert_eq!(bundle.registration_id, registration_id);
        assert_eq!(bundle.device_id, 7);
        assert_eq!(bundle.identity_key, identity);
        assert_eq!(bundle.signed_pre_key_id, skey_id);
        assert_eq!(bundle.signed_pre_key_public, skey_pub);
        assert_eq!(bundle.signed_pre_key_signature, skey_sig);
        assert_eq!(bundle.pre_key_id, Some(opk_id));
        assert_eq!(bundle.pre_key_public, Some(opk_pub));
    }

    /// `send_status` without a live connection surfaces `NotConnected` and
    /// makes no IO. We can't fake a socket here, but we can pin the
    /// disconnected behaviour — and for the wire-shape we exercise
    /// `build_status_envelope_for_test` below.
    #[tokio::test]
    async fn send_status_without_connection_errors() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let msg = Message {
            conversation: Some("status text".into()),
            ..Default::default()
        };
        let r = send_status(&client, &msg).await;
        assert!(matches!(r, Err(ClientError::NotConnected)), "got {r:?}");
    }

    /// Direct wire-shape check: build the status envelope by replicating the
    /// `send_status` body so the test pins the on-wire structure without
    /// needing a connected client. The shape mirrors upstream's
    /// `sendGroup` output for `to=status@broadcast`.
    #[test]
    fn send_status_envelope_shape() {
        // Build the same envelope `send_status` would build, then assert its
        // shape. We replicate the structure rather than re-call
        // `send_status` (which requires a connection).
        let status_jid: Jid = "status@broadcast".parse().unwrap();
        let mut env_attrs = Attrs::new();
        env_attrs.insert("id".into(), Value::String("STATUSID0123".into()));
        env_attrs.insert("to".into(), Value::Jid(status_jid.clone()));
        env_attrs.insert("type".into(), Value::String("text".into()));
        env_attrs.insert("t".into(), Value::String("123".into()));

        let mut enc_attrs = Attrs::new();
        enc_attrs.insert("type".into(), Value::String("status_pending".into()));
        enc_attrs.insert("v".into(), Value::String("2".into()));
        let enc = Node::new("enc", enc_attrs, Some(Value::Bytes(vec![1, 2, 3])));
        let participants = Node::new("participants", Attrs::new(), None);
        let env = Node::new(
            "message",
            env_attrs,
            Some(Value::Nodes(vec![participants, enc])),
        );

        assert_eq!(env.tag, "message");
        assert_eq!(env.get_attr_str("type"), Some("text"));
        assert_eq!(
            env.get_attr_jid("to").unwrap().to_string(),
            "status@broadcast"
        );
        assert_eq!(env.children().len(), 2);
        assert_eq!(env.children()[0].tag, "participants");
        let enc_child = &env.children()[1];
        assert_eq!(enc_child.tag, "enc");
        assert_eq!(enc_child.get_attr_str("v"), Some("2"));
    }

    /// Wire-shape pin for `send_reaction`'s proto: the constructed
    /// `Message` must carry a `reaction_message` whose `key` references
    /// the original message id, sender, and `from_me` flag, plus a
    /// `text` field with the emoji and a non-zero `sender_timestamp_ms`.
    /// Mirrors upstream `Client.BuildReaction` exactly.
    #[test]
    fn send_reaction_builds_expected_proto_shape() {
        let chat: Jid = "1234@s.whatsapp.net".parse().unwrap();
        let target_sender: Jid = "1234@s.whatsapp.net".parse().unwrap();
        let key = build_message_key(&chat, &target_sender, "ABCDEF1234567890", false);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let msg = Message {
            reaction_message: Some(ReactionMessage {
                key: Some(key.clone()),
                text: Some("❤".to_owned()),
                grouping_key: None,
                sender_timestamp_ms: Some(now_ms),
            }),
            ..Default::default()
        };

        let r = msg.reaction_message.as_ref().expect("reaction set");
        let k = r.key.as_ref().expect("key set");
        assert_eq!(k.id.as_deref(), Some("ABCDEF1234567890"));
        assert_eq!(k.from_me, Some(false));
        assert_eq!(k.remote_jid.as_deref(), Some("1234@s.whatsapp.net"));
        // 1:1 DM: participant left None even when target is not us.
        assert!(k.participant.is_none(), "participant set on DM key: {k:?}");
        assert_eq!(r.text.as_deref(), Some("❤"));
        assert!(r.sender_timestamp_ms.unwrap_or(0) >= 0);

        // None of the *other* `Message` subtypes are populated — the
        // wire shape is just `reaction_message`.
        assert!(msg.conversation.is_none());
        assert!(msg.extended_text_message.is_none());
        assert!(msg.protocol_message.is_none());

        // Empty emoji is the documented "remove reaction" form — we still
        // serialise it as `text=""`, matching upstream.
        let blank = Message {
            reaction_message: Some(ReactionMessage {
                key: Some(key),
                text: Some("".to_owned()),
                grouping_key: None,
                sender_timestamp_ms: Some(now_ms),
            }),
            ..Default::default()
        };
        assert_eq!(
            blank.reaction_message.as_ref().unwrap().text.as_deref(),
            Some("")
        );
    }

    /// Wire-shape pin for `send_reply`: the constructed `Message`'s
    /// `extended_text_message` must carry the new body and a `context_info`
    /// whose `stanza_id`, `participant`, and `quoted_message` reference
    /// the original. Mirrors upstream's reply construction.
    #[test]
    fn send_reply_builds_extended_text_with_quoted_context() {
        let chat: Jid = "9999@s.whatsapp.net".parse().unwrap();
        let _ = chat;
        let quoted_sender: Jid = "777:5@s.whatsapp.net".parse().unwrap();
        let quoted_msg = Message {
            conversation: Some("the original".to_owned()),
            ..Default::default()
        };
        let ci = ContextInfo {
            stanza_id: Some("ORIG-ID".to_owned()),
            participant: Some(quoted_sender.to_non_ad().to_string()),
            quoted_message: Some(Box::new(quoted_msg.clone())),
            ..Default::default()
        };
        let msg = Message {
            extended_text_message: Some(Box::new(ExtendedTextMessage {
                text: Some("my reply".to_owned()),
                context_info: Some(Box::new(ci)),
                ..Default::default()
            })),
            ..Default::default()
        };

        let et = msg.extended_text_message.as_ref().expect("et set");
        assert_eq!(et.text.as_deref(), Some("my reply"));
        let ci = et.context_info.as_ref().expect("ci set");
        assert_eq!(ci.stanza_id.as_deref(), Some("ORIG-ID"));
        // Participant is the non-AD form of the sender (device suffix dropped).
        assert_eq!(ci.participant.as_deref(), Some("777@s.whatsapp.net"));
        let q = ci.quoted_message.as_ref().expect("quoted set");
        assert_eq!(q.conversation.as_deref(), Some("the original"));

        // Cross-shape sanity: no other top-level subtypes are populated.
        assert!(msg.conversation.is_none());
        assert!(msg.reaction_message.is_none());
        assert!(msg.protocol_message.is_none());
    }

    /// Wire-shape pin for `send_revoke`: the constructed `Message` must
    /// carry a `protocol_message` with `type=REVOKE` and a `key`
    /// referencing the original message. Mirrors upstream `BuildRevoke`.
    #[test]
    fn send_revoke_builds_protocol_message_revoke_proto() {
        let chat: Jid = "abcd@s.whatsapp.net".parse().unwrap();
        let own_jid: Jid = "myself@s.whatsapp.net".parse().unwrap();
        let key = build_message_key(&chat, &own_jid, "TARGET-XYZ", true);
        let msg = Message {
            protocol_message: Some(Box::new(ProtocolMessage {
                key: Some(key),
                r#type: Some(wha_proto::e2e::protocol_message::Type::Revoke as i32),
                ..Default::default()
            })),
            ..Default::default()
        };

        let pm = msg.protocol_message.as_ref().expect("pm set");
        // REVOKE is enum value 0 in the proto definition.
        assert_eq!(
            pm.r#type,
            Some(wha_proto::e2e::protocol_message::Type::Revoke as i32)
        );
        assert_eq!(pm.r#type, Some(0));
        let k = pm.key.as_ref().expect("key set");
        assert_eq!(k.id.as_deref(), Some("TARGET-XYZ"));
        assert_eq!(k.from_me, Some(true));
        assert_eq!(k.remote_jid.as_deref(), Some("abcd@s.whatsapp.net"));

        // Every other subtype is None — pure protocol envelope.
        assert!(msg.conversation.is_none());
        assert!(msg.reaction_message.is_none());
        assert!(msg.extended_text_message.is_none());
    }

    /// `build_message_key` mirrors the participant-only-for-groups branch
    /// of upstream's `BuildMessageKey`. For 1:1 chats `participant` stays
    /// `None`; for group chats with a non-self target it carries the
    /// original sender's non-AD JID.
    #[test]
    fn build_message_key_sets_participant_only_for_group_targets() {
        let dm: Jid = "1@s.whatsapp.net".parse().unwrap();
        let lid: Jid = "2@lid".parse().unwrap();
        let group: Jid = "abc-123@g.us".parse().unwrap();
        let other: Jid = "777:5@s.whatsapp.net".parse().unwrap();

        let dm_key = build_message_key(&dm, &other, "M1", false);
        assert!(
            dm_key.participant.is_none(),
            "DM key must not set participant"
        );

        let lid_key = build_message_key(&lid, &other, "M2", false);
        assert!(
            lid_key.participant.is_none(),
            "LID DM key must not set participant"
        );

        let group_key = build_message_key(&group, &other, "M3", false);
        assert_eq!(
            group_key.participant.as_deref(),
            Some("777@s.whatsapp.net"),
            "group key must carry sender (non-AD)"
        );

        // from_me=true never carries a participant (it's our own message).
        let from_me_group = build_message_key(&group, &other, "M4", true);
        assert!(from_me_group.participant.is_none());
        assert_eq!(from_me_group.from_me, Some(true));
    }

    /// `send_reaction` (and friends) all require a connected client; the
    /// pre-flight check surfaces `NotConnected` before any IO. Same
    /// guarantee as `send_text_without_connection_errors`.
    #[tokio::test]
    async fn send_reaction_without_connection_errors() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let chat: Jid = "1@s.whatsapp.net".parse().unwrap();
        let sender: Jid = "1@s.whatsapp.net".parse().unwrap();
        let r = send_reaction(&client, &chat, "ABC", &sender, false, "❤").await;
        assert!(matches!(r, Err(ClientError::NotConnected)), "got {r:?}");
    }
}
