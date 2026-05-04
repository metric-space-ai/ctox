//! Armadillo: decode/encode of Messenger-side `MessageTransport` envelopes.
//!
//! The upstream file `_upstream/whatsmeow/armadillomessage.go` (133 LOC)
//! is **not** about WhatsApp Newsletter / Channels encryption. It decodes
//! Facebook / Messenger end-to-end-encrypted payloads that arrive as a
//! `waMsgTransport.MessageTransport` proto wrapping a
//! `waMsgApplication.MessageApplication` payload (the FB / Messenger wire
//! used for `@msgr` JIDs and Meta interop). This module is the Rust port:
//!
//!   * [`decrypt_msg_transport`] — strip libsignal-style block padding off
//!     a decrypted plaintext, parse the outer `MessageTransport`, then
//!     unwrap the inner `MessageApplication` (mirrors the path through
//!     `decodeArmadillo` + `MessageTransport_Payload.DecodeFB` upstream).
//!   * [`encode_msg_transport`] — the inverse. Wraps a typed
//!     `MessageApplication` in a `MessageTransport` with the supplied
//!     ancillary metadata (skdm / dsm / device-list / icdc / backup
//!     directive) and appends `padding` libsignal-style trailing bytes.
//!     Mirrors the marshal step inside `prepareFBMessage` /
//!     `sendDMV3` / `sendGroupV3`.
//!   * [`decode_armadillo_message`] — given a fully-decoded
//!     `MessageApplication`, dispatch on its `payload.subProtocol`
//!     oneof and parse the inner sub-protocol payload (`Consumer`,
//!     `Business`, `Payment`, `MultiDevice`, `Voip`, `Armadillo`).
//!     Mirrors `decodeFBArmadillo` upstream. Only `Consumer` is fully
//!     decoded; the rest return [`ArmadilloError::UnsupportedSubProtocol`]
//!     until their proto bindings land.
//!
//! The legacy Newsletter-key store + IQ helper continues to live here
//! ([`fetch_newsletter_keys`], [`build_fetch_newsletter_keys_iq`],
//! [`parse_newsletter_keys`]) — those are an open-ended placeholder for
//! a future WhatsApp Channels E2EE rollout. Their wire format is not
//! finalised upstream so the trait + IQ shape are best-effort.

use prost::Message as ProstMessage;
use thiserror::Error;

use wha_binary::{Attrs, Node, Value};
use wha_crypto::CryptoError;
use wha_proto::common::{FutureProofBehavior, SubProtocol};
use wha_proto::consumer_application::ConsumerApplication;
use wha_proto::messenger::message_application::payload::Content as AppPayloadContent;
use wha_proto::messenger::message_application::sub_protocol_payload::SubProtocol as AppSubProtocol;
use wha_proto::messenger::transport::message_transport::{
    protocol::{Ancillary as TransportAncillary, Integral as TransportIntegral},
    Payload as TransportPayload, Protocol as TransportProtocol,
};
use wha_proto::messenger::transport::MessageTransport;
use wha_proto::messenger::MessageApplication;
use wha_types::{jid::server, Jid};

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

/// Upstream's `FBMessageApplicationVersion` constant — the `version` field
/// stamped on the inner `SubProtocol` wrapping the `MessageApplication`
/// payload. Mirrors `_upstream/whatsmeow/proto/waMsgTransport/extra.go`.
pub const FB_MESSAGE_APPLICATION_VERSION: i32 = 2;

/// Errors returned by the armadillo encode / decode pipeline.
#[derive(Debug, Error)]
pub enum ArmadilloError {
    #[error("payload too short for armadillo (got {0} bytes)")]
    TooShort(usize),
    #[error("invalid padding (declared {pad}, available {avail})")]
    BadPadding { pad: usize, avail: usize },
    #[error("transport payload field is missing")]
    MissingPayload,
    #[error("application_payload field on MessageTransport.payload is missing")]
    MissingApplicationPayload,
    #[error(
        "unsupported MessageApplication SubProtocol version {got} (expected {expected})"
    )]
    UnsupportedAppVersion { got: i32, expected: i32 },
    #[error("MessageApplication payload content arm is unset")]
    MissingApplicationContent,
    #[error("unsupported sub-protocol: {0}")]
    UnsupportedSubProtocol(&'static str),
    #[error("proto decode: {0}")]
    Proto(String),
    #[error("crypto: {0}")]
    Crypto(String),
}

impl From<prost::DecodeError> for ArmadilloError {
    fn from(e: prost::DecodeError) -> Self {
        ArmadilloError::Proto(e.to_string())
    }
}

impl From<CryptoError> for ArmadilloError {
    fn from(e: CryptoError) -> Self {
        ArmadilloError::Crypto(e.to_string())
    }
}

impl From<ArmadilloError> for ClientError {
    fn from(e: ArmadilloError) -> Self {
        ClientError::Other(format!("armadillo: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Padding helpers (libsignal-style trailing block padding).
// ---------------------------------------------------------------------------

/// Strip `padding` trailing bytes off `data` after verifying every byte
/// equals `padding`. This is the "v2 unpad" rule used upstream — for v3
/// (`unpadMessage`) the padding strip is a no-op, so callers that never
/// applied padding pass `0` and the function short-circuits.
fn strip_padding(data: &[u8], padding: u8) -> Result<&[u8], ArmadilloError> {
    if padding == 0 {
        return Ok(data);
    }
    let pad = padding as usize;
    if data.len() < pad {
        return Err(ArmadilloError::BadPadding {
            pad,
            avail: data.len(),
        });
    }
    let split = data.len() - pad;
    if !data[split..].iter().all(|b| *b == padding) {
        return Err(ArmadilloError::BadPadding {
            pad,
            avail: data.len(),
        });
    }
    Ok(&data[..split])
}

/// Append `padding` repetitions of `padding` to `out`. `padding == 0` is a
/// no-op (matches upstream's `padMessage(nil)` short-circuit when the
/// caller doesn't want trailing pad bytes).
fn append_padding(out: &mut Vec<u8>, padding: u8) {
    if padding == 0 {
        return;
    }
    out.extend(std::iter::repeat(padding).take(padding as usize));
}

// ---------------------------------------------------------------------------
// SignalMessageMetadata: ancillary fields stamped onto MessageTransport.
// ---------------------------------------------------------------------------

/// Ancillary metadata stamped onto an outgoing `MessageTransport`. These
/// are the four non-padding `Ancillary` fields upstream — the
/// signal-layer SKDM, the device-list metadata, ICDC participant devices,
/// and the backup directive. All optional; defaults to all-`None`.
///
/// Mirrors `waMsgTransport.MessageTransport_Protocol_Ancillary` upstream.
#[derive(Debug, Clone, Default)]
pub struct SignalMessageMetadata {
    pub skdm: Option<wha_proto::messenger::transport::message_transport::protocol::ancillary::SenderKeyDistributionMessage>,
    pub device_list_metadata: Option<wha_proto::messenger::transport::DeviceListMetadata>,
    pub icdc: Option<wha_proto::messenger::transport::message_transport::protocol::ancillary::IcdcParticipantDevices>,
    pub backup_directive: Option<wha_proto::messenger::transport::message_transport::protocol::ancillary::BackupDirective>,
}

impl From<SignalMessageMetadata> for TransportAncillary {
    fn from(meta: SignalMessageMetadata) -> Self {
        TransportAncillary {
            skdm: meta.skdm,
            device_list_metadata: meta.device_list_metadata,
            icdc: meta.icdc,
            backup_directive: meta.backup_directive,
        }
    }
}

// ---------------------------------------------------------------------------
// Decrypt / encode the MessageTransport <-> MessageApplication boundary.
// ---------------------------------------------------------------------------

/// Strip libsignal-style padding off `payload`, parse the resulting bytes
/// as a `MessageTransport` proto, and unwrap the inner `MessageApplication`.
///
/// `padding` is the trailing-byte run length applied by the sender (matches
/// the value of every byte in the padded run, libsignal convention). Pass
/// `0` for v3 wires that never apply transport-level padding.
///
/// Mirrors the path through `decodeArmadillo` +
/// `MessageTransport_Payload.DecodeFB` upstream.
pub fn decrypt_msg_transport(
    payload: &[u8],
    padding: u8,
) -> Result<MessageApplication, ArmadilloError> {
    let unpadded = strip_padding(payload, padding)?;

    let transport = MessageTransport::decode(unpadded)?;
    let payload = transport.payload.ok_or(ArmadilloError::MissingPayload)?;
    let app_proto = payload
        .application_payload
        .ok_or(ArmadilloError::MissingApplicationPayload)?;

    let version = app_proto.version.unwrap_or(0);
    if version != FB_MESSAGE_APPLICATION_VERSION {
        return Err(ArmadilloError::UnsupportedAppVersion {
            got: version,
            expected: FB_MESSAGE_APPLICATION_VERSION,
        });
    }
    let inner = app_proto.payload.unwrap_or_default();
    let app = MessageApplication::decode(inner.as_slice())?;
    Ok(app)
}

/// Wrap a typed `MessageApplication` in a `MessageTransport` envelope and
/// append `padding` trailing pad bytes. Inverse of [`decrypt_msg_transport`].
///
/// `signal_metadata` populates the four ancillary fields. The Integral
/// section is left empty save for the proto's own optional `padding` /
/// `dsm` slots (callers that need DSM stamping continue to use
/// [`crate::send_fb::wrap_in_transport`]).
pub fn encode_msg_transport(
    app: &MessageApplication,
    signal_metadata: &SignalMessageMetadata,
    padding: u8,
) -> Result<Vec<u8>, ArmadilloError> {
    let mut app_bytes = Vec::with_capacity(64);
    // prost::Message::encode into Vec<u8> is infallible (the writer never
    // fails); the fallible `EncodeError` arm is reachable only with custom
    // BufMut impls. Same expect()-shape as `encode_armadillo_message`
    // below and the rest of the codebase.
    app.encode(&mut app_bytes)
        .expect("prost encode into Vec is infallible");

    let transport = MessageTransport {
        payload: Some(TransportPayload {
            application_payload: Some(SubProtocol {
                payload: Some(app_bytes),
                version: Some(FB_MESSAGE_APPLICATION_VERSION),
            }),
            future_proof: Some(FutureProofBehavior::Placeholder as i32),
        }),
        protocol: Some(TransportProtocol {
            integral: Some(TransportIntegral {
                padding: None,
                dsm: None,
            }),
            ancillary: Some(signal_metadata.clone().into()),
        }),
    };

    let mut out = Vec::with_capacity(prost::Message::encoded_len(&transport) + padding as usize);
    transport
        .encode(&mut out)
        .expect("prost encode into Vec is infallible");
    append_padding(&mut out, padding);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Sub-protocol dispatch on a decoded MessageApplication.
// ---------------------------------------------------------------------------

/// A parsed sub-protocol payload pulled out of a `MessageApplication`.
/// The arms mirror `MessageApplication.Payload.SubProtocolPayload` upstream
/// — the only fully-typed arm today is `Consumer`; the rest are stubbed
/// until their proto bindings are wired into `wha-proto`.
#[derive(Debug, Clone)]
pub enum ArmadilloMessage {
    /// `consumerMessage` arm — a [`ConsumerApplication`] proto.
    Consumer(ConsumerApplication),
    /// `applicationData` arm on the outer payload (no sub-protocol; a
    /// MessageApplication-level metadata payload).
    ApplicationData,
    /// `signal` arm on the outer payload (signal-layer-only payload).
    Signal,
    /// `coreContent` arm on the outer payload.
    CoreContent,
}

/// Inspect `app.payload.content` and parse the inner sub-protocol bytes.
/// Mirrors `decodeFBArmadillo` upstream. Returns
/// [`ArmadilloError::UnsupportedSubProtocol`] for sub-protocols whose
/// inner proto bindings aren't yet wired into `wha-proto`.
pub fn decode_armadillo_message(
    app: &MessageApplication,
) -> Result<ArmadilloMessage, ArmadilloError> {
    let payload = app
        .payload
        .as_ref()
        .ok_or(ArmadilloError::MissingApplicationContent)?;
    let content = payload
        .content
        .as_ref()
        .ok_or(ArmadilloError::MissingApplicationContent)?;

    match content {
        AppPayloadContent::CoreContent(_) => Ok(ArmadilloMessage::CoreContent),
        AppPayloadContent::Signal(_) => Ok(ArmadilloMessage::Signal),
        AppPayloadContent::ApplicationData(_) => Ok(ArmadilloMessage::ApplicationData),
        AppPayloadContent::SubProtocol(sub) => {
            let inner = sub
                .sub_protocol
                .as_ref()
                .ok_or_else(|| ArmadilloError::UnsupportedSubProtocol("subProtocol unset"))?;
            match inner {
                AppSubProtocol::ConsumerMessage(sp) => {
                    let bytes = sp.payload.as_deref().unwrap_or(&[]);
                    let consumer = ConsumerApplication::decode(bytes)?;
                    Ok(ArmadilloMessage::Consumer(consumer))
                }
                AppSubProtocol::BusinessMessage(_) => {
                    Err(ArmadilloError::UnsupportedSubProtocol("businessMessage"))
                }
                AppSubProtocol::PaymentMessage(_) => {
                    Err(ArmadilloError::UnsupportedSubProtocol("paymentMessage"))
                }
                AppSubProtocol::MultiDevice(_) => {
                    Err(ArmadilloError::UnsupportedSubProtocol("multiDevice"))
                }
                AppSubProtocol::Voip(_) => {
                    Err(ArmadilloError::UnsupportedSubProtocol("voip"))
                }
                AppSubProtocol::Armadillo(_) => {
                    Err(ArmadilloError::UnsupportedSubProtocol("armadillo"))
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Newsletter-key IQ + store wiring (placeholder for future Channels E2EE).
// ---------------------------------------------------------------------------

/// Bag of keys returned by [`fetch_newsletter_keys`]. Only the
/// `channel_key` is required by the (future) per-channel E2EE pipeline.
#[derive(Debug, Clone)]
pub struct NewsletterKeys {
    /// The 32-byte symmetric channel key.
    pub channel_key: [u8; 32],
}

/// Build the `<iq xmlns="newsletter" type="get">` body asking the server
/// for the channel's E2EE keys. Mirrors the existing
/// `<iq xmlns="newsletter">` shape in `crate::newsletter::*`.
pub(crate) fn build_fetch_newsletter_keys_iq(channel: &Jid) -> InfoQuery {
    let mut nl_attrs = Attrs::new();
    nl_attrs.insert("jid".into(), Value::Jid(channel.clone()));
    let nl = Node::new(
        "newsletter",
        nl_attrs,
        Some(Value::Nodes(vec![Node::tag_only("keys")])),
    );

    InfoQuery::new("newsletter", IqType::Get)
        .to(Jid::new("", server::DEFAULT_USER))
        .content(Value::Nodes(vec![nl]))
}

/// Walk the IQ response for the `<key/>` child carrying the 32-byte
/// channel key. Returns `Err` if no recognisable child is present.
pub(crate) fn parse_newsletter_keys(resp: &Node) -> Result<NewsletterKeys, ClientError> {
    // Look for `<keys><key>BYTES</key></keys>` or `<key>BYTES</key>` directly.
    let key_node = resp
        .child_by_tag(&["newsletter", "keys", "key"])
        .or_else(|| resp.child_by_tag(&["keys", "key"]))
        .or_else(|| resp.child_by_tag(&["key"]))
        .ok_or_else(|| {
            ClientError::Malformed("no <key> child in newsletter-keys response".into())
        })?;

    let bytes: Vec<u8> = match &key_node.content {
        Value::Bytes(b) => b.clone(),
        Value::String(s) => hex::decode(s.trim()).unwrap_or_default(),
        _ => Vec::new(),
    };
    if bytes.len() != 32 {
        let attr = key_node.get_attr_str("value").unwrap_or("");
        if attr.len() == 64 {
            if let Ok(decoded) = hex::decode(attr) {
                if decoded.len() == 32 {
                    let mut k = [0u8; 32];
                    k.copy_from_slice(&decoded);
                    return Ok(NewsletterKeys { channel_key: k });
                }
            }
        }
        return Err(ClientError::Malformed(format!(
            "newsletter <key> bytes wrong length {}",
            bytes.len()
        )));
    }
    let mut k = [0u8; 32];
    k.copy_from_slice(&bytes);
    Ok(NewsletterKeys { channel_key: k })
}

/// Ask the server for the channel's E2EE keys and persist them into
/// `client.device.newsletter_keys`.
///
/// **Note on upstream parity.** This IQ is best-effort against the same
/// `xmlns="newsletter"` namespace upstream uses for other newsletter
/// queries; the exact response shape is not finalised. The parser is
/// tolerant — bytes child, hex string content, or hex `value` attribute
/// all decode if 32 bytes long.
pub async fn fetch_newsletter_keys(
    client: &Client,
    channel: &Jid,
) -> Result<NewsletterKeys, ClientError> {
    let resp = client.send_iq(build_fetch_newsletter_keys_iq(channel)).await?;
    let keys = parse_newsletter_keys(&resp)?;
    client
        .device
        .newsletter_keys
        .put_newsletter_key(channel, keys.channel_key)
        .await?;
    Ok(keys)
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use wha_proto::common::MessageText;
    use wha_proto::consumer_application::consumer_application::content::Content as ConsumerContent;
    use wha_proto::consumer_application::consumer_application::payload::Payload as ConsumerPayload;
    use wha_proto::consumer_application::consumer_application::{Content, Payload as CAPayload};
    use wha_proto::messenger::message_application::payload::Content as ApAppContent;
    use wha_proto::messenger::message_application::sub_protocol_payload::SubProtocol as ApSP;
    use wha_proto::messenger::message_application::{Payload as AppPayload, SubProtocolPayload};

    fn channel_jid() -> Jid {
        "111111111@newsletter".parse().unwrap()
    }

    fn sample_consumer_message_application() -> MessageApplication {
        let consumer = ConsumerApplication {
            payload: Some(CAPayload {
                payload: Some(ConsumerPayload::Content(Content {
                    content: Some(ConsumerContent::MessageText(MessageText {
                        text: Some("hello via armadillo".into()),
                        ..Default::default()
                    })),
                })),
            }),
            metadata: None,
        };
        let mut consumer_bytes = Vec::with_capacity(64);
        consumer.encode(&mut consumer_bytes).unwrap();

        MessageApplication {
            payload: Some(AppPayload {
                content: Some(ApAppContent::SubProtocol(SubProtocolPayload {
                    future_proof: None,
                    sub_protocol: Some(ApSP::ConsumerMessage(SubProtocol {
                        payload: Some(consumer_bytes),
                        version: Some(1),
                    })),
                })),
            }),
            metadata: None,
        }
    }

    /// Encode a MessageApplication through `encode_msg_transport`, then
    /// decrypt it back with `decrypt_msg_transport` and assert the bytes
    /// round-trip. Uses non-zero padding to exercise the strip path.
    #[test]
    fn transport_round_trip_with_padding() {
        let original = sample_consumer_message_application();
        let meta = SignalMessageMetadata::default();
        let bytes = encode_msg_transport(&original, &meta, 11).expect("encode");

        // Sanity: the last 11 bytes must all equal 11.
        assert_eq!(*bytes.last().unwrap(), 11);
        assert!(bytes.iter().rev().take(11).all(|b| *b == 11));

        let back = decrypt_msg_transport(&bytes, 11).expect("decrypt");

        let mut a = Vec::new();
        let mut b = Vec::new();
        original.encode(&mut a).unwrap();
        back.encode(&mut b).unwrap();
        assert_eq!(a, b, "MessageApplication round-trip must be byte-identical");
    }

    /// Tampering an interior byte of the trailing pad run flips at least
    /// one of `last_byte == padding` or "all bytes equal padding" — both
    /// must be detected.
    #[test]
    fn decrypt_rejects_invalid_padding() {
        let original = sample_consumer_message_application();
        let mut bytes =
            encode_msg_transport(&original, &SignalMessageMetadata::default(), 7).unwrap();

        // Flip an interior padding byte so the trailing run is no longer
        // uniform — `strip_padding` must reject.
        let n = bytes.len();
        bytes[n - 4] = 0;
        let r = decrypt_msg_transport(&bytes, 7);
        assert!(matches!(r, Err(ArmadilloError::BadPadding { .. })));

        // Declared padding longer than the buffer → BadPadding.
        let r2 = decrypt_msg_transport(&[0u8; 3], 5);
        assert!(matches!(r2, Err(ArmadilloError::BadPadding { .. })));
    }

    /// Round-trip a `Consumer` sub-protocol through the full pipeline:
    /// build a `MessageApplication`, encode → transport bytes → decrypt
    /// → application back, then dispatch through `decode_armadillo_message`
    /// and assert we land in the `Consumer(_)` arm with the original text.
    #[test]
    fn decode_consumer_application_subprotocol() {
        let app = sample_consumer_message_application();
        let bytes =
            encode_msg_transport(&app, &SignalMessageMetadata::default(), 0).unwrap();
        let app_back = decrypt_msg_transport(&bytes, 0).expect("decrypt");
        let dec = decode_armadillo_message(&app_back).expect("decode");
        match dec {
            ArmadilloMessage::Consumer(consumer) => {
                let text = consumer
                    .payload
                    .and_then(|p| p.payload)
                    .and_then(|inner| match inner {
                        ConsumerPayload::Content(c) => c.content,
                        _ => None,
                    })
                    .and_then(|c| match c {
                        ConsumerContent::MessageText(t) => t.text,
                        _ => None,
                    })
                    .expect("messageText.text present");
                assert_eq!(text, "hello via armadillo");
            }
            other => panic!("expected Consumer arm, got {other:?}"),
        }
    }

    /// A SubProtocolPayload whose `sub_protocol` arm is `BusinessMessage`
    /// must surface `UnsupportedSubProtocol("businessMessage")`. Same shape
    /// as the upstream "Unsupported_BusinessApplication" arm.
    #[test]
    fn decode_unknown_subprotocol_errors() {
        let app = MessageApplication {
            payload: Some(AppPayload {
                content: Some(ApAppContent::SubProtocol(SubProtocolPayload {
                    future_proof: None,
                    sub_protocol: Some(ApSP::BusinessMessage(SubProtocol {
                        payload: Some(b"opaque".to_vec()),
                        version: Some(1),
                    })),
                })),
            }),
            metadata: None,
        };
        let r = decode_armadillo_message(&app);
        assert!(matches!(
            r,
            Err(ArmadilloError::UnsupportedSubProtocol("businessMessage"))
        ));

        // And a SubProtocolPayload with no arm set surfaces a different
        // UnsupportedSubProtocol("subProtocol unset").
        let app2 = MessageApplication {
            payload: Some(AppPayload {
                content: Some(ApAppContent::SubProtocol(SubProtocolPayload {
                    future_proof: None,
                    sub_protocol: None,
                })),
            }),
            metadata: None,
        };
        let r2 = decode_armadillo_message(&app2);
        assert!(matches!(
            r2,
            Err(ArmadilloError::UnsupportedSubProtocol("subProtocol unset"))
        ));
    }

    /// `build_fetch_newsletter_keys_iq` produces the right namespace and
    /// channel-jid attr.
    #[test]
    fn build_fetch_newsletter_keys_iq_shape() {
        let q = build_fetch_newsletter_keys_iq(&channel_jid());
        let n = q.into_node("REQ-NL".into());
        assert_eq!(n.tag, "iq");
        assert_eq!(n.get_attr_str("xmlns"), Some("newsletter"));
        assert_eq!(n.get_attr_str("type"), Some("get"));
        let nl = n
            .children()
            .iter()
            .find(|c| c.tag == "newsletter")
            .expect("<newsletter> child");
        assert_eq!(nl.get_attr_jid("jid").unwrap().server, "newsletter");
        let keys = nl
            .children()
            .iter()
            .find(|c| c.tag == "keys")
            .expect("<keys> child");
        assert_eq!(keys.children().len(), 0);
    }

    /// `parse_newsletter_keys` accepts a 32-byte `<key>` payload.
    #[test]
    fn parse_newsletter_keys_extracts_32_bytes() {
        let key_bytes = vec![0x37u8; 32];
        let key_node = Node::new("key", Attrs::new(), Some(Value::Bytes(key_bytes.clone())));
        let keys = Node::new("keys", Attrs::new(), Some(Value::Nodes(vec![key_node])));
        let nl = Node::new(
            "newsletter",
            Attrs::new(),
            Some(Value::Nodes(vec![keys])),
        );
        let resp = Node::new("iq", Attrs::new(), Some(Value::Nodes(vec![nl])));

        let parsed = parse_newsletter_keys(&resp).expect("parse");
        assert_eq!(parsed.channel_key.as_slice(), key_bytes.as_slice());
    }
}
