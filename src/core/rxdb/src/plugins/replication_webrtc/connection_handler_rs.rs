//! **gap-item N5** — Rust-native WebRTC connection handler.
//!
//! Replaces upstream's `connection-handler-simple-peer.ts` (which wraps the
//! `simple-peer` NPM package). CTOX uses `webrtc-rs` for RTCPeerConnection /
//! DataChannel and the same simple-peer signaling server contract as the
//! browser bundle.
//!
//! Wire format on the DataChannel: one JSON `WebRTCWireFrame` per message,
//! matching upstream `JSON.stringify(messageOrResponse)` semantics.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use serde_json::Value;
use tokio_stream::StreamExt;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    register_default_interceptors, MediaEngine, PeerConnection, PeerConnectionBuilder,
    PeerConnectionEventHandler, RTCConfigurationBuilder, RTCIceCandidateInit, RTCIceServer,
    RTCPeerConnectionState, RTCSessionDescription, Registry,
};
use webrtc::runtime::default_runtime;

use crate::plugins::replication_webrtc::signaling_client::SignalingClient;
use crate::plugins::replication_webrtc::signaling_protocol::{PeerId, RoomId, ServerToClient};
use crate::plugins::replication_webrtc::webrtc_types::{
    PeerWithMessage, PeerWithResponse, WebRTCConnectionHandler, WebRTCResponse, WebRTCWireFrame,
};
use crate::rx_error::{new_rx_error, RxError, RxResult};
use crate::rxjs_compat::{RxStream, RxSubject};

/// Peer identifier assigned by the shared signaling server.
pub type WebRTCRsPeer = PeerId;

#[derive(Clone)]
pub struct WebRTCRsConfig {
    pub signaling: Arc<SignalingClient>,
    pub room: RoomId,
    pub ice_servers: Vec<RTCIceServer>,
    pub data_channel_label: String,
    pub udp_bind_addr: String,
}

impl WebRTCRsConfig {
    pub fn new(signaling: Arc<SignalingClient>, room: impl Into<RoomId>) -> Self {
        Self {
            signaling,
            room: room.into(),
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                ..Default::default()
            }],
            data_channel_label: "rxdb".to_string(),
            udp_bind_addr: "127.0.0.1:0".to_string(),
        }
    }
}

struct PeerEntry {
    peer_connection: Arc<dyn PeerConnection>,
    data_channel: Option<Arc<dyn DataChannel>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

/// WebRTC connection-handler implementation backed by `webrtc-rs`.
pub struct WebRTCRsConnectionHandler {
    connect_subject: RxSubject<WebRTCRsPeer>,
    disconnect_subject: RxSubject<WebRTCRsPeer>,
    message_subject: RxSubject<PeerWithMessage<WebRTCRsPeer>>,
    response_subject: RxSubject<PeerWithResponse<WebRTCRsPeer>>,
    error_subject: RxSubject<RxError>,
    peers: Arc<Mutex<HashMap<WebRTCRsPeer, PeerEntry>>>,
    signaling: Option<Arc<SignalingClient>>,
    ice_servers: Vec<RTCIceServer>,
    data_channel_label: String,
    udp_bind_addr: String,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl WebRTCRsConnectionHandler {
    /// Empty handler useful for unit tests or callers that install peers later.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::empty(None, Vec::new(), "rxdb", "127.0.0.1:0"))
    }

    pub async fn new_with_signaling(config: WebRTCRsConfig) -> RxResult<Arc<Self>> {
        let handler = Arc::new(Self::empty(
            Some(Arc::clone(&config.signaling)),
            config.ice_servers,
            &config.data_channel_label,
            &config.udp_bind_addr,
        ));
        wait_for_own_peer_id(&config.signaling).await?;
        config.signaling.join(config.room).await?;
        handler.start_signaling_tasks();
        Ok(handler)
    }

    fn empty(
        signaling: Option<Arc<SignalingClient>>,
        ice_servers: Vec<RTCIceServer>,
        data_channel_label: &str,
        udp_bind_addr: &str,
    ) -> Self {
        Self {
            connect_subject: RxSubject::new(),
            disconnect_subject: RxSubject::new(),
            message_subject: RxSubject::new(),
            response_subject: RxSubject::new(),
            error_subject: RxSubject::new(),
            peers: Arc::new(Mutex::new(HashMap::new())),
            signaling,
            ice_servers,
            data_channel_label: data_channel_label.to_string(),
            udp_bind_addr: udp_bind_addr.to_string(),
            tasks: Mutex::new(Vec::new()),
        }
    }

    fn start_signaling_tasks(self: &Arc<Self>) {
        let Some(signaling) = self.signaling.as_ref().cloned() else {
            return;
        };

        let handler = Arc::clone(self);
        let signaling_for_peers = Arc::clone(&signaling);
        let mut peer_list_stream = signaling.peer_list_stream();
        let peer_task = tokio::spawn(async move {
            while let Some(peer_ids) = peer_list_stream.next().await {
                let own_peer_id = signaling_for_peers.own_peer_id();
                for remote_peer_id in peer_ids {
                    if Some(remote_peer_id.as_str()) == own_peer_id.as_deref()
                        || handler.peers.lock().contains_key(&remote_peer_id)
                    {
                        continue;
                    }
                    let is_initiator = own_peer_id
                        .as_ref()
                        .map(|own| remote_peer_id.as_str() > own.as_str())
                        .unwrap_or(false);
                    if let Err(err) = handler
                        .ensure_peer_connection(remote_peer_id, is_initiator)
                        .await
                    {
                        handler.error_subject.next(err);
                    }
                }
            }
        });
        self.tasks.lock().push(peer_task);

        let handler = Arc::clone(self);
        let mut signal_stream = signaling.server_messages_stream();
        let signal_task = tokio::spawn(async move {
            while let Some(frame) = signal_stream.next().await {
                let ServerToClient::Signal {
                    sender_peer_id,
                    data,
                    ..
                } = frame
                else {
                    continue;
                };
                if let Err(err) = handler.handle_signal(sender_peer_id, data).await {
                    handler.error_subject.next(err);
                }
            }
        });
        self.tasks.lock().push(signal_task);
    }

    async fn ensure_peer_connection(
        self: &Arc<Self>,
        remote_peer_id: PeerId,
        initiator: bool,
    ) -> RxResult<Arc<dyn PeerConnection>> {
        if let Some(existing) = self
            .peers
            .lock()
            .get(&remote_peer_id)
            .map(|entry| Arc::clone(&entry.peer_connection))
        {
            return Ok(existing);
        }

        let signaling = self.signaling.as_ref().cloned().ok_or_else(|| {
            new_rx_error(
                "RC_WEBRTC_SIGNAL",
                Some(serde_json::json!({ "message": "missing signaling client" })),
            )
        })?;

        let pc = build_peer_connection(
            Arc::clone(self),
            Arc::clone(&signaling),
            remote_peer_id.clone(),
        )
        .await?;
        self.peers.lock().insert(
            remote_peer_id.clone(),
            PeerEntry {
                peer_connection: Arc::clone(&pc),
                data_channel: None,
                tasks: Vec::new(),
            },
        );

        if initiator {
            let data_channel = pc
                .create_data_channel(&self.data_channel_label, None)
                .await
                .map_err(|e| webrtc_error("create data channel", e))?;
            install_data_channel(Arc::clone(self), remote_peer_id.clone(), data_channel);
            let offer = pc
                .create_offer(None)
                .await
                .map_err(|e| webrtc_error("create offer", e))?;
            pc.set_local_description(offer)
                .await
                .map_err(|e| webrtc_error("set local offer", e))?;
            if let Some(local_description) = pc.local_description().await {
                signaling
                    .send_signal(
                        remote_peer_id,
                        serde_json::to_value(local_description).unwrap_or(Value::Null),
                    )
                    .await?;
            }
        }

        Ok(pc)
    }

    async fn handle_signal(self: &Arc<Self>, remote_peer_id: PeerId, data: Value) -> RxResult<()> {
        let pc = self
            .ensure_peer_connection(remote_peer_id.clone(), false)
            .await?;
        if data.get("sdp").is_some() {
            let description: RTCSessionDescription =
                serde_json::from_value(data.clone()).map_err(|e| {
                    new_rx_error(
                        "RC_WEBRTC_SIGNAL",
                        Some(serde_json::json!({
                            "message": format!("decode SDP signal failed: {e}"),
                            "signal": data,
                        })),
                    )
                })?;
            let is_offer = data.get("type").and_then(Value::as_str) == Some("offer");
            pc.set_remote_description(description)
                .await
                .map_err(|e| webrtc_error("set remote description", e))?;
            if is_offer {
                let answer = pc
                    .create_answer(None)
                    .await
                    .map_err(|e| webrtc_error("create answer", e))?;
                pc.set_local_description(answer)
                    .await
                    .map_err(|e| webrtc_error("set local answer", e))?;
                if let (Some(signaling), Some(local_description)) =
                    (self.signaling.as_ref(), pc.local_description().await)
                {
                    signaling
                        .send_signal(
                            remote_peer_id,
                            serde_json::to_value(local_description).unwrap_or(Value::Null),
                        )
                        .await?;
                }
            }
        } else if data.get("candidate").is_some() {
            let candidate = decode_simple_peer_ice_candidate(&data).map_err(|e| {
                new_rx_error(
                    "RC_WEBRTC_SIGNAL",
                    Some(serde_json::json!({
                        "message": format!("decode ICE signal failed: {e}"),
                        "signal": data,
                    })),
                )
            })?;
            pc.add_ice_candidate(candidate)
                .await
                .map_err(|e| webrtc_error("add ice candidate", e))?;
        }
        Ok(())
    }
}

#[async_trait]
impl WebRTCConnectionHandler for WebRTCRsConnectionHandler {
    type Peer = WebRTCRsPeer;

    fn connect_stream(&self) -> RxStream<Self::Peer> {
        self.connect_subject.subscribe()
    }
    fn disconnect_stream(&self) -> RxStream<Self::Peer> {
        self.disconnect_subject.subscribe()
    }
    fn message_stream(&self) -> RxStream<PeerWithMessage<Self::Peer>> {
        self.message_subject.subscribe()
    }
    fn response_stream(&self) -> RxStream<PeerWithResponse<Self::Peer>> {
        self.response_subject.subscribe()
    }
    fn error_stream(&self) -> RxStream<RxError> {
        self.error_subject.subscribe()
    }

    async fn send(&self, peer: &Self::Peer, frame: WebRTCWireFrame) -> Result<(), RxError> {
        let data_channel = self
            .peers
            .lock()
            .get(peer)
            .and_then(|entry| entry.data_channel.clone())
            .ok_or_else(|| {
                new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(serde_json::json!({
                        "message": "unknown or unopened peer",
                        "peer": peer,
                    })),
                )
            })?;
        let text = serde_json::to_string(&frame).map_err(|e| {
            new_rx_error(
                "RC_WEBRTC_PEER",
                Some(serde_json::json!({
                    "message": format!("serialize WebRTC frame failed: {e}"),
                    "peer": peer,
                })),
            )
        })?;
        data_channel
            .send_text(&text)
            .await
            .map_err(|e| webrtc_error("send data channel frame", e))
    }

    async fn close(&self) -> Result<(), RxError> {
        let tasks = std::mem::take(&mut *self.tasks.lock());
        for task in tasks {
            task.abort();
        }
        let peers = std::mem::take(&mut *self.peers.lock());
        for (peer, mut entry) in peers {
            for task in entry.tasks.drain(..) {
                task.abort();
            }
            if let Some(data_channel) = entry.data_channel {
                let _ = data_channel.close().await;
            }
            let _ = entry.peer_connection.close().await;
            self.disconnect_subject.next(peer);
        }
        if let Some(signaling) = &self.signaling {
            signaling.close().await;
        }
        Ok(())
    }
}

struct RsPeerConnectionEvents {
    handler: Arc<WebRTCRsConnectionHandler>,
    signaling: Arc<SignalingClient>,
    remote_peer_id: PeerId,
}

#[async_trait]
impl PeerConnectionEventHandler for RsPeerConnectionEvents {
    async fn on_ice_candidate(&self, event: webrtc::peer_connection::RTCPeerConnectionIceEvent) {
        match event.candidate.to_json() {
            Ok(candidate) => {
                let data = simple_peer_ice_signal(candidate);
                if let Err(err) = self
                    .signaling
                    .send_signal(self.remote_peer_id.clone(), data)
                    .await
                {
                    self.handler.error_subject.next(err);
                }
            }
            Err(err) => self
                .handler
                .error_subject
                .next(webrtc_error("serialize ice candidate", err)),
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if matches!(
            state,
            RTCPeerConnectionState::Failed
                | RTCPeerConnectionState::Closed
                | RTCPeerConnectionState::Disconnected
        ) {
            remove_peer(&self.handler, &self.remote_peer_id);
        }
    }

    async fn on_data_channel(&self, data_channel: Arc<dyn DataChannel>) {
        install_data_channel(
            Arc::clone(&self.handler),
            self.remote_peer_id.clone(),
            data_channel,
        );
    }
}

async fn build_peer_connection(
    handler: Arc<WebRTCRsConnectionHandler>,
    signaling: Arc<SignalingClient>,
    remote_peer_id: PeerId,
) -> RxResult<Arc<dyn PeerConnection>> {
    let event_handler = Arc::new(RsPeerConnectionEvents {
        handler: Arc::clone(&handler),
        signaling,
        remote_peer_id,
    });

    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .map_err(|e| webrtc_error("register default codecs", e))?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .map_err(|e| webrtc_error("register default interceptors", e))?;
    let runtime = default_runtime().ok_or_else(|| {
        new_rx_error(
            "RC_WEBRTC_PEER",
            Some(serde_json::json!({ "message": "no async runtime for webrtc-rs" })),
        )
    })?;
    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(handler.ice_servers.clone())
        .build();

    let pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(event_handler)
        .with_runtime(runtime)
        .with_udp_addrs(vec![handler.udp_bind_addr.clone()])
        .build()
        .await
        .map_err(|e| webrtc_error("build peer connection", e))?;
    Ok(Arc::new(pc))
}

fn install_data_channel(
    handler: Arc<WebRTCRsConnectionHandler>,
    remote_peer_id: PeerId,
    data_channel: Arc<dyn DataChannel>,
) {
    {
        let mut peers = handler.peers.lock();
        if let Some(entry) = peers.get_mut(&remote_peer_id) {
            entry.data_channel = Some(Arc::clone(&data_channel));
        }
    }

    let message_subject = handler.message_subject.clone();
    let response_subject = handler.response_subject.clone();
    let connect_subject = handler.connect_subject.clone();
    let disconnect_subject = handler.disconnect_subject.clone();
    let error_subject = handler.error_subject.clone();
    let peer_for_task = remote_peer_id.clone();
    let task = tokio::spawn(async move {
        while let Some(event) = data_channel.poll().await {
            match event {
                DataChannelEvent::OnOpen => connect_subject.next(peer_for_task.clone()),
                DataChannelEvent::OnMessage(msg) => {
                    let text = String::from_utf8_lossy(&msg.data).to_string();
                    match serde_json::from_str::<Value>(&text) {
                        Ok(value)
                            if value.get("result").is_some() || value.get("error").is_some() =>
                        {
                            match serde_json::from_value::<WebRTCResponse>(value) {
                                Ok(response) => response_subject.next(PeerWithResponse {
                                    peer: peer_for_task.clone(),
                                    response,
                                }),
                                Err(err) => {
                                    error_subject.next(decode_error("response", err, &text))
                                }
                            }
                        }
                        Ok(value) => match serde_json::from_value(value) {
                            Ok(message) => message_subject.next(PeerWithMessage {
                                peer: peer_for_task.clone(),
                                message,
                            }),
                            Err(err) => error_subject.next(decode_error("message", err, &text)),
                        },
                        Err(err) => error_subject.next(decode_error("frame", err, &text)),
                    }
                }
                DataChannelEvent::OnClose => {
                    disconnect_subject.next(peer_for_task.clone());
                    break;
                }
                DataChannelEvent::OnError => {
                    error_subject.next(new_rx_error(
                        "RC_WEBRTC_PEER",
                        Some(serde_json::json!({
                            "message": "data channel error",
                            "peer": peer_for_task,
                        })),
                    ));
                }
                _ => {}
            }
        }
    });

    if let Some(entry) = handler.peers.lock().get_mut(&remote_peer_id) {
        entry.tasks.push(task);
    }
}

fn remove_peer(handler: &Arc<WebRTCRsConnectionHandler>, peer: &str) {
    if let Some(mut entry) = handler.peers.lock().remove(peer) {
        for task in entry.tasks.drain(..) {
            task.abort();
        }
        let peer_id = peer.to_string();
        tokio::spawn(async move {
            if let Some(data_channel) = entry.data_channel {
                let _ = data_channel.close().await;
            }
            let _ = entry.peer_connection.close().await;
        });
        handler.disconnect_subject.next(peer_id);
    }
}

async fn wait_for_own_peer_id(signaling: &Arc<SignalingClient>) -> RxResult<PeerId> {
    for _ in 0..100 {
        if let Some(peer_id) = signaling.own_peer_id() {
            return Ok(peer_id);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(new_rx_error(
        "RC_WEBRTC_SIGNAL",
        Some(serde_json::json!({ "message": "timed out waiting for signaling init" })),
    ))
}

fn decode_error(kind: &str, err: serde_json::Error, text: &str) -> RxError {
    new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({
            "message": format!("decode WebRTC {kind} failed: {err}"),
            "frame": text,
        })),
    )
}

fn webrtc_error(context: &str, err: impl std::fmt::Display) -> RxError {
    new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({
            "message": format!("{context}: {err}"),
        })),
    )
}

fn simple_peer_ice_signal(candidate: RTCIceCandidateInit) -> Value {
    serde_json::json!({
        "type": "candidate",
        "candidate": candidate,
    })
}

fn decode_simple_peer_ice_candidate(
    data: &Value,
) -> Result<RTCIceCandidateInit, serde_json::Error> {
    let candidate_value = match data.get("candidate") {
        Some(candidate) if candidate.is_object() => candidate.clone(),
        Some(candidate) if candidate.is_string() => data.clone(),
        _ => data.clone(),
    };
    serde_json::from_value(candidate_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::replication_webrtc::webrtc_types::{WebRTCMessage, WebRTCResponse};

    #[test]
    fn classifies_wire_frames_by_result_or_error_field() {
        let response = serde_json::to_value(WebRTCWireFrame::Response(WebRTCResponse {
            id: "r1".to_string(),
            result: Value::Null,
            error: None,
        }))
        .unwrap();
        let message = serde_json::to_value(WebRTCWireFrame::Message(WebRTCMessage {
            id: "m1".to_string(),
            method: "token".to_string(),
            params: Vec::new(),
        }))
        .unwrap();

        assert!(response.get("result").is_some() || response.get("error").is_some());
        assert!(message.get("result").is_none() && message.get("error").is_none());
    }

    #[test]
    fn wraps_ice_candidates_for_simple_peer_signal_shape() {
        let signal = simple_peer_ice_signal(RTCIceCandidateInit {
            candidate: "candidate:1 1 udp 1 127.0.0.1 123 typ host".to_string(),
            sdp_mid: Some("0".to_string()),
            sdp_mline_index: Some(0),
            username_fragment: Some("ufrag".to_string()),
            url: None,
        });

        assert_eq!(
            signal.get("type").and_then(Value::as_str),
            Some("candidate")
        );
        assert_eq!(
            signal
                .get("candidate")
                .and_then(|candidate| candidate.get("sdpMid"))
                .and_then(Value::as_str),
            Some("0")
        );
        assert_eq!(
            signal
                .get("candidate")
                .and_then(|candidate| candidate.get("sdpMLineIndex"))
                .and_then(Value::as_u64),
            Some(0)
        );
    }

    #[test]
    fn decodes_simple_peer_candidate_wrapper() {
        let signal = serde_json::json!({
            "type": "candidate",
            "candidate": {
                "candidate": "candidate:1 1 udp 1 127.0.0.1 123 typ host",
                "sdpMid": "0",
                "sdpMLineIndex": 0,
                "usernameFragment": "ufrag"
            }
        });

        let candidate = decode_simple_peer_ice_candidate(&signal).unwrap();

        assert_eq!(candidate.sdp_mid.as_deref(), Some("0"));
        assert_eq!(candidate.sdp_mline_index, Some(0));
        assert_eq!(candidate.username_fragment.as_deref(), Some("ufrag"));
    }
}
