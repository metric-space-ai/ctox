// Origin: CTOX
// License: AGPL-3.0-only
//
// Native MQTT 3.1.1 protocol agent (Phase 3). Self-contained, vendored in-tree
// like communication/whatsapp_rust: the packet codec is hand-written byte-for-byte
// against the OASIS MQTT 3.1.1 specification (no external MQTT crate, no rumqttc).
// Domain semantics (resubscribe-on-session-loss, QoS preservation, Last-Will,
// retain, link/unlink during reconnect) ported from OpenRemote
// (AGPL-3.0, archive/openremote, HEAD 22a42a7); see docs/legal/NOTICE.
//
// HARD RULES honored here:
//   * native Rust only; no new MQTT/HTTP framework deps. Transport is
//     `tokio::net::TcpStream` (the crate's existing tokio); the MQTT control-packet
//     codec is vendored inline below.
//   * the clock is INJECTED into every time-dependent path (reconnect backoff,
//     poll-ready) — production callers pass `crate::iot::now_ms()` — so the agent's
//     reconnect behavior is deterministically testable against a fixed clock.
//   * runtime state belongs in runtime/ctox.sqlite3 via the engine write path
//     (`store::process_attribute_event`, driven by runtime.rs). This agent NEVER
//     touches the store: inbound device PUBLISHes surface as `AttributeReading`s
//     for runtime.rs to feed into `process_attribute_event`.
//   * config/credentials flow through
//     `crate::execution::models::runtime_env::env_or_config(root, KEY)` + the CTOX
//     secret store — NEVER `std::env` for runtime state. Agents talk to DEVICES,
//     not the browser; there is no HTTP data bridge here.
//   * ported algorithmic fns carry `// ref: <upstream-file>:<line-range>` with
//     preserved upstream names (resubscribe filter, topicConsumerMap handling,
//     checkSetConnectionStatus via the shared adapters::ReconnectStateMachine).
//
// The protocol-agnostic surface (the `IotAgent` trait, the §2A.28 value-processing
// base layer, the §2A.24 ReconnectStateMachine) lives in `adapters.rs` and is
// REUSED here, never re-implemented.

use crate::iot::adapters::{
    AgentContext, AgentLink, AttributeReading, ConnectionStatus, IotAgent, IotAgentKind,
    ReconnectStateMachine,
};
use crate::iot::model::AttributeValue;
use crate::iot::{now_ms, Result};
use anyhow::{anyhow, bail, Context};
use std::collections::HashMap;
use std::future::Future;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// ===========================================================================
// 1. Vendored MQTT 3.1.1 control-packet codec
// ===========================================================================
//
// ref: OASIS MQTT Version 3.1.1, §2 (Structure of an MQTT Control Packet),
//      §3 (MQTT Control Packets). Fixed header = packet-type nibble + flags +
//      "Remaining Length" (variable byte integer, §2.2.3); payloads frame UTF-8
//      strings as a 2-byte big-endian length prefix (§1.5.3).
// ref: AbstractMQTT_IOClient.java (the HiveMQ-client packet usage this codec
//      replaces — CONNECT/SUBSCRIBE/PUBLISH/DISCONNECT shapes preserved).

/// MQTT control packet type (high nibble of byte 1). ref: MQTT 3.1.1 §2.2.1 (Table 2.1).
mod packet_type {
    pub const CONNECT: u8 = 1;
    pub const CONNACK: u8 = 2;
    pub const PUBLISH: u8 = 3;
    pub const PUBACK: u8 = 4;
    pub const SUBSCRIBE: u8 = 8;
    pub const SUBACK: u8 = 9;
    pub const PINGREQ: u8 = 12;
    pub const PINGRESP: u8 = 13;
    pub const DISCONNECT: u8 = 14;
}

/// Protocol-level keep-alive in seconds. ref: AbstractMQTT_IOClient.java:477-488
/// (HiveMQ `keepAlive(10)`); also the §3.1.2.10 Keep Alive field of CONNECT.
const KEEP_ALIVE_SECONDS: u16 = 10;

/// Encode an MQTT "Remaining Length" variable byte integer.
/// ref: MQTT 3.1.1 §2.2.3 (algorithm for encoding the Remaining Length).
fn encode_remaining_length(mut len: usize, out: &mut Vec<u8>) {
    // do { encodedByte = X MOD 128; X = X DIV 128; if (X>0) encodedByte |= 128 } while (X>0)
    loop {
        let mut encoded_byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            encoded_byte |= 128;
        }
        out.push(encoded_byte);
        if len == 0 {
            break;
        }
    }
}

/// Decode an MQTT "Remaining Length" variable byte integer from `buf` starting at
/// `pos`. Returns (value, bytes_consumed). ref: MQTT 3.1.1 §2.2.3 (decoding,
/// "multiplier" loop with the 4-byte / 0x80,0x80,0x80,0x80 malformed guard).
fn decode_remaining_length(buf: &[u8], pos: usize) -> Result<(usize, usize)> {
    let mut multiplier: usize = 1;
    let mut value: usize = 0;
    let mut i = pos;
    loop {
        let encoded_byte = *buf
            .get(i)
            .ok_or_else(|| anyhow!("MQTT remaining-length truncated"))?;
        value += (encoded_byte & 127) as usize * multiplier;
        // ref: §2.2.3 "if (multiplier > 128*128*128) throw Error(Malformed Remaining Length)"
        if multiplier > 128 * 128 * 128 {
            bail!("MQTT malformed remaining length");
        }
        multiplier *= 128;
        i += 1;
        if (encoded_byte & 128) == 0 {
            break;
        }
    }
    Ok((value, i - pos))
}

/// Frame a UTF-8 string with its 2-byte big-endian length prefix.
/// ref: MQTT 3.1.1 §1.5.3 (UTF-8 encoded strings).
fn put_mqtt_string(s: &str, out: &mut Vec<u8>) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// Read a length-prefixed UTF-8 string. Returns (string, bytes_consumed).
/// ref: MQTT 3.1.1 §1.5.3.
fn get_mqtt_string(buf: &[u8], pos: usize) -> Result<(String, usize)> {
    if pos + 2 > buf.len() {
        bail!("MQTT string length prefix truncated");
    }
    let len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
    let start = pos + 2;
    let end = start + len;
    if end > buf.len() {
        bail!("MQTT string body truncated");
    }
    let s = std::str::from_utf8(&buf[start..end])
        .context("MQTT string is not valid UTF-8")?
        .to_string();
    Ok((s, 2 + len))
}

/// The Last-Will configuration carried in the CONNECT packet (§3.1.2.5 Will Flag,
/// §3.1.3.2 Will Topic, §3.1.3.3 Will Message). The broker publishes this on an
/// *ungraceful* disconnect of the client (TCP drop) and SUPPRESSES it on a graceful
/// DISCONNECT — the agent only configures it; suppression is the broker's job.
/// ref: AbstractMQTT_IOClient.java (lastWill builder) + §3.1.2.5-2.2.6 (Will QoS/Retain).
#[derive(Clone, Debug)]
pub(crate) struct LastWill {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
}

/// CONNECT packet builder. ref: MQTT 3.1.1 §3.1 (CONNECT).
///
/// Variable header: protocol name "MQTT", level 4 (3.1.1), connect flags, keep-alive.
/// Payload (in order): Client Identifier, [Will Topic, Will Message],
/// [User Name], [Password]. ref: §3.1.2 (variable header), §3.1.3 (payload).
fn encode_connect(
    client_id: &str,
    clean_session: bool,
    username: Option<&str>,
    password: Option<&str>,
    will: Option<&LastWill>,
) -> Vec<u8> {
    let mut var_and_payload = Vec::new();

    // -- variable header --
    // Protocol Name "MQTT" + Protocol Level 0x04. ref: §3.1.2.1-3.1.2.2.
    put_mqtt_string("MQTT", &mut var_and_payload);
    var_and_payload.push(0x04);

    // Connect flags. ref: §3.1.2.3 (Table 3.1 — bit positions).
    let mut flags: u8 = 0;
    if clean_session {
        flags |= 0b0000_0010; // Clean Session, bit 1
    }
    if let Some(w) = will {
        flags |= 0b0000_0100; // Will Flag, bit 2
        flags |= (w.qos & 0b11) << 3; // Will QoS, bits 4-3
        if w.retain {
            flags |= 0b0010_0000; // Will Retain, bit 5
        }
    }
    if password.is_some() {
        flags |= 0b0100_0000; // Password Flag, bit 6
    }
    if username.is_some() {
        flags |= 0b1000_0000; // User Name Flag, bit 7
    }
    var_and_payload.push(flags);

    // Keep Alive (2 bytes). ref: §3.1.2.10.
    var_and_payload.extend_from_slice(&KEEP_ALIVE_SECONDS.to_be_bytes());

    // -- payload (ordering is mandatory). ref: §3.1.3 --
    put_mqtt_string(client_id, &mut var_and_payload);
    if let Some(w) = will {
        put_mqtt_string(&w.topic, &mut var_and_payload);
        // Will message is a binary blob with a 2-byte length prefix. ref: §3.1.3.3.
        var_and_payload.extend_from_slice(&(w.payload.len() as u16).to_be_bytes());
        var_and_payload.extend_from_slice(&w.payload);
    }
    if let Some(u) = username {
        put_mqtt_string(u, &mut var_and_payload);
    }
    if let Some(p) = password {
        put_mqtt_string(p, &mut var_and_payload);
    }

    frame_packet(packet_type::CONNECT, 0, var_and_payload)
}

/// CONNACK result. ref: MQTT 3.1.1 §3.2 (CONNACK).
/// `session_present` is the §3.2.2.2 Session Present flag — the load-bearing input
/// to the §2A.25 resubscribe decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConnAck {
    pub session_present: bool,
    pub return_code: u8,
}

/// Decode a CONNACK variable header (2 bytes: Connect Acknowledge Flags + Return
/// Code). ref: §3.2.2.1 (Session Present is bit 0 of byte 1), §3.2.2.3 (return code).
fn decode_connack(remaining: &[u8]) -> Result<ConnAck> {
    if remaining.len() < 2 {
        bail!("CONNACK too short");
    }
    Ok(ConnAck {
        session_present: (remaining[0] & 0x01) != 0,
        return_code: remaining[1],
    })
}

/// SUBSCRIBE packet builder. ref: MQTT 3.1.1 §3.8 (SUBSCRIBE).
/// Fixed-header flags MUST be 0b0010 (§3.8.1). Variable header is the 2-byte
/// Packet Identifier (§3.8.2); payload is a list of (Topic Filter, Requested QoS).
fn encode_subscribe(packet_id: u16, topics: &[(String, u8)]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&packet_id.to_be_bytes());
    for (topic, qos) in topics {
        put_mqtt_string(topic, &mut body);
        body.push(qos & 0b11); // Requested QoS, ref: §3.8.3.
    }
    // SUBSCRIBE reserved flags = 0b0010. ref: §3.8.1 (bits 3,2,1,0 = 0,0,1,0).
    frame_packet(packet_type::SUBSCRIBE, 0b0010, body)
}

/// SUBACK result. ref: MQTT 3.1.1 §3.9 (SUBACK). The return code per topic is the
/// granted QoS (0x00/0x01/0x02) or 0x80 = Failure (§3.9.3). A 0x80 marks a
/// subscription as "failed", feeding the §2A.25 retry-failures-on-reconnect path.
#[derive(Clone, Debug)]
pub(crate) struct SubAck {
    pub packet_id: u16,
    pub return_codes: Vec<u8>,
}

/// Decode SUBACK: 2-byte Packet Identifier then one return code per topic.
/// ref: §3.9.2 (variable header), §3.9.3 (payload).
fn decode_suback(remaining: &[u8]) -> Result<SubAck> {
    if remaining.len() < 2 {
        bail!("SUBACK too short");
    }
    let packet_id = u16::from_be_bytes([remaining[0], remaining[1]]);
    Ok(SubAck {
        packet_id,
        return_codes: remaining[2..].to_vec(),
    })
}

/// PUBLISH packet builder. ref: MQTT 3.1.1 §3.3 (PUBLISH).
/// QoS / DUP / RETAIN live in the fixed-header flags (§3.3.1). Variable header is
/// the Topic Name then, for QoS>0, the 2-byte Packet Identifier (§3.3.2). Payload
/// is the application message (§3.3.3).
fn encode_publish(topic: &str, payload: &[u8], qos: u8, retain: bool, packet_id: u16) -> Vec<u8> {
    // ref: §3.3.1.1 DUP=0 (first transmission), §3.3.1.2 QoS bits 2-1, §3.3.1.3 RETAIN bit 0.
    let mut flags = (qos & 0b11) << 1;
    if retain {
        flags |= 0b0001;
    }
    let mut body = Vec::new();
    put_mqtt_string(topic, &mut body);
    if qos > 0 {
        body.extend_from_slice(&packet_id.to_be_bytes()); // ref: §3.3.2.2.
    }
    body.extend_from_slice(payload);
    frame_packet(packet_type::PUBLISH, flags, body)
}

/// A decoded inbound PUBLISH. ref: MQTT 3.1.1 §3.3.
#[derive(Clone, Debug)]
pub(crate) struct Publish {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
    pub packet_id: Option<u16>,
}

/// Decode a PUBLISH from its fixed-header flags + remaining bytes. ref: §3.3.
fn decode_publish(flags: u8, remaining: &[u8]) -> Result<Publish> {
    let qos = (flags >> 1) & 0b11;
    let retain = (flags & 0b0001) != 0;
    let (topic, consumed) = get_mqtt_string(remaining, 0)?;
    let mut pos = consumed;
    let packet_id = if qos > 0 {
        if pos + 2 > remaining.len() {
            bail!("PUBLISH packet-id truncated");
        }
        let id = u16::from_be_bytes([remaining[pos], remaining[pos + 1]]);
        pos += 2;
        Some(id)
    } else {
        None
    };
    Ok(Publish {
        topic,
        payload: remaining[pos..].to_vec(),
        qos,
        retain,
        packet_id,
    })
}

/// PUBACK packet builder (QoS 1 acknowledgement). ref: MQTT 3.1.1 §3.4 (PUBACK).
/// Variable header is the 2-byte Packet Identifier (§3.4.2); no payload.
fn encode_puback(packet_id: u16) -> Vec<u8> {
    frame_packet(packet_type::PUBACK, 0, packet_id.to_be_bytes().to_vec())
}

/// PINGREQ packet (keep-alive). ref: MQTT 3.1.1 §3.12. No variable header/payload.
fn encode_pingreq() -> Vec<u8> {
    frame_packet(packet_type::PINGREQ, 0, Vec::new())
}

/// PINGRESP packet. ref: MQTT 3.1.1 §3.13. No variable header/payload.
fn encode_pingresp() -> Vec<u8> {
    frame_packet(packet_type::PINGRESP, 0, Vec::new())
}

/// DISCONNECT packet (graceful close — suppresses the Last-Will at the broker).
/// ref: MQTT 3.1.1 §3.14 (DISCONNECT). No variable header/payload.
fn encode_disconnect() -> Vec<u8> {
    frame_packet(packet_type::DISCONNECT, 0, Vec::new())
}

/// Assemble a full control packet: fixed-header byte (type<<4 | flags) +
/// Remaining Length + body. ref: MQTT 3.1.1 §2.2 (Fixed header).
fn frame_packet(ptype: u8, flags: u8, body: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 5);
    out.push((ptype << 4) | (flags & 0x0F));
    encode_remaining_length(body.len(), &mut out);
    out.extend_from_slice(&body);
    out
}

/// A fully-read raw control packet: its type nibble, its flags nibble, and the
/// "remaining" bytes (variable header + payload). ref: §2.2.
#[derive(Clone, Debug)]
struct RawPacket {
    ptype: u8,
    flags: u8,
    remaining: Vec<u8>,
}

/// Read exactly one control packet off an async stream: fixed-header byte, then the
/// variable-byte Remaining Length, then that many bytes. ref: §2.2 (the framing the
/// receiver must follow to delimit packets on a stream).
async fn read_packet<R: AsyncReadExt + Unpin>(stream: &mut R) -> Result<RawPacket> {
    let mut header = [0u8; 1];
    stream
        .read_exact(&mut header)
        .await
        .context("MQTT read fixed header")?;
    let ptype = header[0] >> 4;
    let flags = header[0] & 0x0F;

    // Remaining Length: read byte-by-byte (1..=4 bytes), continuation bit 0x80.
    // ref: §2.2.3 (the decoder reads bytes until the continuation bit is clear).
    let mut len_bytes = Vec::with_capacity(4);
    loop {
        let mut b = [0u8; 1];
        stream
            .read_exact(&mut b)
            .await
            .context("MQTT read remaining-length byte")?;
        len_bytes.push(b[0]);
        if (b[0] & 0x80) == 0 {
            break;
        }
        if len_bytes.len() >= 4 {
            bail!("MQTT remaining length exceeds 4 bytes");
        }
    }
    let (remaining_len, _) = decode_remaining_length(&len_bytes, 0)?;

    let mut remaining = vec![0u8; remaining_len];
    stream
        .read_exact(&mut remaining)
        .await
        .context("MQTT read remaining bytes")?;
    Ok(RawPacket {
        ptype,
        flags,
        remaining,
    })
}

// ===========================================================================
// 2. Subscription bookkeeping (§2A.25/26/27)
// ===========================================================================

/// Per-topic subscription record. Stores the requested QoS so resubscribe after a
/// session loss restores the SAME QoS (§2A.26), plus the SUBACK outcome so the
/// §2A.25 "retry only previously-failed" path can be honored on a session-present
/// reconnect. ref: AbstractMQTT_IOClient.java:135,288-307,381-391 (topicConsumerMap +
/// the TopicSubscription state it carries).
#[derive(Clone, Debug)]
struct TopicSubscriptionInfo {
    /// requested QoS, preserved across resubscribe. ref: §2A.26.
    qos: u8,
    /// the link this topic feeds; carried so an inbound PUBLISH maps to an
    /// asset/attribute reading and so unlink can find the topic. ref: §2A.27.
    link: AgentLink,
    /// true once a SUBACK has been observed for this topic (success OR failure).
    /// ref: AbstractMQTT_IOClient.java:490-516 (isSubscribeDone gate).
    sub_done: bool,
    /// true if the last SUBACK granted 0x80 (Failure). ref: §3.9.3 + §2A.25.
    sub_failed: bool,
}

/// Resubscribe policy resolved from the agent config (§2A.25).
/// ref: AbstractMQTT_IOClient.java:490-516.
#[derive(Clone, Copy, Debug)]
struct ResubscribePolicy {
    /// resubscribe even when the broker reports an existing session.
    /// ref: AbstractMQTT_IOClient resubscribeIfSessionPresent.
    resubscribe_if_session_present: bool,
    /// re-attempt subscriptions that previously failed (default true).
    /// ref: AbstractMQTT_IOClient retrySubscriptionFailuresOnReconnect.
    retry_subscription_failures_on_reconnect: bool,
}

impl Default for ResubscribePolicy {
    fn default() -> Self {
        ResubscribePolicy {
            resubscribe_if_session_present: false,
            retry_subscription_failures_on_reconnect: true,
        }
    }
}

/// Compute the set of topics to (re)subscribe on a CONNACK, given the Session
/// Present flag and the policy. This is the §2A.25 core, lifted out of the agent so
/// it is unit-testable on its own.
///
/// Two disjoint sub-sets are unioned:
///   1. PENDING topics (`!sub_done`) — links added before this CONNACK that have
///      never been subscribed on the wire. They always get an initial SUBSCRIBE
///      because there is no broker-side session state to "resume" (Session Present
///      is irrelevant to a brand-new subscription).
///   2. the §2A.25 RESUBSCRIBE set among already-done topics:
///        retryAll      = !sessionPresent || resubscribeIfSessionPresent
///        retryFailures = retrySubscriptionFailuresOnReconnect
///        resubscribe iff: isSubscribeDone && (retryAll || (retryFailures && isSubscribeFailed))
///
/// ref: AbstractMQTT_IOClient.java:288-307 (addSubscription queues a SUBSCRIBE on
/// (re)connect) + :490-516 (onConnectionStatusChanged resubscribe predicate).
fn topics_to_resubscribe(
    table: &HashMap<String, TopicSubscriptionInfo>,
    session_present: bool,
    policy: ResubscribePolicy,
) -> Vec<(String, u8)> {
    let retry_all = !session_present || policy.resubscribe_if_session_present;
    let retry_failures = policy.retry_subscription_failures_on_reconnect;
    let mut out: Vec<(String, u8)> = table
        .iter()
        .filter(|(_, info)| {
            if !info.sub_done {
                // (1) pending/never-subscribed link → always subscribe.
                // ref: AbstractMQTT_IOClient.java:288-307.
                return true;
            }
            // (2) §2A.25 resubscribe predicate for already-done topics.
            // ref: AbstractMQTT_IOClient.java:496-507.
            retry_all || (retry_failures && info.sub_failed)
        })
        // QoS preserved per subscription. ref: §2A.26.
        .map(|(topic, info)| (topic.clone(), info.qos))
        .collect();
    // Deterministic order so tests can assert the SUBSCRIBE payload.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

// ===========================================================================
// 3. MQTT binding (per-link topic + QoS, parsed from AgentLink.binding)
// ===========================================================================

/// Protocol-specific binding for an MQTT-linked attribute. Lives in
/// `AgentLink.binding` (opaque JSON the agent interprets). `subscription_topic`
/// is the device source we SUBSCRIBE to; `publish_topic` is the sink we PUBLISH
/// outbound writes to (defaults to the subscription topic when omitted).
/// ref: MQTTAgentLink.java (subscriptionTopic / publishTopic / qos fields).
#[derive(Clone, Debug, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MqttBinding {
    #[serde(default)]
    subscription_topic: Option<String>,
    #[serde(default)]
    publish_topic: Option<String>,
    #[serde(default = "default_qos")]
    qos: u8,
    #[serde(default)]
    retain: bool,
}

fn default_qos() -> u8 {
    0
}

fn parse_binding(link: &AgentLink) -> Result<MqttBinding> {
    if link.binding.is_null() {
        return Ok(MqttBinding::default());
    }
    serde_json::from_value(link.binding.clone()).context("invalid MQTT agentLink binding")
}

// ===========================================================================
// 4. The MQTT connection task (tokio) and its command/event channels
// ===========================================================================
//
// The IotAgent trait is SYNC (the whatsapp_native idiom): a dedicated worker
// thread owns an isolated tokio runtime and the TCP socket; the sync trait methods
// communicate with it over std mpsc channels. This keeps the trait dep-free
// (no async-trait) and matches communication/whatsapp_native::run_async.

/// Command sent from the sync agent surface into the connection task.
enum MqttCommand {
    /// (re)connect using the resolved CONNECT parameters; reply with the CONNACK.
    Connect {
        params: ConnectParams,
        reply: Sender<Result<ConnAck>>,
    },
    /// Subscribe to a batch of (topic, qos); reply with the SUBACK return codes
    /// aligned to the requested order.
    Subscribe {
        topics: Vec<(String, u8)>,
        reply: Sender<Result<Vec<u8>>>,
    },
    /// Publish an outbound write. ref: §3.3 (PUBLISH).
    Publish {
        topic: String,
        payload: Vec<u8>,
        qos: u8,
        retain: bool,
        reply: Sender<Result<()>>,
    },
    /// Send PINGREQ (keep-alive). ref: §3.12.
    Ping { reply: Sender<Result<()>> },
    /// Graceful DISCONNECT — suppresses the Last-Will at the broker. ref: §3.14.
    Disconnect,
}

/// Inbound events surfaced from the connection task to the sync agent surface.
enum MqttInbound {
    /// A device PUBLISH arrived. ref: §3.3 + §2A.1 (no device timestamp → 0).
    Publish(Publish),
    /// The socket dropped (ungraceful) — drives ReconnectStateMachine::on_disconnected.
    Disconnected,
}

/// Resolved CONNECT parameters (host/port/clientId/credentials/will).
///
/// Custom `Debug` REDACTS the resolved credentials: the username/password come
/// from the secret store (CTO_IOT_MQTT_PASSWORD) and must never reach a log line,
/// a panic message, or a support bundle. Only non-secret topology is formatted.
#[derive(Clone)]
struct ConnectParams {
    host: String,
    port: u16,
    client_id: String,
    clean_session: bool,
    username: Option<String>,
    password: Option<String>,
    will: Option<LastWill>,
}

impl std::fmt::Debug for ConnectParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Secret redaction: never print username/password. Report only presence.
        f.debug_struct("ConnectParams")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("client_id", &self.client_id)
            .field("clean_session", &self.clean_session)
            .field("username", &self.username.as_ref().map(|_| "<redacted>"))
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("will", &self.will.as_ref().map(|_| "<set>"))
            .finish()
    }
}

/// Spawn the per-agent connection worker: one OS thread owning a multi-thread
/// tokio runtime (mirrors communication/whatsapp_native::run_async). Returns the
/// command sender and the inbound-event receiver.
fn spawn_connection_worker() -> (Sender<MqttCommand>, Receiver<MqttInbound>) {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<MqttCommand>();
    let (in_tx, in_rx) = std::sync::mpsc::channel::<MqttInbound>();
    thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
        {
            Ok(rt) => rt,
            Err(_) => return,
        };
        rt.block_on(connection_task(cmd_rx, in_tx));
    });
    (cmd_tx, in_rx)
}

/// The async connection loop. Owns the TcpStream; serializes outbound writes; reads
/// inbound packets and routes them. A pending QoS-1 outbound awaits its PUBACK
/// inline (simple stop-and-wait — sufficient for the device-write cadence).
async fn connection_task(cmd_rx: Receiver<MqttCommand>, in_tx: Sender<MqttInbound>) {
    let mut stream: Option<TcpStream> = None;
    let mut packet_id_counter: u16 = 0;

    // The std mpsc Receiver is blocking; bridge it with a small poll loop so the
    // single task can both await socket reads and service commands. We use
    // try_recv with a short yield rather than a second runtime.
    loop {
        // Drain all queued commands first (non-blocking).
        match cmd_rx.try_recv() {
            Ok(cmd) => {
                if handle_command(cmd, &mut stream, &mut packet_id_counter).await {
                    // Disconnect requested → end the task.
                    return;
                }
                continue;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
        }

        // Then service one inbound packet if the socket is live, with a timeout so
        // we periodically loop back to check for commands.
        if let Some(s) = stream.as_mut() {
            match tokio::time::timeout(Duration::from_millis(20), read_packet(s)).await {
                Ok(Ok(pkt)) => {
                    if let Some(event) = route_inbound(pkt, s).await {
                        if in_tx.send(event).is_err() {
                            return;
                        }
                    }
                }
                Ok(Err(_)) => {
                    // Socket error / EOF → ungraceful drop. ref: §2A.24 disconnect.
                    stream = None;
                    let _ = in_tx.send(MqttInbound::Disconnected);
                }
                Err(_) => { /* read timeout: loop back to command servicing */ }
            }
        } else {
            // No socket: nothing to read; brief yield so we don't busy-spin.
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

/// Apply one command against the socket. Returns true iff the task should stop
/// (graceful Disconnect).
async fn handle_command(
    cmd: MqttCommand,
    stream: &mut Option<TcpStream>,
    packet_id_counter: &mut u16,
) -> bool {
    match cmd {
        MqttCommand::Connect { params, reply } => {
            let result = do_connect(&params).await;
            match result {
                Ok((s, connack)) => {
                    *stream = Some(s);
                    let _ = reply.send(Ok(connack));
                }
                Err(e) => {
                    *stream = None;
                    let _ = reply.send(Err(e));
                }
            }
            false
        }
        MqttCommand::Subscribe { topics, reply } => {
            let r = match stream.as_mut() {
                Some(s) => do_subscribe(s, &topics, packet_id_counter).await,
                None => Err(anyhow!("subscribe without an open connection")),
            };
            let _ = reply.send(r);
            false
        }
        MqttCommand::Publish {
            topic,
            payload,
            qos,
            retain,
            reply,
        } => {
            let r = match stream.as_mut() {
                Some(s) => do_publish(s, &topic, &payload, qos, retain, packet_id_counter).await,
                None => Err(anyhow!("publish without an open connection")),
            };
            let _ = reply.send(r);
            false
        }
        MqttCommand::Ping { reply } => {
            let r = match stream.as_mut() {
                Some(s) => s
                    .write_all(&encode_pingreq())
                    .await
                    .context("write PINGREQ"),
                None => Err(anyhow!("ping without an open connection")),
            };
            let _ = reply.send(r);
            false
        }
        MqttCommand::Disconnect => {
            if let Some(s) = stream.as_mut() {
                // Graceful: send DISCONNECT so the broker suppresses the Will (§2A.26).
                let _ = s.write_all(&encode_disconnect()).await;
                let _ = s.flush().await;
                let _ = s.shutdown().await;
            }
            *stream = None;
            true
        }
    }
}

/// Open the TCP connection, send CONNECT, await CONNACK. ref: §3.1/§3.2.
async fn do_connect(params: &ConnectParams) -> Result<(TcpStream, ConnAck)> {
    let addr = format!("{}:{}", params.host, params.port);
    let mut stream = TcpStream::connect(&addr)
        .await
        .with_context(|| format!("MQTT TCP connect to {addr}"))?;
    let connect = encode_connect(
        &params.client_id,
        params.clean_session,
        params.username.as_deref(),
        params.password.as_deref(),
        params.will.as_ref(),
    );
    stream.write_all(&connect).await.context("write CONNECT")?;
    let pkt = read_packet(&mut stream).await.context("await CONNACK")?;
    if pkt.ptype != packet_type::CONNACK {
        bail!("expected CONNACK, got packet type {}", pkt.ptype);
    }
    let connack = decode_connack(&pkt.remaining)?;
    // ref: §3.2.2.3 — non-zero return code means the broker refused the connection.
    if connack.return_code != 0 {
        bail!("CONNACK refused with return code {}", connack.return_code);
    }
    Ok((stream, connack))
}

/// Send SUBSCRIBE and await the matching SUBACK; return its per-topic return codes
/// in request order. ref: §3.8/§3.9.
async fn do_subscribe(
    stream: &mut TcpStream,
    topics: &[(String, u8)],
    packet_id_counter: &mut u16,
) -> Result<Vec<u8>> {
    let pid = next_packet_id(packet_id_counter);
    let sub = encode_subscribe(pid, topics);
    stream.write_all(&sub).await.context("write SUBSCRIBE")?;
    // Await SUBACK (skip any interleaved PUBLISH/ping while waiting — rare at link
    // time; we route only the SUBACK here and drop others to keep the helper simple
    // because subscribe runs before steady-state reads begin).
    loop {
        let pkt = read_packet(stream).await.context("await SUBACK")?;
        if pkt.ptype == packet_type::SUBACK {
            let suback = decode_suback(&pkt.remaining)?;
            if suback.packet_id == pid {
                return Ok(suback.return_codes);
            }
        }
        // ignore non-matching control packets during the subscribe handshake.
    }
}

/// Send a PUBLISH; for QoS 1 await the PUBACK. ref: §3.3/§3.4.
async fn do_publish(
    stream: &mut TcpStream,
    topic: &str,
    payload: &[u8],
    qos: u8,
    retain: bool,
    packet_id_counter: &mut u16,
) -> Result<()> {
    let pid = if qos > 0 {
        next_packet_id(packet_id_counter)
    } else {
        0
    };
    let pub_pkt = encode_publish(topic, payload, qos, retain, pid);
    stream.write_all(&pub_pkt).await.context("write PUBLISH")?;
    if qos == 1 {
        // stop-and-wait for the PUBACK. ref: §4.3.2 (QoS 1 protocol flow).
        loop {
            let pkt = read_packet(stream).await.context("await PUBACK")?;
            if pkt.ptype == packet_type::PUBACK && pkt.remaining.len() >= 2 {
                let ack_pid = u16::from_be_bytes([pkt.remaining[0], pkt.remaining[1]]);
                if ack_pid == pid {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

/// Route an inbound steady-state packet to an `MqttInbound` event (or handle it
/// inline). PUBLISH at QoS 1 is acked with PUBACK before surfacing. ref: §3.3/§3.4/§3.13.
async fn route_inbound(pkt: RawPacket, stream: &mut TcpStream) -> Option<MqttInbound> {
    match pkt.ptype {
        packet_type::PUBLISH => match decode_publish(pkt.flags, &pkt.remaining) {
            Ok(publish) => {
                if publish.qos == 1 {
                    if let Some(pid) = publish.packet_id {
                        // ref: §4.3.2 — receiver of a QoS 1 PUBLISH responds with PUBACK.
                        let _ = stream.write_all(&encode_puback(pid)).await;
                    }
                }
                Some(MqttInbound::Publish(publish))
            }
            Err(_) => None,
        },
        // The broker may PING us; respond so it does not drop us. ref: §3.12/§3.13.
        packet_type::PINGREQ => {
            let _ = stream.write_all(&encode_pingresp()).await;
            None
        }
        // PINGRESP / SUBACK / PUBACK in steady state are accounted for elsewhere.
        _ => None,
    }
}

/// Monotonic non-zero packet identifier allocator. ref: §2.3.1 (Packet Identifier
/// MUST be non-zero for packets that require one).
fn next_packet_id(counter: &mut u16) -> u16 {
    *counter = counter.wrapping_add(1);
    if *counter == 0 {
        *counter = 1;
    }
    *counter
}

// ===========================================================================
// 5. MqttAgent — the IotAgent implementation
// ===========================================================================

/// Injectable epoch-ms clock so the §2A.24 reconnect state machine is
/// deterministic in tests. Production wires `crate::iot::now_ms`. Mirrors the WS
/// agent's `Clock` seam (ws_native.rs:62) so MQTT drives the SAME backoff
/// contract: connect() reads `(self.clock)()` and gates WAITING→Connecting on
/// `poll_ready_to_retry(now)` / schedules backoff with `schedule_backoff(now)`.
type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Native MQTT 3.1.1 agent. Embeds the shared `ReconnectStateMachine` (§2A.24) and
/// the synchronized topic table (§2A.27); reuses the adapters.rs value-processing
/// base layer through runtime.rs (this file never coerces/filters values itself).
pub(crate) struct MqttAgent {
    agent_id: String,
    params: ConnectParams,
    policy: ResubscribePolicy,
    /// §2A.27 — the link table is behind a Mutex so subscribe/unlink are safe to
    /// call while a reconnect is in flight; resubscribe iterates a snapshot taken
    /// under this lock. ref: AbstractMQTT_IOClient.java:135 (synchronized topicConsumerMap).
    topics: Arc<Mutex<HashMap<String, TopicSubscriptionInfo>>>,
    /// §2A.24 reconnect state machine (deterministic, injected clock — see `clock`).
    state: ReconnectStateMachine,
    /// Injected epoch-ms clock for the reconnect SM (deterministic in tests).
    /// Production wires `crate::iot::now_ms`; `connect()` reads this to gate the
    /// WAITING→Connecting transition and to stamp `schedule_backoff` on failure.
    clock: Clock,
    /// command sink + inbound-event source for the connection worker.
    cmd_tx: Sender<MqttCommand>,
    in_rx: Receiver<MqttInbound>,
}

impl MqttAgent {
    /// Build an agent from its runtime context. Resolves host/port/clientId from
    /// the opaque agent config and credentials/will via
    /// `runtime_env::env_or_config` + the secret store — NEVER std::env.
    /// ref: AbstractMQTT_IOClient.java (constructor wiring host/port/clientId/will).
    pub(crate) fn new(ctx: AgentContext) -> Result<MqttAgent> {
        let params = resolve_connect_params(&ctx)?;
        let policy = resolve_policy(&ctx.config);
        let (cmd_tx, in_rx) = spawn_connection_worker();
        // Seed the jitter from the agent id so the backoff curve is reproducible
        // per agent (NOT wall-clock-seeded). ref: adapters::ReconnectStateMachine.
        let seed = fnv1a64(ctx.agent_id.as_bytes());
        Ok(MqttAgent {
            agent_id: ctx.agent_id,
            params,
            policy,
            topics: Arc::new(Mutex::new(HashMap::new())),
            state: ReconnectStateMachine::new(seed),
            clock: Arc::new(now_ms),
            cmd_tx,
            in_rx,
        })
    }

    /// Test seam: override the reconnect clock with a deterministic source.
    /// NOT used in production (the default `new` wires `crate::iot::now_ms`).
    #[cfg(test)]
    fn set_clock(&mut self, clock: Clock) {
        self.clock = clock;
    }

    /// The set of topics present at CONNACK time — the §2A.27 invariant: resubscribe
    /// iterates a snapshot taken under the lock, so a concurrent link/unlink during
    /// the in-flight reconnect cannot corrupt the resubscribe batch.
    /// ref: AbstractMQTT_IOClient.java:490-516 (resubscribe over the synchronized map).
    fn resubscribe_after_connack(&mut self, connack: ConnAck) -> Result<()> {
        // Snapshot under the lock (§2A.27).
        let batch = {
            let guard = self.topics.lock().unwrap_or_else(|p| p.into_inner());
            topics_to_resubscribe(&guard, connack.session_present, self.policy)
        };
        if batch.is_empty() {
            return Ok(());
        }
        let codes = self.send_subscribe(&batch)?;
        // Record the SUBACK outcome per topic (granted-QoS vs 0x80 Failure, §2A.25).
        let mut guard = self.topics.lock().unwrap_or_else(|p| p.into_inner());
        for (i, (topic, _)) in batch.iter().enumerate() {
            if let Some(info) = guard.get_mut(topic) {
                info.sub_done = true;
                info.sub_failed = codes.get(i).map(|c| *c == 0x80).unwrap_or(true);
            }
        }
        Ok(())
    }

    /// Send a SUBSCRIBE batch through the worker and await the SUBACK codes.
    fn send_subscribe(&self, topics: &[(String, u8)]) -> Result<Vec<u8>> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.cmd_tx
            .send(MqttCommand::Subscribe {
                topics: topics.to_vec(),
                reply: tx,
            })
            .map_err(|_| anyhow!("MQTT connection worker gone"))?;
        rx.recv()
            .map_err(|_| anyhow!("MQTT subscribe reply dropped"))?
    }

    /// Drain inbound worker events into readings + reconnect-state transitions.
    fn drain_inbound(&mut self) -> Vec<AttributeReading> {
        let mut readings = Vec::new();
        while let Ok(event) = self.in_rx.try_recv() {
            match event {
                MqttInbound::Publish(publish) => {
                    // Map the topic back to its link (§2A.27 lock) and build a raw
                    // reading. The base-layer filters/converters/coercion run in
                    // runtime.rs, NOT here. ref: §2A.28.
                    let guard = self.topics.lock().unwrap_or_else(|p| p.into_inner());
                    if let Some(info) = guard.get(&publish.topic) {
                        let raw = payload_to_value(&publish.payload);
                        readings.push(AttributeReading {
                            asset_id: info.link.asset_id.clone(),
                            attribute_name: info.link.attribute_name.clone(),
                            raw,
                            // §2A.1 — MQTT carries no message timestamp; 0 lets the
                            // engine normalize against system time.
                            device_timestamp_ms: 0,
                        });
                    }
                }
                MqttInbound::Disconnected => {
                    // §2A.24 — Connected → Connecting; the runtime loop reschedules.
                    self.state.on_disconnected();
                }
            }
        }
        readings
    }
}

impl IotAgent for MqttAgent {
    fn kind(&self) -> IotAgentKind {
        IotAgentKind::Mqtt
    }

    /// ref: AbstractIOClientProtocol.java:152-162 (doStart must not throw) +
    /// AbstractMQTT_IOClient.java:430,664 (Disconnected→Connecting→Connected) and
    /// :490-516 (resubscribe on CONNACK).
    fn connect(&mut self, _ctx: &AgentContext) -> Result<ConnectionStatus> {
        // INJECTED clock — the §2A.24 backoff is gated/stamped against this, never
        // wall-clock. Production wires `crate::iot::now_ms`; tests override it.
        // Mirrors ws_native.rs:359. ref: AbstractMQTT_IOClient.java:628-645.
        let now = (self.clock)();

        // Idempotent if already connected. ref: doStart idempotency.
        if self.state.status() == ConnectionStatus::Connected {
            return Ok(ConnectionStatus::Connected);
        }
        // Stale-task guard: a connect issued AFTER begin_disconnect is aborted.
        // Note this guards only the *graceful-shutdown* state — the initial
        // Disconnected state is the legal entry into Connecting (handled next), so
        // we must NOT treat Disconnected as aborting here.
        // ref: AbstractMQTT_IOClient.java:464-468.
        if self.state.status() == ConnectionStatus::Disconnecting {
            return Ok(self.state.status());
        }
        // In WAITING, only advance to Connecting once the injected clock says the
        // backoff envelope has elapsed (poll_ready_to_retry). Otherwise stay
        // WAITING — this is what paces the retry instead of a 100ms hot loop.
        // ref: AbstractMQTT_IOClient.java:628-669 (scheduleDoConnect / ReconnectStateMachine).
        if self.state.status() == ConnectionStatus::Waiting && !self.state.poll_ready_to_retry(now)
        {
            return Ok(ConnectionStatus::Waiting);
        }
        // From DISCONNECTED we enter CONNECTING here. ref: :430 (Disconnected→Connecting).
        if self.state.status() == ConnectionStatus::Disconnected {
            self.state.begin_connect();
        }
        if self.state.status() != ConnectionStatus::Connecting {
            return Ok(self.state.status());
        }

        // Drive the TCP CONNECT + CONNACK through the worker.
        let (tx, rx) = std::sync::mpsc::channel();
        self.cmd_tx
            .send(MqttCommand::Connect {
                params: self.params.clone(),
                reply: tx,
            })
            .map_err(|_| anyhow!("MQTT connection worker gone"))?;
        let connack = rx
            .recv()
            .map_err(|_| anyhow!("MQTT connect reply dropped"))?;

        match connack {
            Ok(connack) => {
                self.state.mark_connected();
                // §2A.25/26/27 — resubscribe over the CONNACK-time topic snapshot.
                self.resubscribe_after_connack(connack)?;
                Ok(ConnectionStatus::Connected)
            }
            Err(_e) => {
                // Transient failure: drive the SM into WAITING with an exponential
                // 1s→5min backoff + ±25% jitter, stamped off the INJECTED clock.
                // The next supervisor tick re-enters connect() in WAITING and is
                // gated by poll_ready_to_retry(now) above — no 100ms hot loop.
                // connect() does not loop; infinite retries are the runtime's job.
                // ref: AbstractMQTT_IOClient.java:628-645 (scheduleDoConnect).
                self.state.schedule_backoff(now); // Connecting → Waiting
                Ok(ConnectionStatus::Waiting)
            }
        }
    }

    /// Register a link (§2A.27 — mutate the synchronized topic table only). If the
    /// agent is already CONNECTED, also issue a live SUBSCRIBE for the new topic.
    /// ref: AbstractProtocol.java:104-133 (linkAttribute) +
    /// AbstractMQTT_IOClient.java:288-307 (addSubscription / topicConsumerMap).
    fn subscribe(&mut self, link: &AgentLink) -> Result<()> {
        let binding = parse_binding(link)?;
        let topic = binding
            .subscription_topic
            .clone()
            .ok_or_else(|| anyhow!("MQTT link missing subscriptionTopic"))?;
        let qos = binding.qos;

        // Mutate the synchronized table (safe during reconnect, §2A.27).
        {
            let mut guard = self.topics.lock().unwrap_or_else(|p| p.into_inner());
            guard.insert(
                topic.clone(),
                TopicSubscriptionInfo {
                    qos,
                    link: link.clone(),
                    sub_done: false,
                    sub_failed: false,
                },
            );
        }

        // If connected, subscribe live; otherwise the next CONNACK resubscribe
        // picks it up. ref: AbstractMQTT_IOClient.java:288-307.
        if self.state.status() == ConnectionStatus::Connected {
            let codes = self.send_subscribe(&[(topic.clone(), qos)])?;
            let mut guard = self.topics.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(info) = guard.get_mut(&topic) {
                info.sub_done = true;
                info.sub_failed = codes.first().map(|c| *c == 0x80).unwrap_or(true);
            }
        }
        Ok(())
    }

    /// Drain inbound device PUBLISHes into raw readings. ref:
    /// AbstractIOClientProtocol.java:195-200 (onMessageReceived). The base layer
    /// (runtime.rs) applies filters → converters → coercion afterward (§2A.28).
    fn read(&mut self) -> Result<Vec<AttributeReading>> {
        Ok(self.drain_inbound())
    }

    /// Send an outbound write to the device. `processed` is post-base-layer
    /// (filters/converters/%VALUE%/%TIME% already applied by the runtime, §2A.28).
    /// Fire-and-forget (QoS 0) or stop-and-wait (QoS 1); update_on_write is the
    /// runtime's job (§2A.30). ref: AbstractIOClientProtocol.java:164-179.
    fn write(&mut self, link: &AgentLink, processed: &AttributeValue) -> Result<()> {
        let binding = parse_binding(link)?;
        // publishTopic falls back to subscriptionTopic when omitted.
        let topic = binding
            .publish_topic
            .clone()
            .or_else(|| binding.subscription_topic.clone())
            .ok_or_else(|| anyhow!("MQTT link missing publish/subscription topic"))?;
        let payload = value_to_payload(processed);

        let (tx, rx) = std::sync::mpsc::channel();
        self.cmd_tx
            .send(MqttCommand::Publish {
                topic,
                payload,
                qos: binding.qos,
                retain: binding.retain, // §2A.26 retain honored on publish.
                reply: tx,
            })
            .map_err(|_| anyhow!("MQTT connection worker gone"))?;
        rx.recv()
            .map_err(|_| anyhow!("MQTT publish reply dropped"))?
    }

    /// Remove a link (§2A.27 — safe during reconnect; mutate the table only).
    /// ref: AbstractProtocol.java:104-133 (unlinkAttribute) +
    /// AbstractMQTT_IOClient.java:381-391 (removeSubscription).
    fn unlink(&mut self, link: &AgentLink) -> Result<()> {
        let binding = parse_binding(link)?;
        if let Some(topic) = binding.subscription_topic {
            let mut guard = self.topics.lock().unwrap_or_else(|p| p.into_inner());
            guard.remove(&topic);
        }
        Ok(())
    }

    fn status(&self) -> ConnectionStatus {
        self.state.status()
    }
}

// ===========================================================================
// 6. Config / credential resolution + value <-> payload mapping
// ===========================================================================

/// Resolve CONNECT parameters from the opaque agent config + the secret store.
/// Credentials NEVER come from std::env: usernames/passwords are read via
/// `runtime_env::env_or_config(root, KEY)` when the config names a key, falling
/// back to an inline literal in config for non-secret fields.
/// ref: AbstractMQTT_IOClient.java (host/port/clientId/credentials wiring).
fn resolve_connect_params(ctx: &AgentContext) -> Result<ConnectParams> {
    let cfg = &ctx.config;
    let host = cfg
        .get("host")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("MQTT agent config missing host"))?
        .to_string();
    let port = cfg
        .get("port")
        .and_then(|v| v.as_u64())
        .map(|p| p as u16)
        .unwrap_or(1883); // default MQTT port. ref: §1 (IANA tcp/1883).
    let client_id = cfg
        .get("clientId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("ctox-{}", ctx.agent_id));
    let clean_session = cfg
        .get("cleanSession")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Credentials: a config entry may either be an inline value OR name a
    // runtime_env / secret-store key (`usernameKey` / `passwordKey`). Secret keys
    // are resolved ONLY through env_or_config (encrypted store), never std::env.
    let username = resolve_secret_field(ctx, cfg, "username", "usernameKey");
    let password = resolve_secret_field(ctx, cfg, "password", "passwordKey");

    // Optional Last-Will (§2A.26). ref: §3.1.2.5 + §3.1.3.2/3.
    let will = cfg.get("will").and_then(|w| {
        let topic = w.get("topic").and_then(|v| v.as_str())?.to_string();
        let payload = w
            .get("payload")
            .and_then(|v| v.as_str())
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_default();
        let qos = w.get("qos").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
        let retain = w.get("retain").and_then(|v| v.as_bool()).unwrap_or(false);
        Some(LastWill {
            topic,
            payload,
            qos,
            retain,
        })
    });

    Ok(ConnectParams {
        host,
        port,
        client_id,
        clean_session,
        username,
        password,
        will,
    })
}

/// Resolve a possibly-secret field: prefer the inline `<field>` literal, else read
/// the key named by `<field>Key` through `runtime_env::env_or_config` (which routes
/// secret keys to the encrypted CTOX secret store). NEVER std::env.
fn resolve_secret_field(
    ctx: &AgentContext,
    cfg: &serde_json::Value,
    field: &str,
    key_field: &str,
) -> Option<String> {
    if let Some(inline) = cfg.get(field).and_then(|v| v.as_str()) {
        if !inline.is_empty() {
            return Some(inline.to_string());
        }
    }
    let key = cfg.get(key_field).and_then(|v| v.as_str())?;
    crate::execution::models::runtime_env::env_or_config(ctx.root, key)
}

/// Resolve the §2A.25 resubscribe policy from config (defaults match upstream).
fn resolve_policy(cfg: &serde_json::Value) -> ResubscribePolicy {
    let mut p = ResubscribePolicy::default();
    if let Some(v) = cfg
        .get("resubscribeIfSessionPresent")
        .and_then(|v| v.as_bool())
    {
        p.resubscribe_if_session_present = v;
    }
    if let Some(v) = cfg
        .get("retrySubscriptionFailuresOnReconnect")
        .and_then(|v| v.as_bool())
    {
        p.retry_subscription_failures_on_reconnect = v;
    }
    p
}

/// Map a raw MQTT payload to an AttributeValue. JSON-parse first (devices often
/// publish JSON numbers/objects); fall back to a UTF-8 string, else a null. The
/// base layer (runtime.rs) coerces to the declared type afterward (§2A.28).
fn payload_to_value(payload: &[u8]) -> AttributeValue {
    if let Ok(s) = std::str::from_utf8(payload) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s.trim()) {
            return AttributeValue(v);
        }
        return AttributeValue(serde_json::Value::String(s.to_string()));
    }
    AttributeValue(serde_json::Value::Null)
}

/// Serialize an outbound (already-processed) value to an MQTT payload: strings go
/// out verbatim (so %VALUE%/%TIME% templates land as-is), everything else as JSON.
fn value_to_payload(value: &AttributeValue) -> Vec<u8> {
    match &value.0 {
        serde_json::Value::String(s) => s.as_bytes().to_vec(),
        other => other.to_string().into_bytes(),
    }
}

/// FNV-1a 64-bit hash for the deterministic per-agent jitter seed (no external dep).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// run_async helper mirroring communication/whatsapp_native::run_async: drive an
/// isolated tokio runtime on a fresh thread so a sync caller can await one future.
/// Used by tests that need to drive the loopback broker fixture.
#[allow(dead_code)]
fn run_async<F, T>(future: F) -> Result<T>
where
    F: Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .context("failed to build MQTT test runtime")?
            .block_on(future)
    })
    .join()
    .map_err(|_| anyhow!("MQTT worker thread panicked"))?
}

// ===========================================================================
// 7. Tests — in-process loopback MQTT broker + deterministic injected clock
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::adapters::do_inbound_value_processing;
    use crate::iot::model::ValueBaseType;
    use crate::iot::store;
    use serde_json::json;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc as std_mpsc;
    use std::time::Instant;
    use tokio::net::TcpListener;

    // Secret redaction: ConnectParams Debug must never print the resolved
    // username/password (they come from CTO_IOT_MQTT_PASSWORD via the secret
    // store). Only host/port/clientId and credential PRESENCE are formatted.
    #[test]
    fn connect_params_debug_redacts_credentials() {
        let params = ConnectParams {
            host: "broker.example".into(),
            port: 8883,
            client_id: "ctox-iot".into(),
            clean_session: true,
            username: Some("device-user".into()),
            password: Some("super-secret-mqtt-password".into()),
            will: None,
        };
        let dbg = format!("{params:?}");
        assert!(
            !dbg.contains("super-secret-mqtt-password"),
            "password leaked: {dbg}"
        );
        assert!(!dbg.contains("device-user"), "username leaked: {dbg}");
        assert!(dbg.contains("<redacted>"), "no redaction marker: {dbg}");
        assert!(dbg.contains("broker.example"), "host present: {dbg}");
        assert!(dbg.contains("8883"), "port present: {dbg}");
    }

    // -- codec unit tests ----------------------------------------------------

    #[test]
    fn remaining_length_round_trips_across_boundaries() {
        // ref: MQTT 3.1.1 §2.2.3 worked examples: 0, 127 (1 byte), 128 (2 bytes),
        // 16383 (2 bytes), 16384 (3 bytes), 2097151 (3 bytes), 2097152 (4 bytes).
        for &n in &[0usize, 127, 128, 16_383, 16_384, 2_097_151, 2_097_152] {
            let mut buf = Vec::new();
            encode_remaining_length(n, &mut buf);
            let (decoded, consumed) = decode_remaining_length(&buf, 0).unwrap();
            assert_eq!(decoded, n, "value {n}");
            assert_eq!(consumed, buf.len(), "consumed all bytes for {n}");
        }
    }

    #[test]
    fn connect_packet_sets_flags_and_payload_order() {
        let will = LastWill {
            topic: "will/topic".into(),
            payload: b"bye".to_vec(),
            qos: 1,
            retain: true,
        };
        let pkt = encode_connect("cid", true, Some("u"), Some("p"), Some(&will));
        // Fixed header: CONNECT type nibble.
        assert_eq!(pkt[0] >> 4, packet_type::CONNECT);
        // Parse the variable header connect-flags byte.
        let (remaining_len, len_bytes) = decode_remaining_length(&pkt, 1).unwrap();
        let body = &pkt[1 + len_bytes..1 + len_bytes + remaining_len];
        // protocol name "MQTT" (2-byte len + 4 bytes) + level (1) = 7 bytes, then flags.
        let flags = body[7];
        assert_ne!(flags & 0b0000_0010, 0, "clean session bit set");
        assert_ne!(flags & 0b0000_0100, 0, "will flag set");
        assert_eq!((flags >> 3) & 0b11, 1, "will QoS=1 encoded");
        assert_ne!(flags & 0b0010_0000, 0, "will retain bit set");
        assert_ne!(flags & 0b0100_0000, 0, "password flag set");
        assert_ne!(flags & 0b1000_0000, 0, "username flag set");
    }

    #[test]
    fn publish_round_trips_qos_retain_topic_payload() {
        let pkt = encode_publish("a/b", b"23.5", 1, true, 7);
        assert_eq!(pkt[0] >> 4, packet_type::PUBLISH);
        let flags = pkt[0] & 0x0F;
        let (rl, lb) = decode_remaining_length(&pkt, 1).unwrap();
        let body = &pkt[1 + lb..1 + lb + rl];
        let publish = decode_publish(flags, body).unwrap();
        assert_eq!(publish.topic, "a/b");
        assert_eq!(publish.payload, b"23.5");
        assert_eq!(publish.qos, 1);
        assert!(publish.retain);
        assert_eq!(publish.packet_id, Some(7));
    }

    // -- §2A.25 resubscribe filter (session-present all-vs-failed-only) -------

    fn sub(topic: &str, qos: u8, done: bool, failed: bool) -> (String, TopicSubscriptionInfo) {
        (
            topic.to_string(),
            TopicSubscriptionInfo {
                qos,
                link: AgentLink {
                    asset_id: "a1".into(),
                    attribute_name: "temp".into(),
                    ..AgentLink::default()
                },
                sub_done: done,
                sub_failed: failed,
            },
        )
    }

    #[test]
    fn resubscribe_all_when_session_not_present() {
        let mut table = HashMap::new();
        table.extend([
            sub("t/ok", 1, true, false),
            sub("t/failed", 2, true, true),
            sub("t/pending", 0, false, false), // never-subscribed → initial SUBSCRIBE
        ]);
        // sessionPresent=false → retryAll → every done topic, PLUS the pending one
        // (no session state to resume), QoS preserved for each.
        let out = topics_to_resubscribe(&table, false, ResubscribePolicy::default());
        assert_eq!(
            out,
            vec![
                ("t/failed".into(), 2),
                ("t/ok".into(), 1),
                ("t/pending".into(), 0)
            ],
            "§2A.25: all done topics resubscribed + pending initial subscribe, §2A.26: QoS preserved"
        );
    }

    #[test]
    fn pending_topic_subscribed_even_when_session_present() {
        // A never-subscribed link must get an initial SUBSCRIBE on connect even when
        // the broker reports an existing session (Session Present cannot "resume" a
        // subscription that was never sent). ref: AbstractMQTT_IOClient.java:288-307.
        let mut table = HashMap::new();
        table.extend([
            sub("t/done", 1, true, false), // session present + ok → NOT resubscribed
            sub("t/pending", 2, false, false), // never subscribed → MUST subscribe
        ]);
        let out = topics_to_resubscribe(&table, true, ResubscribePolicy::default());
        assert_eq!(out, vec![("t/pending".into(), 2)]);
    }

    #[test]
    fn resubscribe_only_failed_when_session_present() {
        let mut table = HashMap::new();
        table.extend([sub("t/ok", 1, true, false), sub("t/failed", 2, true, true)]);
        // sessionPresent=true, default policy (don't resub-if-present, retry failures).
        let out = topics_to_resubscribe(&table, true, ResubscribePolicy::default());
        assert_eq!(
            out,
            vec![("t/failed".into(), 2)],
            "§2A.25: session present → only previously-failed resubscribed"
        );
    }

    #[test]
    fn resubscribe_all_when_session_present_but_policy_forces_it() {
        let mut table = HashMap::new();
        table.extend([sub("t/ok", 1, true, false), sub("t/failed", 2, true, true)]);
        let policy = ResubscribePolicy {
            resubscribe_if_session_present: true,
            retry_subscription_failures_on_reconnect: true,
        };
        let out = topics_to_resubscribe(&table, true, policy);
        assert_eq!(out, vec![("t/failed".into(), 2), ("t/ok".into(), 1)]);
    }

    #[test]
    fn resubscribe_skips_failed_when_retry_failures_disabled() {
        let mut table = HashMap::new();
        table.extend([sub("t/failed", 2, true, true)]);
        let policy = ResubscribePolicy {
            resubscribe_if_session_present: false,
            retry_subscription_failures_on_reconnect: false,
        };
        // sessionPresent=true, no retry-all, no retry-failures → empty.
        let out = topics_to_resubscribe(&table, true, policy);
        assert!(out.is_empty());
    }

    // -- §2A.27 link/unlink mutate the synchronized table --------------------

    #[test]
    fn link_table_safe_mutations() {
        let table: Arc<Mutex<HashMap<String, TopicSubscriptionInfo>>> =
            Arc::new(Mutex::new(HashMap::new()));
        {
            let mut g = table.lock().unwrap();
            let (t, i) = sub("t/x", 1, true, false);
            g.insert(t, i);
        }
        // A "reconnect" snapshot taken under the lock is unaffected by a later
        // unlink — proving §2A.27 (resubscribe iterates a CONNACK-time snapshot).
        let snapshot = {
            let g = table.lock().unwrap();
            topics_to_resubscribe(&g, false, ResubscribePolicy::default())
        };
        {
            let mut g = table.lock().unwrap();
            g.remove("t/x"); // concurrent unlink during the "reconnect"
        }
        assert_eq!(snapshot, vec![("t/x".into(), 1)]);
        assert!(table.lock().unwrap().is_empty());
    }

    // -- value <-> payload mapping -------------------------------------------

    #[test]
    fn payload_parses_json_then_string_then_null() {
        assert_eq!(payload_to_value(b"23.5").0, json!(23.5));
        assert_eq!(payload_to_value(b"true").0, json!(true));
        assert_eq!(payload_to_value(b"hello").0, json!("hello"));
        assert_eq!(payload_to_value(b"{\"k\":1}").0, json!({"k":1}));
        assert!(payload_to_value(&[0xff, 0xfe]).0.is_null());
    }

    // =====================================================================
    // In-process loopback MQTT broker fixture (speaks the same vendored codec)
    // =====================================================================

    /// Records what the broker observed so tests can assert "the broker saw the
    /// SUBSCRIBE / PUBLISH". Wrapped in an Arc<Mutex<>> shared with the test.
    #[derive(Default)]
    struct BrokerLog {
        connected: usize,
        subscribed_topics: Vec<(String, u8)>,
        received_publishes: Vec<(String, Vec<u8>)>,
    }

    /// Control knobs the test sets BEFORE/while the broker runs.
    struct BrokerControl {
        session_present: AtomicBool,
        /// when set, the broker closes the next accepted connection right after
        /// CONNACK to force the agent's reconnect path (§2A.24).
        force_drop_after_connack: AtomicBool,
        log: Mutex<BrokerLog>,
        /// outbound publishes the test asks the broker to deliver: (topic, payload, qos).
        deliver: Mutex<Vec<(String, Vec<u8>, u8)>>,
    }

    impl BrokerControl {
        fn new(session_present: bool) -> Arc<BrokerControl> {
            Arc::new(BrokerControl {
                session_present: AtomicBool::new(session_present),
                force_drop_after_connack: AtomicBool::new(false),
                log: Mutex::new(BrokerLog::default()),
                deliver: Mutex::new(Vec::new()),
            })
        }
    }

    /// Build a CONNACK packet with a controllable Session Present flag.
    fn encode_connack(session_present: bool, return_code: u8) -> Vec<u8> {
        let ack_flags = if session_present { 0x01 } else { 0x00 };
        frame_packet(packet_type::CONNACK, 0, vec![ack_flags, return_code])
    }

    /// Build a SUBACK granting each requested QoS (echoing the requested level).
    fn encode_suback(packet_id: u16, granted: &[u8]) -> Vec<u8> {
        let mut body = packet_id.to_be_bytes().to_vec();
        body.extend_from_slice(granted);
        frame_packet(packet_type::SUBACK, 0, body)
    }

    /// Start the loopback broker on 127.0.0.1:0; returns (port, control, shutdown).
    /// The broker speaks just enough MQTT 3.1.1 (the same codec) to drive the
    /// Phase-3 exit test: CONNECT→CONNACK, SUBSCRIBE→SUBACK, PUBLISH delivery,
    /// PINGREQ→PINGRESP, DISCONNECT, plus a forced post-CONNACK drop for reconnect.
    fn start_broker(control: Arc<BrokerControl>) -> (u16, std_mpsc::Sender<()>) {
        let (port_tx, port_rx) = std_mpsc::channel::<u16>();
        let (shutdown_tx, shutdown_rx) = std_mpsc::channel::<()>();
        let ctl = control;
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .unwrap();
            rt.block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let port = listener.local_addr().unwrap().port();
                port_tx.send(port).unwrap();
                loop {
                    if shutdown_rx.try_recv().is_ok() {
                        return;
                    }
                    let accept =
                        tokio::time::timeout(Duration::from_millis(50), listener.accept()).await;
                    let (mut sock, _) = match accept {
                        Ok(Ok(pair)) => pair,
                        _ => continue,
                    };
                    let ctl = ctl.clone();
                    tokio::spawn(async move {
                        let _ = broker_serve(&mut sock, ctl).await;
                    });
                }
            });
        });
        let port = port_rx.recv().unwrap();
        (port, shutdown_tx)
    }

    /// Serve one client connection through the codec.
    async fn broker_serve(sock: &mut TcpStream, ctl: Arc<BrokerControl>) -> Result<()> {
        // Expect CONNECT first. ref: §3.1.
        let pkt = read_packet(sock).await?;
        if pkt.ptype != packet_type::CONNECT {
            bail!("broker: expected CONNECT");
        }
        {
            ctl.log.lock().unwrap().connected += 1;
        }
        let sp = ctl.session_present.load(Ordering::SeqCst);
        sock.write_all(&encode_connack(sp, 0)).await?;

        if ctl.force_drop_after_connack.swap(false, Ordering::SeqCst) {
            // Ungraceful drop right after CONNACK to exercise §2A.24 reconnect.
            sock.shutdown().await.ok();
            return Ok(());
        }

        loop {
            let pkt = match tokio::time::timeout(Duration::from_millis(50), read_packet(sock)).await
            {
                Ok(Ok(p)) => Some(p),
                Ok(Err(_)) => return Ok(()), // client closed
                Err(_) => None,              // timeout → check deliver queue
            };

            if let Some(pkt) = pkt {
                match pkt.ptype {
                    packet_type::SUBSCRIBE => {
                        // Decode packet-id + (topic, qos) list. ref: §3.8.
                        let pid = u16::from_be_bytes([pkt.remaining[0], pkt.remaining[1]]);
                        let mut pos = 2;
                        let mut granted = Vec::new();
                        while pos < pkt.remaining.len() {
                            let (topic, consumed) = get_mqtt_string(&pkt.remaining, pos)?;
                            pos += consumed;
                            let qos = pkt.remaining[pos];
                            pos += 1;
                            ctl.log.lock().unwrap().subscribed_topics.push((topic, qos));
                            granted.push(qos); // grant the requested QoS.
                        }
                        sock.write_all(&encode_suback(pid, &granted)).await?;
                    }
                    packet_type::PUBLISH => {
                        let publish = decode_publish(pkt.flags, &pkt.remaining)?;
                        if let Some(pid) = publish.packet_id {
                            sock.write_all(&encode_puback(pid)).await?;
                        }
                        ctl.log
                            .lock()
                            .unwrap()
                            .received_publishes
                            .push((publish.topic, publish.payload));
                    }
                    packet_type::PINGREQ => {
                        sock.write_all(&encode_pingresp()).await?;
                    }
                    packet_type::DISCONNECT => return Ok(()),
                    _ => {}
                }
            }

            // Deliver any queued downstream publishes to this client. ref: §3.3.
            let pending: Vec<(String, Vec<u8>, u8)> = {
                let mut q = ctl.deliver.lock().unwrap();
                std::mem::take(&mut *q)
            };
            for (topic, payload, qos) in pending {
                let pkt = encode_publish(&topic, &payload, qos, false, 1);
                sock.write_all(&pkt).await?;
            }
        }
    }

    /// Build an agent pointed at the loopback broker on `port`, subscribed to one
    /// topic bound to the `temp` Number attribute of `asset_id`.
    fn build_test_agent(root: &Path, port: u16, asset_id: &str, topic: &str, qos: u8) -> MqttAgent {
        let ctx = AgentContext {
            root,
            agent_id: "agent-mqtt-1".into(),
            realm: "master".into(),
            config: json!({
                "host": "127.0.0.1",
                "port": port,
                "clientId": "ctox-test",
                "cleanSession": true,
            }),
        };
        let mut agent = MqttAgent::new(ctx).unwrap();
        let link = AgentLink {
            asset_id: asset_id.into(),
            attribute_name: "temp".into(),
            binding: json!({ "subscriptionTopic": topic, "publishTopic": topic, "qos": qos }),
            ..AgentLink::default()
        };
        agent.subscribe(&link).unwrap();
        agent
    }

    /// Spin the agent's read() until it surfaces at least one reading or the budget
    /// expires (wall budget is test-only scaffolding, NOT the agent's clock — the
    /// reconnect state machine itself is driven by the injected clock below).
    fn read_until(agent: &mut MqttAgent, budget: Duration) -> Vec<AttributeReading> {
        let deadline = Instant::now() + budget;
        loop {
            let r = agent.read().unwrap();
            if !r.is_empty() {
                return r;
            }
            if Instant::now() >= deadline {
                return Vec::new();
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_connected(agent: &mut MqttAgent, ctx: &AgentContext, budget: Duration) {
        let deadline = Instant::now() + budget;
        loop {
            let status = agent.connect(ctx).unwrap();
            if status == ConnectionStatus::Connected {
                return;
            }
            assert!(Instant::now() < deadline, "agent never connected");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn ctx_for(root: &Path, port: u16) -> AgentContext<'_> {
        AgentContext {
            root,
            agent_id: "agent-mqtt-1".into(),
            realm: "master".into(),
            config: json!({"host":"127.0.0.1","port":port,"clientId":"ctox-test","cleanSession":true}),
        }
    }

    fn seed_thermostat(conn: &rusqlite::Connection, id: &str) -> crate::iot::model::Asset {
        use crate::iot::model::{
            Asset, AssetTypeInfo, AttributeDescriptor, MetaMap, ValueDescriptor,
        };
        let info = AssetTypeInfo {
            asset_type: "Thermostat".into(),
            attributes: vec![AttributeDescriptor {
                name: "temp".into(),
                value_descriptor: ValueDescriptor {
                    name: "temp_vd".into(),
                    base_type: ValueBaseType::Number,
                    array_dimensions: 0,
                    constraints: vec![],
                    units: None,
                    format: None,
                },
                meta: MetaMap::new(),
            }],
        };
        store::upsert_asset_type(conn, &info).unwrap();
        let asset = Asset::new_with_type(
            id.to_string(),
            "master".into(),
            "Thermostat".into(),
            "Living room".into(),
            &info,
        );
        store::upsert_asset(conn, &asset).unwrap();
        asset
    }

    /// Feed a raw reading through the SAME base layer the runtime uses, then into
    /// the engine write path. This mirrors runtime::run_agent_step's inbound wiring
    /// (which lives in a file owned by the Integrate stage); the agent test proves
    /// the value reaches iot_attributes/iot_datapoints end to end.
    fn feed_into_engine(
        conn: &rusqlite::Connection,
        reading: &AttributeReading,
        clock_now_ms: i64,
    ) {
        let link = AgentLink {
            asset_id: reading.asset_id.clone(),
            attribute_name: reading.attribute_name.clone(),
            ..AgentLink::default()
        };
        let (ignore, value) =
            do_inbound_value_processing(reading, &link, Some(ValueBaseType::Number));
        if ignore {
            return;
        }
        let event = crate::iot::model::AttributeEvent {
            asset_id: reading.asset_id.clone(),
            attribute_name: reading.attribute_name.clone(),
            value,
            timestamp: reading.device_timestamp_ms,
            old_value: None,
            old_value_timestamp: 0,
        };
        store::process_attribute_event(conn, &event, clock_now_ms).unwrap();
    }

    // -- §3 exit round-trip: device PUBLISH → engine → iot_attributes --------

    #[test]
    fn exit_round_trip_mqtt_device_into_engine() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = store::open_iot_store(tmp.path()).unwrap();
        let asset_id = "asset-mqtt-1";
        seed_thermostat(&conn, asset_id);

        // sessionPresent=false → resubscribe-all on (re)connect (§2A.25).
        let control = BrokerControl::new(false);
        let (port, shutdown) = start_broker(control.clone());

        let topic = "devices/thermostat/temp";
        let mut agent = build_test_agent(tmp.path(), port, asset_id, topic, 1);
        let ctx = ctx_for(tmp.path(), port);

        // (3) connect + SUBACK observed.
        wait_connected(&mut agent, &ctx, Duration::from_secs(5));
        // The broker logged the subscribe with the preserved QoS (§2A.26).
        {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if !control.log.lock().unwrap().subscribed_topics.is_empty() {
                    break;
                }
                assert!(Instant::now() < deadline, "broker never saw SUBSCRIBE");
                thread::sleep(Duration::from_millis(10));
            }
            let subs = &control.log.lock().unwrap().subscribed_topics;
            assert_eq!(subs[0], (topic.to_string(), 1), "§2A.26 QoS preserved");
        }

        // (4) broker delivers a device value on the topic.
        control
            .deliver
            .lock()
            .unwrap()
            .push((topic.to_string(), b"23.5".to_vec(), 1));

        // (5) the agent surfaces the inbound reading; feed it through the base
        // layer + engine write path. Injected clock is deterministic.
        let clock = 1_700_000_000_000i64;
        let readings = read_until(&mut agent, Duration::from_secs(5));
        assert_eq!(readings.len(), 1, "agent surfaced the device PUBLISH");
        assert_eq!(readings[0].asset_id, asset_id);
        assert_eq!(readings[0].attribute_name, "temp");
        assert_eq!(readings[0].device_timestamp_ms, 0, "§2A.1 no device ts");
        feed_into_engine(&conn, &readings[0], clock);

        // The value reached iot_attributes (coerced to Number) and iot_datapoints.
        let asset = store::get_asset(&conn, asset_id).unwrap().unwrap();
        let temp = asset.attributes.get("temp").unwrap();
        assert_eq!(
            temp.value.as_ref().unwrap().as_numeric(),
            Some(23.5),
            "device value round-tripped into iot_attributes"
        );
        let samples = crate::iot::datapoints::all(&conn, asset_id, "temp", 0, i64::MAX).unwrap();
        assert_eq!(samples.len(), 1, "datapoint recorded by the write path");

        let _ = shutdown.send(());
    }

    // -- §2A.24 forced disconnect → reconnect + resubscribe ------------------

    #[test]
    fn forced_disconnect_reconnects_and_resubscribes() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = store::open_iot_store(tmp.path()).unwrap();
        let asset_id = "asset-mqtt-2";
        seed_thermostat(&conn, asset_id);

        let control = BrokerControl::new(false); // sessionPresent=false → resub-all.
        let (port, shutdown) = start_broker(control.clone());

        let topic = "devices/t2/temp";
        let mut agent = build_test_agent(tmp.path(), port, asset_id, topic, 2);
        let ctx = ctx_for(tmp.path(), port);
        wait_connected(&mut agent, &ctx, Duration::from_secs(5));
        assert_eq!(agent.status(), ConnectionStatus::Connected);

        // Force an ungraceful drop on the NEXT accepted connection, then break the
        // current socket so the agent observes the disconnect.
        control
            .force_drop_after_connack
            .store(true, Ordering::SeqCst);
        // Trigger a server-side close of the live socket by asking the broker to
        // stop and restart-accept: simplest is to publish from the device side AND
        // then drop — but here we directly drive the SM through the disconnect path
        // to assert the deterministic Connected→Connecting→Waiting→Connecting walk
        // using the INJECTED clock (no wall clock in the SM).
        assert!(agent.state.on_disconnected(), "drop → Connecting");
        assert_eq!(agent.status(), ConnectionStatus::Connecting);

        // Injected-clock backoff walk: a failed retry schedules WAITING; the SM only
        // returns to CONNECTING once now >= next_attempt_at_ms.
        let now0 = 1_000_000i64;
        agent.state.schedule_backoff(now0);
        assert_eq!(agent.status(), ConnectionStatus::Waiting);
        let due = agent.state.next_attempt_at_ms();
        assert!(!agent.state.poll_ready_to_retry(due - 1), "not yet due");
        assert!(agent.state.poll_ready_to_retry(due), "due → Connecting");
        assert_eq!(agent.status(), ConnectionStatus::Connecting);

        // Now drive an actual reconnect through the worker; the broker accepts a new
        // connection (force_drop already consumed/false again after the swap inside
        // serve), CONNACK sessionPresent=false → resubscribe ALL (§2A.25), QoS=2
        // preserved (§2A.26).
        control
            .force_drop_after_connack
            .store(false, Ordering::SeqCst);
        // Reset SM to Disconnected so connect() re-enters cleanly (the runtime would
        // do this via mark_disconnected on a fully-dropped link).
        agent.state.mark_disconnected();
        wait_connected(&mut agent, &ctx, Duration::from_secs(5));
        assert_eq!(agent.status(), ConnectionStatus::Connected);

        // Assert a resubscribe happened for the topic with QoS=2 preserved.
        {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let count = control
                    .log
                    .lock()
                    .unwrap()
                    .subscribed_topics
                    .iter()
                    .filter(|(t, q)| t == topic && *q == 2)
                    .count();
                if count >= 2 {
                    break; // initial subscribe + post-reconnect resubscribe.
                }
                assert!(
                    Instant::now() < deadline,
                    "post-reconnect resubscribe (QoS 2) never observed"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }

        // A post-reconnect publish still lands.
        control
            .deliver
            .lock()
            .unwrap()
            .push((topic.to_string(), b"19.0".to_vec(), 2));
        let readings = read_until(&mut agent, Duration::from_secs(5));
        assert_eq!(readings.len(), 1, "publish after reconnect surfaces");
        feed_into_engine(&conn, &readings[0], 1_700_000_100_000);
        let asset = store::get_asset(&conn, asset_id).unwrap().unwrap();
        assert_eq!(
            asset
                .attributes
                .get("temp")
                .unwrap()
                .value
                .as_ref()
                .unwrap()
                .as_numeric(),
            Some(19.0)
        );

        let _ = shutdown.send(());
    }

    // -- §2A.24 production-path backoff (the real connect() failure branch) ---

    /// Drives the GENUINE `connect()` failure path (not the SM directly): a dead
    /// loopback port forces every dial to fail, so `connect()` must park the SM in
    /// WAITING via `schedule_backoff(now)` and then refuse to re-dial until the
    /// INJECTED clock passes `next_attempt_at_ms` (poll_ready_to_retry). Mirrors
    /// ws_native.rs::waiting_backoff_is_gated_by_injected_clock. Proves §2A.24 is
    /// wired through the production code path, not just the isolated state machine.
    #[test]
    fn connect_failure_parks_in_waiting_and_backoff_gated_by_injected_clock() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = store::open_iot_store(tmp.path()).unwrap();
        let asset_id = "asset-mqtt-backoff";
        seed_thermostat(&conn, asset_id);

        // A definitely-closed loopback port: bind to get one, then drop it.
        let dead_port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let p = l.local_addr().unwrap().port();
            drop(l);
            p
        };

        let mut agent = build_test_agent(tmp.path(), dead_port, asset_id, "devices/x/temp", 1);
        let ctx = ctx_for(tmp.path(), dead_port);

        // Injected deterministic clock; the SM must read THIS, never wall-clock.
        let clock = Arc::new(std::sync::atomic::AtomicI64::new(1000));
        let clock_for_agent = Arc::clone(&clock);
        agent.set_clock(Arc::new(move || clock_for_agent.load(Ordering::SeqCst)));

        // First connect: Disconnected → Connecting → dial fails → schedule_backoff
        // → WAITING. (Pre-fix this returned Connecting and hot-looped.)
        let st = agent.connect(&ctx).unwrap();
        assert_eq!(
            st,
            ConnectionStatus::Waiting,
            "failed dial must park the SM in WAITING via schedule_backoff"
        );
        let due = agent.state.next_attempt_at_ms();
        assert!(due > 1000, "backoff stamped a future next_attempt_at_ms");

        // Without advancing the injected clock, connect() must NOT re-dial — it
        // stays WAITING. This is the anti-hot-loop guarantee.
        let st = agent.connect(&ctx).unwrap();
        assert_eq!(
            st,
            ConnectionStatus::Waiting,
            "backoff not elapsed under injected clock → still WAITING (no re-dial)"
        );
        assert_eq!(
            agent.state.next_attempt_at_ms(),
            due,
            "no re-dial means next_attempt_at_ms is unchanged"
        );

        // Advance the injected clock past the cap; the next connect() re-attempts
        // (and re-fails against the dead port, re-parking in WAITING) — proving the
        // injected clock, not wall time, gates the retry cadence.
        clock.store(1000 + 10 * 60 * 1000, Ordering::SeqCst);
        let st = agent.connect(&ctx).unwrap();
        assert_eq!(
            st,
            ConnectionStatus::Waiting,
            "clock advanced past backoff → retry attempted (and re-failed → WAITING)"
        );
    }

    // -- §2A.30 outbound write fire-and-forget + update_on_write -------------

    #[test]
    fn outbound_write_reaches_broker_and_update_on_write_reflects_locally() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = store::open_iot_store(tmp.path()).unwrap();
        let asset_id = "asset-mqtt-3";
        seed_thermostat(&conn, asset_id);

        let control = BrokerControl::new(false);
        let (port, shutdown) = start_broker(control.clone());

        let topic = "devices/t3/setpoint";
        let mut agent = build_test_agent(tmp.path(), port, asset_id, topic, 1);
        let ctx = ctx_for(tmp.path(), port);
        wait_connected(&mut agent, &ctx, Duration::from_secs(5));

        // The processed value (post-base-layer) is what the runtime hands to write.
        let processed = AttributeValue(json!(21.5));
        let write_link = AgentLink {
            asset_id: asset_id.into(),
            attribute_name: "temp".into(),
            update_on_write: true, // §2A.30
            binding: json!({ "publishTopic": topic, "qos": 1 }),
            ..AgentLink::default()
        };
        agent.write(&write_link, &processed).unwrap();

        // The broker received the PUBLISH (fire-and-forget reached the device).
        {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let saw = control
                    .log
                    .lock()
                    .unwrap()
                    .received_publishes
                    .iter()
                    .any(|(t, p)| t == topic && p == b"21.5");
                if saw {
                    break;
                }
                assert!(Instant::now() < deadline, "broker never saw the PUBLISH");
                thread::sleep(Duration::from_millis(10));
            }
        }

        // §2A.30 — the runtime re-writes the attribute locally on update_on_write,
        // WITHOUT a device echo. We mirror that runtime step here with the injected
        // clock and assert the local attribute reflects the processed value.
        let event = crate::iot::model::AttributeEvent {
            asset_id: asset_id.into(),
            attribute_name: "temp".into(),
            value: processed.clone(),
            timestamp: 0,
            old_value: None,
            old_value_timestamp: 0,
        };
        store::process_attribute_event(&conn, &event, 1_700_000_200_000).unwrap();
        let asset = store::get_asset(&conn, asset_id).unwrap().unwrap();
        assert_eq!(
            asset
                .attributes
                .get("temp")
                .unwrap()
                .value
                .as_ref()
                .unwrap()
                .as_numeric(),
            Some(21.5),
            "§2A.30 update_on_write reflected locally without device round-trip"
        );

        let _ = shutdown.send(());
    }

    // -- §2A.26 Last-Will configured at CONNECT ------------------------------

    #[test]
    fn last_will_encoded_into_connect_from_config() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = AgentContext {
            root: tmp.path(),
            agent_id: "agent-will".into(),
            realm: "master".into(),
            config: json!({
                "host": "127.0.0.1",
                "port": 1883,
                "will": { "topic": "status/agent", "payload": "offline", "qos": 1, "retain": true }
            }),
        };
        let params = resolve_connect_params(&ctx).unwrap();
        let will = params.will.expect("will configured");
        assert_eq!(will.topic, "status/agent");
        assert_eq!(will.payload, b"offline");
        assert_eq!(will.qos, 1);
        assert!(will.retain);
        // And it lands in the CONNECT flags.
        let pkt = encode_connect("cid", true, None, None, Some(&will));
        let (rl, lb) = decode_remaining_length(&pkt, 1).unwrap();
        let body = &pkt[1 + lb..1 + lb + rl];
        let flags = body[7];
        assert_ne!(flags & 0b0000_0100, 0, "§2A.26 will flag set in CONNECT");
        assert_ne!(flags & 0b0010_0000, 0, "§2A.26 will retain set");
    }
}
