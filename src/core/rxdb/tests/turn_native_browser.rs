//! Real Chromium ↔ native relay-only acceptance. Run ignored fixture through
//! scripts/sync-v4/test-turn-native.sh; all credentials are disposable fixtures.
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use webrtc::{
    data_channel::{DataChannel, DataChannelEvent},
    peer_connection::*,
};

struct Handler {
    gathered: tokio::sync::Notify,
    relay_seen: Mutex<bool>,
    messages: Arc<Mutex<Vec<String>>>,
}
#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.gathered.notify_one();
        }
    }
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if event.candidate.typ == RTCIceCandidateType::Relay {
            *self.relay_seen.lock().unwrap() = true;
        }
        assert_eq!(
            event.candidate.typ,
            RTCIceCandidateType::Relay,
            "native relay policy leaked a direct candidate"
        );
    }
    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let messages = self.messages.clone();
        tokio::spawn(async move {
            while let Some(event) = dc.poll().await {
                match event {
                    DataChannelEvent::OnMessage(message) => {
                        let text = String::from_utf8(message.data.to_vec()).unwrap();
                        messages.lock().unwrap().push(text.clone());
                        dc.send_text(&text).await.unwrap();
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        });
    }
}

async fn wait_file(path: &std::path::Path) -> String {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(text) = tokio::fs::read_to_string(path).await {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("fixture file deadline")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "isolated coturn + Chromium; see test-turn-native.sh"]
async fn turn_native_browser_reliable_tcp_tls() {
    assert_eq!(
        NATIVE_TURN_GATHERER_MARKER,
        "ctox-turn-native-2-zero-related-v1"
    );
    println!("TURN_GATHERER_BINARY_MARKER={NATIVE_TURN_GATHERER_MARKER}");
    struct Errors;
    impl log::Log for Errors {
        fn enabled(&self, m: &log::Metadata<'_>) -> bool {
            m.level() <= log::Level::Warn
        }
        fn log(&self, r: &log::Record<'_>) {
            if self.enabled(r.metadata()) {
                eprintln!("TURN_NATIVE_DIAGNOSTIC {}", r.args());
            }
        }
        fn flush(&self) {}
    }
    static ERRORS: Errors = Errors;
    let _ = log::set_logger(&ERRORS);
    log::set_max_level(log::LevelFilter::Warn);
    let root = PathBuf::from(std::env::var_os("TURN_NATIVE_FIXTURE").expect("fixture root"));
    let config: serde_json::Value =
        serde_json::from_str(&wait_file(&root.join("config.json")).await).unwrap();
    use futures::{SinkExt, StreamExt};
    use rxdb::plugins::replication_webrtc::SignalingClient;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let signaling_addr = listener.local_addr().unwrap();
    let issued = config.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        use tokio_tungstenite::tungstenite::Message;
        ws.send(Message::Text(
            r#"{"type":"init","yourPeerId":"turn-fixture-peer"}"#.into(),
        ))
        .await
        .unwrap();
        let mut generation = 0;
        while let Some(Ok(message)) = ws.next().await {
            if let Message::Text(text) = message {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value["type"] == "join" {
                    generation += 1;
                    let credentials = if generation == 1 {
                        &issued["initial"]
                    } else {
                        &issued["renewed"]
                    };
                    let frame = serde_json::json!({"type":"ctoxIceServers", "iceServers":[{"urls":issued["nativeUrl"],"username":credentials["username"],"credential":credentials["credential"]}]});
                    ws.send(Message::Text(frame.to_string().into()))
                        .await
                        .unwrap();
                    ws.send(Message::Text(
                        r#"{"type":"joined","otherPeerIds":[],"peers":[]}"#.into(),
                    ))
                    .await
                    .unwrap();
                }
            }
        }
    });
    let client = SignalingClient::connect(format!("ws://{signaling_addr}"))
        .await
        .unwrap();
    client.join("turn-fixture-room".to_owned()).await.unwrap();
    let messages = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(Handler {
        gathered: tokio::sync::Notify::new(),
        relay_seen: Mutex::new(false),
        messages: messages.clone(),
    });
    let configuration = RTCConfigurationBuilder::new()
        .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
        .with_ice_servers(client.ice_servers(&[]))
        .build();
    let pc: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_configuration(configuration)
            .with_handler(handler.clone())
            .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
            .build()
            .await
            .expect("native authenticated TURN allocation"),
    );
    let offer: RTCSessionDescription =
        serde_json::from_str(&wait_file(&root.join("offer.json")).await).unwrap();
    pc.set_remote_description(offer).await.unwrap();
    let answer = pc.create_answer(None).await.unwrap();
    pc.set_local_description(answer).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), handler.gathered.notified())
        .await
        .expect("native gathering");
    // Require the actual SDP publication, not callback scheduling relative to
    // the gather-complete notification. The browser also verifies the selected
    // local AND remote candidates after the encrypted exchange.
    let answer = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let description = pc.local_description().await.unwrap();
            let candidates: Vec<_> = description
                .sdp
                .lines()
                .filter(|line| line.starts_with("a=candidate:"))
                .map(str::to_owned)
                .collect();
            if !candidates.is_empty() {
                assert!(
                    candidates.iter().all(|line| line.contains(" typ relay")),
                    "native SDP leaked a direct candidate"
                );
                tokio::fs::write(
                    root.join("native-candidates.json"),
                    serde_json::to_vec(&candidates).unwrap(),
                )
                .await
                .unwrap();
                break serde_json::to_vec(&description).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("native relay candidate publication");
    tokio::fs::write(root.join("answer.tmp"), answer)
        .await
        .unwrap();
    tokio::fs::rename(root.join("answer.tmp"), root.join("answer.json"))
        .await
        .unwrap();
    if config["m4"].as_bool() == Some(true) {
        run_m4(&pc, &root, &config, &messages).await;
        pc.close().await.unwrap();
        client.close().await;
        server.abort();
        let _ = server.await;
        return;
    }
    let result: serde_json::Value =
        serde_json::from_str(&wait_file(&root.join("browser-result.json")).await).unwrap();
    let ice_status = client.ice_configuration_status();
    assert!(
        ice_status["revision"].as_u64().unwrap_or(0) >= 2,
        "credentials did not renew on the live signaling socket"
    );
    assert_eq!(client.own_peer_id().as_deref(), Some("turn-fixture-peer"));
    pc.close().await.unwrap();
    client.close().await;
    server.abort();
    let _ = server.await;
    assert_eq!(
        result["pass"], true,
        "browser relay exchange failed: {result}"
    );
    let received = messages.lock().unwrap();
    let expected: Vec<_> = (0..128).map(|i| format!("turn-message-{i}")).collect();
    assert_eq!(
        *received, expected,
        "reliable native delivery lost or duplicated a message"
    );
}

#[test]
fn turn_relay_gatherer_wire_roundtrips_agent_parser() {
    for address in ["192.0.2.10:50000", "[2001:db8::10]:50000"] {
        let init = native_relay_candidate(address.parse().unwrap()).unwrap();
        let raw = init
            .candidate
            .strip_prefix("candidate:")
            .unwrap_or(&init.candidate);
        let (related, expected) = if address.starts_with('[') {
            ("raddr ::", " typ relay raddr :: rport 0")
        } else {
            ("raddr 0.0.0.0", " typ relay raddr 0.0.0.0 rport 0")
        };
        assert!(raw.ends_with(expected), "actual gatherer wire: {raw}");
        // Exactly the parser invoked by RTCPeerConnectionInternal::add_ice_local_candidate.
        let parsed =
            ice::candidate::unmarshal_candidate(raw).expect("agent accepts exact gatherer output");
        assert_eq!(parsed.marshal(), raw);
        assert!(
            ice::candidate::unmarshal_candidate(&raw.replace(related, "raddr ")).is_err(),
            "empty related address remains rejected"
        );
        println!("TURN_GATHERER_BINARY_MARKER={NATIVE_TURN_GATHERER_MARKER} wire={raw}");
    }
}

// Reuses the same real Chromium/TURN allocation and SDP fixture above. The
// benchmark deliberately targets P5/E14 transport, not an App API or enrollment.
async fn run_m4(
    pc: &Arc<dyn PeerConnection>,
    root: &std::path::Path,
    config: &serde_json::Value,
    messages: &Arc<Mutex<Vec<String>>>,
) {
    use rxdb::plugins::replication_webrtc::index_mod::{
        hello::{Codec, TransportSelection, WireSeq},
        lanes::Lane,
        scheduler::{
            session_resume::{ReliableSession, SessionHello},
            Incoming, NativeSendScheduler, SessionBinding, SessionPayload,
        },
    };
    use rxdb::plugins::replication_webrtc::webrtc_types::WebRTCPublicationGuard;
    struct FixtureAuthority;
    impl WebRTCPublicationGuard for FixtureAuthority {
        fn with_current(
            &self,
            f: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
        ) -> rxdb::rx_error::RxResult<()> {
            f()
        }
    }
    let state = ReliableSession::<SessionPayload>::new(
        "browser-m4".into(),
        [
            128 * 1024,
            1024 * 1024,
            128 * 1024,
            4 * 1024 * 1024,
            8 * 1024 * 1024,
            16 * 1024 * 1024,
        ],
    )
    .unwrap();
    let hello = SessionHello {
        session_id: "browser-m4".into(),
        resume_from: std::array::from_fn(|_| WireSeq(0)),
    };
    let binding = SessionBinding::attach(state.clone(), &hello).unwrap();
    let generation = binding.generation;
    let scheduler = NativeSendScheduler::create_resumed(
        pc,
        TransportSelection {
            codec: Codec::Gzip,
            frame_bytes: 16384,
            fragmentation: true,
            reassembly: true,
        },
        binding,
        |_, _, _| Ok(Arc::new(FixtureAuthority)),
    )
    .await
    .unwrap();
    let _ = wait_file(&root.join("m4-browser-ready.json")).await;
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut echoes = Vec::new();
    for lane in [Lane::Control, Lane::Commands] {
        let mut incoming = scheduler.take_incoming(Some(lane)).unwrap();
        let sender = scheduler.clone();
        echoes.push(tokio::spawn(async move {
            for id in 0..64 {
                let Some(Incoming::Reliable {
                    bytes,
                    delivery: Some(token),
                    ..
                }) = incoming.recv().await
                else {
                    panic!("M4 browser control missing")
                };
                token.acknowledge().unwrap();
                sender
                    .send(lane, WireSeq(id), &bytes, Some(Arc::new(FixtureAuthority)))
                    .await
                    .unwrap();
            }
        }));
    }
    let sender = scheduler.clone();
    let stopping = stop.clone();
    let payload = config["payload"].as_str().unwrap().to_owned();
    let window = Arc::new(tokio::sync::Semaphore::new(16));
    let permits = window.clone();
    let producer = tokio::spawn(async move {
        let mut id = 0;
        let mut bytes = 0;
        while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
            let permit = permits.acquire().await.unwrap();
            if stopping.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            permit.forget();
            let message = serde_json::to_vec(
                &serde_json::json!({"type":"bulk","ordinal":id,"payload":payload}),
            )
            .unwrap();
            bytes += message.len();
            sender
                .send(
                    Lane::Transfer,
                    WireSeq(id),
                    &message,
                    Some(Arc::new(FixtureAuthority)),
                )
                .await
                .unwrap();
            id += 1;
        }
        (id, bytes)
    });
    // Browser returns one small delivery notice per object on the existing
    // bootstrap channel. This bounds the fixture producer independently of E14.
    let permits = window.clone();
    let observations = messages.clone();
    let stopping = stop.clone();
    let notices = tokio::spawn(async move {
        let mut seen = 0;
        while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
            let current = observations
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.as_str() == "m4-bulk-delivered")
                .count();
            if current > seen {
                permits.add_permits(current - seen);
                seen = current;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    });
    tokio::time::timeout(Duration::from_secs(600), async {
        loop {
            if messages.lock().unwrap().iter().any(|s| s == "m4-stop") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("M4 observation deadline");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    window.add_permits(16);
    let (bulk_sent, bulk_bytes) = producer.await.unwrap();
    for echo in echoes {
        echo.await.unwrap();
    }
    notices.await.unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if [Lane::Control, Lane::Commands, Lane::Transfer]
                .iter()
                .all(|lane| state.replay(generation, *lane).unwrap().is_empty())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("M4 bulk E14 delivery ACKs missing");
    let handoff = scheduler.handoff_counters();
    tokio::fs::write(root.join("m4-native-result.json"),serde_json::to_vec(&serde_json::json!({"bulk_sent":bulk_sent,"bulk_bytes":bulk_bytes,"buffer_peak":handoff.buffered_upper_bound_peak,"attempted":handoff.attempted,"handed_to_sctp":handoff.handed_to_sctp,"failed":handoff.failed})).unwrap()).await.unwrap();
    let result: serde_json::Value =
        serde_json::from_str(&wait_file(&root.join("browser-result.json")).await).unwrap();
    scheduler.close_now();
    assert_eq!(result["pass"], true, "M4 Chromium fixture failed: {result}");
}
