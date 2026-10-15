mod store_source;
// Test-only stdio controller around the real RxDB WebRTC transport, no apps/store.
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

fn hello_template(
    nonce: String,
    device: String,
    seed: u8,
) -> rxdb::plugins::replication_webrtc::index_mod::hello::Hello {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use ring::signature::KeyPair;
    use rxdb::plugins::replication_webrtc::index_mod::hello::*;
    // Synthetic public fixture keys, never product credentials.
    let key = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap();
    Hello {
        protocol_ver: 4,
        bootstrap_ver: 1,
        app_api_ver: ApiRange {
            min: "4.0".into(),
            max: "4.0".into(),
        },
        instance_id: "e2e-fixture".into(),
        capabilities: vec!["bench".into()],
        device_proof: DeviceProof {
            device_id: device,
            public_key: URL_SAFE_NO_PAD.encode(key.public_key().as_ref()),
            signature: URL_SAFE_NO_PAD.encode(key.sign(nonce.as_bytes()).as_ref()),
            nonce,
        },
        scopes_wanted: vec![],
        transport: TransportOffer {
            codecs: vec![Codec::Gzip],
            max_frame_bytes: 16384,
            fragmentation: true,
            reassembly: true,
        },
    }
}
fn v4_config(
    source: Arc<store_source::Source>,
) -> rxdb::plugins::replication_webrtc::index_mod::v4_router::V4RouterConfig {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use ring::signature::KeyPair;
    use rxdb::plugins::replication_webrtc::index_mod::{
        connection_lifecycle::*, hello::*, v4_router::*,
    };
    let hello = hello_template("N".repeat(43), "native-fixture".into(), 42);
    V4RouterConfig {
        policy: HelloPolicy {
            instance_id: hello.instance_id,
            bootstrap_min: 1,
            bootstrap_max: 1,
            app_api_ver: hello.app_api_ver,
            capabilities: hello.capabilities,
            required_capabilities: vec!["bench".into()],
            transport: hello.transport,
        },
        handshake_timeout: std::time::Duration::from_secs(10),
        application: Some(source.clone()),
        create_local_hello: Arc::new(|_, nonce| {
            Box::pin(async move { Ok(hello_template(nonce, "native-fixture".into(), 42)) })
        }),
        authenticate: Arc::new(move |peer, hello| {
            let source = source.clone();
            Box::pin(async move {
                let invalid = || {
                    HelloReject::new(
                        RejectReason::AuthenticationFailed,
                        "registered fixture device",
                    )
                };
                let slot = hello
                    .device_proof
                    .device_id
                    .strip_prefix("browser")
                    .ok_or_else(invalid)?
                    .parse::<u8>()
                    .map_err(|_| invalid())?;
                if slot > 2 {
                    return Err(invalid());
                }
                let expected =
                    ring::signature::Ed25519KeyPair::from_seed_unchecked(&[11 + slot; 32])
                        .map_err(|_| invalid())?;
                let public = URL_SAFE_NO_PAD
                    .decode(&hello.device_proof.public_key)
                    .map_err(|_| invalid())?;
                if public != expected.public_key().as_ref() {
                    return Err(invalid());
                }
                let signature = URL_SAFE_NO_PAD
                    .decode(&hello.device_proof.signature)
                    .map_err(|_| invalid())?;
                ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public)
                    .verify(hello.device_proof.nonce.as_bytes(), &signature)
                    .map_err(|_| invalid())?;
                source.admit(peer);
                let (epoch, head) = source.head().await.map_err(|_| invalid())?;
                let token = store_source::scope::SignedScope::issue(
                    &store_source::Key,
                    "e2e-fixture",
                    &hello.device_proof.device_id,
                    &store_source::resolved(),
                    store_source::now() / 1000,
                )
                .map_err(|_| invalid())?;
                Ok(AuthorizedSession {
                    instance_id: hello.instance_id,
                    device_id: hello.device_proof.device_id,
                    epoch,
                    head_seq: WireSeq(head),
                    scope_tokens: vec![serde_json::to_string(&token).unwrap()],
                    rights_ver: WireSeq(1),
                })
            })
        }),
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let config: Value =
        serde_json::from_str(&lines.next_line().await?.ok_or("configuration required")?)?;
    let source = store_source::Source::open(config["db"].as_str().ok_or("db")?).await?;
    let signaling =
        SignalingClient::connect(config["signalingUrl"].as_str().ok_or("signal")?).await?;
    let mut options =
        WebRTCRsConfig::new(signaling.clone(), config["room"].as_str().ok_or("room")?);
    options.ice_servers = vec![RTCIceServer {
        urls: vec![config["turnUrl"].as_str().ok_or("turn")?.into()],
        username: "bench".into(),
        credential: "bench".into(),
        ..Default::default()
    }];
    options.udp_bind_addr = "127.0.0.1:0".into();
    let handler = WebRTCRsConnectionHandler::prepare_with_signaling(options).await?;
    handler.configure_v4(v4_config(source.clone()))?;
    let connections: Arc<Mutex<HashMap<String, WebRTCRsConnection>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let mut connected = handler.connect_stream();
    let table = connections.clone();
    tokio::spawn(async move {
        while let Some(peer) = connected.next().await {
            table
                .lock()
                .unwrap()
                .insert(peer.peer_id().to_owned(), peer.clone());
            emit(json!({"kind":"connected","peer":peer.peer_id()}));
        }
    });
    let mut errors = handler.error_stream();
    tokio::spawn(async move {
        while let Some(e) = errors.next().await {
            emit(json!({"kind":"error","code":e.code()}));
        }
    });
    signaling
        .join(config["room"].as_str().unwrap().into())
        .await?;
    emit(json!({"kind":"ready"}));
    while let Some(line) = lines.next_line().await? {
        let request: Value = serde_json::from_str(&line)?;
        match request["kind"].as_str() {
            Some("shutdown") => break,
            Some("expire") => source.expire().await?,
            Some("arm_drop") => {
                source
                    .drop_next
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                emit(json!({"kind":"drop_armed"}));
            }
            Some("drop") => {
                let peers: Vec<_> = connections.lock().unwrap().values().cloned().collect();
                for p in peers {
                    handler.close_peer(&p).await;
                }
                emit(json!({"kind":"dropped"}));
            }
            Some("push") => {
                source.writer.submit(ctox_store::WriteJob::new(ctox_store::JobOptions::default(),|tx|{tx.execute("UPDATE c_notes SET rev='r-live',data='{\"id\":\"seed\",\"title\":\"live from writer\"}' WHERE id='seed'",[])?;Ok(())}))?.await?;
                emit(json!({"kind":"pushed"}));
            }
            Some("status") => {
                let status=source.writer.submit(ctox_store::WriteJob::new(ctox_store::JobOptions::default(),|tx|{let ops:u64=tx.query_row("SELECT count(*) FROM _ops",[],|r|r.get(0))?;let rows:u64=tx.query_row("SELECT count(*) FROM c_notes WHERE id='resend'",[],|r|r.get(0))?;let activations:u64=tx.query_row("SELECT count(*) FROM e2e_activations",[],|r|r.get(0))?;let command_ids:Vec<String>=tx.prepare("SELECT id FROM c_business_commands WHERE deleted=0 ORDER BY id")?.query_map([],|r|r.get(0))?.collect::<rusqlite::Result<_>>()?;Ok(json!({"kind":"status","ops":ops,"resend_rows":rows,"activations":activations,"command_rows":command_ids.len(),"command_ids":command_ids,"head_seq":ctox_store::change_log::head(tx)?.head_seq.to_string()}))}))?.await?;
                emit(status);
            }
            _ => return Err("unknown controller message".into()),
        }
    }
    handler.close().await?;
    source.shutdown().await;
    Ok(())
}
