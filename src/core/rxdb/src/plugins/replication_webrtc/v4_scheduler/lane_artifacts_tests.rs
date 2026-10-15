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
        // Counterfactual: the same control path with no blob and no CAS IO.
        // Keep the loaded assertion below; idle data only locates the delay.
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
                if sink.offset().unwrap() % (5 * 1024 * 1024) == 0 { eprintln!("LANE4_PROGRESS bytes={} elapsed_ms={}", sink.offset().unwrap(), transfer_clock.elapsed().as_millis()); }
            }
            assert_eq!(verified.unwrap().reference(), expected);
            eprintln!("LANE4_RECEIVE_STAGES {}", serde_json::json!({"wait_seconds":wait_seconds,"persist_verify_seconds":io_seconds,"debug_assertions":cfg!(debug_assertions)}));
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
        assert!(idle_p99 <= 20.0, "idle control p99 {idle_p99}ms");
        assert!(SIZE as f64 / 1_000_000.0 / elapsed >= 5.0, "bulk throughput below 5 MB/s");
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
        let (source, target) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::try_join!(create, async {
                Ok::<_, crate::rx_error::RxError>(accept.await)
            })
        })
        .await
        .expect("native post-bootstrap channels did not open")
        .unwrap_or_else(|e| panic!("native resumed constructor: {e:?}"));
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
    tokio::time::timeout(Duration::from_secs(10), async {
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
