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
use std::sync::Arc;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex as PlMutex;
use tokio::net::TcpStream;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use url::Url;

use crate::plugins::replication_webrtc::signaling_protocol::{
    ClientToServer, PeerId, RoomId, SIMPLE_PEER_PING_INTERVAL_MS, ServerToClient,
};
use crate::rx_error::{RxError, new_rx_error};
use crate::rxjs_compat::{RxBehaviorSubject, RxStream, RxSubject};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// FIX 3: bound the WebSocket handshake. An unreachable or slow signaling
/// server used to let `connect_async` hang indefinitely, wedging the
/// per-collection bring-up that awaits it. 20s matches the per-collection
/// timeout enforced at the bring-up loop.
const SIGNALING_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

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
    /// Produces the URL for every (re)connect attempt. Time-windowed query
    /// params (`token_iat`/`token_exp`, TTL 24h) used to be frozen into the
    /// connect-time URL; after >24h uptime any socket drop then turned into a
    /// permanent join-rejection loop. The provider recomputes them per attempt.
    url_provider: Arc<dyn Fn() -> String + Send + Sync>,
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
        let url_provider: Arc<dyn Fn() -> String + Send + Sync> = Arc::new(url_provider);
        let url_string = url_provider();
        let (write_half, read_half) = establish_ws(&url_string).await?;
        let client = Arc::new(Self {
            url: url_string,
            url_provider,
            server_messages: RxSubject::new(),
            peer_list: RxBehaviorSubject::new(Vec::new()),
            peer_roles: PlMutex::new(HashMap::new()),
            own_peer_id: RxBehaviorSubject::new(None),
            joined_room: PlMutex::new(None),
            writer: TokioMutex::new(Some(write_half)),
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
            loop {
                run_reader(&supervisor_client, &mut read_half).await;
                if supervisor_client.closed.load(Ordering::Acquire) {
                    return;
                }
                // Drop the dead writer so send_frame fails fast until reconnect.
                *supervisor_client.writer.lock().await = None;
                let mut delay = SIGNALING_RECONNECT_BASE_DELAY;
                loop {
                    if supervisor_client.closed.load(Ordering::Acquire) {
                        return;
                    }
                    tokio::time::sleep(delay).await;
                    let fresh_url = (supervisor_client.url_provider)();
                    match establish_ws(&fresh_url).await {
                        Ok((write_half, new_read)) => {
                            *supervisor_client.writer.lock().await = Some(write_half);
                            read_half = new_read;
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
                                url = %supervisor_client.url,
                                "signaling socket reconnected",
                            );
                            break;
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "ctox_rxdb::signaling_client",
                                delay_secs = delay.as_secs(),
                                "signaling reconnect failed: {e}; backing off",
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
        self.send_frame(&ClientToServer::Join { room }).await
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
    let (ws_stream, _resp) =
        match tokio::time::timeout(SIGNALING_CONNECT_TIMEOUT, connect_async(parsed.as_str())).await
        {
            Ok(result) => result.map_err(|e| {
                new_rx_error(
                    "RC_WEBRTC_SIGNAL",
                    Some(serde_json::json!({
                        "message": format!("WebSocket connect failed: {e}"),
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

/// Decode frames off `read_half` and fan them out via the client's subjects.
/// Returns when the socket closes or errors, so the supervisor can reconnect.
async fn run_reader(client: &Arc<SignalingClient>, read_half: &mut WsRead) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
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
}
