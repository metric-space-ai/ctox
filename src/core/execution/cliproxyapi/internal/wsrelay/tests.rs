// Origin: CTOX
// License: AGPL-3.0-only

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Map, Value};
use tokio::io::DuplexStream;
use tokio_tungstenite::tungstenite::protocol::frame::coding::{Data, OpCode};
use tokio_tungstenite::tungstenite::protocol::frame::Frame;
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;
use tokio_tungstenite::WebSocketStream;

use super::*;

struct FixedAuthority(&'static str);

impl RelayAuthority for FixedAuthority {
    fn authorize(&self, _request: &RelayHandshake) -> Result<String, RelayError> {
        Ok(self.0.into())
    }
}

#[derive(Default)]
struct CaptureEvents {
    connected: Mutex<Vec<String>>,
    disconnected: Mutex<Vec<(String, RelayError)>>,
    unknown: Mutex<Vec<(String, String)>>,
}

impl RelayEventSink for CaptureEvents {
    fn connected(&self, provider: &str) {
        self.connected.lock().unwrap().push(provider.into());
    }

    fn disconnected(&self, provider: &str, cause: &RelayError) {
        self.disconnected
            .lock()
            .unwrap()
            .push((provider.into(), cause.clone()));
    }

    fn unknown_terminal_message(&self, provider: &str, message_id: &str) {
        self.unknown
            .lock()
            .unwrap()
            .push((provider.into(), message_id.into()));
    }
}

fn manager(events: Option<Arc<CaptureEvents>>, limits: RelayLimits) -> Manager {
    Manager::new(ManagerOptions {
        path: "v1/ws".into(),
        authority: Arc::new(FixedAuthority("  Studio-A  ")),
        events: events.map(|events| events as Arc<dyn RelayEventSink>),
        clock: None,
        limits,
    })
    .unwrap()
}

async fn websocket_pair() -> (WebSocketStream<DuplexStream>, WebSocketStream<DuplexStream>) {
    let (server_io, client_io) = tokio::io::duplex(256 * 1024);
    let (server, client) = tokio::join!(
        tokio_tungstenite::accept_async(server_io),
        tokio_tungstenite::client_async("ws://localhost/v1/ws", client_io)
    );
    (server.unwrap(), client.unwrap().0)
}

async fn connect(manager: &Manager) -> WebSocketStream<DuplexStream> {
    let (server, client) = websocket_pair().await;
    let provider = manager
        .accept_websocket(
            RelayHandshake {
                method: "get".into(),
                path: "/v1/ws".into(),
                ..RelayHandshake::default()
            },
            server,
        )
        .await
        .unwrap();
    assert_eq!(provider, "studio-a");
    client
}

fn payload(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    entries
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}

fn response_message(id: &str, kind: &str, body: &str) -> Message {
    Message::with_payload(
        id,
        kind,
        payload([
            ("status", json!(201)),
            ("headers", json!({"X-Test":["a","b"]})),
            ("body", json!(body)),
        ]),
    )
}
