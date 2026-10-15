use super::*;
use super::super::{AssociationBudget, Framer, Incoming, ReliableSession, SessionBinding, ADMISSION};
use super::super::super::{hello::{Codec, TransportSelection}, lanes::{FRAME_BYTES, REALTIME_LABEL}};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::{io::Write, time::{Duration, Instant}};
use tokio::sync::mpsc;
use webrtc::{data_channel::DataChannel, peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit, RTCPeerConnectionIceEvent}};

#[test]
fn lane4_packet_resume_and_budget_contract() {
    let address = ArtifactAddress::parse("/~a/mail@42~shell@7/assets/main.js").unwrap();
    assert_eq!((address.app, address.app_version, address.shell_version, address.path), ("mail", "42", "7", "assets/main.js"));
    for bad in ["/~a/mail@42/main.js", "/~a/mail@42~shell@7/../x", "/~a/mail@42~shell@7/%2e%2e/x"] { assert!(ArtifactAddress::parse(bad).is_err()); }
    let dir = tempfile::tempdir().unwrap(); let cas = Cas::open(dir.path()).unwrap();
    let data = b"partial durable bytes"; let reference = format!("{:x}", Sha256::digest(data));
    let scope = "a".repeat(64);
    let mut target = ArtifactReceiver::open(&cas, &scope, &reference, data.len() as u64).unwrap();
    target.apply(&packet(&reference, data.len() as u64, 0, &data[..7]).unwrap()).unwrap();
    drop(target);
    let mut target = ArtifactReceiver::open(&cas, &scope, &reference, data.len() as u64).unwrap();
    assert_eq!(target.offset().unwrap(), 7);
    assert!(target.apply(&packet(&reference, data.len() as u64, 8, &data[8..]).unwrap()).is_err(), "resume gap accepted");
    let blob = target.apply(&packet(&reference, data.len() as u64, 7, &data[7..]).unwrap()).unwrap().unwrap();
    assert_eq!(blob.reference(), reference);
    let mut budget = AssociationBudget::default();
    // Frame-size guard and aggregate credit must both reject this oversized
    // native handoff, even if the other channels are idle.
    assert!(!budget.reserve(Lane::Transfer.index(), ASSOCIATION_BUDGET + 1), "budget overflow admitted");
    for lane in 0..7 { assert!(budget.reserve(lane, FRAME_BYTES)); }
    assert!(budget.buffered() <= ASSOCIATION_BUDGET);
    assert!(!budget.reserve(Lane::Transfer.index(), FRAME_BYTES));
    budget.drained(Lane::Control.index());
    assert!(budget.reserve(Lane::Control.index(), FRAME_BYTES), "bulk blocked control credit");
}

struct Events { candidates: mpsc::UnboundedSender<RTCIceCandidateInit>, channels: mpsc::UnboundedSender<Arc<dyn DataChannel>> }
#[async_trait]
impl PeerConnectionEventHandler for Events {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Ok(candidate) = event.candidate.to_json() { let _ = self.candidates.send(candidate); }
    }
    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) { let _ = self.channels.send(channel); }
}
struct Authorized;
impl WebRTCPublicationGuard for Authorized {
    fn with_current(&self, effect: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> { effect() }
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
    let mut hash = Sha256::new(); let mut seed = 0x6a09e667f3bcc909u64;
    let mut block = [0; CHUNK_BYTES];
    for _ in 0..SIZE / CHUNK_BYTES as u64 {
        for b in &mut block { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; *b = seed as u8; }
        hash.update(block); file.write_all(&block).unwrap();
    }
    file.sync_all().unwrap(); drop(file);
    let reference = format!("{:x}", hash.finalize());
    std::fs::rename(root.path().join("fixture"), root.path().join("source/objects").join(&reference)).unwrap();
    let blob = source.verified(&reference, SIZE).unwrap();
    let (atx, mut ac) = mpsc::unbounded_channel(); let (btx, mut bc) = mpsc::unbounded_channel();
    let (unused, _discard) = mpsc::unbounded_channel(); let (incoming, mut channels) = mpsc::unbounded_channel();
    let a: Arc<dyn PeerConnection> = Arc::new(PeerConnectionBuilder::new().with_handler(Arc::new(Events { candidates: atx, channels: unused })).with_udp_addrs(vec!["127.0.0.1:0"]).build().await.unwrap());
    let b: Arc<dyn PeerConnection> = Arc::new(PeerConnectionBuilder::new().with_handler(Arc::new(Events { candidates: btx, channels: incoming })).with_udp_addrs(vec!["127.0.0.1:0"]).build().await.unwrap());
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
        for _ in 0..7 { let channel = channels.recv().await.unwrap(); let label = channel.label().await.unwrap(); let index = Lane::ALL.iter().position(|l| l.label() == label).unwrap_or_else(|| { assert_eq!(label, REALTIME_LABEL); 6 }); accepted[index] = Some(channel); }
        let receiver = NativeSendScheduler::from_channels_session(accepted.into_iter().map(Option::unwrap).collect(), Framer::new(transport).unwrap(), Some(binding_b)).await.unwrap();
        let mut controls = receiver.take_incoming(Some(Lane::Control)).unwrap();
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
                assert_eq!(bytes, b"lane4-probe"); latencies.push(start.elapsed().as_secs_f64() * 1000.0); delivery.acknowledge().unwrap();
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
        assert!(p99 <= 100.0, "control p99 {p99}ms during bulk");
        target.verified(&reference, SIZE).unwrap();
        eprintln!("LANE4_MEASUREMENT {}", serde_json::json!({"transport":"native-native real WebRTC loopback", "relay":false,"bytes":SIZE,"seconds":elapsed,"mb_per_second":SIZE as f64/1_000_000.0/elapsed,"control_p99_ms":p99,"sctp_buffer_upper_bound_peak":peak,"sha256_verified":true}));
        sender.close_now(); receiver.close_now();
    }).await;
    tasks.abort_all(); while tasks.join_next().await.is_some() {}
    a.close().await.unwrap(); b.close().await.unwrap(); result.unwrap();
}
