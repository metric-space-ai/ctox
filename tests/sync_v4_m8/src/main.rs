//! Native M8 component profile. Test fixture admission is NOT CUTOVER-3 acceptance.
mod commands;
mod network;
use async_trait::async_trait;
use futures::StreamExt;
use rxdb::plugins::replication_webrtc::index_mod::{
    hello::{Codec, TransportSelection, WireSeq},
    lanes::{Lane, FRAME_BYTES, REALTIME_LABEL},
    scheduler::{m8_bridge, Incoming, NativeSendScheduler},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use webrtc::{
    data_channel::DataChannel,
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
        RTCPeerConnectionIceEvent,
    },
};
static START: OnceLock<Instant> = OnceLock::new();
pub fn elapsed_ns() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}
fn transport() -> TransportSelection {
    TransportSelection {
        codec: Codec::Zstd,
        frame_bytes: FRAME_BYTES as u32,
        fragmentation: true,
        reassembly: true,
    }
}
struct Events {
    candidates: mpsc::UnboundedSender<RTCIceCandidateInit>,
    channels: Option<mpsc::UnboundedSender<Arc<dyn DataChannel>>>,
    opened: Arc<AtomicU64>,
}
#[async_trait]
impl PeerConnectionEventHandler for Events {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Ok(c) = event.candidate.to_json() {
            let _ = self.candidates.send(c);
        }
    }
    async fn on_data_channel(&self, ch: Arc<dyn DataChannel>) {
        // The pinned native driver invokes this only for an actual OnOpen.
        // Do not poll its local callback handle: the scheduler owns the real one.
        let label = ch.label().await.unwrap();
        if Lane::ALL.iter().any(|lane| lane.label() == label) {
            self.opened.fetch_add(1, Ordering::SeqCst);
        } else {
            assert_eq!(label, REALTIME_LABEL);
        }
        if let Some(channels) = &self.channels {
            assert!(channels.send(ch).is_ok(), "native accept receiver closed");
        }
    }
}
#[derive(Default)]
struct Bulk {
    bytes: u64,
    messages: u64,
    sha: Sha256,
}
impl Bulk {
    fn accept(&mut self, id: u64, b: &[u8]) {
        assert_eq!(
            id,
            self.messages + 1,
            "bulk sequence must be contiguous, no silent loss/duplicate"
        );
        self.bytes += b.len() as u64;
        self.messages += 1;
        self.sha.update(b);
    }
    fn value(&self) -> Value {
        json!({"bytes":self.bytes,"messages":self.messages,"sha256":format!("{:x}",self.sha.clone().finalize())})
    }
}
#[derive(Default)]
struct Observed {
    command_ms: Vec<f64>,
    terminal_ms: Vec<f64>,
    live_ms: Vec<f64>,
    sent: u64,
    received: u64,
    duplicates: u64,
    live_docs: u64,
    live_packets: u64,
    bulk: Bulk,
    source: Bulk,
    bulk_active_probes: u64,
    live_ids: std::collections::HashSet<String>,
    live_document_ids: std::collections::HashSet<String>,
    cursor: u64,
}
struct Endpoint {
    sender: OnceLock<NativeSendScheduler>,
    waiters: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
    id: AtomicU64,
    stats: Mutex<Observed>,
}
async fn send_json(e: &Endpoint, lane: Lane, id: u64, value: Value) {
    e.sender
        .get()
        .expect("native sender initialized")
        .send(
            lane,
            WireSeq(id),
            &serde_json::to_vec(&value).unwrap(),
            None,
        )
        .await
        .unwrap();
}
fn verify_page(value: &Value) -> u64 {
    use ctox_store::change_log::subscription::protocol::{InstanceVerifier, StreamPacket};
    struct Key(Vec<u8>);
    impl InstanceVerifier for Key {
        fn verify(&self, instance: &str, key: &str, b: &[u8], sig: &[u8]) -> Result<(), String> {
            if instance != "m8-component" || key != "public-test-17" {
                return Err("identity".into());
            }
            ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &self.0)
                .verify(b, sig)
                .map_err(|_| "signature".into())
        }
    }
    let packet: StreamPacket = serde_json::from_value(value["page"].clone()).unwrap();
    let key: Vec<u8> = serde_json::from_value(value["public_key"].clone()).unwrap();
    // Pin the test identity independently; a packet cannot choose its own key.
    assert_eq!(key, commands::Signer::new().public());
    packet.verify(&Key(key)).unwrap();
    packet.docs.len() as u64
}
async fn application(
    index: usize,
    e: Arc<Endpoint>,
    mut rx: mpsc::Receiver<Incoming>,
    driver: Arc<tokio::sync::Mutex<commands::Driver>>,
    live_tx: [mpsc::Sender<Value>; 2],
) {
    while let Some(message) = rx.recv().await {
        let Incoming::Reliable {
            lane,
            id,
            bytes,
            delivery,
        } = message
        else {
            panic!("unexpected native input: {message:?}");
        };
        if lane == Lane::Transfer {
            e.stats.lock().unwrap().bulk.accept(id.0, &bytes);
            if let Some(delivery) = delivery {
                delivery.acknowledge().unwrap();
            }
            continue;
        }
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        match value["kind"].as_str().unwrap() {
            "terminal" => {
                value["kind"] = json!("terminal_reply");
                send_json(&e, lane, id.0, value).await;
            }
            "write" => {
                assert_eq!(index % 2, 0, "writes terminate at Core, never a client");
                let mut locked = driver.lock().await;
                let mut result = locked.execute(index / 2, value["wire"].clone()).await;
                // Queue both recipients while still ordered by the ONE Core command path.
                // Dedicated bounded lane actors perform network I/O outside the writer/driver.
                for delivery in result["deliveries"].as_array().unwrap() {
                    live_tx[delivery["target"].as_u64().unwrap() as usize]
                        .send(delivery.clone())
                        .await
                        .unwrap();
                }
                drop(locked);
                result.as_object_mut().unwrap().remove("deliveries");
                result["kind"] = json!("write_result");
                result["started_ns"] = value["started_ns"].clone();
                send_json(&e, Lane::Commands, id.0, result).await;
            }
            "terminal_reply" | "write_result" => {
                let terminal = value["kind"] == "terminal_reply";
                let latency = (elapsed_ns() - value["started_ns"].as_u64().unwrap()) as f64 / 1e6;
                let mut stats = e.stats.lock().unwrap();
                stats.received += 1;
                if terminal {
                    stats.terminal_ms.push(latency)
                } else {
                    stats.command_ms.push(latency)
                }
                drop(stats);
                if let Some(wait) = e.waiters.lock().unwrap().remove(&id.0) {
                    let _ = wait.send(value);
                } else {
                    e.stats.lock().unwrap().duplicates += 1;
                }
            }
            "live" => {
                let latency = (elapsed_ns() - value["commit_ns"].as_u64().unwrap()) as f64 / 1e6;
                let docs = verify_page(&value);
                let mut stats = e.stats.lock().unwrap();
                // The live stream must retain the terminal document produced by this command.
                let page = &value["page"];
                assert_eq!(page["participant"], format!("client-{}", index / 2));
                assert_eq!(page["scope_hash"], "m8-fixture-rights-1");
                assert_eq!(page["gen"], 1);
                let from = page["seq_from"].as_str().unwrap().parse::<u64>().unwrap();
                let to = page["seq_to"].as_str().unwrap().parse::<u64>().unwrap();
                assert_eq!(from, stats.cursor, "live cursor never skips or regresses");
                stats.cursor = to;
                assert!(page["docs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| d["body"]["status"] == "done"));
                for doc in page["docs"].as_array().unwrap() {
                    stats
                        .live_document_ids
                        .insert(doc["id"].as_str().unwrap().to_owned());
                }
                let package = page["package_id"].as_str().unwrap().to_owned();
                if !stats.live_ids.insert(package) {
                    stats.duplicates += 1;
                }
                stats.live_docs += docs;
                stats.live_packets += 1;
                stats.live_ms.push(latency);
            }
            other => panic!("unknown component input {other}"),
        }
        if let Some(delivery) = delivery {
            delivery.acknowledge().unwrap();
        }
    }
}
async fn probe(e: Arc<Endpoint>, lane: Lane, value: Value, bulk_active: &AtomicBool) {
    let id = e.id.fetch_add(1, Ordering::Relaxed) + 1;
    let (tx, rx) = oneshot::channel();
    e.waiters.lock().unwrap().insert(id, tx);
    let mut value = value;
    value["started_ns"] = json!(elapsed_ns());
    {
        let mut stats = e.stats.lock().unwrap();
        stats.sent += 1;
        if bulk_active.load(Ordering::SeqCst) {
            stats.bulk_active_probes += 1;
        }
    }
    send_json(&e, lane, id, value).await;
    let reply = tokio::time::timeout(Duration::from_secs(45), rx)
        .await
        .expect("probe response deadline")
        .expect("probe response");
    if lane == Lane::Commands {
        assert_eq!(
            reply["receipt"]["results"][0]["result"]["status"],
            "accepted"
        );
        assert_eq!(
            reply["terminal"]["results"][0]["result"]["status"],
            "accepted"
        );
    }
}
fn entropy() -> Vec<u8> {
    let mut x = 0x123456789abcdef_u64;
    (0..256 * 1024)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}
async fn bulk(e: Arc<Endpoint>, stop: Arc<AtomicBool>, active: Arc<AtomicBool>) {
    let data = entropy();
    let mut id = 0;
    active.store(true, Ordering::SeqCst);
    while !stop.load(Ordering::SeqCst) || (id * data.len() as u64) < 50 * 1024 * 1024 {
        id += 1;
        e.sender
            .get()
            .expect("native sender initialized")
            .send(Lane::Transfer, WireSeq(id), &data, None)
            .await
            .unwrap();
        e.stats.lock().unwrap().source.accept(id, &data);
    }
    active.store(false, Ordering::SeqCst);
}
fn observe_sender(
    sender: &NativeSendScheduler,
    index: usize,
    errors: Arc<Mutex<Vec<String>>>,
    tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    collecting: Arc<AtomicBool>,
) {
    let mut failures = sender.failures();
    tasks.lock().unwrap().push(tokio::spawn(async move {
        while let Some(failure) = failures.next().await {
            if !collecting.load(Ordering::SeqCst) {
                break;
            }
            let message = format!("native sender {index}: {failure:?}");
            eprintln!("M8_NATIVE_FAILURE {message}");
            errors.lock().unwrap().push(message);
        }
    }));
}
async fn profile(rtt: u64, out: &std::path::Path, seconds: u64, use_shim: bool) -> Value {
    let counters = Arc::new(network::Counters::default());
    let impaired = Arc::new(AtomicBool::new(false));
    let runtime = Arc::new(network::ImpairedRuntime {
        base: webrtc::runtime::default_runtime().unwrap(),
        delay: Duration::from_millis(rtt / 2),
        counters: counters.clone(),
        seed: AtomicU64::new(0x9e3779b97f4a7c15 + rtt),
        active: impaired.clone(),
    });
    // tc mode uses the same default runtime as the proven lane4 setup.
    let peer_runtime: Arc<dyn webrtc::runtime::Runtime> = if use_shim {
        runtime
    } else {
        webrtc::runtime::default_runtime().unwrap()
    };
    let tasks = Arc::new(Mutex::new(Vec::new()));
    let errors = Arc::new(Mutex::new(Vec::new()));
    let collecting = Arc::new(AtomicBool::new(true));
    let mut pcs: Vec<Arc<dyn PeerConnection>> = Vec::new();
    let mut ends = Vec::new();
    let mut receivers = Vec::new();
    let mut candidates = Vec::new();
    let mut opens = Vec::new();
    for index in 0..4 {
        let (channels_tx, channels_rx) = mpsc::unbounded_channel();
        let (ct, cr) = mpsc::unbounded_channel();
        let opened = Arc::new(AtomicU64::new(0));

        let pc: Arc<dyn PeerConnection> = Arc::new(
            PeerConnectionBuilder::new()
                .with_runtime(peer_runtime.clone())
                .with_handler(Arc::new(Events {
                    candidates: ct,
                    channels: if index % 2 == 1 {
                        Some(channels_tx)
                    } else {
                        None
                    },
                    opened: opened.clone(),
                }))
                .with_udp_addrs(vec!["127.0.0.1:0"])
                .build()
                .await
                .unwrap(),
        );
        let sender = OnceLock::new();
        if index % 2 == 0 {
            let native = NativeSendScheduler::create(&pc, transport()).await.unwrap();
            observe_sender(
                &native,
                index,
                errors.clone(),
                tasks.clone(),
                collecting.clone(),
            );
            sender.set(native).ok().unwrap();
        }
        ends.push(Arc::new(Endpoint {
            sender,
            waiters: Mutex::new(HashMap::new()),
            id: AtomicU64::new(0),
            stats: Mutex::new(Observed::default()),
        }));
        pcs.push(pc);
        receivers.push(Some(channels_rx));
        candidates.push(Some(cr));
        opens.push(opened);
    }
    let driver = Arc::new(tokio::sync::Mutex::new(
        commands::Driver::open(out.join(format!("rtt-{rtt}.sqlite"))).await,
    ));
    let mut jobs = tokio::task::JoinSet::new();
    let live_tx = std::array::from_fn(|client| {
        let (tx, mut rx) = mpsc::channel::<Value>(64);
        let endpoint = ends[client * 2].clone();
        jobs.spawn(async move {
            while let Some(value) = rx.recv().await {
                let seq = value["page"]["seq_to"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                send_json(&endpoint, Lane::Live, seq, value).await;
            }
        });
        tx
    });
    let mut forwarding = tokio::task::JoinSet::new();
    eprintln!("M8_SETUP strategy=lane4-accepted-channels-v1 rtt={rtt}");
    let setup = tokio::time::timeout(Duration::from_secs(30), async {
        for a in [0, 2] {
            let b = a + 1;
            let offer = pcs[a].create_offer(None).await.unwrap();
            pcs[a].set_local_description(offer.clone()).await.unwrap();
            pcs[b].set_remote_description(offer).await.unwrap();

            let answer = pcs[b].create_answer(None).await.unwrap();
            pcs[b].set_local_description(answer.clone()).await.unwrap();
            pcs[a].set_remote_description(answer).await.unwrap();
            for (from, to) in [(a, b), (b, a)] {
                let target = pcs[to].clone();
                let mut rx = candidates[from].take().unwrap();
                forwarding.spawn(async move {
                    while let Some(c) = rx.recv().await {
                        target.add_ice_candidate(c).await.unwrap()
                    }
                });
            }
            // Same accepted-channel setup as lane4_real_webrtc_50mb_control_latency_and_verified_arrival.
            let mut accepted = vec![None; 7];
            let channels = receivers[b].as_mut().unwrap();
            for _ in 0..7 {
                let channel = channels.recv().await.unwrap();
                let label = channel.label().await.unwrap();
                eprintln!("M8_ACCEPTED endpoint={b} label={label}");
                let slot = Lane::ALL
                    .iter()
                    .position(|lane| lane.label() == label)
                    .unwrap_or_else(|| {
                        assert_eq!(label, REALTIME_LABEL);
                        6
                    });
                assert!(
                    accepted[slot].replace(channel).is_none(),
                    "duplicate native channel"
                );
            }
            let native = m8_bridge::accept(
                accepted.into_iter().map(Option::unwrap).collect(),
                transport(),
            )
            .await
            .unwrap();
            observe_sender(
                &native,
                b,
                errors.clone(),
                tasks.clone(),
                collecting.clone(),
            );
            ends[b].sender.set(native).ok().unwrap();
        }
        for (i, endpoint) in ends.iter().enumerate() {
            for lane in Lane::ALL {
                let rx = endpoint
                    .sender
                    .get()
                    .unwrap()
                    .take_incoming(Some(lane))
                    .unwrap();
                jobs.spawn(application(
                    i,
                    ends[i].clone(),
                    rx,
                    driver.clone(),
                    live_tx.clone(),
                ));
            }
        }
        eprintln!("M8_SETUP schedulers_attached rtt={rtt}");
        while opens.iter().any(|n| n.load(Ordering::SeqCst) != 6) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    eprintln!(
        "M8_SETUP ready={} opens={:?}",
        setup.is_ok(),
        opens
            .iter()
            .map(|n| n.load(Ordering::SeqCst))
            .collect::<Vec<_>>()
    );
    let stop = Arc::new(AtomicBool::new(false));
    let mut load = tokio::task::JoinSet::new();
    let mut probes = tokio::task::JoinSet::new();
    let active: Vec<_> = (0..4).map(|_| Arc::new(AtomicBool::new(false))).collect();
    impaired.store(use_shim, Ordering::SeqCst);
    let started = Instant::now();
    let run=tokio::time::timeout(Duration::from_secs(seconds),async {
        if setup.is_err(){return Err("native SCTP channel setup timed out".to_owned())}
        for i in 0..4 {load.spawn(bulk(ends[i].clone(),stop.clone(),active[i].clone()));}
        tokio::time::sleep(Duration::from_millis(600)).await;
        for i in 0..4 {
            let e=ends[i].clone();let a=active[i].clone();
            probes.spawn(async move {
                for seq in 1..=32 {
                    probe(e.clone(),Lane::Interactive,json!({"kind":"terminal","nonce":seq}),&a).await;
                    if i%2==1 {
                        probe(e.clone(),Lane::Commands,json!({"kind":"write","wire":ctox_store::change_log::subscription::protocol::ClientMessage::Write(commands::request(i/2,seq))}),&a).await;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            });
        }
        while let Some(result)=probes.join_next().await{result.map_err(|e|format!("native workload task failed: {e}"))?;}
        stop.store(true,Ordering::SeqCst);
        while let Some(result)=load.join_next().await{result.map_err(|e|format!("native workload task failed: {e}"))?;}
        // Wait for actual receiver hashes and signed live delivery, not send admission.
        loop {
            let complete=(0..4).all(|i|ends[i].stats.lock().unwrap().bulk.bytes==ends[i^1].stats.lock().unwrap().source.bytes)
                && [1,3].iter().all(|&i|ends[i].stats.lock().unwrap().live_packets==64);
            if complete {break}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<(),String>(())
    }).await;
    match &run {
        Err(_) => errors
            .lock()
            .unwrap()
            .push("native profile deadline expired".into()),
        Ok(Err(e)) => errors.lock().unwrap().push(e.clone()),
        _ => {}
    }
    stop.store(true, Ordering::SeqCst);
    collecting.store(false, Ordering::SeqCst);
    load.abort_all();
    probes.abort_all();
    while load.join_next().await.is_some() {}
    while probes.join_next().await.is_some() {}
    let duration = started.elapsed().as_secs_f64();
    // Stop and join all owned transports, forwarding, ingress and UDP send queues.
    for e in &ends {
        if let Some(sender) = e.sender.get() {
            sender.close_now();
        }
    }
    for pc in &pcs {
        let _ = pc.close().await;
    }
    jobs.abort_all();
    forwarding.abort_all();
    while jobs.join_next().await.is_some() {}
    while forwarding.join_next().await.is_some() {}
    let receiver_tasks = std::mem::take(&mut *tasks.lock().unwrap());
    for h in &receiver_tasks {
        h.abort();
    }
    for h in receiver_tasks {
        let _ = h.await;
    }
    let mut cmd: Vec<f64> = Vec::new();
    let mut term: Vec<f64> = Vec::new();
    let mut live: Vec<f64> = Vec::new();
    let mut directions = Vec::new();
    let mut sent = 0;
    let mut received = 0;
    let mut duplicates = 0;
    let mut live_packets = 0;
    let mut active_probes = 0;
    let mut live_replicas = Vec::new();
    for i in 0..4 {
        let s = ends[i].stats.lock().unwrap();
        cmd.extend(&s.command_ms);
        term.extend(&s.terminal_ms);
        live.extend(&s.live_ms);
        sent += s.sent;
        received += s.received;
        duplicates += s.duplicates;
        live_packets += s.live_packets;
        active_probes += s.bulk_active_probes;
        if i % 2 == 1 {
            live_replicas.push(json!({"participant":format!("client-{}",i/2),"documents":s.live_document_ids.len(),"packets":s.live_packets,"cursor":s.cursor.to_string()}));
        }
        let peer = ends[i ^ 1].stats.lock().unwrap();
        directions.push(json!({"from":if i%2==0{"core"}else{if i==1{"client-0"}else{"client-1"}},"to":if i%2==0{if i==0{"client-0"}else{"client-1"}}else{"core"},
            "source":s.source.value(),"received":peer.bulk.value(),"hash_match":s.source.value()==peer.bulk.value()}));
    }
    let d = driver.lock().await;
    let result = json!({"rtt_ms":rtt,"profile":"component profile, pre-cutover","transport":"native SCTP/DTLS/UDP; production V4 scheduler/framer",
        "peers":["core","client-0","client-1"],"associations":2,"opened_reliable_channels":opens.iter().map(|n|n.load(Ordering::SeqCst)).collect::<Vec<_>>(),"duration_s":duration,"completed":matches!(run,Ok(Ok(()))),
        "impairment":{"mode":if use_shim {"userspace UDP shim"} else {"unprivileged-netns-tc"},"location":"below DTLS/SCTP","requested_loss_pct":1,"one_way_delay_ms":rtt/2,
            "udp_offered":counters.offered.load(Ordering::Relaxed),"udp_dropped":counters.dropped.load(Ordering::Relaxed),"udp_sent":counters.sent.load(Ordering::Relaxed),
            "udp_send_errors":counters.failed.load(Ordering::Relaxed),"peak_queue_datagrams":counters.peak_queue.load(Ordering::Relaxed),"rate_bytes_per_s_per_direction":8388608,
            "active_after_handshake":true,"seed":format!("{:x}",0x9e3779b97f4a7c15u64+rtt)},
        "command_samples_ms":cmd,"terminal_samples_ms":term,"live_push_samples_ms":live,
        "logical_sent":sent,"logical_received":received,"application_duplicate_replays":duplicates,"session_reconnect_replay":"not exercised","sctp_retransmissions":null,
        "signed_live_packets":live_packets,"live_replicas":live_replicas,"accepted_commands":d.accepted,"terminal_commands":d.terminal,"projection_reads":d.projection_reads,
        "bulk_active_probes":active_probes,"directions":directions,"errors":*errors.lock().unwrap(),
        "clock":{"domain":"one native process monotonic Instant","offset_ms":0,"uncertainty_ms":0,"cross_host_sync_measured":false}});
    d.shutdown().await;
    result
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let source_hash = Sha256::digest(
        concat!(
            include_str!("main.rs"),
            include_str!("commands.rs"),
            include_str!("network.rs"),
            include_str!("scheduler_bridge.rs")
        )
        .as_bytes(),
    );
    println!("M8_NATIVE_SOURCE_SHA256 {source_hash:x}");
    if std::env::args().any(|a| a == "--source-marker") {
        return;
    }
    START.set(Instant::now()).unwrap();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args: Vec<_> = std::env::args().collect();
    let out = std::path::PathBuf::from(args.get(1).expect("output directory"));
    let seconds = args.get(2).map(|x| x.parse().unwrap()).unwrap_or(360);
    std::fs::create_dir_all(&out).unwrap();
    for rtt in [args.get(3).expect("RTT profile").parse::<u64>().unwrap()] {
        let result = profile(
            rtt,
            &out,
            seconds,
            args.get(4).map(String::as_str) != Some("netns-tc"),
        )
        .await;
        std::fs::write(
            out.join(format!("profile-{rtt}.json")),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
        println!(
            "M8_NATIVE_PROFILE rtt={rtt} completed={}",
            result["completed"]
        );
    }
}
