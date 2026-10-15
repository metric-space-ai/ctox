//! Test-only stdio controller around the real RxDB WebRTC transport, no apps/store.
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::StreamExt;
use rxdb::plugins::replication_webrtc::connection_handler_rs::{
    WebRTCRsConfig, WebRTCRsConnection, WebRTCRsConnectionHandler,
};
use rxdb::plugins::replication_webrtc::signaling_client::SignalingClient;
use rxdb::plugins::replication_webrtc::webrtc_types::{
    WebRTCConnectionHandler, WebRTCMessage, WebRTCResponse,
};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use webrtc::peer_connection::RTCIceServer;

fn emit(row: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{row}");
    let _ = out.flush();
}

fn bulk_payload(bytes: usize) -> String {
    let mut seed = 0x12345678u32;
    let bytes: Vec<u8> = (0..bytes)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            32 + ((seed >> 24) % 95) as u8
        })
        .collect();
    String::from_utf8(bytes).expect("ASCII fixture")
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let config: Value = serde_json::from_str(&lines.next_line().await?.ok_or("missing config")?)?;
    let signaling =
        SignalingClient::connect(config["signalingUrl"].as_str().ok_or("signalingUrl")?).await?;
    let mut options =
        WebRTCRsConfig::new(signaling.clone(), config["room"].as_str().ok_or("room")?);
    options.ice_servers = if config["nativeHostOnly"] == true {
        vec![]
    } else {
        vec![RTCIceServer {
            urls: vec![config["turnUrl"].as_str().ok_or("turnUrl")?.to_string()],
            username: "bench".into(),
            credential: "bench".into(),
            ..Default::default()
        }]
    };
    options.udp_bind_addr = "192.0.2.1:0".into();
    let handler = WebRTCRsConnectionHandler::prepare_with_signaling(options).await?;
    let connections: Arc<Mutex<HashMap<String, WebRTCRsConnection>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let pending: Arc<Mutex<HashMap<String, Instant>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut connected = handler.connect_stream();
    let mut disconnected = handler.disconnect_stream();
    let mut messages = handler.message_stream();
    let mut responses = handler.response_stream();
    let mut errors = handler.error_stream();
    let table = connections.clone();
    tokio::spawn(async move {
        while let Some(peer) = connected.next().await {
            table
                .lock()
                .unwrap()
                .insert(peer.peer_id().to_string(), peer.clone());
            emit(
                json!({"kind":"connected", "peer":peer.peer_id(), "generation":peer.generation()}),
            );
        }
    });
    let table = connections.clone();
    tokio::spawn(async move {
        while let Some(peer) = disconnected.next().await {
            let mut table = table.lock().unwrap();
            if table.get(peer.peer_id()) == Some(&peer) {
                table.remove(peer.peer_id());
            }
            emit(json!({"kind":"disconnected", "peer":peer.peer_id()}));
        }
    });
    let responder = handler.clone();
    tokio::spawn(async move {
        while let Some(event) = messages.next().await {
            let envelope = event.message.params.first().cloned().unwrap_or(Value::Null);
            let id = event.message.id.clone();
            let class = envelope["class"].as_str().unwrap_or("unknown").to_string();
            let body_bytes = envelope["body"].as_str().map(str::len).unwrap_or(0);
            emit(
                json!({"kind":"received", "id":id, "class":class, "bytes":body_bytes, "peer":event.peer.peer_id()}),
            );
            let responder = responder.clone();
            tokio::spawn(async move {
                let frame = WebRTCResponse {
                    id,
                    result: json!({"benchAck":true}),
                    error: None,
                    collection: None,
                };
                if let Err(error) = responder.send(&event.peer, frame.into()).await {
                    emit(json!({"kind":"ack_error", "code":error.code()}));
                }
            });
        }
    });
    let waits = pending.clone();
    tokio::spawn(async move {
        while let Some(event) = responses.next().await {
            if let Some(start) = waits.lock().unwrap().remove(&event.response.id) {
                emit(
                    json!({"kind":"ack", "id":event.response.id, "latencyMs":start.elapsed().as_secs_f64()*1000.0,
                    "error":event.response.error.is_some()}),
                );
            }
        }
    });
    tokio::spawn(async move {
        while let Some(error) = errors.next().await {
            emit(json!({"kind":"transport_error", "code":error.code()}));
        }
    });
    signaling
        .join(config["room"].as_str().unwrap().to_string())
        .await?;
    let bulk = Arc::new(bulk_payload(5_000_000));
    let permits = Arc::new(tokio::sync::Semaphore::new(32));
    emit(
        json!({"kind":"ready", "peer":signaling.own_peer_id(), "wire":"ctox.transport.bench.stdio.v1"}),
    );
    while let Some(line) = lines.next_line().await? {
        let request: Value = serde_json::from_str(&line)?;
        if request["kind"] == "shutdown" {
            break;
        }
        if request["kind"] == "status" {
            emit(json!({"kind":"status", "transport":handler.frame_transport_status_json()}));
            continue;
        }
        if request["kind"] != "send" {
            continue;
        }
        let permit = permits.clone().acquire_owned().await?;
        let handler = handler.clone();
        let table = connections.clone();
        let pending = pending.clone();
        let bulk = bulk.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let id = request["id"].as_str().unwrap_or("").to_string();
            let class = request["class"].as_str().unwrap_or("unknown").to_string();
            let peer_name = request["peer"].as_str().unwrap_or("").to_string();
            let peer = table.lock().unwrap().get(&peer_name).cloned();
            let bytes = if class == "bulk" {
                5_000_000
            } else {
                "small control fixture".len()
            };
            let Some(peer) = peer else {
                emit(
                    json!({"kind":"send_result","id":id,"class":class,"bytes":bytes,"ok":false,"code":"not_connected"}),
                );
                return;
            };
            let body = if class == "bulk" {
                bulk.as_str()
            } else {
                "small control fixture"
            };
            let method = if class == "control" {
                "ctoxProtocol"
            } else {
                "masterWrite"
            };
            let message = WebRTCMessage {
                id: id.clone(),
                method: method.into(),
                params: vec![json!({"id":id,"class":class,"body":body})],
                collection: Some(format!("bench_{class}")),
            };
            let start = Instant::now();
            pending.lock().unwrap().insert(id.clone(), start);
            let sent = handler.send(&peer, message.into()).await;
            if sent.is_err() {
                pending.lock().unwrap().remove(&id);
            }
            emit(
                json!({"kind":"send_result","id":id,"class":class,"bytes":body.len(),"ok":sent.is_ok(),
                "code":sent.err().map(|e|e.code().to_string()), "sendCompletionMs":start.elapsed().as_secs_f64()*1000.0}),
            );
        });
    }
    handler.close().await?;
    signaling.close().await;
    Ok(())
}
