// Live WhatsApp pairing — opens a tiny HTTP server on http://localhost:9090
// that renders the rotating QR code in your browser, then connects to
// wss://web.whatsapp.com/ws/chat and runs the real pairing flow.
//
// Usage:
//   cargo run --example pair_live
//   open http://localhost:9090 in your browser
//   open WhatsApp on your phone -> Settings -> Linked Devices -> Link a Device
//   scan the QR
//
// Endpoints:
//   GET /          → HTML page that polls /qr.svg + /status
//   GET /qr.svg    → current QR code as SVG (200), or 204 No Content if
//                    nothing to scan (yet) or pairing is finished
//   GET /status    → "waiting" | "qr" | "paired:<jid>" | "error:<msg>"
//
// Manual routing note: dispatch_node in client.rs does not (yet) auto-route
// inbound <iq><pair-device> / <iq><pair-success>. We dispatch them here from
// the Event::UnhandledNode branch.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use qrcode::render::svg;
use qrcode::QrCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use whatsapp::client::{pair, Client, Event};
use whatsapp::store::sqlite::SqliteStore;

/// Where the persisted device blob lives. Mirrors whatsmeow's behaviour of
/// keeping the device DB at a stable on-disk path so subsequent runs resume
/// the existing pairing instead of re-running the QR flow.
const DEVICE_DB_PATH: &str = "/tmp/wha-rust-pair.sqlite";

fn type_summary(m: &wha_proto::e2e::Message) -> String {
    let mut parts = Vec::new();
    if m.conversation.is_some() { parts.push("conversation"); }
    if m.image_message.is_some() { parts.push("image"); }
    if m.video_message.is_some() { parts.push("video"); }
    if m.audio_message.is_some() { parts.push("audio"); }
    if m.document_message.is_some() { parts.push("document"); }
    if m.sticker_message.is_some() { parts.push("sticker"); }
    if m.contact_message.is_some() { parts.push("contact"); }
    if m.location_message.is_some() { parts.push("location"); }
    if m.protocol_message.is_some() { parts.push("protocol"); }
    if m.reaction_message.is_some() { parts.push("reaction"); }
    if m.sender_key_distribution_message.is_some() { parts.push("skdm"); }
    if parts.is_empty() { "(unknown)".into() } else { parts.join(",") }
}

#[derive(Debug, Clone)]
enum PairStatus {
    Connecting,
    Waiting,
    Qr(String),                              // raw QR string, ready for SVG rendering
    Paired(String),                          // JID, but not yet logged in
    LoggedIn(String),                        // JID, logged-in connection live
    GotMessage { from: String, body: String },// proof we can read messages
    Error(String),
}

type SharedStatus = Arc<RwLock<PairStatus>>;
/// Set to Some(client) once Phase 2 is live; None otherwise. The /send
/// HTTP endpoint reads through this — sending before Phase 2 → HTTP 503.
type SharedClient = Arc<RwLock<Option<Arc<whatsapp::client::Client>>>>;

/// Append-only ring of human-readable log lines, exposed via HTTP at /log.
struct LogBuffer {
    lines: Vec<(String, String)>, // (timestamp, message)
}
type SharedLog = Arc<RwLock<LogBuffer>>;

/// In-memory cache of recently-received `<message>` bodies so the
/// `GET /media/<msg_id>` HTTP endpoint can find them again. We keep it as a
/// fixed-size ring of (id, decoded_message) pairs — no persistence, so a
/// process restart loses everything (which is fine for an example).
const MEDIA_RING_CAP: usize = 64;
struct MediaRing {
    items: Vec<(String, wha_proto::e2e::Message)>,
}
impl MediaRing {
    fn new() -> Self {
        Self {
            items: Vec::with_capacity(MEDIA_RING_CAP),
        }
    }
    fn push(&mut self, id: String, msg: wha_proto::e2e::Message) {
        // De-dup on id so a re-delivery doesn't leave two copies behind.
        self.items.retain(|(k, _)| k != &id);
        self.items.push((id, msg));
        if self.items.len() > MEDIA_RING_CAP {
            let drop = self.items.len() - MEDIA_RING_CAP;
            self.items.drain(..drop);
        }
    }
    fn get(&self, id: &str) -> Option<&wha_proto::e2e::Message> {
        self.items.iter().find(|(k, _)| k == id).map(|(_, v)| v)
    }
}
type SharedMediaRing = Arc<RwLock<MediaRing>>;

fn now_hms() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let s = now % 60;
    let m = (now / 60) % 60;
    let h = (now / 3600) % 24;
    format!("{h:02}:{m:02}:{s:02}")
}

fn log_push(log: &SharedLog, msg: impl Into<String>) {
    let m = msg.into();
    println!("[pair_live] {m}");
    let mut w = log.write();
    w.lines.push((now_hms(), m));
    if w.lines.len() > 200 {
        let drop = w.lines.len() - 200;
        w.lines.drain(..drop);
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    // rustls 0.23 requires an explicit crypto provider when multiple are
    // available (aws-lc-rs vs ring). Without this the websocket dial panics.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install rustls crypto provider");

    // Wire up tracing so we see post-handshake decrypt warnings + frame
    // boundaries. This is what told us where the drop happens.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .init();

    let status: SharedStatus = Arc::new(RwLock::new(PairStatus::Connecting));
    let log: SharedLog = Arc::new(RwLock::new(LogBuffer { lines: Vec::new() }));
    let shared_client: SharedClient = Arc::new(RwLock::new(None));
    let media_ring: SharedMediaRing = Arc::new(RwLock::new(MediaRing::new()));

    // NOTE: WA's `check-update` endpoint reports the smartphone-bridge web
    // version (e.g. 2.2413.51), not the multi-device version we need.
    // Multi-device wants the (2, 3000, X) series — use the compile-time
    // constant whatsmeow ships.
    println!(
        "[pair_live] using WA Web version: {}",
        wha_client::payload::WA_VERSION
    );

    // Spawn the HTTP server first so the user can open the browser
    // before the websocket dance even starts.
    let http_status = status.clone();
    let http_log = log.clone();
    let http_client = shared_client.clone();
    let http_media = media_ring.clone();
    let http_handle = tokio::spawn(async move {
        if let Err(e) = run_http_server(http_status, http_log, http_client, http_media).await {
            eprintln!("[pair_live] HTTP server error: {e}");
        }
    });

    println!();
    println!("┌─────────────────────────────────────────────────┐");
    println!("│  Open http://localhost:9090 in your browser.    │");
    println!("│  Then on your phone: WhatsApp → Settings →      │");
    println!("│  Linked Devices → Link a Device → scan the QR.  │");
    println!("└─────────────────────────────────────────────────┘");
    println!();

    // Open (or create) the on-disk device store. If an already-paired
    // device is sitting in the DB we skip Phase 1 (the QR flow) and
    // jump straight to Phase 2.
    let store = match SqliteStore::open(DEVICE_DB_PATH) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("[pair_live] failed to open device DB at {DEVICE_DB_PATH}: {e}");
            return;
        }
    };
    log_push(&log, format!("device DB at {DEVICE_DB_PATH}"));

    let preloaded = match store.load_device().await {
        Ok(opt) => opt,
        Err(e) => {
            log_push(&log, format!("load_device errored: {e}; treating as fresh"));
            None
        }
    };

    // Phase 1: only run if we don't already have a paired device on disk.
    // `id.is_some()` is the exact predicate whatsmeow uses to decide
    // "device is paired" — that field is only populated by handle_pair_success.
    let paired_device = match preloaded {
        Some(dev) if dev.id.is_some() => {
            let jid = dev.id.as_ref().unwrap().to_string();
            log_push(
                &log,
                format!("loaded paired device for {jid} from {DEVICE_DB_PATH}; skipping QR"),
            );
            *status.write() = PairStatus::Paired(jid);
            dev
        }
        _ => {
            // Either no row yet, or a row exists but pairing never finished
            // (id is None) — pump the QR flow until success.
            let dev = loop {
                let attempt_device = match preloaded.as_ref() {
                    // Re-use the unpaired device on the first iteration so we
                    // keep the same identity / signed pre-key id across the
                    // pairing flow. Subsequent iterations always mint fresh.
                    Some(d) if d.id.is_none() => d.clone(),
                    _ => store.new_device(),
                };
                match run_pairing(status.clone(), log.clone(), attempt_device).await {
                    Ok(Some(device)) => break device,
                    Ok(None) => {
                        log_push(&log, "pairing pump exited without success; reconnecting…");
                    }
                    Err(e) => {
                        log_push(&log, format!("pairing errored: {e}; reconnect in 2s"));
                        *status.write() = PairStatus::Error(e);
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            };

            // Persist the freshly-paired device so subsequent runs skip Phase 1.
            if let Err(e) = store.save_device(&dev).await {
                log_push(&log, format!("save_device errored: {e}"));
            } else {
                log_push(&log, "saved paired device blob to disk");
            }
            dev
        }
    };

    // Phase 2: re-connect as logged-in client and listen for one real message.
    log_push(&log, "PAIRED. Phase 2: re-connecting as logged-in client…");
    if let Err(e) = run_logged_in(
        status.clone(),
        log.clone(),
        paired_device,
        shared_client.clone(),
        media_ring.clone(),
    )
    .await
    {
        log_push(&log, format!("logged-in errored: {e}"));
        *status.write() = PairStatus::Error(e);
    }

    // Keep page up so user can read final state.
    println!("[pair_live] holding HTTP server up — Ctrl-C to exit.");
    let _ = http_handle.await;
}

/// Returns Ok(Some(device)) on successful pair (with the paired device, JID
/// and identity already mutated in), Ok(None) on clean shutdown of the
/// event channel without pair, Err(msg) on disconnect/stream-error before
/// pairing.
async fn run_pairing(
    status: SharedStatus,
    log: SharedLog,
    device: whatsapp::store::Device,
) -> Result<Option<whatsapp::store::Device>, String> {
    log_push(&log, "Phase 1: opening connection with sqlite-backed device…");
    let (mut client, mut events) = Client::new(device);

    log_push(&log, "opening websocket to wss://web.whatsapp.com/ws/chat …");
    log_push(&log, "running Noise XX handshake + sending ClientPayload (registration)…");
    client.connect().await.map_err(|e| format!("connect failed: {e}"))?;
    log_push(&log, "handshake complete; waiting for <iq><pair-device> from server…");
    *status.write() = PairStatus::Waiting;

    // Subscribe to the focused QR/pair-result side-channel exposed by
    // [`Client::get_qr_channel`] (mirrors upstream's `GetQRChannel`). The
    // main `events` loop below still drives `<iq><pair-device>` /
    // `<iq><pair-success>` routing — the QR channel is just a high-level
    // "what should I show the user right now" stream that we tee onto the
    // status bar.
    {
        let qr_log = log.clone();
        let qr_status = status.clone();
        let mut qr_chan = client.get_qr_channel();
        tokio::spawn(async move {
            while let Some(evt) = qr_chan.recv().await {
                match evt {
                    wha_client::qr_channel::QrEvent::Code(_) => {
                        log_push(&qr_log, "qr_channel: new QR code");
                    }
                    wha_client::qr_channel::QrEvent::Success => {
                        log_push(&qr_log, "qr_channel: pair success");
                    }
                    wha_client::qr_channel::QrEvent::Timeout => {
                        log_push(&qr_log, "qr_channel: timed out — user did not scan in time");
                        *qr_status.write() = PairStatus::Error("qr-timeout".into());
                    }
                    wha_client::qr_channel::QrEvent::Error(e) => {
                        log_push(&qr_log, format!("qr_channel: error {e}"));
                    }
                }
            }
        });
    }

    while let Some(evt) = events.recv().await {
        match evt {
            Event::Connected => {
                log_push(&log, "event: Connected");
            }
            Event::QrCode { code } => {
                log_push(&log, "new QR ref → page refreshes");
                *status.write() = PairStatus::Qr(code);
            }
            Event::PairSuccess { id } => {
                let jid = id.to_string();
                log_push(&log, format!("event: PairSuccess id={jid} (handle_pair_success done, ack sent)"));
                *status.write() = PairStatus::Paired(jid);
                return Ok(Some(client.device.clone()));
            }
            Event::Disconnected { reason } => {
                log_push(&log, format!("event: Disconnected ({reason})"));
                return Err(format!("Disconnected: {reason}"));
            }
            Event::StreamError { code, text } => {
                log_push(&log, format!("event: StreamError code={code} text={text}"));
                return Err(format!("StreamError code={code} text={text}"));
            }
            Event::UnhandledNode { node } => {
                if node.tag == "iq" {
                    if node.child_by_tag(&["pair-device"]).is_some() {
                        log_push(&log, "got <iq><pair-device>; routing to handle_pair_device");
                        pair::handle_pair_device(&client, &node)
                            .await
                            .map_err(|e| format!("handle_pair_device: {e}"))?;
                        continue;
                    }
                    if node.child_by_tag(&["pair-success"]).is_some() {
                        log_push(&log, "got <iq><pair-success>; running handle_pair_success (HMAC + ADV verify + sign + ack)");
                        pair::handle_pair_success(&mut client, &node)
                            .await
                            .map_err(|e| format!("handle_pair_success: {e}"))?;
                        continue;
                    }
                    // Server keepalive ping — must be answered or WA drops us.
                    if node.get_attr_str("xmlns") == Some("urn:xmpp:ping")
                        && node.get_attr_str("type") == Some("get")
                    {
                        let mut attrs = wha_binary::Attrs::new();
                        if let Some(id) = node.get_attr_str("id") {
                            attrs.insert("id".into(), wha_binary::Value::String(id.to_owned()));
                        }
                        if let Some(from) = node.attrs.get("from") {
                            attrs.insert("to".into(), from.clone());
                        }
                        attrs.insert("type".into(), wha_binary::Value::String("result".into()));
                        let pong = wha_binary::Node::new("iq", attrs, None);
                        if let Err(e) = client.send_node(&pong).await {
                            eprintln!("[pair_live] failed to pong: {e}");
                        }
                        continue;
                    }
                }
                println!(
                    "[pair_live] unhandled <{}> attrs={:?} children={}",
                    node.tag,
                    node.attrs,
                    node.children().len()
                );
                for c in node.children() {
                    println!(
                        "    <{}> attrs={:?} content_kind={}",
                        c.tag,
                        c.attrs,
                        match &c.content {
                            wha_binary::Value::None => "none",
                            wha_binary::Value::Nodes(_) => "nodes",
                            wha_binary::Value::Bytes(_) => "bytes",
                            wha_binary::Value::String(_) => "str",
                            wha_binary::Value::Jid(_) => "jid",
                        }
                    );
                }
            }
            _ => {
                // Other typed events (notification-derived, offline-sync,
                // dirty notifications, …) are irrelevant to the pair flow.
            }
        }
    }

    Ok(None)
}

/// Phase 2: connect as a logged-in client (device.id is Some, so the payload
/// builder branches to login automatically) and wait for the first real
/// `<message>` stanza. Prints it and exits — that's the proof end-to-end.
async fn run_logged_in(
    status: SharedStatus,
    log: SharedLog,
    device: whatsapp::store::Device,
    shared_client: SharedClient,
    media_ring: SharedMediaRing,
) -> Result<(), String> {
    log_push(&log, "Phase 2: waiting 1s for WA's post-pair disconnect to settle…");
    tokio::time::sleep(Duration::from_secs(1)).await;

    let (mut client, mut events) = Client::new(device);

    // "set push name on connect": ensure the device carries a non-empty
    // push name *before* we open the socket. WA refuses to send presence
    // (`ErrNoPushName`) without one, and the value gets echoed back on
    // every `<presence type="available">`.
    if client.device.push_name.is_empty() {
        client.device.push_name = "WhatsApp Rust".to_owned();
        log_push(
            &log,
            "device.push_name was empty; defaulting to 'WhatsApp Rust'",
        );
    } else {
        log_push(
            &log,
            format!("using existing device.push_name = {:?}", client.device.push_name),
        );
    }

    log_push(&log, "Phase 2: re-opening websocket (login mode, passive=true)…");
    client.connect().await.map_err(|e| format!("logged-in connect: {e}"))?;
    let jid_str = client
        .device
        .jid()
        .map(|j| j.to_string())
        .unwrap_or_else(|| "?".into());
    *status.write() = PairStatus::LoggedIn(jid_str.clone());
    log_push(&log, format!("logged in as {jid_str}; waiting for inbound <message>…"));

    // Share client between the main event loop and the HTTP /send endpoint.
    // The keepalive loop is now spawned automatically by `Client::connect`
    // (see `wha_client::keepalive::spawn_keepalive_loop`) — this example no
    // longer needs an inline ticker.
    let client = Arc::new(client);
    *shared_client.write() = Some(client.clone());

    while let Some(evt) = events.recv().await {
        match evt {
            Event::Connected => {
                log_push(&log, "phase-2 event: Connected");
            }
            Event::QrCode { code } => {
                log_push(&log, "phase-2: second QR ref arrived");
                *status.write() = PairStatus::Qr(code);
            }
            Event::PairSuccess { id } => {
                let jid = id.to_string();
                log_push(&log, format!("phase-2 PairSuccess id={jid}"));
                *status.write() = PairStatus::LoggedIn(jid);
            }
            Event::Disconnected { reason } => {
                log_push(&log, format!("phase-2 Disconnected: {reason}"));
                return Err(format!("logged-in disconnected: {reason}"));
            }
            Event::StreamError { code, text } => {
                log_push(&log, format!("phase-2 StreamError code={code} text={text}"));
                return Err(format!("logged-in stream:error code={code} text={text}"));
            }
            Event::UnhandledNode { node } => {
                // 0) WA may send a SECOND pair-device IQ after login (some
                //    flows show two QRs in sequence). Route it the same way
                //    as phase 1.
                if node.tag == "iq" && node.child_by_tag(&["pair-device"]).is_some() {
                    log_push(&log, "phase-2: another pair-device — handing back to handle_pair_device");
                    if let Err(e) = pair::handle_pair_device(&client, &node).await {
                        return Err(format!("phase-2 handle_pair_device: {e}"));
                    }
                    continue;
                }

                // <success> means we're logged in — WA expects us to send
                // `<iq xmlns="passive" type="set"><active/></iq>` to confirm
                // the device is ready. Without this the linking phone stays
                // on "wird angemeldet" / "logging in" forever.
                if node.tag == "success" {
                    log_push(&log, "got <success>; sending <iq xmlns=passive><active/></iq>");
                    let mut iq_attrs = wha_binary::Attrs::new();
                    iq_attrs.insert(
                        "id".into(),
                        wha_binary::Value::String(format!("act{}", rand::random::<u32>())),
                    );
                    iq_attrs.insert(
                        "to".into(),
                        wha_binary::Value::Jid(wha_types::Jid::new(
                            "",
                            wha_types::jid::server::DEFAULT_USER,
                        )),
                    );
                    iq_attrs.insert("type".into(), wha_binary::Value::String("set".into()));
                    iq_attrs.insert("xmlns".into(), wha_binary::Value::String("passive".into()));
                    let iq = wha_binary::Node::new(
                        "iq",
                        iq_attrs,
                        Some(wha_binary::Value::Nodes(vec![wha_binary::Node::tag_only("active")])),
                    );
                    if let Err(e) = client.send_node(&iq).await {
                        log_push(&log, format!("active-iq send failed: {e}"));
                    } else {
                        log_push(&log, "active-iq sent — phone should leave 'logging in' now");
                    }

                    log_push(&log, "uploading one-time pre-keys to server…");
                    let before = client.device.pre_keys.uploaded_pre_key_count().await.unwrap_or(0);
                    match wha_client::prekeys::upload_pre_keys(&client).await {
                        Ok(()) => {
                            let after = client.device.pre_keys.uploaded_pre_key_count().await.unwrap_or(0);
                            log_push(&log, format!("pre-keys uploaded; store count {before}→{after}"));
                            // probe ALL ids 1..=100 to see exactly what we have
                            let mut have = Vec::new();
                            let mut miss = Vec::new();
                            for id in 1u32..=100 {
                                match client.device.pre_keys.get_pre_key(id).await {
                                    Ok(Some(_)) => have.push(id),
                                    Ok(None) => { if id <= 60 { miss.push(id); } }
                                    Err(_) => {}
                                }
                            }
                            log_push(&log, format!("store probe HAVE: {have:?}"));
                            if !miss.is_empty() {
                                log_push(&log, format!("store probe MISS in 1..60: {miss:?}"));
                            }
                        }
                        Err(e) => log_push(&log, format!("prekey upload failed: {e}")),
                    }

                    // Send <presence type="available" name="..."> so the server
                    // marks us active and starts fanning out messages. The
                    // `name` attribute is filled from `device.push_name`,
                    // which we ensured was non-empty up-top via the
                    // "set push name on connect" step.
                    use wha_client::presence_receipt::PresenceState;
                    if let Err(e) = client.send_presence(PresenceState::Available).await {
                        log_push(&log, format!("presence send failed: {e}"));
                    } else {
                        log_push(
                            &log,
                            format!(
                                "sent <presence type=available name={:?}> — server can now fan out messages",
                                client.device.push_name
                            ),
                        );
                    }

                    continue;
                }

                // 1) keepalive ping
                if node.tag == "iq"
                    && node.get_attr_str("xmlns") == Some("urn:xmpp:ping")
                    && node.get_attr_str("type") == Some("get")
                {
                    let mut attrs = wha_binary::Attrs::new();
                    if let Some(id) = node.get_attr_str("id") {
                        attrs.insert("id".into(), wha_binary::Value::String(id.to_owned()));
                    }
                    if let Some(from) = node.attrs.get("from") {
                        attrs.insert("to".into(), from.clone());
                    }
                    attrs.insert("type".into(), wha_binary::Value::String("result".into()));
                    let pong = wha_binary::Node::new("iq", attrs, None);
                    let _ = client.send_node(&pong).await;
                    continue;
                }

                if node.tag == "iq"
                    && node.get_attr_str("xmlns") == Some("urn:xmpp:ping")
                {
                    log_push(&log, "ponged server ping");
                }

                // 2) ack notifications so the server keeps sending us things
                if node.tag == "notification" {
                    let mut ack_attrs = wha_binary::Attrs::new();
                    if let Some(id) = node.get_attr_str("id") {
                        ack_attrs.insert("id".into(), wha_binary::Value::String(id.to_owned()));
                    }
                    ack_attrs.insert("class".into(), wha_binary::Value::String("notification".into()));
                    if let Some(from) = node.attrs.get("from") {
                        ack_attrs.insert("to".into(), from.clone());
                    }
                    if let Some(t) = node.get_attr_str("type") {
                        ack_attrs.insert("type".into(), wha_binary::Value::String(t.to_owned()));
                    }
                    let ack = wha_binary::Node::new("ack", ack_attrs, None);
                    let _ = client.send_node(&ack).await;
                    log_push(&log, format!("acked <notification type={:?}>", node.get_attr_str("type")));
                    continue;
                }

                // 3) THE proof — a real <message> arrived. Try to decrypt
                //    via the wha-client recv_message path so the user sees
                //    plaintext rather than the encrypted blob. The
                //    `recv_message` layer now sends the matching <ack
                //    class="message"> + <receipt> on a successful decrypt
                //    (mirror of `sendMessageReceipt` + `sendAck` upstream),
                //    so we no longer ack here.
                if node.tag == "message" {
                    log_push(&log, format!("inbound <message> id={:?}", node.get_attr_str("id")));
                    match wha_client::recv_message::handle_encrypted_message(&client, &node).await {
                        Ok(dec) => {
                            // dec.plaintext is a proto-encoded
                            // wha_proto::e2e::Message — decode the body for
                            // display.
                            let decoded =
                                <wha_proto::e2e::Message as prost::Message>::decode(
                                    dec.plaintext.as_slice(),
                                );
                            let preview = match &decoded {
                                Ok(m) => {
                                    if let Some(c) = m.conversation.clone() {
                                        format!("text: {c}")
                                    } else if let Some(et) = m.extended_text_message.as_ref() {
                                        format!("text: {}", et.text.as_deref().unwrap_or("(empty)"))
                                    } else {
                                        format!(
                                            "non-text message types_set={}",
                                            type_summary(m)
                                        )
                                    }
                                }
                                Err(e) => format!("(could not decode protobuf body: {e})"),
                            };
                            log_push(&log, format!("DECRYPTED from {}: {}", dec.from, preview));
                            *status.write() = PairStatus::GotMessage {
                                from: dec.from.to_string(),
                                body: preview.clone(),
                            };

                            // Stash messages with downloadable media in the
                            // ring so the GET /media/<msg_id> HTTP endpoint
                            // can find them. We key on the wire <message id="…">
                            // so the URL the user types matches what
                            // recv_message sees.
                            if let Ok(m) = decoded.as_ref() {
                                let has_media = m.image_message.is_some()
                                    || m.video_message.is_some()
                                    || m.audio_message.is_some()
                                    || m.document_message.is_some()
                                    || m.sticker_message.is_some();
                                if has_media {
                                    if let Some(id) = node.get_attr_str("id") {
                                        media_ring
                                            .write()
                                            .push(id.to_owned(), m.clone());
                                        log_push(
                                            &log,
                                            format!(
                                                "cached media-bearing message id={id} (ring size={})",
                                                media_ring.read().items.len()
                                            ),
                                        );
                                    }
                                }
                            }

                            // History-sync hook. WhatsApp ships its initial
                            // chat archive as
                            // `protocolMessage.historySyncNotification` —
                            // an external-blob pointer that we have to
                            // refresh-mediaConn → download → decrypt →
                            // inflate → decode. The result is a
                            // `wha_proto::history_sync::HistorySync` proto
                            // carrying conversations, status, push-names,
                            // etc.
                            if let Ok(m) = decoded {
                                if let Some(pm) = m.protocol_message.as_ref() {
                                    if let Some(notif) = pm.history_sync_notification.as_ref() {
                                        log_push(
                                            &log,
                                            "history sync notification received, downloading…",
                                        );
                                        match wha_client::history_sync::handle_history_sync_notification(
                                            &client, notif,
                                        )
                                        .await
                                        {
                                            Ok(hs) => log_push(
                                                &log,
                                                format!(
                                                    "HistorySync syncType={:?} conversations={} pushnames={}",
                                                    hs.sync_type(),
                                                    hs.conversations.len(),
                                                    hs.pushnames.len()
                                                ),
                                            ),
                                            Err(e) => log_push(
                                                &log,
                                                format!("history sync download/decode failed: {e}"),
                                            ),
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log_push(
                                &log,
                                format!("decrypt failed for inbound message: {e}"),
                            );
                            // Decrypt failed — `recv_message` did NOT send an
                            // ack, but we still want the server to stop
                            // retrying. Send a bare <ack class="message"> so
                            // the stream stays alive.
                            let mut ack_attrs = wha_binary::Attrs::new();
                            if let Some(id) = node.get_attr_str("id") {
                                ack_attrs.insert("id".into(), wha_binary::Value::String(id.to_owned()));
                            }
                            ack_attrs.insert("class".into(), wha_binary::Value::String("message".into()));
                            if let Some(from) = node.attrs.get("from") {
                                ack_attrs.insert("to".into(), from.clone());
                            }
                            let ack = wha_binary::Node::new("ack", ack_attrs, None);
                            let _ = client.send_node(&ack).await;
                        }
                    }
                    continue;
                }
                if false && node.tag == "message" {
                    let from = node
                        .attrs
                        .get("from")
                        .map(|v| match v {
                            wha_binary::Value::Jid(j) => j.to_string(),
                            wha_binary::Value::String(s) => s.clone(),
                            _ => "?".to_string(),
                        })
                        .unwrap_or_else(|| "?".to_string());
                    let body_summary = format!(
                        "tag=message from={} children={} attrs={:?}",
                        from,
                        node.children().len(),
                        node.attrs.keys().collect::<Vec<_>>()
                    );
                    log_push(&log, format!("GOT MESSAGE from {from}"));
                    log_push(&log, format!("full node: {:?}", node));
                    *status.write() = PairStatus::GotMessage {
                        from,
                        body: body_summary,
                    };
                    // ack the message so the server keeps the stream alive
                    let mut ack_attrs = wha_binary::Attrs::new();
                    if let Some(id) = node.get_attr_str("id") {
                        ack_attrs.insert("id".into(), wha_binary::Value::String(id.to_owned()));
                    }
                    ack_attrs.insert("class".into(), wha_binary::Value::String("message".into()));
                    if let Some(from) = node.attrs.get("from") {
                        ack_attrs.insert("to".into(), from.clone());
                    }
                    let ack = wha_binary::Node::new("ack", ack_attrs, None);
                    let _ = client.send_node(&ack).await;
                    // keep loop running so user can see further messages
                    continue;
                }

                // <ib><dirty type="..." timestamp="..."/></ib>: server says
                // some app-state collection is out of sync. We don't actually
                // re-sync app state yet, but we mark it not-dirty so the
                // phone unblocks the per-device fan-out.
                if node.tag == "ib" {
                    if let Some(dirty) = node.children().iter().find(|c| c.tag == "dirty") {
                        let ts = dirty.get_attr_str("timestamp").unwrap_or("").to_string();
                        let typ = dirty.get_attr_str("type").unwrap_or("").to_string();
                        let mut iq_attrs = wha_binary::Attrs::new();
                        iq_attrs.insert(
                            "id".into(),
                            wha_binary::Value::String(format!("clean{}", rand::random::<u32>())),
                        );
                        iq_attrs.insert(
                            "to".into(),
                            wha_binary::Value::Jid(wha_types::Jid::new(
                                "",
                                wha_types::jid::server::DEFAULT_USER,
                            )),
                        );
                        iq_attrs.insert("type".into(), wha_binary::Value::String("set".into()));
                        iq_attrs.insert(
                            "xmlns".into(),
                            wha_binary::Value::String("urn:xmpp:whatsapp:dirty".into()),
                        );
                        let mut clean_attrs = wha_binary::Attrs::new();
                        clean_attrs
                            .insert("type".into(), wha_binary::Value::String(typ.clone()));
                        clean_attrs
                            .insert("timestamp".into(), wha_binary::Value::String(ts.clone()));
                        let clean = wha_binary::Node::new("clean", clean_attrs, None);
                        let iq = wha_binary::Node::new(
                            "iq",
                            iq_attrs,
                            Some(wha_binary::Value::Nodes(vec![clean])),
                        );
                        let _ = client.send_node(&iq).await;
                        log_push(
                            &log,
                            format!("MarkNotDirty type={typ} ts={ts}"),
                        );
                        continue;
                    }
                }

                // Server-pushed <iq type="get"> we don't recognize: respond
                // with an empty <iq type="result"> so the server doesn't
                // wait on us before fanning out messages. Mirrors the
                // "respond to all server gets" minimum protocol.
                if node.tag == "iq"
                    && node.get_attr_str("type") == Some("get")
                    && node.get_attr_str("t").is_none()
                {
                    let mut attrs = wha_binary::Attrs::new();
                    if let Some(id) = node.get_attr_str("id") {
                        attrs.insert("id".into(), wha_binary::Value::String(id.to_owned()));
                    }
                    if let Some(from) = node.attrs.get("from") {
                        attrs.insert("to".into(), from.clone());
                    }
                    attrs.insert("type".into(), wha_binary::Value::String("result".into()));
                    let resp = wha_binary::Node::new("iq", attrs, None);
                    let _ = client.send_node(&resp).await;
                    let child_tags: Vec<&str> = node
                        .children()
                        .iter()
                        .map(|c| c.tag.as_str())
                        .collect();
                    log_push(
                        &log,
                        format!(
                            "answered server <iq type=get> with empty result; xmlns={:?} children={:?}",
                            node.get_attr_str("xmlns"),
                            child_tags
                        ),
                    );
                    continue;
                }

                // For any other unhandled phase-2 node, dump tag + all attrs
                // (key=value) and child tags+attrs so we see what the server
                // is sending. This is the key diagnostic for understanding
                // why messages aren't arriving.
                let render_attrs = |n: &wha_binary::Node| -> String {
                    n.attrs
                        .iter()
                        .map(|(k, v)| {
                            let v_str = match v {
                                wha_binary::Value::String(s) => s.clone(),
                                wha_binary::Value::Jid(j) => j.to_string(),
                                wha_binary::Value::Bytes(b) => format!("{}b", b.len()),
                                wha_binary::Value::Nodes(_) => "[nodes]".to_string(),
                                _ => "?".to_string(),
                            };
                            format!("{k}={v_str}")
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                };
                log_push(
                    &log,
                    format!(
                        "phase-2 UNHANDLED <{} {}> children=[{}]",
                        node.tag,
                        render_attrs(&node),
                        node.children()
                            .iter()
                            .map(|c| format!("{}({})", c.tag, render_attrs(c)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

// -------- minimal HTTP server -----------------------------------------------

async fn run_http_server(
    status: SharedStatus,
    log: SharedLog,
    shared_client: SharedClient,
    media_ring: SharedMediaRing,
) -> std::io::Result<()> {
    let addr: SocketAddr = "127.0.0.1:9090".parse().unwrap();
    let listener = TcpListener::bind(addr).await?;
    println!("[pair_live] HTTP server listening on http://localhost:9090");
    loop {
        let (mut sock, _peer) = listener.accept().await?;
        let status = status.clone();
        let log = log.clone();
        let client_handle = shared_client.clone();
        let media_handle = media_ring.clone();
        tokio::spawn(async move {
            // Read the head + as much body as we can in one go. For our
            // tiny POST (~80 bytes JSON) this is enough; a real server
            // would loop until Content-Length is satisfied.
            let mut buf = [0u8; 8192];
            let n = match sock.read(&mut buf).await {
                Ok(n) => n,
                Err(_) => return,
            };
            let raw = &buf[..n];
            // Split head/body at the first \r\n\r\n.
            let (head_bytes, body_bytes) = match find_double_crlf(raw) {
                Some(off) => (&raw[..off], &raw[off + 4..]),
                None => (raw, &raw[raw.len()..]),
            };
            let head = String::from_utf8_lossy(head_bytes);
            let mut lines = head.lines();
            let request_line = lines.next().unwrap_or("");
            let mut parts = request_line.split_whitespace();
            let method = parts.next().unwrap_or("GET").to_owned();
            let path = parts.next().unwrap_or("/").to_owned();

            let response = match (method.as_str(), path.as_str()) {
                ("GET", "/") => index_response(),
                ("GET", "/qr.svg") => qr_svg_response(&status),
                ("GET", "/status") => status_response(&status),
                ("GET", "/log") => log_response(&log),
                ("POST", "/send") => {
                    send_endpoint(&log, &client_handle, body_bytes).await
                }
                ("POST", "/react") => {
                    react_endpoint(&log, &client_handle, body_bytes).await
                }
                ("POST", "/reply") => {
                    reply_endpoint(&log, &client_handle, body_bytes).await
                }
                ("POST", "/revoke") => {
                    revoke_endpoint(&log, &client_handle, body_bytes).await
                }
                ("POST", "/newsletter/send") => {
                    newsletter_send_endpoint(&log, &client_handle, body_bytes).await
                }
                ("GET", p) if p.starts_with("/media/") => {
                    let msg_id = p.trim_start_matches("/media/").to_owned();
                    media_endpoint(&log, &client_handle, &media_handle, &msg_id).await
                }
                _ => http_response(404, "text/plain", b"not found"),
            };
            let _ = sock.write_all(&response).await;
        });
    }
}

/// Find the byte offset of the first `\r\n\r\n` in `buf`, marking the
/// transition from HTTP head to body.
fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// `POST /send` handler — body is JSON `{"to":"...", "body":"..."}`.
/// Returns 503 before Phase 2; 400 on parse error; 200 + JSON body
/// `{"id":"..."}` on success; 500 + JSON `{"error":"..."}` on send error.
async fn send_endpoint(
    log: &SharedLog,
    shared_client: &SharedClient,
    body: &[u8],
) -> Vec<u8> {
    // Need a live, logged-in client.
    let client = match shared_client.read().clone() {
        Some(c) => c,
        None => {
            log_push(log, "POST /send rejected: client not in Phase 2 yet");
            return http_response(
                503,
                "application/json",
                br#"{"error":"client not yet in phase 2 (login)"}"#,
            );
        }
    };

    #[derive(serde::Deserialize)]
    struct SendReq {
        to: String,
        body: String,
    }
    let parsed: SendReq = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid json: {e}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    let to_jid: wha_types::Jid = match parsed.to.parse() {
        Ok(j) => j,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid jid: {e:?}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    log_push(
        log,
        format!("POST /send to={} body={}b", to_jid, parsed.body.len()),
    );
    match wha_client::send_message::send_text(&client, &to_jid, &parsed.body).await {
        Ok(id) => {
            log_push(log, format!("POST /send ok id={id}"));
            let body = format!(r#"{{"id":"{id}"}}"#);
            http_response(200, "application/json", body.as_bytes())
        }
        Err(e) => {
            log_push(log, format!("POST /send err {e}"));
            // JSON-quote the error string conservatively (no control chars
            // or quote chars survive the format).
            let safe = e.to_string().replace('"', "'");
            let body = format!(r#"{{"error":"{safe}"}}"#);
            http_response(500, "application/json", body.as_bytes())
        }
    }
}

/// `POST /react` handler — body is JSON
/// `{"to":"...", "target_id":"...", "target_sender":"...", "emoji":"❤"}`.
/// Sends a reaction; an empty `emoji` string removes the reaction.
async fn react_endpoint(
    log: &SharedLog,
    shared_client: &SharedClient,
    body: &[u8],
) -> Vec<u8> {
    let client = match shared_client.read().clone() {
        Some(c) => c,
        None => {
            return http_response(
                503,
                "application/json",
                br#"{"error":"client not yet in phase 2 (login)"}"#,
            );
        }
    };
    #[derive(serde::Deserialize)]
    struct ReactReq {
        to: String,
        target_id: String,
        target_sender: String,
        emoji: String,
        #[serde(default)]
        target_from_me: bool,
    }
    let parsed: ReactReq = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid json: {e}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    let to_jid: wha_types::Jid = match parsed.to.parse() {
        Ok(j) => j,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid jid: {e:?}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    let sender_jid: wha_types::Jid = match parsed.target_sender.parse() {
        Ok(j) => j,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid target_sender: {e:?}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    log_push(
        log,
        format!(
            "POST /react to={} target={} emoji={:?}",
            to_jid, parsed.target_id, parsed.emoji
        ),
    );
    match wha_client::send_message::send_reaction(
        &client,
        &to_jid,
        &parsed.target_id,
        &sender_jid,
        parsed.target_from_me,
        &parsed.emoji,
    )
    .await
    {
        Ok(id) => {
            log_push(log, format!("POST /react ok id={id}"));
            let body = format!(r#"{{"id":"{id}"}}"#);
            http_response(200, "application/json", body.as_bytes())
        }
        Err(e) => {
            log_push(log, format!("POST /react err {e}"));
            let safe = e.to_string().replace('"', "'");
            let body = format!(r#"{{"error":"{safe}"}}"#);
            http_response(500, "application/json", body.as_bytes())
        }
    }
}

/// `POST /reply` handler — body is JSON
/// `{"to":"...", "body":"...", "quoted_id":"...", "quoted_sender":"..."}`.
/// We pass `Message::default()` for the embedded quoted_msg; the spec
/// allows that as a stub — recipients still see the body, just without
/// a thumbnail/preview of the quoted source.
async fn reply_endpoint(
    log: &SharedLog,
    shared_client: &SharedClient,
    body: &[u8],
) -> Vec<u8> {
    let client = match shared_client.read().clone() {
        Some(c) => c,
        None => {
            return http_response(
                503,
                "application/json",
                br#"{"error":"client not yet in phase 2 (login)"}"#,
            );
        }
    };
    #[derive(serde::Deserialize)]
    struct ReplyReq {
        to: String,
        body: String,
        quoted_id: String,
        quoted_sender: String,
    }
    let parsed: ReplyReq = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid json: {e}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    let to_jid: wha_types::Jid = match parsed.to.parse() {
        Ok(j) => j,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid jid: {e:?}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    let qsender: wha_types::Jid = match parsed.quoted_sender.parse() {
        Ok(j) => j,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid quoted_sender: {e:?}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    log_push(
        log,
        format!(
            "POST /reply to={} body={}b quoted_id={}",
            to_jid,
            parsed.body.len(),
            parsed.quoted_id
        ),
    );
    let stub_quoted = wha_proto::e2e::Message::default();
    match wha_client::send_message::send_reply(
        &client,
        &to_jid,
        &parsed.body,
        &parsed.quoted_id,
        &qsender,
        &stub_quoted,
    )
    .await
    {
        Ok(id) => {
            log_push(log, format!("POST /reply ok id={id}"));
            let body = format!(r#"{{"id":"{id}"}}"#);
            http_response(200, "application/json", body.as_bytes())
        }
        Err(e) => {
            log_push(log, format!("POST /reply err {e}"));
            let safe = e.to_string().replace('"', "'");
            let body = format!(r#"{{"error":"{safe}"}}"#);
            http_response(500, "application/json", body.as_bytes())
        }
    }
}

/// `POST /revoke` handler — body is JSON `{"to":"...", "target_id":"..."}`.
/// Sends a delete-for-everyone for our own message identified by `target_id`.
async fn revoke_endpoint(
    log: &SharedLog,
    shared_client: &SharedClient,
    body: &[u8],
) -> Vec<u8> {
    let client = match shared_client.read().clone() {
        Some(c) => c,
        None => {
            return http_response(
                503,
                "application/json",
                br#"{"error":"client not yet in phase 2 (login)"}"#,
            );
        }
    };
    #[derive(serde::Deserialize)]
    struct RevokeReq {
        to: String,
        target_id: String,
    }
    let parsed: RevokeReq = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid json: {e}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    let to_jid: wha_types::Jid = match parsed.to.parse() {
        Ok(j) => j,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid jid: {e:?}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    log_push(
        log,
        format!("POST /revoke to={} target={}", to_jid, parsed.target_id),
    );
    match wha_client::send_message::send_revoke(&client, &to_jid, &parsed.target_id).await {
        Ok(id) => {
            log_push(log, format!("POST /revoke ok id={id}"));
            let body = format!(r#"{{"id":"{id}"}}"#);
            http_response(200, "application/json", body.as_bytes())
        }
        Err(e) => {
            log_push(log, format!("POST /revoke err {e}"));
            let safe = e.to_string().replace('"', "'");
            let body = format!(r#"{{"error":"{safe}"}}"#);
            http_response(500, "application/json", body.as_bytes())
        }
    }
}

/// `POST /newsletter/send` — body is JSON `{"channel":"...", "body":"..."}`.
/// Posts a plain-text message to a WhatsApp channel (newsletter). The
/// `channel` field must be a `…@newsletter` JID. Channel posts are NOT
/// Signal-encrypted; they ship as a bare `<plaintext>` envelope.
async fn newsletter_send_endpoint(
    log: &SharedLog,
    shared_client: &SharedClient,
    body: &[u8],
) -> Vec<u8> {
    let client = match shared_client.read().clone() {
        Some(c) => c,
        None => {
            return http_response(
                503,
                "application/json",
                br#"{"error":"client not yet in phase 2 (login)"}"#,
            );
        }
    };
    #[derive(serde::Deserialize)]
    struct NewsletterSendReq {
        channel: String,
        body: String,
    }
    let parsed: NewsletterSendReq = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid json: {e}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    let channel_jid: wha_types::Jid = match parsed.channel.parse() {
        Ok(j) => j,
        Err(e) => {
            let msg = format!(r#"{{"error":"invalid channel jid: {e:?}"}}"#);
            return http_response(400, "application/json", msg.as_bytes());
        }
    };
    log_push(
        log,
        format!(
            "POST /newsletter/send channel={} body={}b",
            channel_jid,
            parsed.body.len()
        ),
    );
    match wha_client::newsletter::send_newsletter_text(&client, &channel_jid, &parsed.body).await {
        Ok(id) => {
            log_push(log, format!("POST /newsletter/send ok id={id}"));
            let body = format!(r#"{{"id":"{id}"}}"#);
            http_response(200, "application/json", body.as_bytes())
        }
        Err(e) => {
            log_push(log, format!("POST /newsletter/send err {e}"));
            let safe = e.to_string().replace('"', "'");
            let body = format!(r#"{{"error":"{safe}"}}"#);
            http_response(500, "application/json", body.as_bytes())
        }
    }
}

/// `GET /media/<msg_id>` — locate the message in the in-memory ring, refresh
/// the media connection, download + decrypt its attachment, and return the
/// raw plaintext bytes with the message's `mimetype` as Content-Type.
///
/// Status codes:
/// - 503 if Phase 2 isn't live yet,
/// - 404 if the id isn't in the ring, or the message has no downloadable
///   media,
/// - 502 if the download/decrypt fails,
/// - 200 + plaintext on success.
async fn media_endpoint(
    log: &SharedLog,
    shared_client: &SharedClient,
    media_ring: &SharedMediaRing,
    msg_id: &str,
) -> Vec<u8> {
    let client = match shared_client.read().clone() {
        Some(c) => c,
        None => {
            log_push(log, format!("GET /media/{msg_id} rejected: not in phase 2"));
            return http_response(503, "text/plain", b"client not yet in phase 2");
        }
    };

    // Snapshot the message proto from the ring without holding the lock
    // across the `.await` on the download.
    let msg_opt = media_ring.read().get(msg_id).cloned();
    let msg = match msg_opt {
        Some(m) => m,
        None => {
            log_push(log, format!("GET /media/{msg_id}: not in ring"));
            return http_response(404, "text/plain", b"message not in ring");
        }
    };

    // Refresh `MediaConn` over our IQ socket (mirror of history_sync).
    let sender = wha_client::history_sync::ClientIqSender { client: &*client };
    let req_id = client.generate_request_id();
    let conn = match wha_media::refresh_media_conn(&sender, req_id).await {
        Ok(c) => c,
        Err(e) => {
            log_push(log, format!("GET /media/{msg_id}: media_conn failed: {e}"));
            let body = format!("media_conn failed: {e}");
            return http_response(502, "text/plain", body.as_bytes());
        }
    };

    // Pick the right typed downloader and remember the mimetype. Some
    // proto fields land in a `Box<...>` because of recursive `ContextInfo`
    // back-references — store boxed values so we don't have to unbox.
    enum Picked {
        Image(Box<wha_proto::e2e::ImageMessage>),
        Video(Box<wha_proto::e2e::VideoMessage>),
        Audio(Box<wha_proto::e2e::AudioMessage>),
        Document(Box<wha_proto::e2e::DocumentMessage>),
        Sticker(Box<wha_proto::e2e::StickerMessage>),
    }
    // The proto fields are already `Option<Box<…>>` (so prost can fit
    // recursive `ContextInfo` references behind a pointer), which lets us
    // move the box straight into `Picked` without any extra allocation.
    let picked = if let Some(im) = msg.image_message {
        Picked::Image(im)
    } else if let Some(vm) = msg.video_message {
        Picked::Video(vm)
    } else if let Some(am) = msg.audio_message {
        Picked::Audio(am)
    } else if let Some(dm) = msg.document_message {
        Picked::Document(dm)
    } else if let Some(sm) = msg.sticker_message {
        Picked::Sticker(sm)
    } else {
        log_push(log, format!("GET /media/{msg_id}: no downloadable media"));
        return http_response(404, "text/plain", b"no downloadable media on this message");
    };

    // Mimetype + download. We compute the mimetype before consuming the
    // proto in the download call.
    let (mime, fut): (String, _) = match &picked {
        Picked::Image(m) => (
            m.mimetype.clone().unwrap_or_else(|| "image/jpeg".into()),
            "image",
        ),
        Picked::Video(m) => (
            m.mimetype.clone().unwrap_or_else(|| "video/mp4".into()),
            "video",
        ),
        Picked::Audio(m) => (
            m.mimetype.clone().unwrap_or_else(|| "audio/ogg".into()),
            "audio",
        ),
        Picked::Document(m) => (
            m.mimetype.clone().unwrap_or_else(|| "application/octet-stream".into()),
            "document",
        ),
        Picked::Sticker(m) => (
            m.mimetype.clone().unwrap_or_else(|| "image/webp".into()),
            "sticker",
        ),
    };
    log_push(
        log,
        format!("GET /media/{msg_id}: downloading {fut} ({mime})…"),
    );
    let result = match picked {
        Picked::Image(m) => wha_media::download_image(&conn, &m).await,
        Picked::Video(m) => wha_media::download_video(&conn, &m).await,
        Picked::Audio(m) => wha_media::download_audio(&conn, &m).await,
        Picked::Document(m) => wha_media::download_document(&conn, &m).await,
        Picked::Sticker(m) => wha_media::download_sticker(&conn, &m).await,
    };
    match result {
        Ok(bytes) => {
            log_push(
                log,
                format!("GET /media/{msg_id}: ok len={} mime={mime}", bytes.len()),
            );
            http_response(200, &mime, &bytes)
        }
        Err(e) => {
            log_push(log, format!("GET /media/{msg_id}: download failed: {e}"));
            let body = format!("download failed: {e}");
            http_response(502, "text/plain", body.as_bytes())
        }
    }
}

fn index_response() -> Vec<u8> {
    let body = INDEX_HTML.as_bytes();
    http_response(200, "text/html; charset=utf-8", body)
}

fn qr_svg_response(status: &SharedStatus) -> Vec<u8> {
    let s = status.read().clone();
    match s {
        PairStatus::Qr(code) => match QrCode::new(code.as_bytes()) {
            Ok(qr) => {
                let svg = qr
                    .render::<svg::Color>()
                    .min_dimensions(320, 320)
                    .quiet_zone(true)
                    .build();
                http_response(200, "image/svg+xml; charset=utf-8", svg.as_bytes())
            }
            Err(_) => http_response(204, "text/plain", b""),
        },
        _ => http_response(204, "text/plain", b""),
    }
}

fn log_response(log: &SharedLog) -> Vec<u8> {
    let r = log.read();
    let body: String = r
        .lines
        .iter()
        .map(|(t, m)| format!("[{t}] {m}\n"))
        .collect();
    http_response(200, "text/plain; charset=utf-8", body.as_bytes())
}

fn status_response(status: &SharedStatus) -> Vec<u8> {
    let s = match &*status.read() {
        PairStatus::Connecting => "connecting".to_owned(),
        PairStatus::Waiting => "waiting".to_owned(),
        PairStatus::Qr(_) => "qr".to_owned(),
        PairStatus::Paired(jid) => format!("paired:{jid}"),
        PairStatus::LoggedIn(jid) => format!("logged_in:{jid}"),
        PairStatus::GotMessage { from, body } => format!("got_message:{from}|{body}"),
        PairStatus::Error(msg) => format!("error:{msg}"),
    };
    http_response(200, "text/plain; charset=utf-8", s.as_bytes())
}

fn http_response(status: u16, ctype: &str, body: &[u8]) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        _ => "Status",
    };
    let mut out = Vec::with_capacity(256 + body.len());
    out.extend_from_slice(
        format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(body);
    out
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"/>
<title>whatsapp-rust pair_live</title>
<style>
  body { font-family: -apple-system, system-ui, sans-serif; max-width: 640px; margin: 2em auto; padding: 0 1em; color: #222; }
  h1 { font-size: 1.4em; margin-bottom: .2em; }
  .status { padding: .8em 1em; background: #f4f4f4; border-radius: 6px; margin: 1em 0; }
  .qr { text-align: center; padding: 1em; background: white; border: 1px solid #eee; border-radius: 8px; }
  .qr img { display: block; margin: 0 auto; max-width: 320px; height: auto; }
  .ok { background: #d9f5d9; color: #226622; }
  .err { background: #f5d9d9; color: #882222; }
  code { background: #eee; padding: .1em .3em; border-radius: 3px; }
  small { color: #666; }
</style></head>
<body>
<h1>whatsapp-rust live pairing</h1>
<div class="status" id="status">connecting...</div>
<div class="qr"><img id="qr" src="" alt="QR code will appear here"/></div>
<p><small>
  Open WhatsApp on your phone → Settings → Linked Devices → Link a Device → scan.
  The QR auto-refreshes every few seconds. The first ref lives 60s, subsequent ones 20s.
</small></p>
<h3 style="margin-top:1.5em;font-size:1em">live log</h3>
<pre id="log" style="background:#111;color:#cfc;padding:.8em;border-radius:6px;height:240px;overflow-y:scroll;font-size:.78em;line-height:1.35"></pre>
<script>
const $s = document.getElementById('status');
const $q = document.getElementById('qr');
const $l = document.getElementById('log');
async function refreshLog() {
  try {
    const t = await (await fetch('/log', {cache:'no-store'})).text();
    $l.textContent = t;
    $l.scrollTop = $l.scrollHeight;
  } catch (_) {}
  setTimeout(refreshLog, 1000);
}
refreshLog();
async function tick() {
  try {
    const st = await (await fetch('/status', {cache:'no-store'})).text();
    if (st === 'connecting') { $s.textContent = 'Connecting to WhatsApp servers…'; $s.className = 'status'; }
    else if (st === 'waiting') { $s.textContent = 'Handshake complete. Waiting for <pair-device> from server…'; $s.className = 'status'; }
    else if (st === 'qr') { $s.textContent = 'Scan the QR with your phone.'; $s.className = 'status';
      const r = await fetch('/qr.svg', {cache:'no-store'});
      if (r.status === 200) { const t = await r.text(); $q.src = 'data:image/svg+xml;utf8,' + encodeURIComponent(t); }
    }
    else if (st.startsWith('paired:')) { $s.textContent = 'Paired as ' + st.slice(7) + '. Re-connecting as logged-in client…'; $s.className = 'status'; $q.style.display='none'; }
    else if (st.startsWith('logged_in:')) { $s.textContent = 'Logged in as ' + st.slice(10) + '. Waiting for first inbound <message> (send yourself one from another device)…'; $s.className = 'status'; $q.style.display='none'; }
    else if (st.startsWith('got_message:')) { const rest = st.slice(12); const i = rest.indexOf('|'); const from = i > 0 ? rest.slice(0,i) : '?'; const body = i > 0 ? rest.slice(i+1) : rest; $s.innerHTML = '<b>✓ Got message from ' + from + '</b><br><tt style="font-size:.85em">' + body.replace(/[<&>]/g, c => ({'<':'&lt;','&':'&amp;','>':'&gt;'}[c])) + '</tt>'; $s.className = 'status ok'; $q.style.display='none'; return; }
    else if (st.startsWith('error:')) { $s.textContent = 'Error: ' + st.slice(6); $s.className = 'status err'; $q.style.display='none'; return; }
  } catch (e) { $s.textContent = 'Lost connection to local server. (' + e + ')'; $s.className = 'status err'; }
  setTimeout(tick, 1500);
}
tick();
</script>
</body></html>"#;
