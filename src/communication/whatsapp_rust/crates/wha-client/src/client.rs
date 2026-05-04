//! Async [`Client`] wiring everything together.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

use wha_binary::{marshal, unmarshal, Node};
use wha_crypto::KeyPair;
use wha_socket::{FrameSocket, NoiseSocket, URL};
use wha_store::{Device, InspectStore};
use wha_types::Jid;

use crate::error::ClientError;
use crate::events::Event;
use crate::handshake::{build_client_hello, finish_handshake};
use crate::payload::build_client_payload;
use crate::request::InfoQuery;

/// Default request timeout, matching whatsmeow's `defaultRequestTimeout`.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(75);

/// Capacity of the buffered-decrypt cache. Mirrors the spirit of
/// `whatsmeow.bufferedDecrypt`: the server occasionally retries the same
/// `<message>` (same ciphertext bytes, same `prekey_id`) and naively
/// decrypting twice would consume the one-time pre-key on the first run and
/// then fail the second. A small per-process LRU/FIFO of `(hash → plaintext)`
/// short-circuits the second hit.
const DECRYPT_CACHE_CAPACITY: usize = 1024;

/// Capacity of the outgoing-message ring. Mirrors `recentMessagesSize` in
/// `_upstream/whatsmeow/retry.go` (256). Holds the last N outbound plaintexts
/// keyed by `(to, message_id)` so we can re-encrypt them when a peer ships back
/// a `<receipt type="retry">`.
const RECENT_MESSAGES_SIZE: usize = 256;

type Waiter = oneshot::Sender<Node>;

/// The high-level WhatsApp client. Holds a [`Device`] (loaded from a
/// [`wha_store::MemoryStore`] or a sqlite-backed equivalent), an open noise
/// socket once `connect()` has run, and a request-waiter map for `<iq>`
/// responses.
pub struct Client {
    pub device: Device,
    inner: Arc<Inner>,
}

struct Inner {
    unique_id_prefix: String,
    id_counter: AtomicU64,
    response_waiters: Mutex<HashMap<String, Waiter>>,
    state: Mutex<ClientState>,
    events: mpsc::UnboundedSender<Event>,
    /// Buffered-decrypt cache. Mirrors `Client.bufferedDecrypt` in
    /// `_upstream/whatsmeow/message.go`: maps a SHA-256 over
    /// `enc_type ‖ 0x00 ‖ ciphertext ‖ 0x00 ‖ sender_jid` to the decrypted
    /// plaintext. A FIFO of insertion order bounds the size at
    /// [`DECRYPT_CACHE_CAPACITY`] entries.
    decrypt_cache: Mutex<DecryptCache>,
    /// Recent-outgoing-messages ring. Mirrors the
    /// `recentMessagesMap`/`recentMessagesList` pair in
    /// `_upstream/whatsmeow/retry.go` — when a peer sends back a
    /// `<receipt type="retry">` for one of our messages we look up the
    /// original plaintext here so [`crate::retry::handle_retry_receipt`] can
    /// re-encrypt + re-send it. Capacity is [`RECENT_MESSAGES_SIZE`].
    recent_messages: Mutex<RecentMessages>,
    /// Per-incoming-message-id retry counter. Mirrors `messageRetries` in
    /// `_upstream/whatsmeow/retry.go`. Bumped on every
    /// [`crate::retry::send_retry_receipt`] for the same message id; cap at
    /// 5 (also matching upstream).
    message_retries: Mutex<HashMap<String, u32>>,
    /// Optional debug-inspection handle. Used by
    /// [`crate::internals::DangerousInternalClient`] to enumerate per-store
    /// state (session addresses, prekey count, app-state versions). Wired in
    /// by the caller via [`Client::set_inspector`] — defaults to `None`,
    /// matching whatsmeow's "you have to opt in to dangerous internals".
    inspector: Mutex<Option<Arc<dyn InspectStore>>>,
    /// Server-time offset estimated from `<iq>` round-trips. Mirrors
    /// `Client.serverTimeOffset` in `_upstream/whatsmeow/client.go` — used by
    /// `getUnifiedSessionID`, which folds it into the unified-session id
    /// computation. We don't yet update it from receipts; the field is here
    /// so the internals API surface matches.
    server_time_offset_ms: AtomicI64,
    /// Side-channel subscribers for the QR pairing flow. Every active
    /// [`crate::qr_channel::QrChannel`] keeps one entry here; `dispatch_event`
    /// forwards QR-relevant events into each. Mirrors upstream's
    /// `GetQRChannel` pattern: rather than asking the application to walk
    /// the main event channel for QR codes, we publish a focused stream. A
    /// failed `send` (the receiver was dropped) prunes the entry on the next
    /// dispatch.
    qr_subscribers: Mutex<Vec<mpsc::UnboundedSender<crate::qr_channel::QrEvent>>>,
}

/// Plaintext cache keyed by the ciphertext-hash described in
/// `Client.bufferedDecrypt`. The `order` ring is the FIFO of cache keys we
/// drop oldest-first when capacity is reached.
#[derive(Default)]
struct DecryptCache {
    map: HashMap<[u8; 32], Vec<u8>>,
    order: VecDeque<[u8; 32]>,
}

/// Recent-outgoing-messages cache. Layout mirrors upstream's:
/// `map: (to, msg_id) → plaintext`, plus a FIFO of insertion order so we can
/// drop the oldest entry when we exceed [`RECENT_MESSAGES_SIZE`].
#[derive(Default)]
struct RecentMessages {
    map: HashMap<(Jid, String), Vec<u8>>,
    order: VecDeque<(Jid, String)>,
}

#[derive(Default)]
struct ClientState {
    socket: Option<NoiseSocket>,
    sink: Option<wha_socket::frame::SharedSink>,
}

impl Client {
    /// Create a client + the channel where its events arrive.
    pub fn new(device: Device) -> (Self, mpsc::UnboundedReceiver<Event>) {
        let (etx, erx) = mpsc::unbounded_channel();
        let inner = Arc::new(Inner {
            unique_id_prefix: format!("{:04x}", rand::random::<u16>()),
            id_counter: AtomicU64::new(0),
            response_waiters: Mutex::new(HashMap::new()),
            state: Mutex::new(ClientState::default()),
            events: etx,
            decrypt_cache: Mutex::new(DecryptCache::default()),
            recent_messages: Mutex::new(RecentMessages::default()),
            message_retries: Mutex::new(HashMap::new()),
            inspector: Mutex::new(None),
            server_time_offset_ms: AtomicI64::new(0),
            qr_subscribers: Mutex::new(Vec::new()),
        });
        (Client { device, inner }, erx)
    }

    /// Install a debug-inspection handle. Mirrors the wiring in
    /// `_upstream/whatsmeow/internals.go`: production code paths don't need
    /// it, but [`crate::internals::DangerousInternalClient`] uses it to
    /// enumerate session addresses, prekey counts, and app-state versions.
    /// Re-installable — the most recent call wins.
    pub fn set_inspector(&self, inspector: Arc<dyn InspectStore>) {
        *self.inner.inspector.lock() = Some(inspector);
    }

    /// Read the installed inspector handle, if any. Returns a fresh
    /// `Arc` clone so the caller can call across `await` points without
    /// holding the mutex.
    pub fn inspector(&self) -> Option<Arc<dyn InspectStore>> {
        self.inner.inspector.lock().clone()
    }

    /// Read the current server-time offset (milliseconds). Mirrors
    /// `Client.serverTimeOffset.Load()` upstream; used by
    /// `getUnifiedSessionID`. Defaults to `0` until something updates it.
    pub fn server_time_offset_ms(&self) -> i64 {
        self.inner.server_time_offset_ms.load(Ordering::Relaxed)
    }

    /// Update the server-time offset estimate (milliseconds). Mirrors the
    /// upstream `Client.serverTimeOffset.Store(...)` write site.
    pub fn set_server_time_offset_ms(&self, offset: i64) {
        self.inner.server_time_offset_ms.store(offset, Ordering::Relaxed);
    }

    /// Test-only: feed a node directly into the dispatcher. Mirrors the
    /// debug-only `HandleIQ` / `DispatchEvent` re-exports in
    /// `_upstream/whatsmeow/internals.go`. Used by
    /// [`crate::internals::DangerousInternalClient::handle_iq_directly`]
    /// and the dispatch routing tests.
    pub fn dispatch_node_for_test(&self, node: Node) {
        self.dispatch_node(node);
    }

    /// Generate a fresh request ID matching whatsmeow's pattern
    /// (`{4 hex chars}{counter}`).
    pub fn generate_request_id(&self) -> String {
        let n = self.inner.id_counter.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{}.{n}", self.inner.unique_id_prefix)
    }

    /// Whether we have a live noise socket.
    pub fn is_connected(&self) -> bool {
        self.inner.state.lock().socket.is_some()
    }

    /// Open the websocket, run the noise handshake, and start the read pump.
    /// On success the client is ready to send IQs and dispatches inbound
    /// nodes via the event channel returned by [`Client::new`].
    pub async fn connect(&self) -> Result<(), ClientError> {
        if self.is_connected() {
            return Err(ClientError::AlreadyConnected);
        }
        let mut fs = FrameSocket::connect(URL).await?;

        // Generate ephemeral keys + first handshake frame.
        let mut rng = rand::rngs::OsRng;
        let ephemeral = KeyPair::generate(&mut rng);
        let (mut nh, hello_bytes) = build_client_hello(&ephemeral)?;
        fs.send_frame(&hello_bytes).await?;

        // Wait for the server hello.
        let server_hello_bytes = tokio::time::timeout(Duration::from_secs(20), fs.frames.recv())
            .await
            .map_err(|_| ClientError::Handshake("timed out waiting for server hello".into()))?
            .ok_or_else(|| ClientError::Handshake("frame channel closed during handshake".into()))?;

        // Build the client payload proto and run the rest of the handshake.
        let payload = build_client_payload(&self.device);
        let mut payload_bytes = Vec::new();
        prost::Message::encode(&payload, &mut payload_bytes)
            .map_err(|e| ClientError::Proto(e.to_string()))?;

        let (finish_bytes, ns) = finish_handshake(
            &mut nh,
            &ephemeral,
            &self.device.noise_key,
            &server_hello_bytes,
            &payload_bytes,
        )?;
        fs.send_frame(&finish_bytes).await?;

        let sink = fs.shared_sink();
        {
            let mut state = self.inner.state.lock();
            state.socket = Some(ns);
            state.sink = Some(sink);
        }

        // Spawn the read pump that decrypts inbound frames and routes them.
        let inner_clone = self.inner.clone();
        let device_clone = self.device.clone();
        let mut frames = fs.frames;
        tokio::spawn(async move {
            while let Some(frame) = frames.recv().await {
                let plaintext = {
                    let mut state = inner_clone.state.lock();
                    let Some(ns) = state.socket.as_mut() else {
                        return;
                    };
                    match ns.decrypt_frame(&frame) {
                        Ok(b) => b,
                        Err(e) => {
                            warn!(?e, "noise decrypt failed");
                            continue;
                        }
                    }
                };
                let node = match unmarshal(&plaintext) {
                    Ok(n) => n,
                    Err(e) => {
                        let hex_dump: String =
                            plaintext.iter().map(|b| format!("{b:02x}")).collect();
                        warn!(
                            ?e,
                            len = plaintext.len(),
                            plaintext = %hex_dump,
                            "failed to unmarshal inbound node"
                        );
                        continue;
                    }
                };
                let pump_client = Client::from_parts(device_clone.clone(), inner_clone.clone());
                pump_client.dispatch_node(node);
            }
            let _ = inner_clone.events.send(Event::Disconnected {
                reason: "websocket closed".into(),
            });
        });

        let _ = self.inner.events.send(Event::Connected);

        // Spawn the keepalive loop (mirrors upstream's "if everything went
        // well, start keepAliveLoop" branch in `connect`). The loop owns an
        // `Arc<Client>` clone and self-terminates when the websocket goes
        // away.
        let _keepalive = crate::keepalive::spawn_keepalive_loop(self.clone_arc());

        Ok(())
    }

    /// Internal: rebuild a `Client` handle from the cloneable parts. Used by
    /// the read pump and dispatcher to hand fresh `Client` references to
    /// background tasks (recv_message, retry, notification, ...).
    fn from_parts(device: Device, inner: Arc<Inner>) -> Self {
        Client { device, inner }
    }

    /// Hand out a cheap `Arc<Client>` clone of this handle. The expensive
    /// state (`Inner` — IQ-waiter map, recent-messages cache, decrypt cache,
    /// event sender) lives behind `Arc` already; the `Device` is cloneable
    /// (every store handle inside is itself an `Arc`). Background tasks like
    /// the keepalive loop can hold one of these without taking exclusive
    /// ownership of the original.
    pub fn clone_arc(&self) -> Arc<Self> {
        Arc::new(Self::from_parts(self.device.clone(), self.inner.clone()))
    }

    /// Route an inbound decrypted node. Pending IQ/ACK responses go to the
    /// installed waiter; `<message>`, `<receipt type="retry">`, and
    /// `<notification>` are spawned onto async handlers; `<stream:error>` is
    /// surfaced as an event; everything else falls through to
    /// [`Event::UnhandledNode`].
    fn dispatch_node(&self, node: Node) {
        let id_match = if matches!(node.tag.as_str(), "iq" | "ack") {
            node.get_attr_str("id").map(|s| s.to_owned())
        } else {
            None
        };
        if let Some(id) = id_match {
            let waiter = self.inner.response_waiters.lock().remove(&id);
            if let Some(w) = waiter {
                let _ = w.send(node);
                return;
            }
        }
        if node.tag == "stream:error" {
            let code = node.get_attr_str("code").unwrap_or("").to_owned();
            let text = node.get_attr_str("text").unwrap_or("").to_owned();
            let _ = self.inner.events.send(Event::StreamError { code, text });
            return;
        }

        // Connection-lifecycle nodes — the handlers live in
        // `crate::connection_events` (port of upstream's
        // `connectionevents.go`). Route them onto the async runtime so the
        // dispatcher itself stays sync.
        match node.tag.as_str() {
            "failure" => {
                let device = self.device.clone();
                let inner = self.inner.clone();
                tokio::spawn(async move {
                    let cli = Client::from_parts(device, inner);
                    crate::connection_events::handle_connect_failure(&cli, &node).await;
                });
                return;
            }
            "success" => {
                let device = self.device.clone();
                let inner = self.inner.clone();
                tokio::spawn(async move {
                    let cli = Client::from_parts(device, inner);
                    if let Err(e) =
                        crate::connection_events::handle_connect_success(&cli, &node).await
                    {
                        warn!(?e, "handle_connect_success failed");
                    }
                });
                return;
            }
            "ib" => {
                let device = self.device.clone();
                let inner = self.inner.clone();
                tokio::spawn(async move {
                    let cli = Client::from_parts(device, inner);
                    crate::connection_events::handle_ib(&cli, &node).await;
                });
                return;
            }
            _ => {}
        }

        match node.tag.as_str() {
            "message" => {
                // Surface the message node directly; the application calls
                // `recv_message::handle_encrypted_message` exactly once. We
                // deliberately do NOT auto-decrypt here — that would consume
                // the one-time pre-key before the application can see the
                // node, and the application's own decrypt would then fail
                // with "missing one-time pre-key id N".
                let _ = self.inner.events.send(Event::UnhandledNode { node });
                return;
            }
            "receipt" => {
                if node.get_attr_str("type") == Some("retry") {
                    let device = self.device.clone();
                    let inner = self.inner.clone();
                    tokio::spawn(async move {
                        let cli = Client::from_parts(device, inner);
                        if let Err(e) = crate::retry::handle_retry_receipt(&cli, &node).await {
                            warn!(?e, "handle_retry_receipt failed");
                        }
                    });
                    return;
                }
                // Non-retry receipts fall through to UnhandledNode for now.
            }
            "notification" => {
                let device = self.device.clone();
                let inner = self.inner.clone();
                tokio::spawn(async move {
                    let cli = Client::from_parts(device, inner);
                    if let Err(e) = crate::notification::handle_notification(&cli, &node).await {
                        warn!(?e, "handle_notification failed");
                    }
                });
                return;
            }
            "call" => {
                let device = self.device.clone();
                let inner = self.inner.clone();
                tokio::spawn(async move {
                    let cli = Client::from_parts(device, inner);
                    if let Err(e) = crate::call::handle_call_stanza(&cli, &node).await {
                        warn!(?e, "handle_call_stanza failed");
                    }
                });
                return;
            }
            _ => {}
        }

        let _ = self.inner.events.send(Event::UnhandledNode { node });
    }

    /// Send a fully-formed XML node through the noise socket. Used by send_iq
    /// and by upper-layer modules (pair, presence, message send, …).
    pub async fn send_node(&self, node: &Node) -> Result<(), ClientError> {
        let bytes = marshal(node)?;
        let (sink, encrypted) = {
            let mut state = self.inner.state.lock();
            let sink = state.sink.clone().ok_or(ClientError::NotConnected)?;
            let ns = state.socket.as_mut().ok_or(ClientError::NotConnected)?;
            (sink, ns.encrypt_frame(&bytes)?)
        };
        // Frame the encrypted payload with a 3-byte length prefix and
        // hand it to the websocket sink.
        let total = 3 + encrypted.len();
        let mut framed = Vec::with_capacity(total);
        framed.push((encrypted.len() >> 16) as u8);
        framed.push((encrypted.len() >> 8) as u8);
        framed.push(encrypted.len() as u8);
        framed.extend_from_slice(&encrypted);
        let mut sink = sink.lock().await;
        use futures::SinkExt;
        sink.send(tokio_tungstenite::tungstenite::Message::Binary(framed.into()))
            .await
            .map_err(|e| ClientError::Socket(wha_socket::SocketError::Ws(e.to_string())))?;
        Ok(())
    }

    /// Synchronously send an IQ and wait for its `result` / `error`.
    pub async fn send_iq(&self, query: InfoQuery) -> Result<Node, ClientError> {
        let id = query.id.clone().unwrap_or_else(|| self.generate_request_id());
        let timeout = query.timeout.unwrap_or(DEFAULT_REQUEST_TIMEOUT);
        let node = query.into_node(id.clone());

        let (tx, rx) = oneshot::channel();
        self.inner.response_waiters.lock().insert(id.clone(), tx);
        debug!(?id, "sending iq");
        if let Err(e) = self.send_node(&node).await {
            self.inner.response_waiters.lock().remove(&id);
            return Err(e);
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => Err(ClientError::IqDisconnected),
            Err(_) => {
                self.inner.response_waiters.lock().remove(&id);
                Err(ClientError::IqTimedOut)
            }
        }
    }

    /// Raw access for higher layers that want to install their own waiter
    /// for nodes whose tag isn't `iq`/`ack`.
    pub fn install_waiter(&self, id: String, waiter: Waiter) {
        self.inner.response_waiters.lock().insert(id, waiter);
    }

    /// Dispatch an event to the receiver returned from [`Client::new`]. Used
    /// by the periphery modules (pair, presence, …) to surface state changes.
    pub fn dispatch_event(&self, event: Event) {
        // Fan out QR-relevant events to every active [`QrChannel`]
        // subscriber first, *then* push the event to the main channel.
        // Doing it in this order means a single dispatch_event call is the
        // sole publish point — there's no race where the main consumer
        // could observe an event before the QR subscriber.
        if let Some(qr_evt) = crate::qr_channel::map_event_to_qr(&event) {
            let mut subs = self.inner.qr_subscribers.lock();
            subs.retain(|s| s.send(qr_evt.clone()).is_ok());
        }
        let _ = self.inner.events.send(event);
    }

    /// Register a QR-event subscriber. Returns the sender end of the side
    /// channel so [`crate::qr_channel`] can wire up the timeout machinery.
    pub(crate) fn register_qr_subscriber(
        &self,
        sender: mpsc::UnboundedSender<crate::qr_channel::QrEvent>,
    ) {
        self.inner.qr_subscribers.lock().push(sender);
    }

    /// Internal: hand out a clone of the event-channel sender. The
    /// underlying tokio `UnboundedSender` is `Clone`, so background tasks
    /// spawned by periphery modules can keep emitting events without
    /// holding a `&Client` reference.
    pub(crate) fn events_sender_clone(&self) -> mpsc::UnboundedSender<Event> {
        self.inner.events.clone()
    }

    /// Look up a previously-decrypted plaintext for `key` (the SHA-256 hash
    /// described above [`Inner::decrypt_cache`]). Returns the cached
    /// plaintext on hit, `None` on miss. Used by [`crate::recv_message`] to
    /// short-circuit duplicate decrypts.
    pub(crate) fn lookup_decrypted_plaintext(&self, key: &[u8; 32]) -> Option<Vec<u8>> {
        self.inner.decrypt_cache.lock().map.get(key).cloned()
    }

    /// Record a successful decrypt in the cache. Drops the oldest entry
    /// when over [`DECRYPT_CACHE_CAPACITY`]; idempotent on repeated inserts
    /// of the same key (the existing entry is not re-promoted — FIFO, not
    /// LRU, so timing semantics match upstream's "first decrypt wins").
    pub(crate) fn store_decrypted_plaintext(&self, key: [u8; 32], plaintext: Vec<u8>) {
        let mut cache = self.inner.decrypt_cache.lock();
        if cache.map.contains_key(&key) {
            return;
        }
        if cache.order.len() >= DECRYPT_CACHE_CAPACITY {
            if let Some(oldest) = cache.order.pop_front() {
                cache.map.remove(&oldest);
            }
        }
        cache.map.insert(key, plaintext);
        cache.order.push_back(key);
    }

    /// Cache an outgoing message plaintext keyed by `(to, msg_id)`. Mirrors
    /// `Client.addRecentMessage` in upstream — FIFO eviction once we go over
    /// [`RECENT_MESSAGES_SIZE`].
    pub fn add_recent_message(&self, to: Jid, msg_id: String, plaintext: Vec<u8>) {
        let mut cache = self.inner.recent_messages.lock();
        let key = (to, msg_id);
        if cache.map.contains_key(&key) {
            return;
        }
        if cache.order.len() >= RECENT_MESSAGES_SIZE {
            if let Some(oldest) = cache.order.pop_front() {
                cache.map.remove(&oldest);
            }
        }
        cache.order.push_back(key.clone());
        cache.map.insert(key, plaintext);
    }

    /// Look up a previously-cached outgoing plaintext by `(to, msg_id)`. Mirrors
    /// `Client.getRecentMessage` upstream. Returns `None` on miss.
    pub fn get_recent_message(&self, to: &Jid, msg_id: &str) -> Option<Vec<u8>> {
        let cache = self.inner.recent_messages.lock();
        cache.map.get(&(to.clone(), msg_id.to_owned())).cloned()
    }

    /// Increment-and-return the retry counter for `msg_id`. Mirrors the
    /// `cli.messageRetries[id]++` block in `_upstream/whatsmeow/retry.go`.
    /// Returns the post-increment value so the caller can short-circuit at
    /// the upstream cap of 5.
    pub fn bump_message_retry(&self, msg_id: &str) -> u32 {
        let mut map = self.inner.message_retries.lock();
        let entry = map.entry(msg_id.to_owned()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Read the retry counter for `msg_id` without bumping. Returns `0` on
    /// miss — same semantics as a Go zero-value map read.
    pub fn message_retry_count(&self, msg_id: &str) -> u32 {
        self.inner.message_retries.lock().get(msg_id).copied().unwrap_or(0)
    }

    /// Snapshot of every entry in the recent-outgoing-messages cache.
    /// Used by [`crate::internals::DangerousInternalClient::get_recent_messages`].
    /// Returns `(to, msg_id, plaintext)` triples; the order is not
    /// guaranteed (the underlying cache is `HashMap` + ring).
    pub fn snapshot_recent_messages(&self) -> Vec<crate::internals::RecentMessage> {
        let cache = self.inner.recent_messages.lock();
        cache
            .map
            .iter()
            .map(|((to, mid), pt)| crate::internals::RecentMessage {
                to: to.to_string(),
                msg_id: mid.clone(),
                plaintext: pt.clone(),
            })
            .collect()
    }

    /// Hand out the dangerous-internal accessor. Mirrors
    /// `Client.DangerousInternals()` upstream — re-exports otherwise
    /// private state for diagnostics.
    pub fn dangerous_internal(&self) -> crate::internals::DangerousInternalClient<'_> {
        crate::internals::DangerousInternalClient::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wha_store::MemoryStore;

    #[tokio::test]
    async fn generate_request_id_is_unique_and_grows() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let a = cli.generate_request_id();
        let b = cli.generate_request_id();
        assert_ne!(a, b);
        assert!(b.ends_with(".2"));
    }

    #[tokio::test]
    async fn send_node_without_connection_fails_cleanly() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, _evt) = Client::new(device);
        let n = Node::tag_only("iq");
        let r = cli.send_node(&n).await;
        assert!(matches!(r, Err(ClientError::NotConnected)));
    }

    /// Routing test: `<presence>` falls through to `UnhandledNode`, and
    /// `<message>` is also surfaced as `UnhandledNode` — we deliberately do
    /// NOT auto-spawn `recv_message::handle_encrypted_message` from the
    /// dispatcher, because doing so would consume the one-time pre-key
    /// before the application can run its own decrypt (the comment block
    /// in the `"message"` arm of `dispatch_node` documents this in detail).
    #[tokio::test]
    async fn dispatch_node_routes_message_to_recv_message_branch() {
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (cli, mut evt) = Client::new(device);

        // Sanity: an unrouted tag falls through synchronously.
        cli.dispatch_node(Node::tag_only("presence"));
        match evt.try_recv() {
            Ok(Event::UnhandledNode { node }) => assert_eq!(node.tag, "presence"),
            other => panic!("expected synchronous UnhandledNode for <presence>, got {other:?}"),
        }

        // A <message> node is also surfaced as `UnhandledNode` — the
        // application calls `recv_message::handle_encrypted_message` itself
        // exactly once. This pins that intentional non-auto-decrypt wiring.
        cli.dispatch_node(Node::tag_only("message"));
        match evt.try_recv() {
            Ok(Event::UnhandledNode { node }) => assert_eq!(node.tag, "message"),
            other => panic!("expected synchronous UnhandledNode for <message>, got {other:?}"),
        }
    }
}
