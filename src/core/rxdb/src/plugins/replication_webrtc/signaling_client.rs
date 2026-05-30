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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Once;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex as PlMutex;
use tokio::net::TcpStream;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
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
    pub url: String,
    /// Stream of every server frame we received.
    server_messages: RxSubject<ServerToClient>,
    /// Holds the latest list of peer ids we're co-resident with.
    peer_list: RxBehaviorSubject<Vec<PeerId>>,
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
        let url_string = url.into();
        let (write_half, read_half) = establish_ws(&url_string).await?;
        let client = Arc::new(Self {
            url: url_string,
            server_messages: RxSubject::new(),
            peer_list: RxBehaviorSubject::new(Vec::new()),
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
                    match establish_ws(&supervisor_client.url).await {
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
                        ServerToClient::Joined { other_peer_ids } => {
                            client.peer_list.next(other_peer_ids.clone());
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
