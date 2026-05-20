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

use std::sync::Arc;
use std::sync::Once;

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

static RUSTLS_CRYPTO_PROVIDER: Once = Once::new();

pub struct SignalingClient {
    pub url: String,
    /// Stream of every server frame we received.
    server_messages: RxSubject<ServerToClient>,
    /// Holds the latest list of peer ids we're co-resident with.
    peer_list: RxBehaviorSubject<Vec<PeerId>>,
    /// Our server-issued peer id, once the `init` frame arrives.
    own_peer_id: RxBehaviorSubject<Option<PeerId>>,
    joined_room: PlMutex<Option<RoomId>>,
    /// Send half of the WebSocket. `None` until the connect future resolves.
    writer: TokioMutex<Option<futures::stream::SplitSink<WsStream, Message>>>,
    background_tasks: PlMutex<Vec<JoinHandle<()>>>,
}

impl SignalingClient {
    /// Connect to a signaling server (e.g. `ws://localhost:8080`).
    pub async fn connect(url: impl Into<String>) -> Result<Arc<Self>, RxError> {
        install_rustls_crypto_provider();
        let url_string = url.into();
        let parsed = Url::parse(&url_string).map_err(|e| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": format!("invalid signaling URL: {e}"),
                    "url": &url_string,
                })),
            )
        })?;
        let (ws_stream, _resp) = connect_async(parsed.as_str()).await.map_err(|e| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({
                    "message": format!("WebSocket connect failed: {e}"),
                    "url": &url_string,
                })),
            )
        })?;
        let (write_half, mut read_half) = ws_stream.split();
        let client = Arc::new(Self {
            url: url_string,
            server_messages: RxSubject::new(),
            peer_list: RxBehaviorSubject::new(Vec::new()),
            own_peer_id: RxBehaviorSubject::new(None),
            joined_room: PlMutex::new(None),
            writer: TokioMutex::new(Some(write_half)),
            background_tasks: PlMutex::new(Vec::new()),
        });

        // Reader loop: decode frames, fan out via subjects.
        let reader_client = Arc::clone(&client);
        let reader_task: JoinHandle<()> = tokio::spawn(async move {
            while let Some(item) = read_half.next().await {
                match item {
                    Ok(Message::Text(text)) => {
                        let parsed: Result<ServerToClient, _> = serde_json::from_str(&text);
                        match parsed {
                            Ok(frame) => {
                                match &frame {
                                    ServerToClient::Init { your_peer_id } => {
                                        reader_client.own_peer_id.next(Some(your_peer_id.clone()));
                                    }
                                    ServerToClient::Joined { other_peer_ids } => {
                                        reader_client.peer_list.next(other_peer_ids.clone());
                                    }
                                    _ => {}
                                }
                                reader_client.server_messages.next(frame);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    target: "ctox_rxdb::signaling_client",
                                    "received unparseable frame: {e} ({text:?})",
                                );
                            }
                        }
                    }
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
        });
        client.background_tasks.lock().push(reader_task);

        // Keepalive: ping every SIMPLE_PEER_PING_INTERVAL_MS / 2.
        let ping_client = Arc::clone(&client);
        let ping_task: JoinHandle<()> = tokio::spawn(async move {
            let interval = Duration::from_millis(SIMPLE_PEER_PING_INTERVAL_MS / 2);
            loop {
                tokio::time::sleep(interval).await;
                if let Err(e) = ping_client.send_frame(&ClientToServer::Ping).await {
                    tracing::debug!(
                        target: "ctox_rxdb::signaling_client",
                        "keepalive ping failed: {e} — stopping",
                    );
                    break;
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
