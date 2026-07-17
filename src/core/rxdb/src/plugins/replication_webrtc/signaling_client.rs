//! **gap-item N6** — Signaling client that matches the simple-peer signaling
//! server protocol used by upstream RxDB and by business-os.
//!
//! Connects to a `ws://` or `wss://` URL, receives an `init` frame with our
//! peer id, lets callers `join(room)`, `send_signal(receiver_peer_id, data)`,
//! and observes a stream of [`ServerToClient`] frames + a stream of room-mate
//! peer-id-lists.
//!
//! Keepalive: a background task pings every `SIMPLE_PEER_PING_INTERVAL_MS / 2`
//! so the server never times us out.
//!
//! **Status:** functional. Sends/receives the protocol byte-correctly against
//! upstream's `signaling-server.ts`. The downstream WebRTC connection handler
//! (`connection_handler_rs`) consumes peer-list changes and signal frames to
//! drive ICE/SDP exchange with webrtc-rs.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Once;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex as PlMutex;
use tokio::net::{lookup_host, TcpStream};
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{client_async_tls, MaybeTlsStream, WebSocketStream};
use url::Url;

use crate::plugins::replication_webrtc::signaling_protocol::{
    ClientToServer, PeerId, RoomId, ServerToClient, SIMPLE_PEER_PING_INTERVAL_MS,
};
use crate::rx_error::{new_rx_error, RxError};
use crate::rxjs_compat::{RxBehaviorSubject, RxStream, RxSubject};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// FIX 3: bound the WebSocket handshake. An unreachable or slow signaling
/// server used to let `connect_async` hang indefinitely, wedging the
/// per-collection bring-up that awaits it. 20s matches the per-collection
/// timeout enforced at the bring-up loop.
const SIGNALING_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const SIGNALING_ADDRESS_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Reconnection backoff bounds. Post-multiplex the whole instance shares ONE
/// signaling socket, so a clean close or network blip used to deafen the native
/// peer to every browser join until a full process restart. The supervisor now
/// reconnects with exponential backoff and re-joins the room, which re-broadcasts
/// the room peer list and re-drives the connection handler to rebuild peers.
const SIGNALING_RECONNECT_BASE_DELAY: Duration = Duration::from_secs(1);
const SIGNALING_RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);

static RUSTLS_CRYPTO_PROVIDER: Once = Once::new();

type WsWrite = SplitSink<WsStream, Message>;
type WsRead = SplitStream<WsStream>;

pub struct SignalingClient {
    /// URL of the initial connect (identification/logging only — reconnects
    /// ask `url_provider` for a fresh URL).
    pub url: String,
    /// Produces the candidate URL list for every (re)connect attempt.
    /// Time-windowed query params (`token_iat`/`token_exp`, TTL 24h) used to
    /// be frozen into the connect-time URL; after >24h uptime any socket drop
    /// then turned into a permanent join-rejection loop. The provider
    /// recomputes them per attempt. Multiple URLs are a failover list: the
    /// client sticks to the URL that last worked and rotates to the next
    /// candidate only when an establish attempt fails (the configured list
    /// used to be cosmetic — only the first URL was ever tried, so a downed
    /// primary signaling server meant no new session could ever pair).
    url_provider: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
    /// Index into the provider's URL list for the next (re)connect attempt.
    /// Sticky on success, advanced on a failed establish. Never touches the
    /// reconnect backoff: rotation must not reset it (backoff resets only on
    /// a `joined` broadcast, see the supervisor).
    url_rotation: AtomicUsize,
    /// Stream of every server frame we received.
    server_messages: RxSubject<ServerToClient>,
    /// Holds the latest list of peer ids we're co-resident with.
    peer_list: RxBehaviorSubject<Vec<PeerId>>,
    /// Role metadata per room member (from the `joined` peer descriptors).
    /// Drives the initiator decision in the connection handler: the browser
    /// bundle always initiates toward `ctox_instance`, so we must never
    /// initiate toward a `browser` peer.
    peer_roles: PlMutex<HashMap<PeerId, String>>,
    /// Our server-issued peer id, once the `init` frame arrives.
    own_peer_id: RxBehaviorSubject<Option<PeerId>>,
    joined_room: PlMutex<Option<RoomId>>,
    /// Send half of the WebSocket. Replaced on every reconnect; `None` only in
    /// the window between a socket dying and the supervisor re-establishing it.
    writer: TokioMutex<Option<WsWrite>>,
    socket_connected: AtomicBool,
    join_accepted: AtomicBool,
    terminal_rejection: AtomicBool,
    rejection: PlMutex<Option<(String, String)>>,
    join_notify: tokio::sync::Notify,
    /// Set by `close()` so the reconnect supervisor stops instead of fighting an
    /// intentional shutdown.
    closed: Arc<AtomicBool>,
    background_tasks: PlMutex<Vec<JoinHandle<()>>>,
}

impl SignalingClient {
    /// Connect to a signaling server (e.g. `ws://localhost:8080`).
    pub async fn connect(url: impl Into<String>) -> Result<Arc<Self>, RxError> {
        let url_string: String = url.into();
        Self::connect_with_url_provider(move || url_string.clone()).await
    }

    /// Connect with a URL provider that is re-evaluated on every reconnect
    /// attempt, so freshness-windowed query params stay valid across long
    /// uptimes.
    pub async fn connect_with_url_provider<F>(url_provider: F) -> Result<Arc<Self>, RxError>
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        Self::connect_with_url_list_provider(move || vec![url_provider()]).await
    }

    /// Like [`Self::connect_with_url_provider`], but the provider returns a
    /// FAILOVER LIST of candidate URLs (all re-derived per attempt). The
    /// initial connect tries each candidate in order; reconnects stay sticky
    /// on the last-working candidate and rotate to the next one only after a
    /// failed establish attempt.
    pub async fn connect_with_url_list_provider<F>(url_provider: F) -> Result<Arc<Self>, RxError>
    where
        F: Fn() -> Vec<String> + Send + Sync + 'static,
    {
        let url_provider: Arc<dyn Fn() -> Vec<String> + Send + Sync> = Arc::new(url_provider);
        let candidates: Vec<String> = url_provider()
            .into_iter()
            .filter(|url| !url.trim().is_empty())
            .collect();
        if candidates.is_empty() {
            return Err(new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": "signaling URL provider returned no usable URLs",
                })),
            ));
        }
        // Initial connect: walk the failover list once. The index of the
        // first working candidate seeds the sticky rotation so reconnects
        // keep using it.
        let mut connected: Option<(usize, String, (WsWrite, WsRead))> = None;
        let mut last_error: Option<RxError> = None;
        for (index, candidate) in candidates.iter().enumerate() {
            match establish_ws(candidate).await {
                Ok(halves) => {
                    connected = Some((index, candidate.clone(), halves));
                    break;
                }
                Err(error) => {
                    tracing::warn!(
                        target: "ctox_rxdb::signaling_client",
                        url = %candidate,
                        "initial signaling connect failed: {error}; trying next candidate",
                    );
                    last_error = Some(error);
                }
            }
        }
        let Some((initial_index, url_string, (write_half, read_half))) = connected else {
            return Err(last_error.expect("candidates is non-empty, so at least one error"));
        };
        let client = Arc::new(Self {
            url: url_string,
            url_provider,
            url_rotation: AtomicUsize::new(initial_index),
            server_messages: RxSubject::new(),
            peer_list: RxBehaviorSubject::new(Vec::new()),
            peer_roles: PlMutex::new(HashMap::new()),
            own_peer_id: RxBehaviorSubject::new(None),
            joined_room: PlMutex::new(None),
            writer: TokioMutex::new(Some(write_half)),
            socket_connected: AtomicBool::new(true),
            join_accepted: AtomicBool::new(false),
            terminal_rejection: AtomicBool::new(false),
            rejection: PlMutex::new(None),
            join_notify: tokio::sync::Notify::new(),
            closed: Arc::new(AtomicBool::new(false)),
            background_tasks: PlMutex::new(Vec::new()),
        });

        // Supervisor: runs the reader loop and, when the socket dies, reconnects
        // with backoff and re-joins the room so the server re-broadcasts the peer
        // list (which re-drives the connection handler to rebuild peers). The
        // fan-out subjects live on the Arc, so existing subscribers keep observing
        // across reconnects.
        let supervisor_client = Arc::clone(&client);
        let supervisor: JoinHandle<()> = tokio::spawn(async move {
            let mut read_half = read_half;
            let mut delay = SIGNALING_RECONNECT_BASE_DELAY;
            loop {
                let joined_before_disconnect = run_reader(&supervisor_client, &mut read_half).await;
                supervisor_client
                    .socket_connected
                    .store(false, Ordering::Release);
                supervisor_client
                    .join_accepted
                    .store(false, Ordering::Release);
                if supervisor_client.closed.load(Ordering::Acquire) {
                    return;
                }
                if supervisor_client.terminal_rejection.load(Ordering::Acquire) {
                    tracing::error!(
                        target: "ctox_rxdb::signaling_client",
                        "terminal signaling rejection; reconnect paused until configuration changes",
                    );
                    return;
                }
                // Drop the dead writer so send_frame fails fast until reconnect.
                *supervisor_client.writer.lock().await = None;
                if joined_before_disconnect {
                    delay = SIGNALING_RECONNECT_BASE_DELAY;
                }
                loop {
                    if supervisor_client.closed.load(Ordering::Acquire) {
                        return;
                    }
                    tokio::time::sleep(delay).await;
                    // Failover: re-derive the full candidate list, stay sticky
                    // on the index that last worked, rotate only on failure
                    // below. Rotation is independent of the backoff (which
                    // still resets only on a `joined` broadcast).
                    let fresh_candidates: Vec<String> = (supervisor_client.url_provider)()
                        .into_iter()
                        .filter(|url| !url.trim().is_empty())
                        .collect();
                    if fresh_candidates.is_empty() {
                        tracing::warn!(
                            target: "ctox_rxdb::signaling_client",
                            delay_secs = delay.as_secs(),
                            "signaling URL provider returned no usable URLs; backing off",
                        );
                        delay = (delay * 2).min(SIGNALING_RECONNECT_MAX_DELAY);
                        continue;
                    }
                    let rotation = supervisor_client.url_rotation.load(Ordering::Acquire);
                    let fresh_url = fresh_candidates[rotation % fresh_candidates.len()].clone();
                    match establish_ws(&fresh_url).await {
                        Ok((write_half, new_read)) => {
                            *supervisor_client.writer.lock().await = Some(write_half);
                            read_half = new_read;
                            supervisor_client
                                .socket_connected
                                .store(true, Ordering::Release);
                            let room = supervisor_client.joined_room.lock().clone();
                            if let Some(room) = room {
                                if let Err(e) = supervisor_client
                                    .send_frame(&ClientToServer::Join { room })
                                    .await
                                {
                                    tracing::warn!(
                                        target: "ctox_rxdb::signaling_client",
                                        "re-join after reconnect failed: {e}",
                                    );
                                }
                            }
                            tracing::info!(
                                target: "ctox_rxdb::signaling_client",
                                url = %fresh_url,
                                "signaling socket reconnected",
                            );
                            // A socket open is not a successful reconnect. Keep
                            // increasing backoff until a Joined frame proves
                            // that the control plane accepted us.
                            delay = (delay * 2).min(SIGNALING_RECONNECT_MAX_DELAY);
                            break;
                        }
                        Err(e) => {
                            // Rotate to the next failover candidate for the
                            // NEXT attempt; the backoff still applies in full.
                            supervisor_client
                                .url_rotation
                                .fetch_add(1, Ordering::AcqRel);
                            tracing::warn!(
                                target: "ctox_rxdb::signaling_client",
                                url = %fresh_url,
                                delay_secs = delay.as_secs(),
                                "signaling reconnect failed: {e}; rotating to next candidate and backing off",
                            );
                            delay = (delay * 2).min(SIGNALING_RECONNECT_MAX_DELAY);
                        }
                    }
                }
            }
        });
        client.background_tasks.lock().push(supervisor);

        // Keepalive: ping every SIMPLE_PEER_PING_INTERVAL_MS / 2. Resilient to the
        // reconnect window — a failed ping (socket momentarily down) is logged and
        // the loop continues; it resumes once the supervisor restores the writer.
        let ping_client = Arc::clone(&client);
        let ping_task: JoinHandle<()> = tokio::spawn(async move {
            let interval = Duration::from_millis(SIMPLE_PEER_PING_INTERVAL_MS / 2);
            loop {
                tokio::time::sleep(interval).await;
                if ping_client.closed.load(Ordering::Acquire) {
                    break;
                }
                if let Err(e) = ping_client.send_frame(&ClientToServer::Ping).await {
                    tracing::debug!(
                        target: "ctox_rxdb::signaling_client",
                        "keepalive ping failed: {e} — will retry after reconnect",
                    );
                }
            }
        });
        client.background_tasks.lock().push(ping_task);

        Ok(client)
    }

    /// Join a room. Server returns a `joined` broadcast with the room peer list.
    pub async fn join(self: &Arc<Self>, room: RoomId) -> Result<(), RxError> {
        *self.joined_room.lock() = Some(room.clone());
        self.join_accepted.store(false, Ordering::Release);
        self.terminal_rejection.store(false, Ordering::Release);
        *self.rejection.lock() = None;
        self.send_frame(&ClientToServer::Join { room }).await?;
        tokio::time::timeout(SIGNALING_CONNECT_TIMEOUT, async {
            while !self.join_accepted.load(Ordering::Acquire)
                && !self.terminal_rejection.load(Ordering::Acquire)
            {
                self.join_notify.notified().await;
            }
        })
        .await
        .map_err(|_| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": "signaling room join was not accepted before deadline",
                })),
            )
        })?;
        if let Some((code, reason)) = self.rejection.lock().clone() {
            return Err(new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": "signaling room join was rejected",
                    "code": code,
                    "reason": reason,
                    "retryable": false,
                })),
            ));
        }
        Ok(())
    }

    /// Send a signaling payload to a specific other peer.
    /// `data` carries the simple-peer SDP/ICE blob — we forward whatever shape
    /// the user provides.
    pub async fn send_signal(
        self: &Arc<Self>,
        receiver_peer_id: PeerId,
        data: serde_json::Value,
    ) -> Result<(), RxError> {
        let sender_peer_id = self.own_peer_id.get_value().ok_or_else(|| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": "send_signal before init frame received",
                })),
            )
        })?;
        let room = self.joined_room.lock().clone().ok_or_else(|| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": "send_signal before join",
                })),
            )
        })?;
        self.send_frame(&ClientToServer::Signal {
            room,
            sender_peer_id,
            receiver_peer_id,
            data,
        })
        .await
    }

    pub fn server_messages_stream(&self) -> RxStream<ServerToClient> {
        self.server_messages.subscribe()
    }

    pub fn peer_list_stream(&self) -> RxStream<Vec<PeerId>> {
        self.peer_list.subscribe()
    }

    /// Role a room member declared in its signaling-URL query
    /// (`browser`, `ctox_instance`, `desktop_shell`, …), if the server's
    /// `joined` broadcast carried peer descriptors.
    pub fn peer_role(&self, peer_id: &str) -> Option<String> {
        self.peer_roles.lock().get(peer_id).cloned()
    }

    pub fn own_peer_id(&self) -> Option<PeerId> {
        self.own_peer_id.get_value()
    }

    pub fn socket_connected(&self) -> bool {
        self.socket_connected.load(Ordering::Acquire)
    }

    pub fn join_accepted(&self) -> bool {
        self.join_accepted.load(Ordering::Acquire)
    }

    pub async fn close(self: &Arc<Self>) {
        // Stop the reconnect supervisor before aborting tasks so it does not race
        // to re-establish a socket we are tearing down on purpose.
        self.closed.store(true, Ordering::Release);
        // Abort background tasks.
        let tasks = std::mem::take(&mut *self.background_tasks.lock());
        for t in tasks.into_iter() {
            t.abort();
        }
        // Drop writer so the WebSocket closes.
        let mut writer = self.writer.lock().await;
        if let Some(mut w) = writer.take() {
            let _ = w.close().await;
        }
    }

    async fn send_frame(self: &Arc<Self>, frame: &ClientToServer) -> Result<(), RxError> {
        let text = serde_json::to_string(frame).map_err(|e| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": format!("serialize frame failed: {e}"),
                })),
            )
        })?;
        let mut writer = self.writer.lock().await;
        let w = writer.as_mut().ok_or_else(|| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({ "message": "signaling client is closed" })),
            )
        })?;
        w.send(Message::Text(text)).await.map_err(|e| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": format!("WebSocket send failed: {e}"),
                })),
            )
        })?;
        Ok(())
    }
}

fn install_rustls_crypto_provider() {
    RUSTLS_CRYPTO_PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Open one signaling WebSocket (with the bounded connect timeout) and return its
/// split write/read halves. Used both for the initial connect and by the
/// reconnect supervisor.
async fn establish_ws(url: &str) -> Result<(WsWrite, WsRead), RxError> {
    install_rustls_crypto_provider();
    let parsed = Url::parse(url).map_err(|e| {
        new_rx_error(
            "RC_WEBRTC_SIGNAL",
            Some(serde_json::json!({
                "message": format!("invalid signaling URL: {e}"),
                "url": url,
            })),
        )
    })?;
    let ws_stream =
        match tokio::time::timeout(SIGNALING_CONNECT_TIMEOUT, connect_resolved_ws(&parsed)).await {
            Ok(result) => result.map_err(|message| {
                new_rx_error(
                    "RC_WEBRTC_SIGNAL",
                    Some(serde_json::json!({
                        "message": format!("WebSocket connect failed: {message}"),
                        "url": url,
                    })),
                )
            })?,
            Err(_) => {
                return Err(new_rx_error(
                    "RC_WEBRTC_SIGNAL",
                    Some(serde_json::json!({
                        "message": format!(
                            "WebSocket connect timed out after {}s",
                            SIGNALING_CONNECT_TIMEOUT.as_secs()
                        ),
                        "url": url,
                    })),
                ));
            }
        };
    Ok(ws_stream.split())
}

/// Resolve signaling endpoints explicitly and try IPv4 before IPv6. Some
/// otherwise healthy networks advertise IPv6 but black-hole outbound IPv6;
/// `TcpStream::connect(hostname)` can then spend the entire outer deadline on
/// the first IPv6 address and never reach a working IPv4 address. Keeping the
/// fallback here also preserves IPv6-only deployments without introducing a
/// second transport or changing the WebSocket/TLS hostname used for SNI.
async fn connect_resolved_ws(parsed: &Url) -> Result<WsStream, String> {
    let host = parsed
        .host_str()
        .ok_or_else(|| "signaling URL has no host".to_owned())?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "signaling URL has no known port".to_owned())?;
    let mut addresses = lookup_host((host, port))
        .await
        .map_err(|error| format!("failed to resolve {host}:{port}: {error}"))?
        .collect::<Vec<_>>();
    prefer_ipv4_addresses(&mut addresses);
    addresses.dedup();
    if addresses.is_empty() {
        return Err(format!("no addresses resolved for {host}:{port}"));
    }

    let mut failures = Vec::new();
    for address in addresses {
        let stream = match tokio::time::timeout(
            SIGNALING_ADDRESS_CONNECT_TIMEOUT,
            TcpStream::connect(address),
        )
        .await
        {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => {
                failures.push(format!("{address}: {error}"));
                continue;
            }
            Err(_) => {
                failures.push(format!(
                    "{address}: timed out after {}s",
                    SIGNALING_ADDRESS_CONNECT_TIMEOUT.as_secs()
                ));
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        match client_async_tls(parsed.as_str(), stream).await {
            Ok((ws_stream, _response)) => return Ok(ws_stream),
            Err(error) => failures.push(format!(
                "{address}: WebSocket/TLS handshake failed: {error}"
            )),
        }
    }

    Err(format!(
        "all resolved signaling addresses failed ({})",
        failures.join("; ")
    ))
}

fn prefer_ipv4_addresses(addresses: &mut [SocketAddr]) {
    addresses.sort_by_key(|address| if address.is_ipv4() { 0 } else { 1 });
}

/// Decode frames off `read_half` and fan them out via the client's subjects.
/// Returns when the socket closes or errors, so the supervisor can reconnect.
async fn run_reader(client: &Arc<SignalingClient>, read_half: &mut WsRead) -> bool {
    let mut joined_seen = false;
    while let Some(item) = read_half.next().await {
        match item {
            Ok(Message::Text(text)) => match serde_json::from_str::<ServerToClient>(&text) {
                Ok(frame) => {
                    match &frame {
                        ServerToClient::Init { your_peer_id } => {
                            client.own_peer_id.next(Some(your_peer_id.clone()));
                        }
                        ServerToClient::Joined {
                            other_peer_ids,
                            peers,
                        } => {
                            joined_seen = true;
                            client.join_accepted.store(true, Ordering::Release);
                            client.join_notify.notify_waiters();
                            // Update the role map BEFORE emitting the peer
                            // list, so the connection handler's initiator
                            // decision sees fresh roles.
                            {
                                let mut roles = client.peer_roles.lock();
                                roles.clear();
                                for descriptor in peers.iter() {
                                    if !descriptor.peer_id.is_empty() {
                                        roles.insert(
                                            descriptor.peer_id.clone(),
                                            descriptor.role.clone(),
                                        );
                                    }
                                }
                            }
                            client.peer_list.next(other_peer_ids.clone());
                        }
                        ServerToClient::CtoxError {
                            scope,
                            code,
                            reason,
                        } => {
                            // The server closes the socket right after this
                            // frame. Without surfacing it, a rejected join
                            // (expired token, protocol/instance mismatch) is
                            // indistinguishable from a network blip and the
                            // supervisor reconnect-hammers silently.
                            tracing::warn!(
                                target: "ctox_rxdb::signaling_client",
                                scope = %scope,
                                code = %code,
                                reason = %reason,
                                "signaling server rejected this peer (control plane)",
                            );
                            if signaling_rejection_is_terminal(code) {
                                client.terminal_rejection.store(true, Ordering::Release);
                                *client.rejection.lock() = Some((code.clone(), reason.clone()));
                                client.join_notify.notify_waiters();
                            }
                        }
                        _ => {}
                    }
                    client.server_messages.next(frame);
                }
                Err(e) => {
                    tracing::warn!(
                        target: "ctox_rxdb::signaling_client",
                        "received unparseable frame: {e} ({text:?})",
                    );
                }
            },
            Ok(Message::Binary(_)) => {
                tracing::warn!(
                    target: "ctox_rxdb::signaling_client",
                    "received unexpected binary frame; ignoring",
                );
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    joined_seen
}

fn signaling_rejection_is_terminal(code: &str) -> bool {
    matches!(
        code,
        "protocol_missing"
            | "protocol_mismatch"
            | "instance_mismatch"
            | "peer_revoked"
            | "role_mismatch"
            | "token_invalid"
            | "token_signature_invalid"
            | "credentials_revoked"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn terminal_control_plane_rejections_do_not_reconnect_hammer() {
        assert!(signaling_rejection_is_terminal("protocol_mismatch"));
        assert!(signaling_rejection_is_terminal("peer_revoked"));
        assert!(!signaling_rejection_is_terminal(
            "control_plane_token_expired"
        ));
        assert!(!signaling_rejection_is_terminal("temporary_unavailable"));
    }

    #[test]
    fn signaling_addresses_prefer_ipv4_but_keep_ipv6_fallbacks() {
        let mut addresses = vec![
            "[2001:db8::1]:443".parse().unwrap(),
            "192.0.2.10:443".parse().unwrap(),
            "[2001:db8::2]:443".parse().unwrap(),
            "192.0.2.11:443".parse().unwrap(),
        ];
        prefer_ipv4_addresses(&mut addresses);
        assert!(addresses[0].is_ipv4());
        assert!(addresses[1].is_ipv4());
        assert!(addresses[2].is_ipv6());
        assert!(addresses[3].is_ipv6());
    }

    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    /// Chaos test for the reconnect supervisor (P7 / hardening C): when the
    /// signaling socket drops mid-session, the client must reconnect AND re-join
    /// the room on its own — without the caller calling `join` again — so the
    /// server re-broadcasts the peer list and the connection handler can rebuild.
    #[tokio::test]
    async fn signaling_client_reconnects_and_rejoins_after_socket_drop() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let conns = Arc::new(AtomicUsize::new(0));
        let joins = Arc::new(AtomicUsize::new(0));
        let conns_s = Arc::clone(&conns);
        let joins_s = Arc::clone(&joins);

        // Server: accept two connections. On each, send `init`, then read until a
        // `join` arrives (count it). Drop the FIRST connection right after its join
        // to force the client's supervisor to reconnect; the SECOND connection's
        // join therefore proves automatic re-join after reconnect.
        let server = tokio::spawn(async move {
            for i in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                conns_s.fetch_add(1, Ordering::SeqCst);
                let mut ws = accept_async(stream).await.unwrap();
                ws.send(Message::Text(
                    r#"{"type":"init","yourPeerId":"p1"}"#.to_string(),
                ))
                .await
                .unwrap();
                while let Some(Ok(msg)) = ws.next().await {
                    if let Message::Text(t) = msg {
                        if t.contains("\"type\":\"join\"") {
                            joins_s.fetch_add(1, Ordering::SeqCst);
                            break;
                        }
                    }
                }
                ws.send(Message::Text(
                    r#"{"type":"joined","otherPeerIds":[],"peers":[]}"#.to_string(),
                ))
                .await
                .unwrap();
                if i == 0 {
                    drop(ws); // force reconnect
                } else {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            }
        });

        let client = SignalingClient::connect(format!("ws://{addr}"))
            .await
            .unwrap();
        client.join("room-1".to_string()).await.unwrap();

        // Reconnect backoff base is 1s; allow generous time for conn #2 + re-join.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            if joins.load(Ordering::SeqCst) >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        client.close().await;
        let _ = server.await;

        assert!(
            conns.load(Ordering::SeqCst) >= 2,
            "client must reconnect after the socket dropped (saw {} connections)",
            conns.load(Ordering::SeqCst)
        );
        assert!(
            joins.load(Ordering::SeqCst) >= 2,
            "client must auto re-join the room after reconnect (saw {} joins)",
            joins.load(Ordering::SeqCst)
        );
    }

    /// Serve one accepted WebSocket signaling session: init frame, count the
    /// join, answer `joined`, then either drop or linger.
    async fn serve_one_signaling_session(
        listener: &TcpListener,
        joins: &Arc<AtomicUsize>,
        drop_after_join: bool,
    ) {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        ws.send(Message::Text(
            r#"{"type":"init","yourPeerId":"p1"}"#.to_string(),
        ))
        .await
        .unwrap();
        while let Some(Ok(msg)) = ws.next().await {
            if let Message::Text(t) = msg {
                if t.contains("\"type\":\"join\"") {
                    joins.fetch_add(1, Ordering::SeqCst);
                    break;
                }
            }
        }
        ws.send(Message::Text(
            r#"{"type":"joined","otherPeerIds":[],"peers":[]}"#.to_string(),
        ))
        .await
        .unwrap();
        if drop_after_join {
            drop(ws);
        } else {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    /// Failover chaos test (SYNC-31): the URL list is a real failover list.
    /// (1) Initial connect: the first candidate is dead, the client must land
    /// on the second. (2) Reconnect: after the socket drops with the working
    /// candidate gone too, the client must rotate through the list and re-join
    /// on a candidate that came back. Rotation must not bypass the
    /// backoff-resets-only-on-joined rule (unchanged supervisor logic).
    #[tokio::test]
    async fn signaling_client_fails_over_across_url_candidates() {
        // Dead candidate: bind a port, then close the listener so connects
        // are refused fast.
        let dead = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead_addr = dead.local_addr().unwrap();
        drop(dead);

        let live = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let live_addr = live.local_addr().unwrap();
        let joins = Arc::new(AtomicUsize::new(0));

        // Session 1 (initial connect must fail over dead -> live), dropped
        // after join to force the reconnect path; session 2 proves the
        // supervisor stayed on the live candidate and re-joined.
        let joins_s = Arc::clone(&joins);
        let server = tokio::spawn(async move {
            serve_one_signaling_session(&live, &joins_s, true).await;
            serve_one_signaling_session(&live, &joins_s, false).await;
        });

        let dead_url = format!("ws://{dead_addr}");
        let live_url = format!("ws://{live_addr}");
        let client = SignalingClient::connect_with_url_list_provider(move || {
            vec![dead_url.clone(), live_url.clone()]
        })
        .await
        .expect("initial connect must fail over to the live candidate");
        client.join("room-failover".to_string()).await.unwrap();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while tokio::time::Instant::now() < deadline {
            if joins.load(Ordering::SeqCst) >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        client.close().await;
        let _ = server.await;

        assert!(
            joins.load(Ordering::SeqCst) >= 2,
            "client must fail over to the live URL and auto re-join after a drop (saw {} joins)",
            joins.load(Ordering::SeqCst)
        );
    }

    /// SYNC-31: after the working candidate dies, the supervisor must rotate
    /// to the next candidate on the NEXT attempt instead of hammering the
    /// dead one forever.
    #[tokio::test]
    async fn signaling_client_rotates_to_next_candidate_when_current_dies() {
        let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let first_addr = first.local_addr().unwrap();
        let second = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let second_addr = second.local_addr().unwrap();
        let joins = Arc::new(AtomicUsize::new(0));

        // First server: one session, dropped after join, then the listener
        // itself goes away (its port starts refusing connections).
        let joins_a = Arc::clone(&joins);
        let server_a = tokio::spawn(async move {
            serve_one_signaling_session(&first, &joins_a, true).await;
            drop(first);
        });
        // Second server: accepts the failed-over reconnect.
        let joins_b = Arc::clone(&joins);
        let server_b = tokio::spawn(async move {
            serve_one_signaling_session(&second, &joins_b, false).await;
        });

        let first_url = format!("ws://{first_addr}");
        let second_url = format!("ws://{second_addr}");
        let client = SignalingClient::connect_with_url_list_provider(move || {
            vec![first_url.clone(), second_url.clone()]
        })
        .await
        .unwrap();
        client.join("room-rotate".to_string()).await.unwrap();

        // Attempt 1 after the drop hits the dead first URL (sticky index),
        // rotates, attempt 2 lands on the second URL: base 1s + 2s backoff.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        while tokio::time::Instant::now() < deadline {
            if joins.load(Ordering::SeqCst) >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        client.close().await;
        let _ = server_a.await;
        let _ = server_b.await;

        assert!(
            joins.load(Ordering::SeqCst) >= 2,
            "supervisor must rotate to the next candidate after the current one dies (saw {} joins)",
            joins.load(Ordering::SeqCst)
        );
    }
}
