use super::super::super::{
    hello::{Codec, TransportSelection},
    lanes::{ASSOCIATION_BUDGET, FRAME_BYTES, REALTIME_LABEL},
};
use super::super::{
    ADMISSION, AssociationBudget, Framer, Incoming, ReliableSession, SessionBinding,
};
use super::*;
use async_trait::async_trait;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use webrtc::{
    data_channel::DataChannel,
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
        RTCPeerConnectionIceEvent,
    },
};

#[test]
fn lane4_packet_resume_and_budget_contract() {
    let address = ArtifactAddress::parse("/~a/mail@42~shell@7/assets/main.js").unwrap();
    assert_eq!(
        (
            address.app,
            address.app_version,
            address.shell_version,
            address.path.as_ref()
        ),
        ("mail", "42", "7", "assets/main.js")
    );
    assert_eq!(
        ArtifactAddress::parse("/~a/mail@42~shell@7/assets/a%20%C3%A4.svg")
            .unwrap()
            .path,
        "assets/a ä.svg"
    );
    for bad in [
        "/~a/mail@42/main.js",
        "/~a/mail@42~shell@7/../x",
        "/~a/mail@42~shell@7/%2e%2e/x",
    ] {
        assert!(ArtifactAddress::parse(bad).is_err());
    }
    let dir = tempfile::tempdir().unwrap();
    let cas = Cas::open(dir.path()).unwrap();
    let data = b"partial durable bytes";
    let reference = format!("{:x}", Sha256::digest(data));
    let scope = "a".repeat(64);
    let mut target = ArtifactReceiver::open(&cas, &scope, &reference, data.len() as u64).unwrap();
    target
        .apply(&packet(&reference, data.len() as u64, 0, &data[..7]).unwrap())
        .unwrap();
    drop(target);
    let mut target = ArtifactReceiver::open(&cas, &scope, &reference, data.len() as u64).unwrap();
    assert_eq!(target.offset().unwrap(), 7);
    assert!(
        target
            .apply(&packet(&reference, data.len() as u64, 8, &data[8..]).unwrap())
            .is_err(),
        "resume gap accepted"
    );
    let blob = target
        .apply(&packet(&reference, data.len() as u64, 7, &data[7..]).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(blob.reference(), reference);
    assert_eq!(
        serde_json::to_value(ctox_store::cas::BlobRef {
            r#ref: reference.clone()
        })
        .unwrap(),
        serde_json::json!({"ref":reference})
    );
    let mut budget = AssociationBudget::default();
    // Frame-size guard and aggregate credit must both reject this oversized
    // native handoff, even if the other channels are idle.
    assert!(
        !budget.reserve(Lane::Transfer.index(), ASSOCIATION_BUDGET + 1),
        "budget overflow admitted"
    );
    for lane in 0..7 {
        assert!(budget.reserve(lane, FRAME_BYTES));
    }
    assert!(budget.buffered() <= ASSOCIATION_BUDGET);
    assert!(!budget.reserve(Lane::Transfer.index(), FRAME_BYTES));
    budget.drained(Lane::Control.index());
    assert!(
        budget.reserve(Lane::Control.index(), FRAME_BYTES),
        "bulk blocked control credit"
    );
}

struct Events {
    candidates: mpsc::UnboundedSender<RTCIceCandidateInit>,
    channels: mpsc::UnboundedSender<Arc<dyn DataChannel>>,
}
#[async_trait]
impl PeerConnectionEventHandler for Events {
    async fn on_ice_connection_state_change(&self, state: webrtc::peer_connection::RTCIceConnectionState) {
        eprintln!("Z3_ICE_STATE {}",serde_json::json!({"state":format!("{state:?}"),"ms":z3_elapsed_ms()}));
    }
    async fn on_connection_state_change(&self, state: webrtc::peer_connection::RTCPeerConnectionState) {
        eprintln!("Z3_PC_STATE {}",serde_json::json!({"state":format!("{state:?}"),"ms":z3_elapsed_ms()}));
    }
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Ok(candidate) = event.candidate.to_json() {
            let _ = self.candidates.send(candidate);
        }
    }
    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        let _ = self.channels.send(channel);
    }
}
struct Authorized;
impl WebRTCPublicationGuard for Authorized {
    fn with_current(&self, effect: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        effect()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lane4_real_webrtc_50mb_control_latency_and_verified_arrival() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let root = tempfile::tempdir().unwrap();
    let source = Cas::open(root.path().join("source")).unwrap();
    let target = Cas::open(root.path().join("target")).unwrap();
    // Incompressible fixture, file-backed, bounded scratch memory. The source
    // uses the same verified CAS path as a completed primitive result.
    const SIZE: u64 = 50 * 1024 * 1024;
    let mut file = std::fs::File::create(root.path().join("fixture")).unwrap();
    let mut hash = Sha256::new();
    let mut seed = 0x6a09e667f3bcc909u64;
    let mut block = vec![0; CHUNK_BYTES];
    for _ in 0..SIZE / CHUNK_BYTES as u64 {
        for b in &mut block {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *b = seed as u8;
        }
        hash.update(&block);
        file.write_all(&block).unwrap();
    }
    file.sync_all().unwrap();
    drop(file);
    let reference = format!("{:x}", hash.finalize());
    std::fs::rename(
        root.path().join("fixture"),
        root.path().join("source/objects").join(&reference),
    )
    .unwrap();
    let blob = source.verified(&reference, SIZE).unwrap();
    // Isolate durable CAS IO from the network before attributing throughput to
    // the scheduler. Same source, hashes and fsync semantics, two logical sizes.
    for chunk in [64 * 1024, CHUNK_BYTES] {
        let disk = Cas::open(root.path().join(format!("disk-{chunk}"))).unwrap();
        let path = root.path().join("source/objects").join(&reference);
        let hash = reference.clone();
        tokio::task::spawn_blocking(move || {
            let start = Instant::now(); let mut source = std::fs::File::open(path).unwrap();
            let mut target = disk.receive(&"f".repeat(64), &hash, SIZE).unwrap();
            let mut bytes = vec![0; chunk]; let mut cursor = 0; let mut io_ms = Vec::new();
            loop { let n = source.read(&mut bytes).unwrap(); if n == 0 { break; } let clock = Instant::now(); target.append(cursor, &bytes[..n]).unwrap(); io_ms.push(clock.elapsed().as_secs_f64() * 1000.0); cursor += n as u64; }
            target.finish().unwrap(); io_ms.sort_by(f64::total_cmp);
            eprintln!("LANE4_CAS_IO {}", serde_json::json!({"chunk_bytes":chunk,"bytes":SIZE,"seconds":start.elapsed().as_secs_f64(),"append_p99_ms":io_ms[(io_ms.len()*99/100).min(io_ms.len()-1)]}));
        }).await.unwrap();
    }
    let (atx, mut ac) = mpsc::unbounded_channel();
    let (btx, mut bc) = mpsc::unbounded_channel();
    let (unused, _discard) = mpsc::unbounded_channel();
    let (incoming, mut channels) = mpsc::unbounded_channel();
    let a: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_handler(Arc::new(Events {
                candidates: atx,
                channels: unused,
            }))
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap(),
    );
    let b: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_handler(Arc::new(Events {
                candidates: btx,
                channels: incoming,
            }))
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap(),
    );
    let mut tasks = tokio::task::JoinSet::new();
    let result = tokio::time::timeout(Duration::from_secs(420), async {
        let transport = TransportSelection { codec: Codec::Zstd, frame_bytes: FRAME_BYTES as u32, fragmentation: true, reassembly: true };
        let sa = ReliableSession::new("lane4-50mb".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let sb = ReliableSession::new("lane4-50mb".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let binding_a = SessionBinding::attach(sa.clone(), &sb.hello()).unwrap();
        let binding_b = SessionBinding::attach(sb, &sa.hello()).unwrap();
        let sender = NativeSendScheduler::create_inner(&a, transport.clone(), Some(binding_a.clone())).await.unwrap();
        let offer = a.create_offer(None).await.unwrap(); a.set_local_description(offer.clone()).await.unwrap(); b.set_remote_description(offer).await.unwrap();
        let answer = b.create_answer(None).await.unwrap(); b.set_local_description(answer.clone()).await.unwrap(); a.set_remote_description(answer).await.unwrap();
        let pc = b.clone(); tasks.spawn(async move { while let Some(c) = ac.recv().await { pc.add_ice_candidate(c).await.unwrap(); } });
        let pc = a.clone(); tasks.spawn(async move { while let Some(c) = bc.recv().await { pc.add_ice_candidate(c).await.unwrap(); } });
        let mut accepted = vec![None; 7];
        for _ in 0..7 { let channel = channels.recv().await.unwrap(); let label = channel.label().await.unwrap(); eprintln!("LANE4_CHANNEL {label}"); let index = Lane::ALL.iter().position(|l| l.label() == label).unwrap_or_else(|| { assert_eq!(label, REALTIME_LABEL); 6 }); accepted[index] = Some(channel); }
        let receiver = NativeSendScheduler::from_channels_session(accepted.into_iter().map(Option::unwrap).collect(), Framer::new(transport).unwrap(), Some(binding_b)).await.unwrap();
        for scheduler in [sender.clone(), receiver.clone()] {
            let mut failures = scheduler.failures();
            tasks.spawn(async move { while let Some(event) = failures.next().await { eprintln!("LANE4_ACTOR {event:?}"); } });
        }
        let mut controls = receiver.take_incoming(Some(Lane::Control)).unwrap();
        let mut replies = sender.take_incoming(Some(Lane::Control)).unwrap();
        let reply_sender = receiver.clone();
        let mut bulks = receiver.take_incoming(Some(Lane::Transfer)).unwrap();
        let scope = "c".repeat(64);
        let mut sink = ArtifactReceiver::open(&target, &scope, &reference, SIZE).unwrap();
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done = finished.clone(); let expected = reference.clone();
        let persisted = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let receive_progress = persisted.clone();
        // Counterfactual: the same control path with no blob and no CAS IO.
        // Timing is measured here; hardware targets live in gate-bench-lane4.sh.
        let mut idle_ms = Vec::new();
        for _ in 0..64 {
            let start = Instant::now();
            sender.send(Lane::Control, WireSeq(1), b"idle-request", Some(Arc::new(Authorized))).await.unwrap();
            let Some(Incoming::Reliable { bytes, delivery: Some(delivery), .. }) = controls.recv().await else { panic!("idle request missing"); };
            assert_eq!(bytes, b"idle-request"); delivery.acknowledge().unwrap();
            receiver.send(Lane::Control, WireSeq(1), b"idle-reply", Some(Arc::new(Authorized))).await.unwrap();
            let Some(Incoming::Reliable { bytes, delivery: Some(delivery), .. }) = replies.recv().await else { panic!("idle reply missing"); };
            assert_eq!(bytes, b"idle-reply"); idle_ms.push(start.elapsed().as_secs_f64() * 1000.0); delivery.acknowledge().unwrap();
        }
        idle_ms.sort_by(f64::total_cmp);
        let idle_p99 = idle_ms[(idle_ms.len()*99/100).min(idle_ms.len()-1)];
        eprintln!("LANE4_IDLE_CONTROL {}", serde_json::json!({"roundtrip_ms":idle_ms,"roundtrip_p99_ms":idle_p99,"blob_active":false,"cas_io_active":false}));
        let transfer_clock = Instant::now();
        let receive = async move {
            let mut verified = None;
            let mut io_seconds = 0.0;
            let mut wait_seconds = 0.0;
            while verified.is_none() {
                let wait_start = Instant::now();
                let Some(Incoming::Reliable { bytes, delivery: Some(delivery), .. }) = bulks.recv().await else { panic!("missing artifact delivery"); };
                wait_seconds += wait_start.elapsed().as_secs_f64();
                let io_start = Instant::now();
                let (next, blob) = sink.deliver(bytes, delivery).await.unwrap(); sink = next; verified = blob;
                io_seconds += io_start.elapsed().as_secs_f64();
                receive_progress.store(sink.offset().unwrap(), std::sync::atomic::Ordering::Release);
                if sink.offset().unwrap() % (5 * 1024 * 1024) == 0 { eprintln!("LANE4_PROGRESS bytes={} elapsed_ms={}", sink.offset().unwrap(), transfer_clock.elapsed().as_millis()); }
            }
            assert_eq!(verified.unwrap().reference(), expected);
            eprintln!("LANE4_RECEIVE_STAGES {}", serde_json::json!({"wait_seconds":wait_seconds,"persist_verify_seconds":io_seconds,"debug_assertions":cfg!(debug_assertions)}));
            done.store(true, std::sync::atomic::Ordering::Release);
        };
        let control_sender = sender.clone();
        let probe = async move {
            let mut latencies = Vec::new();
            let mut controls_during_bulk = 0;
            while !finished.load(std::sync::atomic::Ordering::Acquire) || latencies.is_empty() {
                let start = Instant::now();
                control_sender.send(Lane::Control, WireSeq(1), b"lane4-probe", Some(Arc::new(Authorized))).await.unwrap();
                let Some(Incoming::Reliable { bytes, delivery: Some(delivery), .. }) = controls.recv().await else { panic!("missing control"); };
                assert_eq!(bytes, b"lane4-probe"); delivery.acknowledge().unwrap();
                reply_sender.send(Lane::Control, WireSeq(1), b"lane4-reply", Some(Arc::new(Authorized))).await.unwrap();
                let Some(Incoming::Reliable { bytes, delivery: Some(delivery), .. }) = replies.recv().await else { panic!("missing control reply"); };
                assert_eq!(bytes, b"lane4-reply"); latencies.push(start.elapsed().as_secs_f64() * 1000.0); delivery.acknowledge().unwrap();
                let progress = persisted.load(std::sync::atomic::Ordering::Acquire);
                if progress > 0 && progress < SIZE { controls_during_bulk += 1; }
                if latencies.len() % 100 == 0 { eprintln!("LANE4_CONTROL latest_ms={}", latencies.last().unwrap()); }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            latencies.sort_by(f64::total_cmp);
            (latencies[(latencies.len() * 99 / 100).min(latencies.len()-1)], controls_during_bulk)
        };
        let start = Instant::now();
        let (sent, (), (p99, controls_during_bulk)) = tokio::join!(send_blob(&sender, source.clone(), blob, 0, Arc::new(Authorized)), receive, probe);
        sent.unwrap(); let elapsed = start.elapsed().as_secs_f64();
        let peak = sender.handoff_counters().buffered_upper_bound_peak;
        assert!(peak <= ASSOCIATION_BUDGET as u64, "connection-wide budget exceeded");

        target.verified(&reference, SIZE).unwrap();
        eprintln!("LANE4_MEASUREMENT {}", serde_json::json!({"transport":"native-native real WebRTC loopback", "relay":false,"bytes":SIZE,"seconds":elapsed,"mb_per_second":SIZE as f64/1_000_000.0/elapsed,"control_roundtrip_p99_ms":p99,"sctp_buffer_upper_bound_peak":peak,"sha256_verified":true,"target_buffer_peak":receiver.handoff_counters().buffered_upper_bound_peak,"debug_assertions":cfg!(debug_assertions),"idle_control_roundtrip_p99_ms":idle_p99,"control_roundtrips_during_bulk":controls_during_bulk}));
        assert!(controls_during_bulk > 0, "control frames waited until bulk completed");
        sender.close_now(); receiver.close_now();
    }).await;
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    a.close().await.unwrap();
    b.close().await.unwrap();
    result.unwrap();
}

struct PairFixture {
    source: NativeSendScheduler,
    target: NativeSendScheduler,
    a: Arc<dyn PeerConnection>,
    b: Arc<dyn PeerConnection>,
    forwarding: tokio::task::JoinSet<()>,
}
impl PairFixture {
    async fn connect(
        sa: Arc<ReliableSession<super::super::SessionPayload>>,
        sb: Arc<ReliableSession<super::super::SessionPayload>>,
    ) -> Self {
        Self::connect_on(sa, sb, "127.0.0.1:0").await
    }
    async fn connect_on(
        sa: Arc<ReliableSession<super::super::SessionPayload>>,
        sb: Arc<ReliableSession<super::super::SessionPayload>>,
        bind: &str,
    ) -> Self {
        Self::connect_configured(sa, sb, bind, Default::default(), false).await
    }
    async fn connect_configured(
        sa: Arc<ReliableSession<super::super::SessionPayload>>,
        sb: Arc<ReliableSession<super::super::SessionPayload>>,
        bind: &str,
        configuration: [webrtc::peer_connection::RTCConfiguration; 2],
        shaped: bool,
    ) -> Self {
        let (atx, mut ac) = mpsc::unbounded_channel();
        let (btx, mut bc) = mpsc::unbounded_channel();
        let (unused, _discard) = mpsc::unbounded_channel();
        let (incoming, mut channels) = mpsc::unbounded_channel();
        let a: Arc<dyn PeerConnection> = Arc::new(
            PeerConnectionBuilder::new()
                .with_configuration(configuration[0].clone())
                .with_handler(Arc::new(Events {
                    candidates: atx,
                    channels: unused,
                }))
                .with_udp_addrs(vec![bind.to_owned()])
                .build()
                .await
                .unwrap(),
        );
        let b: Arc<dyn PeerConnection> = Arc::new(
            PeerConnectionBuilder::new()
                .with_configuration(configuration[1].clone())
                .with_handler(Arc::new(Events {
                    candidates: btx,
                    channels: incoming,
                }))
                .with_udp_addrs(vec![bind.to_owned()])
                .build()
                .await
                .unwrap(),
        );
        // A real already-connected bootstrap association, as the P4 hello host
        // supplies. Replayed frames must not wait for a future SDP exchange.
        eprintln!("LANE4_RESUME stage=creating-bootstrap");
        let _bootstrap = a
            .create_data_channel("lane4-bootstrap", None)
            .await
            .unwrap();
        let offer = a.create_offer(None).await.unwrap();
        a.set_local_description(offer.clone()).await.unwrap();
        b.set_remote_description(offer).await.unwrap();
        let answer = b.create_answer(None).await.unwrap();
        b.set_local_description(answer.clone()).await.unwrap();
        a.set_remote_description(answer).await.unwrap();
        let mut forwarding = tokio::task::JoinSet::new();
        let pc = b.clone();
        forwarding.spawn(async move {
            while let Some(c) = ac.recv().await {
                pc.add_ice_candidate(c).await.unwrap();
            }
        });
        let pc = a.clone();
        forwarding.spawn(async move {
            while let Some(c) = bc.recv().await {
                pc.add_ice_candidate(c).await.unwrap();
            }
        });
        eprintln!("LANE4_RESUME stage=waiting-bootstrap");
        assert_eq!(
            channels.recv().await.unwrap().label().await.unwrap(),
            "lane4-bootstrap"
        );
        eprintln!("LANE4_RESUME stage=bootstrap-open");
        let transport = TransportSelection {
            codec: Codec::Zstd,
            frame_bytes: FRAME_BYTES as u32,
            fragmentation: true,
            reassembly: true,
        };
        let ba = SessionBinding::attach(sa.clone(), &sb.hello()).unwrap();
        let bb = SessionBinding::attach(sb, &sa.hello()).unwrap();
        eprintln!("LANE4_RESUME stage=creating-v4");
        let accept = async {
            let mut accepted = vec![None; 7];
            for _ in 0..7 {
                let channel = channels.recv().await.unwrap();
                let label = channel.label().await.unwrap();
                eprintln!("LANE4_CHANNEL {label} sid={:?}", channel.id());
                let index = Lane::ALL
                    .iter()
                    .position(|l| l.label() == label)
                    .unwrap_or_else(|| {
                        assert_eq!(label, REALTIME_LABEL);
                        6
                    });
                accepted[index] = Some(channel);
            }
            let target = NativeSendScheduler::from_channels_session(
                accepted.into_iter().map(Option::unwrap).collect(),
                Framer::new(transport.clone()).unwrap(),
                Some(bb),
            )
            .await
            .unwrap();
            // This fixture owns both native endpoints. Resume the accepted
            // endpoint too; create_resumed already does this for the opener.
            target.replay_pending(&|_, _, _| {
                Ok(Arc::new(Authorized) as Arc<dyn WebRTCPublicationGuard>)
            }).await.unwrap();
            target
        };
        let create = async {
            let result =
                NativeSendScheduler::create_resumed(&a, transport.clone(), ba, |_, _, _| {
                    Ok(Arc::new(Authorized) as Arc<dyn WebRTCPublicationGuard>)
                })
                .await;
            eprintln!("LANE4_RESUME constructor={:?}", result.as_ref().map(|_| ()));
            let source = result?;
            // Existing SCTP uses in-band DCEP. No second SDP exchange and no
            // external signalling workaround may be needed after bootstrap.
            Ok(source)
        };
        // Surface constructor failure immediately. join! used to hide it
        // behind the unrelated accept future until the fixture watchdog fired.
        let dynamic_started = Instant::now();
        let (source, target) = tokio::time::timeout(Duration::from_secs(if shaped {10} else {1}), async {
            tokio::try_join!(create, async {
                Ok::<_, crate::rx_error::RxError>(accept.await)
            })
        })
        .await
        .expect("native post-bootstrap channels did not open within 1 s")
        .unwrap_or_else(|e| panic!("native resumed constructor: {e:?}"));
        eprintln!(
            "DCEP_DYNAMIC_OPEN {}",
            serde_json::json!({"peer":"native", "channels":7, "elapsed_ms":dynamic_started.elapsed().as_secs_f64()*1000.0, "renegotiated":false})
        );
        for scheduler in [source.clone(), target.clone()] {
            let mut failures = scheduler.failures();
            forwarding.spawn(async move {
                while let Some(event) = failures.next().await {
                    eprintln!("LANE4_RESUME_ACTOR {event:?}");
                }
            });
        }
        eprintln!("LANE4_RESUME stage=connected");
        Self {
            source,
            target,
            a,
            b,
            forwarding,
        }
    }
    async fn close(mut self) {
        self.source.close_now();
        self.target.close_now();
        self.forwarding.abort_all();
        while self.forwarding.join_next().await.is_some() {}
        eprintln!("LANE4_RESUME stage=close-a");
        self.a.close().await.unwrap();
        eprintln!("LANE4_RESUME stage=close-b");
        self.b.close().await.unwrap();
        eprintln!("LANE4_RESUME stage=closed");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lane4_dynamic_channels_open_after_handshake_within_one_second() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(10), async {
        let sa = ReliableSession::new(
            "dcep-after-hello".into(),
            ADMISSION[..6].try_into().unwrap(),
        )
        .unwrap();
        let sb = ReliableSession::new(
            "dcep-after-hello".into(),
            ADMISSION[..6].try_into().unwrap(),
        )
        .unwrap();
        let pair = PairFixture::connect(sa, sb).await;
        let result = tokio::time::timeout(Duration::from_secs(1), async {
            let mut received = pair.target.take_incoming(Some(Lane::Control)).unwrap();
            let mut replies = pair.source.take_incoming(Some(Lane::Control)).unwrap();
            pair.source
                .send(
                    Lane::Control,
                    WireSeq(0),
                    b"after-handshake",
                    Some(Arc::new(Authorized)),
                )
                .await
                .unwrap();
            let Some(Incoming::Reliable {
                bytes,
                delivery: Some(token),
                ..
            }) = received.recv().await
            else {
                panic!("dynamic channel missing request");
            };
            assert_eq!(bytes, b"after-handshake");
            token.acknowledge().unwrap();
            pair.target
                .send(
                    Lane::Control,
                    WireSeq(0),
                    b"dcep-open",
                    Some(Arc::new(Authorized)),
                )
                .await
                .unwrap();
            let Some(Incoming::Reliable {
                bytes,
                delivery: Some(token),
                ..
            }) = replies.recv().await
            else {
                panic!("dynamic channel missing reply");
            };
            assert_eq!(bytes, b"dcep-open");
            token.acknowledge().unwrap();
        })
        .await;
        pair.close().await;
        result.expect("dynamic channels opened without bidirectional delivery");
    })
    .await
    .expect("bootstrap fixture did not terminate");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lane4_real_webrtc_drop_after_persistence_replays_then_resumes_prefix() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(90), async {
        let dir = tempfile::tempdir().unwrap();
        let source = Cas::open(dir.path().join("source")).unwrap();
        let target = Cas::open(dir.path().join("target")).unwrap();
        let mut seed = 0x6a09e667f3bcc909u64;
        let mut bytes = vec![0; 50 * 1024 * 1024];
        for byte in &mut bytes { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; *byte = seed as u8; }
        std::fs::write(dir.path().join("primitive"), &bytes).unwrap();
        let blob = source.import(dir.path().join("primitive"), "d".repeat(64)).await.unwrap();
        let sa = ReliableSession::new("artifact-drop".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let sb = ReliableSession::new("artifact-drop".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let pair = PairFixture::connect(sa.clone(), sb.clone()).await;
        let mut bulk = pair.target.take_incoming(Some(Lane::Transfer)).unwrap();
        let scope = "e".repeat(64);
        let mut sink = ArtifactReceiver::open(&target, &scope, blob.reference(), blob.size()).unwrap();
        let middle = bytes.len() / 2;
        let mut last_payload = Vec::new();
        for offset in (0..middle).step_by(CHUNK_BYTES) {
            let payload = packet(blob.reference(), blob.size(), offset as u64, &bytes[offset..offset+CHUNK_BYTES]).unwrap();
            pair.source.send(Lane::Transfer, WireSeq(0), &payload, Some(Arc::new(Authorized))).await.unwrap();
            let Some(Incoming::Reliable { bytes: received, delivery: Some(token), .. }) = bulk.recv().await else { panic!("missing pre-drop chunk"); };
            if offset + CHUNK_BYTES == middle {
                let (next, completed) = tokio::task::spawn_blocking(move || { let completed = sink.apply(&received).unwrap(); (sink, completed) }).await.unwrap();
                sink = next; assert!(completed.is_none());
                drop(token); // persisted effect, but ACK lost with the connection
                last_payload = payload;
            } else {
                let (next, completed) = sink.deliver(received, token).await.unwrap(); sink = next; assert!(completed.is_none());
            }
        }
        assert_eq!(sink.offset().unwrap(), middle as u64);
        drop(sink); pair.close().await;
        assert_eq!(sb.hello().resume_from[Lane::Transfer.index()], WireSeq((middle / CHUNK_BYTES - 1) as u64));
        let pair = PairFixture::connect(sa.clone(), sb).await;
        let mut bulk = pair.target.take_incoming(Some(Lane::Transfer)).unwrap();
        let sink = ArtifactReceiver::open(&target, &scope, blob.reference(), blob.size()).unwrap();
        assert_eq!(sink.offset().unwrap(), middle as u64, "resume offset was not retained");
        let Some(Incoming::Reliable { bytes: replay, delivery: Some(token), .. }) = bulk.recv().await else { panic!("lost message not replayed"); };
        assert_eq!(replay, last_payload, "resume did not replay the unacknowledged chunk");
        let (mut sink, completed) = sink.deliver(replay, token).await.unwrap();
        assert!(completed.is_none()); assert_eq!(sink.offset().unwrap(), middle as u64);
        let receive = async {
            loop {
                let Some(Incoming::Reliable { bytes, delivery: Some(token), .. }) = bulk.recv().await else { panic!("missing resumed suffix"); };
                let (next, completed) = sink.deliver(bytes, token).await.unwrap(); sink = next;
                if let Some(result) = completed { assert_eq!(result.reference(), blob.reference()); break sink; }
            }
        };
        let (sent, sink) = tokio::join!(send_blob(&pair.source, source.clone(), blob.clone(), middle as u64, Arc::new(Authorized)), receive);
        sent.unwrap();
        assert_eq!(sink.offset().unwrap(), blob.size());
        target.verified(blob.reference(), blob.size()).unwrap();
        eprintln!("LANE4_RESUME_MEASUREMENT {}", serde_json::json!({"bytes":blob.size(),"connection_drop":true,"resume_offset":middle,"replayed_unacknowledged_prefix":true,"sha256_verified":true}));
        let closed = pair.source.clone(); pair.close().await;
        assert!(tokio::time::timeout(Duration::from_secs(1), send_blob(&closed, source, blob, 0, Arc::new(Authorized))).await.unwrap().is_err(), "closed association left producer waiting");
    }).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lane4_library_file_is_pinned_lazy_and_resumable() {
    use super::manifest::{FileManifest, LibraryFileRequest};
    let _ = rustls::crypto::ring::default_provider().install_default();
    // This is an integrity/resume contract, not a ten-second latency target.
    // Real ICE/SCTP work shares the suite's CPU. Keep a deadlock watchdog large
    // enough for suite contention; pinning, offset, hash and ACK assertions stay.
    tokio::time::timeout(Duration::from_secs(300), async {
        for route in ["/~a/lib:monaco@0/editor.js", "/~a/lib:monaco@00.1/editor.js", "/~a/lib:monaco@0.01/editor.js", "/~a/lib:monaco@0.9/%2e%2e/editor.js"] {
            assert!(LibraryFileRequest::parse(route).is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        let source = Cas::open(dir.path().join("source")).unwrap();
        let target = Cas::open(dir.path().join("target")).unwrap();
        let data: Vec<_> = (0..65_536).map(|i| (i % 251) as u8).collect();
        std::fs::write(dir.path().join("wanted"), &data).unwrap();
        std::fs::write(dir.path().join("wrong"), b"not in this manifest").unwrap();
        let blob = source.import(dir.path().join("wanted"), "b".repeat(64)).await.unwrap();
        let wrong = source.import(dir.path().join("wrong"), "c".repeat(64)).await.unwrap();
        let manifest: FileManifest = serde_json::from_value(serde_json::json!({"app_id":"lib:monaco","app_ver":"0.9","entry":"esm/editor.js","files":{"esm/editor.js":blob.reference(),"unused/huge.wasm":"f".repeat(64)}})).unwrap();
        let request = LibraryFileRequest::parse("/~a/lib:monaco@0.9/esm/editor.js").unwrap();
        assert_eq!(request.reference(&manifest).unwrap(), blob.reference());
        assert!(LibraryFileRequest::parse("/~a/lib:monaco@0.10/esm/editor.js").unwrap().reference(&manifest).is_err(), "unpinned minor accepted");
        assert!(LibraryFileRequest::parse("/~a/lib:other@0.9/esm/editor.js").unwrap().reference(&manifest).is_err());
        assert!(LibraryFileRequest::parse("/~a/lib:monaco@0.9/missing.js").unwrap().reference(&manifest).is_err());
        let sa = ReliableSession::new("library-file".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let sb = ReliableSession::new("library-file".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let pair = PairFixture::connect(sa, sb).await;
        assert!(request.send(&manifest, &pair.source, source.clone(), wrong, 0, Arc::new(Authorized)).await.is_err(), "unpinned blob admitted");
        let mut incoming = pair.target.take_incoming(Some(Lane::Transfer)).unwrap();
        let mut sink = ArtifactReceiver::open(&target, &"d".repeat(64), blob.reference(), blob.size()).unwrap();
        const PREFIX: usize = 8192;
        sink.apply(&packet(blob.reference(), blob.size(), 0, &data[..PREFIX]).unwrap()).unwrap();
        let receive = async {
            let Some(Incoming::Reliable { bytes, delivery: Some(token), .. }) = incoming.recv().await else { panic!("requested library file missing"); };
            let (header, _) = decode(&bytes).unwrap();
            assert_eq!(header.offset, WireSeq(PREFIX as u64), "library resume offset changed");
            let (sink, verified) = sink.deliver(bytes, token).await.unwrap();
            assert_eq!(verified.unwrap().reference(), blob.reference());
            assert_eq!(sink.offset().unwrap(), blob.size());
        };
        let (sent, ()) = tokio::join!(request.send(&manifest, &pair.source, source.clone(), blob.clone(), PREFIX as u64, Arc::new(Authorized)), receive);
        sent.unwrap();
        target.verified(blob.reference(), blob.size()).unwrap();
        assert!(source.verified(&"f".repeat(64), 1).is_err());
        assert!(target.verified(&"f".repeat(64), 1).is_err(), "unrequested library file fetched eagerly");
        pair.close().await;
    }).await.unwrap();
}

// Explicit opt-in: requires an existing Playwright/Chromium installation on the
// acceptance lane. Never downloads dependencies or connects a product tenant.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires CTOX_DCEP_BROWSER_MODULES and a lane Chromium installation"]
async fn lane4_dynamic_browser_channel_opens_after_handshake_within_one_second() {
    use std::io::{BufRead, BufReader};
    use webrtc::data_channel::DataChannelEvent;
    use webrtc::peer_connection::RTCSessionDescription;
    let _ = rustls::crypto::ring::default_provider().install_default();
    struct Browser(std::process::Child);
    impl Drop for Browser {
        fn drop(&mut self) {
            // stdin closes first on unwind: let Node close its owned browser.
            for _ in 0..40 {
                if self.0.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut browser = Browser(
        std::process::Command::new(
            std::env::var("CTOX_DCEP_NODE").unwrap_or_else(|_| "node".into()),
        )
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/plugins/replication_webrtc/v4_scheduler/dcep_browser_fixture.mjs"
        ))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .unwrap(),
    );
    let mut signal = browser.0.stdin.take().unwrap();
    let stdout = browser.0.stdout.take().unwrap();
    let (rows, mut input) = mpsc::unbounded_channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(line) => {
                    if rows
                        .send(serde_json::from_str::<serde_json::Value>(&line).unwrap())
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    fn emit(out: &mut std::process::ChildStdin, row: serde_json::Value) {
        writeln!(out, "{row}").unwrap();
        out.flush().unwrap();
    }
    let (atx, mut candidates) = mpsc::unbounded_channel();
    let (unused, _discard) = mpsc::unbounded_channel();
    let a: Arc<dyn PeerConnection> = Arc::new(
        PeerConnectionBuilder::new()
            .with_handler(Arc::new(Events {
                candidates: atx,
                channels: unused,
            }))
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap(),
    );
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        let bootstrap = a.create_data_channel("lane4-bootstrap", None).await.unwrap();
        let offer = a.create_offer(None).await.unwrap();
        a.set_local_description(offer.clone()).await.unwrap();
        emit(&mut signal, serde_json::json!({"type":"offer", "description":offer}));
        let mut remote = false;
        let mut pending = Vec::new();
        loop {
            tokio::select! {
                Some(candidate) = candidates.recv() => emit(&mut signal, serde_json::json!({"type":"candidate", "candidate":candidate})),
                row = input.recv() => {
                    let row = row.expect("browser fixture exited before bootstrap");
                    match row["type"].as_str().unwrap() {
                        "answer" => {
                            let answer: RTCSessionDescription = serde_json::from_value(row["description"].clone()).unwrap();
                            a.set_remote_description(answer).await.unwrap(); remote = true;
                            for candidate in pending.drain(..) { a.add_ice_candidate(candidate).await.unwrap(); }
                        },
                        "candidate" => {
                            let candidate: RTCIceCandidateInit = serde_json::from_value(row["candidate"].clone()).unwrap();
                            if remote { a.add_ice_candidate(candidate).await.unwrap(); } else { pending.push(candidate); }
                        },
                        "open" if row["label"] == "lane4-bootstrap" => break,
                        _ => panic!("unexpected browser bootstrap event: {row}"),
                    }
                }
            }
        }
        while !matches!(bootstrap.poll().await, Some(DataChannelEvent::OnOpen)) {}
        let start = Instant::now();
        let opened = tokio::time::timeout(Duration::from_secs(1), async {
            let channel = a.create_data_channel(Lane::Control.label(), None).await.unwrap();
            while !matches!(channel.poll().await, Some(DataChannelEvent::OnOpen)) {}
            loop {
                let row = input.recv().await.expect("browser fixture exited before DCEP OPEN");
                if row["type"] == "open" && row["label"] == Lane::Control.label() { break; }
                assert_ne!(row["type"], "error", "browser data channel error: {row}");
            }
            channel
        }).await.expect("browser post-bootstrap channel did not open within 1 s");
        eprintln!("DCEP_DYNAMIC_OPEN {}", serde_json::json!({"peer":"chromium", "elapsed_ms":start.elapsed().as_secs_f64()*1000.0, "renegotiated":false}));
        opened.send_text("after-handshake").await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Some(DataChannelEvent::OnMessage(message)) = opened.poll().await {
                    assert_eq!(message.data.as_ref(), b"after-handshake"); break;
                }
            }
        }).await.expect("browser dynamic channel did not echo");
    }).await;
    a.close().await.unwrap();
    emit(&mut signal, serde_json::json!({"type":"close"}));
    drop(signal);
    drop(browser);
    reader.join().unwrap();
    result.expect("browser fixture timed out");
}

mod e25_range_tests {
    use super::*;
    include!("../v4_session_tests.rs");
}

// Z3 uses the existing real SCTP pair, not a second mock transport.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn z3_p4_twenty_reconnects_preserve_every_application_delivery() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(120), async {
        let sa = ReliableSession::new("z3-reconnect".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let sb = ReliableSession::new("z3-reconnect".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let mut pair = PairFixture::connect(sa.clone(), sb.clone()).await;
        let mut effects_a = Vec::new();
        let mut effects_b = Vec::new();
        let mut explicit_results = 0usize;
        let mut replayed = 0usize;
        let mut at = pair.source.take_incoming(Some(Lane::Commands)).unwrap();
        let mut bt = pair.target.take_incoming(Some(Lane::Commands)).unwrap();
        for cycle in 0..20u64 {
            let mut uncommitted = Vec::new();
            for index in 0..16u64 {
                let id = cycle * 16 + index;
                pair.source.send(Lane::Commands, WireSeq(id), &id.to_be_bytes(), Some(Arc::new(Authorized))).await.expect("explicit A send result");
                pair.target.send(Lane::Commands, WireSeq(id), &id.to_be_bytes(), Some(Arc::new(Authorized))).await.expect("explicit B send result");
                explicit_results += 2;
                for (incoming, effects) in [(&mut at, &mut effects_a), (&mut bt, &mut effects_b)] {
                    let Some(Incoming::Reliable { bytes, delivery: Some(token), .. }) = incoming.recv().await else { panic!("application message missing"); };
                    assert_eq!(bytes, id.to_be_bytes());
                    if index == 15 {
                        // Channel loss before the application commits: no ACK.
                        uncommitted.push(token);
                    } else {
                        effects.push(id);
                        token.acknowledge().unwrap();
                    }
                }
            }
            pair.close().await;
            drop(uncommitted);
            pair = PairFixture::connect(sa.clone(), sb.clone()).await;
            at = pair.source.take_incoming(Some(Lane::Commands)).unwrap();
            bt = pair.target.take_incoming(Some(Lane::Commands)).unwrap();
            let expected = cycle * 16 + 15;
            for (incoming, effects) in [(&mut at, &mut effects_a), (&mut bt, &mut effects_b)] {
                let replay = tokio::time::timeout(Duration::from_secs(2), incoming.recv()).await.expect("unacknowledged application message must replay after reconnect");
                let Some(Incoming::Reliable { bytes, delivery: Some(token), .. }) = replay else { panic!("replay missing"); };
                assert_eq!(bytes, expected.to_be_bytes());
                effects.push(expected);
                token.acknowledge().unwrap();
                replayed += 1;
            }
            // ACK retirement is the delivery witness, separate from send admission.
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let ga = pair.source.session.as_ref().unwrap().generation;
                    let gb = pair.target.session.as_ref().unwrap().generation;
                    if sa.replay(ga, Lane::Commands).unwrap().is_empty() && sb.replay(gb, Lane::Commands).unwrap().is_empty() { break; }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }).await.expect("every admitted send needs cumulative delivery acknowledgement");
        }
        let expected: Vec<_> = (0..320u64).collect();
        assert_eq!(effects_a, expected, "A application lost/duplicated delivery");
        assert_eq!(effects_b, expected, "B application lost/duplicated delivery");
        pair.close().await;
        eprintln!("Z3_P4_RECONNECT {}", serde_json::json!({"scenario":1,"forced_loss_reconnects":20,"additional_clean_rebinds":0,"sent":640,"explicit_send_results":explicit_results,"delivery_acknowledged":640,"replayed_after_loss":replayed,"lost":0,"duplicated":0,"transport":"native-native real WebRTC"}));
    }).await.expect("twenty reconnects must finish within fixture deadline");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires isolated unshare -Urn with Z3_OWN_NETNS=1"]
async fn z3_p4_netns_network_change() {
    assert_eq!(std::env::var("Z3_OWN_NETNS").as_deref(), Ok("1"));
    let links = std::process::Command::new("ip").args(["-j", "link", "show"]).output().unwrap();
    let links: Vec<serde_json::Value> = serde_json::from_slice(&links.stdout).unwrap();
    assert_eq!(links.len(), 1, "refusing interface fault outside an isolated namespace");
    assert_eq!(links[0]["ifname"], "lo");
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(40), async {
        let sa = ReliableSession::new("z3-network".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let sb = ReliableSession::new("z3-network".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let mut pair = PairFixture::connect_on(sa.clone(), sb.clone(), "127.0.0.2:0").await;
        let before = z3_candidate_endpoints(&pair.a).await;
        assert!(before.iter().all(|(address, _, _)| address == "127.0.0.2"));
        let mut received = Vec::new();
        let mut incoming = pair.target.take_incoming(Some(Lane::Commands)).unwrap();
        let mut held = None;
        for id in 0..16u64 {
            pair.source.send(Lane::Commands, WireSeq(id), &id.to_be_bytes(), Some(Arc::new(Authorized))).await.unwrap();
            let Some(Incoming::Reliable {bytes, delivery:Some(token), ..}) = incoming.recv().await else {panic!("network prefix missing");};
            assert_eq!(bytes, id.to_be_bytes());
            if id == 15 {held = Some(token);} else {received.push(id); token.acknowledge().unwrap();}
        }
        // Flip the active interface while the final delivery is uncommitted.
        assert!(std::process::Command::new("ip").args(["link", "set", "lo", "down"]).status().unwrap().success());
        pair.close().await; drop(held);
        assert!(std::process::Command::new("ip").args(["link", "set", "lo", "up"]).status().unwrap().success());
        pair = PairFixture::connect_on(sa.clone(), sb, "127.0.0.3:0").await;
        let after = z3_candidate_endpoints(&pair.a).await;
        assert!(after.iter().all(|(address, _, _)| address == "127.0.0.3"));
        assert_ne!(before, after, "network recovery retained the original candidates");
        incoming = pair.target.take_incoming(Some(Lane::Commands)).unwrap();
        for id in 15..32u64 {
            if id != 15 {pair.source.send(Lane::Commands, WireSeq(id), &id.to_be_bytes(), Some(Arc::new(Authorized))).await.unwrap();}
            let replay = tokio::time::timeout(Duration::from_secs(2), incoming.recv()).await.expect("network change must replay from prefix");
            let Some(Incoming::Reliable {bytes, delivery:Some(token), ..}) = replay else {panic!("network suffix missing");};
            assert_eq!(bytes, id.to_be_bytes()); received.push(id); token.acknowledge().unwrap();
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while !sa.replay(pair.source.session.as_ref().unwrap().generation, Lane::Commands).unwrap().is_empty() {tokio::time::sleep(Duration::from_millis(1)).await;}
        }).await.expect("network suffix was not acknowledged");
        assert_eq!(received, (0..32u64).collect::<Vec<_>>()); pair.close().await;
        eprintln!("Z3_P4_NETWORK {}", serde_json::json!({"scenario":2,"interface":"lo","interface_flips":1,"candidates_before":before,"candidates_after":after,"sent":32,"explicit_send_results":32,"delivery_acknowledged":32,"replayed":1,"lost":0,"duplicated":0}));
    }).await.expect("network change fixture deadline");
}

#[test]
#[ignore = "launches existing PairFixture in fresh native processes"]
fn z3_p4_process_restart_from_persisted_cursor() {
    let root = tempfile::tempdir().unwrap();
    let data: Vec<_> = (0..CHUNK_BYTES*16).map(|i| (i%251) as u8).collect();
    std::fs::write(root.path().join("source"), &data).unwrap();
    let reference = format!("{:x}", Sha256::digest(&data));
    let executable = std::env::current_exe().unwrap(); let mut pids = Vec::new();
    for phase in ["prefix", "suffix"] {
        let mut child = std::process::Command::new(&executable).args(["z3_p4_restart_child", "--ignored", "--nocapture", "--test-threads=1"])
            .env("Z3_RESTART_ROOT", root.path()).env("Z3_RESTART_PHASE", phase).spawn().unwrap();
        pids.push(child.id()); assert!(child.wait().unwrap().success(), "peer process restart must recover persisted cursor");
    }
    assert_ne!(pids[0], pids[1]);
    let effects: Vec<usize> = std::fs::read_to_string(root.path().join("effects")).unwrap().lines().map(|n| n.parse().unwrap()).collect();
    assert_eq!(effects, (0..data.len()).step_by(CHUNK_BYTES).collect::<Vec<_>>(), "persisted cursor replay lost or duplicated an application effect");
    Cas::open(root.path().join("target")).unwrap().verified(&reference, data.len() as u64).unwrap();
    eprintln!("Z3_P4_RESTART {}", serde_json::json!({"scenario":4,"pids":pids,"process_restarts":1,"endpoints_recreated":2,"session_reset":true,"cursor":"CAS durable byte prefix","resume_offset":data.len()/2,"bytes":data.len(),"sent":16,"explicit_send_results":16,"delivery_acknowledged":16,"lost":0,"duplicated":0,"sha256_verified":true}));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "subprocess helper for persisted cursor restart fixture"]
async fn z3_p4_restart_child() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let root = std::path::PathBuf::from(std::env::var_os("Z3_RESTART_ROOT").expect("restart fixture root"));
    let phase = std::env::var("Z3_RESTART_PHASE").unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        let data = std::fs::read(root.join("source")).unwrap(); let reference = format!("{:x}", Sha256::digest(&data));
        let target = Cas::open(root.join("target")).unwrap();
        let mut sink = ArtifactReceiver::open(&target, &"f".repeat(64), &reference, data.len() as u64).unwrap();
        let offset = sink.offset().unwrap() as usize;
        assert_eq!(offset, if phase == "prefix" {0} else {data.len()/2}, "persisted cursor was not recovered after process restart");
        let id = format!("restart-{phase}");
        let sa = ReliableSession::new(id.clone(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let sb = ReliableSession::new(id, ADMISSION[..6].try_into().unwrap()).unwrap();
        assert_eq!(sa.hello().resume_from, [WireSeq(0);6], "new process must not invent old E14 delivery state");
        let pair = PairFixture::connect(sa.clone(), sb).await;
        let mut cursor = pair.source.take_incoming(Some(Lane::Commands)).unwrap();
        pair.target.send(Lane::Commands, WireSeq(0), &(offset as u64).to_be_bytes(), Some(Arc::new(Authorized))).await.unwrap();
        let Some(Incoming::Reliable {bytes, delivery:Some(token), ..}) = cursor.recv().await else {panic!("durable cursor response missing");};
        let resume = u64::from_be_bytes(bytes.try_into().unwrap()) as usize; token.acknowledge().unwrap();
        let mut incoming = pair.target.take_incoming(Some(Lane::Transfer)).unwrap();
        let end = if phase == "prefix" {data.len()/2} else {data.len()};
        let mut effects = std::fs::OpenOptions::new().create(true).append(true).open(root.join("effects")).unwrap();
        for next in (resume..end).step_by(CHUNK_BYTES) {
            let payload = packet(&reference, data.len() as u64, next as u64, &data[next..next+CHUNK_BYTES]).unwrap();
            pair.source.send(Lane::Transfer, WireSeq(next as u64), &payload, Some(Arc::new(Authorized))).await.unwrap();
            let Some(Incoming::Reliable {bytes, delivery:Some(token), ..}) = incoming.recv().await else {panic!("restart suffix missing");};
            let (header, _) = decode(&bytes).unwrap(); assert_eq!(header.offset, WireSeq(next as u64));
            let (updated, _) = sink.deliver(bytes, token).await.unwrap(); sink = updated;
            writeln!(effects, "{next}").unwrap(); effects.sync_all().unwrap();
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while !sa.replay(pair.source.session.as_ref().unwrap().generation, Lane::Transfer).unwrap().is_empty() {tokio::time::sleep(Duration::from_millis(1)).await;}
        }).await.expect("restart suffix needs cumulative delivery acknowledgement");
        assert_eq!(sink.offset().unwrap(), end as u64); pair.close().await;
    }).await.expect("restart child deadline");
}


async fn z3_candidate_endpoints(pc: &Arc<dyn PeerConnection>) -> Vec<(String, u16, String)> {
    let description = pc.local_description().await.unwrap();
    let candidates: Vec<_> = description.sdp.lines().filter(|line| line.starts_with("a=candidate:")).map(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        (fields[4].to_owned(), fields[5].parse().unwrap(), fields[7].to_owned())
    }).collect();
    assert!(!candidates.is_empty(), "connected peer must publish actual candidates");
    candidates
}

/// Network shaping sits on the TURN datagram path, not on application sends.
/// A frame crosses two 37.5-ms shims per direction: nominal end-to-end RTT 150ms.
struct Z3UdpShape {
    port: u16,
    task: tokio::task::JoinHandle<()>,
    packets: Arc<std::sync::atomic::AtomicU64>,
    drops: Arc<std::sync::atomic::AtomicU64>,
}
impl Z3UdpShape {
    async fn start(server: std::net::SocketAddr, rtt_ms: u64) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        let front = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let back = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        back.connect(server).await.unwrap();
        let port = front.local_addr().unwrap().port();
        let packets = Arc::new(AtomicU64::new(0)); let drops = Arc::new(AtomicU64::new(0));
        let seen = packets.clone(); let lost = drops.clone();
        let task = tokio::spawn(async move {
            let mut from_native = vec![0;65536]; let mut from_turn = vec![0;65536];
            let mut native = None;
            let mut delayed: futures::stream::FuturesUnordered<futures::future::BoxFuture<'static, (bool, Vec<u8>)>> = futures::stream::FuturesUnordered::new();
            loop {
                let packet = tokio::select! {
                    received = front.recv_from(&mut from_native), if delayed.len() < 128 => {
                        let (n, addr) = received.unwrap(); native = Some(addr);
                        (true, from_native[..n].to_vec())
                    }
                    received = back.recv(&mut from_turn), if delayed.len() < 128 => {
                        let n = received.unwrap(); (false, from_turn[..n].to_vec())
                    }
                    Some((outbound, bytes)) = delayed.next(), if !delayed.is_empty() => {
                        if outbound {back.send(&bytes).await.unwrap();} else {front.send_to(&bytes, native.unwrap()).await.unwrap();}
                        continue;
                    }
                };
                let n = seen.fetch_add(1, Ordering::Relaxed)+1;
                if n%100 == 0 {lost.fetch_add(1, Ordering::Relaxed); continue;}
                delayed.push(Box::pin(async move {
                    tokio::time::sleep(Duration::from_micros(rtt_ms * 1000 / 4)).await;
                    packet
                }));
            }
        });
        Self {port,task,packets,drops}
    }
}
impl Drop for Z3UdpShape {fn drop(&mut self) {self.task.abort();}}

static Z3_CLOCK: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
fn z3_elapsed_ms() -> f64 {Z3_CLOCK.get_or_init(Instant::now).elapsed().as_secs_f64()*1000.0}
fn z3_app_timeline(id: u64, event: &str) {
    eprintln!("SCTP_APP {}",serde_json::json!({"id":id,"event":event,"ms":z3_elapsed_ms()}));
}
struct Z3SctpLogger;
static Z3_SCTP_LOGGER: Z3SctpLogger = Z3SctpLogger;
impl log::Log for Z3SctpLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {metadata.target()=="ctox_sctp_timeline" || metadata.level()<=log::Level::Warn}
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            let prefix = if record.target()=="ctox_sctp_timeline" {"SCTP_TIMELINE"} else {"TURN_TRANSPORT_WARNING"};
            eprintln!("{prefix} {}",serde_json::json!({"ms":z3_elapsed_ms(),"target":record.target(),"message":record.args().to_string()}));
        }
    }
    fn flush(&self) {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "isolated coturn UDP listener and unprivileged netns required"]
async fn z3_p4_turn_loss_rtt() {
    use webrtc::peer_connection::{RTCConfigurationBuilder,RTCIceServer,RTCIceTransportPolicy};
    use std::sync::atomic::Ordering;
    assert_eq!(std::env::var("Z3_OWN_NETNS").as_deref(), Ok("1"));
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = log::set_logger(&Z3_SCTP_LOGGER);
    log::set_max_level(log::LevelFilter::Trace);
    let rtt_ms = std::env::var("Z3_RTT_MS").unwrap_or_else(|_| "150".into()).parse::<u64>().unwrap();
    assert!([20,150,600].contains(&rtt_ms));
    // This benchmark serializes three application roundtrips per probe. A
    // 600-ms bulk path needs a longer observation window, not a runtime timeout.
    let observation_seconds = if rtt_ms == 600 { 600 } else { 180 };
    // Full acceptance always defaults to 64. A smaller explicitly tagged sample
    // can diagnose a high-RTT path within a bounded task; it cannot pass the gate.
    let probes = std::env::var("Z3_PROBE_COUNT").map(|value| value.parse::<u64>().unwrap()).unwrap_or(64);
    assert!([16,64].contains(&probes));
    tokio::time::timeout(Duration::from_secs(observation_seconds), async {
        let bulk_enabled = std::env::var("Z3_TURN_BULK").as_deref() == Ok("1");
        let server: std::net::SocketAddr = std::env::var("Z3_TURN_LISTENER").unwrap().parse().unwrap();
        let a = Z3UdpShape::start(server,rtt_ms).await; let b = Z3UdpShape::start(server,rtt_ms).await;
        let configuration = |port| RTCConfigurationBuilder::new()
            .with_ice_transport_policy(RTCIceTransportPolicy::Relay)
            .with_ice_servers(vec![RTCIceServer {urls:vec![format!("turn:127.0.0.1:{port}?transport=udp")],username:"z3-fixture".into(),credential:"z3-fixture-only".into()}]).build();
        let sa = ReliableSession::new("z3-turn".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let sb = ReliableSession::new("z3-turn".into(), ADMISSION[..6].try_into().unwrap()).unwrap();
        let pair = PairFixture::connect_configured(sa.clone(),sb.clone(),"127.0.0.1:0",[configuration(a.port),configuration(b.port)],true).await;
        let candidates_a = z3_candidate_endpoints(&pair.a).await;
        let candidates_b = z3_candidate_endpoints(&pair.b).await;
        assert!(candidates_a.iter().chain(&candidates_b).all(|(_,_,kind)| kind == "relay"), "TURN-only fixture published a host/srflx candidate");
        let mut ac = pair.source.take_incoming(Some(Lane::Commands)).unwrap();
        let mut bc = pair.target.take_incoming(Some(Lane::Commands)).unwrap();
        let mut control = pair.target.take_incoming(Some(Lane::Control)).unwrap();
        let mut replies = pair.source.take_incoming(Some(Lane::Control)).unwrap();
        let mut bulk = pair.target.take_incoming(Some(Lane::Transfer)).unwrap();
        let bulk_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let bulk_received = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let window = Arc::new(tokio::sync::Semaphore::new(16));
        let mut seed = 0x6a09e667f3bcc909u64;
        let mut payload = vec![0;16_000];
        for byte in &mut payload {seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; *byte=seed as u8;}
        let expected = payload.clone(); let count = bulk_received.clone(); let permits = window.clone();
        let receive_bulk = tokio::spawn(async move {
            while let Some(Incoming::Reliable{bytes,delivery:Some(token),..})=bulk.recv().await {
                assert_eq!(bytes,expected);token.acknowledge().unwrap();count.fetch_add(1,Ordering::Relaxed);permits.add_permits(1);
            }
        });
        let send_bulk = pair.source.clone(); let stop = bulk_stop.clone(); let permits = window.clone();
        let bulk_clock = Instant::now();
        let produce_bulk = tokio::spawn(async move {
            let mut count=0u64;
            while bulk_enabled && !stop.load(Ordering::Relaxed) {
                let permit = permits.acquire().await.unwrap();
                if stop.load(Ordering::Relaxed) {break;}
                permit.forget();
                send_bulk.send(Lane::Transfer,WireSeq(count),&payload,Some(Arc::new(Authorized))).await.unwrap();count+=1;
            }
            count
        });
        let mut latency = Vec::new(); let mut effects_a=Vec::new(); let mut effects_b=Vec::new();
        for id in 0..probes {
            pair.source.send(Lane::Commands,WireSeq(id),&id.to_be_bytes(),Some(Arc::new(Authorized))).await.unwrap();
            pair.target.send(Lane::Commands,WireSeq(id),&id.to_be_bytes(),Some(Arc::new(Authorized))).await.unwrap();
            for (incoming,effects) in [(&mut ac,&mut effects_a),(&mut bc,&mut effects_b)] {
                let Some(Incoming::Reliable{bytes,delivery:Some(token),..})=incoming.recv().await else {panic!("TURN application message missing");};
                assert_eq!(bytes,id.to_be_bytes()); effects.push(id); token.acknowledge().unwrap();
            }
            let clock=Instant::now();
            let probe = [b"PROB".as_slice(),id.to_be_bytes().as_slice()].concat();
            z3_app_timeline(id,"send");
            pair.source.send(Lane::Control,WireSeq(id),&probe,Some(Arc::new(Authorized))).await.unwrap();
            let Some(Incoming::Reliable{bytes,delivery:Some(token),..})=control.recv().await else {panic!("TURN control missing");};
            assert_eq!(bytes,probe);z3_app_timeline(id,"deliver_request");token.acknowledge().unwrap();
            pair.target.send(Lane::Control,WireSeq(id),&probe,Some(Arc::new(Authorized))).await.unwrap();
            let Some(Incoming::Reliable{bytes,delivery:Some(token),..})=replies.recv().await else {panic!("TURN control reply missing");};
            assert_eq!(bytes,probe);z3_app_timeline(id,"deliver_reply");token.acknowledge().unwrap();latency.push(clock.elapsed().as_secs_f64()*1000.0);
        }
        bulk_stop.store(true,Ordering::Relaxed);
        let bulk_sent = produce_bulk.await.unwrap();
        tokio::time::timeout(Duration::from_secs(20),async {while bulk_received.load(Ordering::Relaxed)!=bulk_sent {tokio::time::sleep(Duration::from_millis(5)).await;}}).await.expect("bulk admission lost delivery");
        receive_bulk.abort();let _=receive_bulk.await;
        let bulk_seconds=bulk_clock.elapsed().as_secs_f64();
        tokio::time::timeout(Duration::from_secs(5),async {
            loop {
                if [(&sa,&pair.source),(&sb,&pair.target)].iter().all(|(session,scheduler)| [Lane::Commands,Lane::Control,Lane::Transfer].iter().all(|lane| session.replay(scheduler.session.as_ref().unwrap().generation,*lane).unwrap().is_empty())) {break;}
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }).await.expect("TURN application delivery acknowledgements missing");
        assert_eq!(effects_a,(0..probes).collect::<Vec<_>>());assert_eq!(effects_b,effects_a);
        let packets=a.packets.load(Ordering::Relaxed)+b.packets.load(Ordering::Relaxed);
        let drops=a.drops.load(Ordering::Relaxed)+b.drops.load(Ordering::Relaxed);
        assert!(drops>0,"loss shaper must drop actual network datagrams");
        latency.sort_by(f64::total_cmp);let p95=latency[(latency.len()*95/100).min(latency.len()-1)];
        let p99=latency[(latency.len()*99/100).min(latency.len()-1)];
        pair.close().await;
        eprintln!("Z3_P4_TURN {}",serde_json::json!({"scenario":3,"relay_only":true,"candidates_a":candidates_a,"candidates_b":candidates_b,"shim":"UDP at TURN control legs","nominal_rtt_ms":rtt_ms,"drop_every":100,"datagrams":packets,"dropped_datagrams":drops,"probes":probes,"full_workload":probes==64,"sent":2*probes,"explicit_send_results":2*probes,"delivery_acknowledged":2*probes,"lost":0,"duplicated":0,"control_p95_ms":p95,"control_p99_ms":p99,"control_roundtrip_ms":latency,"bulk_enabled":bulk_enabled,"bulk_bytes":bulk_sent*16_000,"bulk_seconds":bulk_seconds,"bulk_mb_s":bulk_sent as f64*16_000.0/bulk_seconds/1_000_000.0}));
    }).await.expect("TURN loss RTT fixture deadline");
}
