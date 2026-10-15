use super::super::super::{
    hello::{Codec, TransportSelection},
    lanes::{ASSOCIATION_BUDGET, FRAME_BYTES, REALTIME_LABEL},
};
use super::super::{
    AssociationBudget, Framer, Incoming, ReliableSession, SessionBinding, ADMISSION,
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
        let transfer_clock = Instant::now();
        let receive = async move {
            let mut verified = None;
            while verified.is_none() {
                let Some(Incoming::Reliable { bytes, delivery: Some(delivery), .. }) = bulks.recv().await else { panic!("missing artifact delivery"); };
                let (next, blob) = sink.deliver(bytes, delivery).await.unwrap(); sink = next; verified = blob;
                if sink.offset().unwrap() % (5 * 1024 * 1024) == 0 { eprintln!("LANE4_PROGRESS bytes={} elapsed_ms={}", sink.offset().unwrap(), transfer_clock.elapsed().as_millis()); }
            }
            assert_eq!(verified.unwrap().reference(), expected);
            done.store(true, std::sync::atomic::Ordering::Release);
        };
        let control_sender = sender.clone();
        let probe = async move {
            let mut latencies = Vec::new();
            while !finished.load(std::sync::atomic::Ordering::Acquire) || latencies.is_empty() {
                let start = Instant::now();
                control_sender.send(Lane::Control, WireSeq(1), b"lane4-probe", Some(Arc::new(Authorized))).await.unwrap();
                let Some(Incoming::Reliable { bytes, delivery: Some(delivery), .. }) = controls.recv().await else { panic!("missing control"); };
                assert_eq!(bytes, b"lane4-probe"); delivery.acknowledge().unwrap();
                reply_sender.send(Lane::Control, WireSeq(1), b"lane4-reply", Some(Arc::new(Authorized))).await.unwrap();
                let Some(Incoming::Reliable { bytes, delivery: Some(delivery), .. }) = replies.recv().await else { panic!("missing control reply"); };
                assert_eq!(bytes, b"lane4-reply"); latencies.push(start.elapsed().as_secs_f64() * 1000.0); delivery.acknowledge().unwrap();
                if latencies.len() % 100 == 0 { eprintln!("LANE4_CONTROL latest_ms={}", latencies.last().unwrap()); }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            latencies.sort_by(f64::total_cmp); latencies[(latencies.len() * 99 / 100).min(latencies.len()-1)]
        };
        let start = Instant::now();
        let (sent, (), p99) = tokio::join!(send_blob(&sender, source.clone(), blob, 0, Arc::new(Authorized)), receive, probe);
        sent.unwrap(); let elapsed = start.elapsed().as_secs_f64();
        let peak = sender.handoff_counters().buffered_upper_bound_peak;
        assert!(peak <= ASSOCIATION_BUDGET as u64, "connection-wide budget exceeded");

        target.verified(&reference, SIZE).unwrap();
        eprintln!("LANE4_MEASUREMENT {}", serde_json::json!({"transport":"native-native real WebRTC loopback", "relay":false,"bytes":SIZE,"seconds":elapsed,"mb_per_second":SIZE as f64/1_000_000.0/elapsed,"control_roundtrip_p99_ms":p99,"sctp_buffer_upper_bound_peak":peak,"sha256_verified":true,"target_buffer_peak":receiver.handoff_counters().buffered_upper_bound_peak}));
        assert!(p99 <= 100.0, "control p99 {p99}ms during bulk");
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
                eprintln!("LANE4_CHANNEL {label}");
                let index = Lane::ALL
                    .iter()
                    .position(|l| l.label() == label)
                    .unwrap_or_else(|| {
                        assert_eq!(label, REALTIME_LABEL);
                        6
                    });
                accepted[index] = Some(channel);
            }
            NativeSendScheduler::from_channels_session(
                accepted.into_iter().map(Option::unwrap).collect(),
                Framer::new(transport.clone()).unwrap(),
                Some(bb),
            )
            .await
            .unwrap()
        };
        let create = NativeSendScheduler::create_resumed(&a, transport.clone(), ba, |_, _, _| {
            Ok(Arc::new(Authorized) as Arc<dyn WebRTCPublicationGuard>)
        });
        let (source, target) = tokio::join!(create, accept);
        let source = source.unwrap_or_else(|e| panic!("native resumed constructor: {e:?}"));
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
async fn lane4_real_webrtc_drop_after_persistence_replays_then_resumes_prefix() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(30), async {
        let dir = tempfile::tempdir().unwrap();
        let source = Cas::open(dir.path().join("source")).unwrap();
        let target = Cas::open(dir.path().join("target")).unwrap();
        let bytes = vec![93; CHUNK_BYTES * 2];
        std::fs::write(dir.path().join("primitive"), &bytes).unwrap();
        let blob = source
            .import(dir.path().join("primitive"), "d".repeat(64))
            .await
            .unwrap();
        let sa = ReliableSession::new("artifact-drop".into(), ADMISSION[..6].try_into().unwrap())
            .unwrap();
        let sb = ReliableSession::new("artifact-drop".into(), ADMISSION[..6].try_into().unwrap())
            .unwrap();
        let pair = PairFixture::connect(sa.clone(), sb.clone()).await;
        let mut bulk = pair.target.take_incoming(Some(Lane::Transfer)).unwrap();
        let scope = "e".repeat(64);
        let mut sink =
            ArtifactReceiver::open(&target, &scope, blob.reference(), blob.size()).unwrap();
        let payload = packet(blob.reference(), blob.size(), 0, &bytes[..CHUNK_BYTES]).unwrap();
        pair.source
            .send(
                Lane::Transfer,
                WireSeq(0),
                &payload,
                Some(Arc::new(Authorized)),
            )
            .await
            .unwrap();
        let Some(Incoming::Reliable {
            bytes: received,
            delivery: Some(unacknowledged),
            ..
        }) = bulk.recv().await
        else {
            panic!("missing pre-drop chunk");
        };
        sink.apply(&received).unwrap();
        eprintln!("LANE4_RESUME stage=persisted-before-lost-ack"); // effect happened, ACK lost with channel
        assert_eq!(sink.offset().unwrap(), CHUNK_BYTES as u64);
        drop(unacknowledged);
        drop(sink);
        pair.close().await;
        assert_eq!(sb.hello().resume_from[Lane::Transfer.index()], WireSeq(0));
        let pair = PairFixture::connect(sa.clone(), sb).await;
        let mut bulk = pair.target.take_incoming(Some(Lane::Transfer)).unwrap();
        let sink = ArtifactReceiver::open(&target, &scope, blob.reference(), blob.size()).unwrap();
        eprintln!("LANE4_RESUME stage=waiting-replay");
        let Some(Incoming::Reliable {
            bytes: replay,
            delivery: Some(delivery),
            ..
        }) = bulk.recv().await
        else {
            panic!("lost message not replayed");
        };
        assert_eq!(replay, payload);
        eprintln!("LANE4_RESUME stage=replayed");
        let (mut sink, completed) = sink.deliver(replay, delivery).await.unwrap();
        assert!(completed.is_none());
        assert_eq!(sink.offset().unwrap(), CHUNK_BYTES as u64);
        send_blob(
            &pair.source,
            source.clone(),
            blob.clone(),
            sink.offset().unwrap(),
            Arc::new(Authorized),
        )
        .await
        .unwrap();
        let Some(Incoming::Reliable {
            bytes: suffix,
            delivery: Some(delivery),
            ..
        }) = bulk.recv().await
        else {
            panic!("missing resumed suffix");
        };
        let (next, completed) = sink.deliver(suffix, delivery).await.unwrap();
        sink = next;
        assert_eq!(completed.unwrap().reference(), blob.reference());
        assert_eq!(sink.offset().unwrap(), blob.size());
        target.verified(blob.reference(), blob.size()).unwrap();
        let closed = pair.source.clone();
        pair.close().await;
        assert!(
            tokio::time::timeout(
                Duration::from_secs(1),
                send_blob(&closed, source, blob, 0, Arc::new(Authorized))
            )
            .await
            .unwrap()
            .is_err(),
            "closed association left producer waiting"
        );
    })
    .await
    .unwrap();
}
