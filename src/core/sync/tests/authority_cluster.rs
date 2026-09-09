#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn network_timing_confirms_delayed_quorum_but_denies_isolated_leader() {
    let timing = ctox_sync::authority::timing::AuthorityTiming::default();
    // An 80 ms RPC cannot satisfy the old implicit 50 ms read deadline.
    // This exercises actual signed RPCs and durable independent Raft stores.
    let c = Cluster::with_network(
        timing.raft_config().unwrap(),
        Duration::from_millis(80),
        true,
    )
    .await;
    let leader = c.nodes[&1].leader().unwrap();
    let Receipt::Applied(job) = c
        .send(
            leader,
            "create",
            Command::Create {
                spec: spec(),
                owner: leader,
            },
        )
        .await
    else {
        panic!("leader execution was not created");
    };
    for _ in 0..3 {
        c.nodes[&leader]
            .validate_ownership("job", &job.ownership)
            .await
            .unwrap_or_else(|error| {
                panic!("{error}; RPC timings: {:?}", c.bus.timings.read().unwrap())
            });
    }
    c.bus.isolated.write().unwrap().insert(leader);
    assert!(c.nodes[&leader]
        .validate_ownership("job", &job.ownership)
        .await
        .is_err());
    let survivor = (1..=3).find(|id| *id != leader).unwrap();
    let replacement = c.nodes[&survivor]
        .wait_for_other_leader(leader, Duration::from_secs(10))
        .await
        .unwrap();
    // A new leader is not the owner of the old leader's execution. Prove that
    // the surviving majority can authorize its own new execution instead.
    let mut next_spec = spec();
    next_spec.job_id = "survivor-job".into();
    let Receipt::Applied(next_job) = c
        .send(
            replacement,
            "survivor-create",
            Command::Create {
                spec: next_spec,
                owner: replacement,
            },
        )
        .await
    else {
        panic!("surviving majority did not create an execution");
    };
    c.nodes[&replacement]
        .validate_ownership("survivor-job", &next_job.ownership)
        .await
        .unwrap_or_else(|error| {
            panic!("{error}; RPC timings: {:?}", c.bus.timings.read().unwrap())
        });
    assert!(c.nodes[&leader]
        .validate_ownership("job", &job.ownership)
        .await
        .is_err());
    c.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn nonvoting_worker_uses_local_ipc_and_survives_authority_leader_loss() {
    use ctox_sync::{
        authority::{
            client::{ExecutionAuthority, WorkerAuthorityClient},
            WorkerMembership,
        },
        contracts::{SyncIpcOperation, SyncIpcRequest, SyncIpcResult},
        ipc::{AuthorityIpc, IPC_PROTOCOL_VERSION},
    };
    let c = Cluster::new().await;
    let key =
        Arc::new(SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap());
    let member = WorkerMembership {
        node_id: 4,
        identity: key.public_identity(),
        data_replica: true,
        revoked: false,
    };
    let channel = Arc::new(LostCommittedReply {
        bus: c.bus.clone(),
        lose_next: std::sync::atomic::AtomicBool::new(false),
    });
    let client = Arc::new(
        WorkerAuthorityClient::new(
            member.clone(),
            "test-scope".into(),
            c.peers.clone(),
            key,
            channel.clone(),
        )
        .unwrap(),
    );
    let ipc = AuthorityIpc::new(client.clone());
    let request = |id: &str, operation| SyncIpcRequest {
        version: IPC_PROTOCOL_VERSION,
        request_id: id.into(),
        operation,
    };
    assert!(matches!(
        ipc.dispatch(request(
            "unadmitted",
            SyncIpcOperation::Create { spec: spec() }
        ))
        .await
        .result,
        SyncIpcResult::Unavailable { .. }
    ));
    c.send(
        1,
        "admit-ipc-worker",
        Command::AdmitWorker {
            worker: member.clone(),
        },
    )
    .await;
    assert_eq!(
        ipc.dispatch(request(
            "read-admitted",
            SyncIpcOperation::WorkerMembership { node_id: 4 }
        ))
        .await
        .result,
        SyncIpcResult::WorkerMembership {
            node_id: 4,
            worker: Some(member.clone())
        }
    );
    assert!(matches!(
        ipc.dispatch(request(
            "read-another",
            SyncIpcOperation::WorkerMembership { node_id: 5 }
        ))
        .await
        .result,
        SyncIpcResult::Rejected { .. }
    ));
    let created = ipc
        .dispatch(request(
            "worker-create",
            SyncIpcOperation::Create { spec: spec() },
        ))
        .await
        .result;
    assert!(
        matches!(created, SyncIpcResult::Applied { ref ownership, .. } if ownership.node_id == 4 && ownership.generation == 1),
        "{created:?}"
    );
    let own = ownership(1, 4);
    let effect = SyncIpcOperation::BeginEffect {
        job_id: "job".into(),
        ownership: own.clone(),
        effect_id: "worker-publish".into(),
    };
    channel
        .lose_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let uncertain = ipc
        .dispatch(request("worker-effect", effect.clone()))
        .await
        .result;
    assert!(
        matches!(
            uncertain,
            SyncIpcResult::Unavailable { .. } | SyncIpcResult::Replayed { .. }
        ),
        "lost reply must not become a fresh dispatch permission: {uncertain:?}"
    );
    assert!(matches!(
        ipc.dispatch(request("worker-effect", effect)).await.result,
        SyncIpcResult::Replayed { .. }
    ));
    let escalation = ipc
        .dispatch(request(
            "worker-escalation",
            SyncIpcOperation::RevokeWorker { node_id: 4 },
        ))
        .await
        .result;
    assert!(
        matches!(escalation, SyncIpcResult::Rejected { ref reason } if reason.contains("change membership")),
        "{escalation:?}"
    );

    let old_leader = c.nodes[&1].leader().unwrap();
    c.bus.isolated.write().unwrap().insert(old_leader);
    let survivor = (1..=3).find(|id| *id != old_leader).unwrap();
    let new_leader = c.nodes[&survivor]
        .wait_for_other_leader(old_leader, Duration::from_secs(10))
        .await
        .unwrap();
    let valid = ipc
        .dispatch(request(
            "after-election",
            SyncIpcOperation::Validate {
                job_id: "job".into(),
                ownership: own.clone(),
            },
        ))
        .await
        .result;
    assert!(
        matches!(valid, SyncIpcResult::Authorized { .. }),
        "{valid:?}"
    );
    c.send(
        new_leader,
        "revoke-ipc-worker",
        Command::RevokeWorker { node_id: 4 },
    )
    .await;
    let revoked = WorkerMembership {
        revoked: true,
        ..member
    };
    assert_eq!(
        ipc.dispatch(request(
            "read-revoked",
            SyncIpcOperation::WorkerMembership { node_id: 4 }
        ))
        .await
        .result,
        SyncIpcResult::WorkerMembership {
            node_id: 4,
            worker: Some(revoked)
        }
    );
    // A former leader's retained member projection cannot answer a current read.
    assert!(c.nodes[&old_leader].worker_membership(4).await.is_err());
    assert!(client.validate_ownership("job", &own).await.is_err());
    client.shutdown().await.unwrap();
    assert!(client.worker_membership(4).await.is_err());
    assert!(client
        .submit(Request {
            request_id: "after-stop".into(),
            actor: 4,
            command: Command::Stop {
                job_id: "job".into(),
                ownership: own
            }
        })
        .await
        .is_err());
    c.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn minority_cannot_confirm_worker_admission() {
    use ctox_sync::authority::{store::SqliteStore, WorkerMembership};
    let c = Cluster::new().await;
    let leader = c.nodes[&1].leader().unwrap();
    c.bus
        .isolated
        .write()
        .unwrap()
        .extend((1..=3).filter(|id| *id != leader));
    let key = SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap();
    let result = c.nodes[&leader]
        .submit(Request {
            request_id: "isolated-admission".into(),
            actor: leader,
            command: Command::AdmitWorker {
                worker: WorkerMembership {
                    node_id: 4,
                    identity: key.public_identity(),
                    data_replica: true,
                    revoked: false,
                },
            },
        })
        .await;
    assert!(
        result.is_err(),
        "a minority cannot confirm admission: {result:?}"
    );
    for id in 1..=3 {
        let store = SqliteStore::open(&c.root.path().join(format!("{id}.sqlite"))).unwrap();
        assert!(store.worker(4).await.unwrap().is_none());
    }
    // The unconfirmed command may commit after healing; no success was reported here.
    c.close().await;
}

// Consensus tests use signed control messages and durable independent peer stores.
#[path = "support/checkpoint.rs"]
mod checkpoint_fixture;
use async_trait::async_trait;
use ctox_sync::authority::{
    auth::{receive, ControlChannel, SignedTransport, SigningIdentity},
    network::Packet,
    node::AuthorityNode,
    CheckpointCopyReceipt, Command, ExecutionSpec, NodeId, Ownership, Peer, Receipt, Rejection,
    Request,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::{Arc, RwLock, Weak},
    time::Duration,
};

type Endpoint = (Weak<AuthorityNode>, Arc<SigningIdentity>, NodeId);
struct LostCommittedReply {
    bus: Arc<Bus>,
    lose_next: std::sync::atomic::AtomicBool,
}
#[async_trait]
impl ControlChannel for LostCommittedReply {
    async fn request(
        &self,
        target: &str,
        envelope: serde_json::Value,
    ) -> io::Result<serde_json::Value> {
        let reply = self.bus.request(target, envelope).await?;
        let decoded: ctox_sync::authority::network::Reply =
            serde_json::from_value(reply["body"]["data"].clone()).unwrap();
        if matches!(
            decoded,
            ctox_sync::authority::network::Reply::Propose(Ok(Receipt::Applied(_)))
        ) && self
            .lose_next
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "committed test reply was lost",
            ));
        }
        Ok(reply)
    }
}
/// Sender, recipient, RPC method, elapsed milliseconds, last completed stage.
type RpcTiming = (NodeId, NodeId, &'static str, u128, &'static str);
#[derive(Default)]
struct Bus {
    nodes: RwLock<BTreeMap<String, Endpoint>>,
    isolated: RwLock<BTreeSet<NodeId>>,
    delay: Duration,
    timings: RwLock<Vec<RpcTiming>>,
}
struct RpcObservation<'a> {
    bus: &'a Bus,
    from: NodeId,
    to: NodeId,
    method: &'static str,
    started: std::time::Instant,
    stage: &'static str,
}
impl Drop for RpcObservation<'_> {
    fn drop(&mut self) {
        let mut timings = self.bus.timings.write().unwrap();
        if timings.len() == 32 {
            timings.remove(0);
        }
        timings.push((
            self.from,
            self.to,
            self.method,
            self.started.elapsed().as_millis(),
            self.stage,
        ));
    }
}
#[async_trait]
impl ControlChannel for Bus {
    async fn request(
        &self,
        target_identity: &str,
        envelope: serde_json::Value,
    ) -> io::Result<serde_json::Value> {
        let (target, key, target_id) = self
            .nodes
            .read()
            .unwrap()
            .get(target_identity)
            .and_then(|(node, key, id)| node.upgrade().map(|node| (node, key.clone(), *id)))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "peer stopped"))?;
        let from = envelope["body"]["data"]["from"].as_u64().unwrap();
        if self.isolated.read().unwrap().contains(&from)
            || self.isolated.read().unwrap().contains(&target_id)
        {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "test network partition",
            ));
        }
        // Actual signed production envelopes, encoded across the simulated link.
        let mut observation = RpcObservation {
            bus: self,
            from,
            to: target_id,
            method: match envelope["body"]["data"]["rpc"]["method"].as_str() {
                Some("append") => "append",
                Some("vote") => "vote",
                Some("snapshot") => "snapshot",
                Some("propose") => "propose",
                Some("validate") => "validate",
                Some("workerMembership") => "workerMembership",
                _ => "unknown",
            },
            started: std::time::Instant::now(),
            stage: "delay",
        };
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        observation.stage = "receive";
        let bytes = serde_json::to_vec(&envelope).unwrap();
        let reply = receive(
            &key,
            "test-scope",
            &target,
            serde_json::from_slice(&bytes).unwrap(),
        )
        .await?;
        observation.stage = "replied";
        Ok(serde_json::from_slice(&serde_json::to_vec(&reply).unwrap()).unwrap())
    }
}
struct Cluster {
    root: tempfile::TempDir,
    config: openraft::Config,
    bus: Arc<Bus>,
    nodes: BTreeMap<NodeId, Arc<AuthorityNode>>,
    peers: BTreeMap<NodeId, Peer>,
    keys: BTreeMap<NodeId, Arc<SigningIdentity>>,
}
// Failure evidence only; no changed deadlines or production instrumentation.
impl Drop for Cluster {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("authority fixture timing: heartbeat={}ms election={}..{}ms; leaders={:?}; last RPCs (from,to,method,ms,stage)={:?}",
                self.config.heartbeat_interval, self.config.election_timeout_min, self.config.election_timeout_max,
                self.nodes.iter().map(|(id, node)| (*id, node.leader())).collect::<Vec<_>>(),
                self.bus.timings.read().ok().as_deref());
        }
    }
}
impl Cluster {
    async fn new() -> Self {
        // Exercise the same timing contract as native execution hosts. OpenRaft's
        // implicit 50 ms RPC budget caused measured 50–53 ms append cancellations
        // and 153 ms vote cancellations under parallel durable-store load.
        Self::with_network(
            ctox_sync::authority::timing::AuthorityTiming::default()
                .raft_config()
                .unwrap(),
            Duration::ZERO,
            false,
        )
        .await
    }
    async fn with_network(config: openraft::Config, delay: Duration, all_executors: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let bus = Arc::new(Bus {
            delay,
            ..Default::default()
        });
        let keys: BTreeMap<_, _> = (1..=3)
            .map(|id| {
                (
                    id,
                    Arc::new(
                        SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap())
                            .unwrap(),
                    ),
                )
            })
            .collect();
        let peers = keys
            .iter()
            .map(|(&id, key)| {
                (
                    id,
                    Peer {
                        identity: key.public_identity(),
                        executor: all_executors || id != 3,
                        data_replica: id != 3,
                    },
                )
            })
            .collect();
        let mut c = Self {
            root,
            config,
            bus,
            nodes: BTreeMap::new(),
            peers,
            keys,
        };
        for id in 1..=3 {
            c.start(id).await;
        }
        c.nodes[&1].bootstrap().await.unwrap();
        for n in c.nodes.values() {
            n.wait_for_leader(Duration::from_secs(10)).await.unwrap();
        }
        c
    }
    async fn start(&mut self, id: NodeId) {
        let config = self.config.clone();
        let transport = Arc::new(SignedTransport::new(
            self.keys[&id].clone(),
            "test-scope".into(),
            self.bus.clone(),
        ));
        let node = Arc::new(
            AuthorityNode::open(
                id,
                "test-scope".into(),
                self.peers.clone(),
                &self.root.path().join(format!("{id}.sqlite")),
                transport,
                config,
            )
            .await
            .unwrap(),
        );
        self.bus.nodes.write().unwrap().insert(
            self.peers[&id].identity.clone(),
            (Arc::downgrade(&node), self.keys[&id].clone(), id),
        );
        self.nodes.insert(id, node);
    }
    async fn stop(&mut self, id: NodeId) {
        let n = self.nodes.remove(&id).unwrap();
        self.bus
            .nodes
            .write()
            .unwrap()
            .remove(&self.peers[&id].identity);
        n.shutdown().await.unwrap();
    }
    async fn close(mut self) {
        for id in 1..=3 {
            if self.nodes.contains_key(&id) {
                self.stop(id).await;
            }
        }
    }
    async fn send(&self, actor: NodeId, id: &str, command: Command) -> Receipt {
        self.nodes[&actor]
            .submit(Request {
                request_id: id.into(),
                actor,
                command,
            })
            .await
            .unwrap_or_else(|error| panic!("request {id} from {actor}: {error}"))
    }
    async fn create(&self) -> ctox_sync::authority::Job {
        match self
            .send(
                1,
                "create",
                Command::Create {
                    spec: spec(),
                    owner: 1,
                },
            )
            .await
        {
            Receipt::Applied(job) => job,
            other => panic!("{other:?}"),
        }
    }
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_ipc_uses_committed_authority_and_never_reauthorizes_a_replay() {
    use ctox_sync::{
        contracts::{SyncIpcOperation, SyncIpcRequest, SyncIpcResponse, SyncIpcResult},
        local_host::LocalAuthorityHost,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixStream,
    };
    let cluster = Cluster::new().await;
    let directory = cluster.root.path().join("ipc");
    let server = LocalAuthorityHost::start(directory.clone(), cluster.nodes[&1].clone())
        .await
        .unwrap();
    let mut client = UnixStream::connect(server.endpoint()).await.unwrap();
    let duplicate = LocalAuthorityHost::start(directory, cluster.nodes[&1].clone()).await;
    assert_eq!(duplicate.err().unwrap().kind(), io::ErrorKind::AddrInUse);
    async fn exchange(
        client: &mut UnixStream,
        id: &str,
        operation: SyncIpcOperation,
    ) -> SyncIpcResult {
        let input = SyncIpcRequest {
            version: 1,
            request_id: id.into(),
            operation,
        };
        let bytes = serde_json::to_vec(&input).unwrap();
        client.write_u32(bytes.len() as u32).await.unwrap();
        // Split writes exercise stream framing rather than assuming packet boundaries.
        let split = bytes.len() / 2;
        client.write_all(&bytes[..split]).await.unwrap();
        client.write_all(&bytes[split..]).await.unwrap();
        let size = client.read_u32().await.unwrap();
        assert!(size <= 65536);
        let mut bytes = vec![0; size as usize];
        client.read_exact(&mut bytes).await.unwrap();
        let response: SyncIpcResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response.request_id, id);
        assert_eq!(response.version, 1);
        response.result
    }
    assert!(matches!(
        exchange(&mut client, "hello", SyncIpcOperation::Hello {}).await,
        SyncIpcResult::Ready { node_id: 1, .. }
    ));
    assert!(matches!(
        exchange(
            &mut client,
            "create",
            SyncIpcOperation::Create { spec: spec() }
        )
        .await,
        SyncIpcResult::Applied { .. }
    ));
    let effect = SyncIpcOperation::BeginEffect {
        job_id: "job".into(),
        ownership: ownership(1, 1),
        effect_id: "publish".into(),
    };
    let started = exchange(&mut client, "effect", effect.clone()).await;
    assert!(
        matches!(started, SyncIpcResult::Applied { .. }),
        "effect was not confirmed: {started:?}"
    );
    assert!(matches!(
        exchange(&mut client, "effect", effect).await,
        SyncIpcResult::Replayed { .. }
    ));
    cluster.bus.isolated.write().unwrap().insert(1);
    assert!(matches!(
        exchange(
            &mut client,
            "validate",
            SyncIpcOperation::Validate {
                job_id: "job".into(),
                ownership: ownership(1, 1)
            }
        )
        .await,
        SyncIpcResult::Unavailable { .. }
    ));
    let socket = server.endpoint().to_owned();
    server.shutdown().await.unwrap();
    assert_eq!(client.read(&mut [0; 1]).await.unwrap(), 0);
    assert!(!socket.exists());
    cluster.close().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workjet_client_uses_native_quorum_and_observes_host_loss() {
    use ctox_sync::local_host::LocalAuthorityHost;
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    tokio::time::timeout(Duration::from_secs(30), async {
        let cluster = Cluster::new().await;
        let host = LocalAuthorityHost::start(
            cluster.root.path().join("workjet-ipc"),
            cluster.nodes[&1].clone(),
        )
        .await
        .unwrap();
        let target_host = LocalAuthorityHost::start(
            cluster.root.path().join("handoff-ipc"),
            cluster.nodes[&2].clone(),
        )
        .await
        .unwrap();
        let mut handoff_spec = spec();
        handoff_spec.job_id = "workjet-handoff-job".into();
        handoff_spec.session_id = "workjet-handoff-session".into();
        let receipts: Vec<_> = [1, 2]
            .into_iter()
            .map(|id| {
                checkpoint_fixture::copy_receipt(
                    cluster.root.path(),
                    id,
                    &cluster.keys[&id],
                    &handoff_spec,
                    &ownership(1, 1),
                    1,
                )
            })
            .collect();
        let handoff = serde_json::json!({
            "target": target_host.endpoint(), "spec": handoff_spec, "receipts": receipts,
        });
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let workjet = source
            .join("../../../../workjet")
            .canonicalize()
            .expect("the Workjet checkout is required for the actual IPC consumer test");
        let mut child = tokio::process::Command::new("node")
            .arg(source.join("tests/support/workjet_ipc_client.mjs"))
            .arg(workjet.join("apps/server/src/workjet/sync/WorkjetSyncIpc.ts"))
            .arg(host.endpoint())
            .arg(serde_json::to_string(&spec()).unwrap())
            .arg(handoff.to_string())
            .current_dir(&workjet)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
        assert_eq!(
            output.next_line().await.unwrap().as_deref(),
            Some("authorized")
        );
        cluster.bus.isolated.write().unwrap().insert(1);
        input.write_all(b"partition\n").await.unwrap();
        assert_eq!(
            output.next_line().await.unwrap().as_deref(),
            Some("revoked")
        );
        host.shutdown().await.unwrap();
        input.write_all(b"host-stopped\n").await.unwrap();
        assert_eq!(
            output.next_line().await.unwrap().as_deref(),
            Some("disconnected")
        );
        drop(input);
        assert!(child.wait().await.unwrap().success());
        target_host.shutdown().await.unwrap();
        cluster.close().await;
    })
    .await
    .expect("native/Workjet IPC flow timed out");
}

fn spec() -> ExecutionSpec {
    ExecutionSpec {
        job_id: "job".into(),
        session_id: "session".into(),
        scope_id: "test-scope".into(),
        harness: "codex".into(),
        harness_version: "pinned-test".into(),
        model_route_id: "route".into(),
        gateway_account_id: "account".into(),
        model_id: "model".into(),
        required_capabilities: BTreeSet::new(),
    }
}
fn ownership(generation: u64, node_id: u64) -> Ownership {
    Ownership {
        generation,
        node_id,
    }
}
fn checkpoint(c: &Cluster, sequence: u64) -> Vec<CheckpointCopyReceipt> {
    [1, 2]
        .into_iter()
        .map(|id| {
            checkpoint_fixture::copy_receipt(
                c.root.path(),
                id,
                &c.keys[&id],
                &spec(),
                &ownership(1, 1),
                sequence,
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn minority_cannot_authorize_and_majority_fences_old_owner() {
    let c = Cluster::new().await;
    c.create().await;
    assert!(matches!(
        c.send(
            1,
            "checkpoint",
            Command::ProtectCheckpoint {
                job_id: "job".into(),
                ownership: ownership(1, 1),
                receipts: checkpoint(&c, 1)
            }
        )
        .await,
        Receipt::Applied(_)
    ));
    c.nodes[&1]
        .validate_ownership("job", &ownership(1, 1))
        .await
        .unwrap();
    let previous = c.nodes[&2].leader().unwrap();
    c.bus.isolated.write().unwrap().insert(1);
    if previous == 1 {
        c.nodes[&2]
            .wait_for_other_leader(1, Duration::from_secs(10))
            .await
            .unwrap();
    }
    assert!(c.nodes[&1]
        .validate_ownership("job", &ownership(1, 1))
        .await
        .is_err());
    let taken = c
        .send(
            2,
            "takeover",
            Command::TakeOver {
                job_id: "job".into(),
                expected: ownership(1, 1),
                checkpoint_digest: checkpoint(&c, 1)[0].checkpoint_digest.clone(),
                owner: 2,
            },
        )
        .await;
    assert!(matches!(taken,Receipt::Applied(ref j) if j.ownership==ownership(2,2)));
    c.nodes[&2]
        .validate_ownership("job", &ownership(2, 2))
        .await
        .unwrap();
    c.bus.isolated.write().unwrap().clear();
    // A quorum read on the rejoining node either rejects immediately or first redirects; it never authorizes generation 1.
    assert!(c.nodes[&1]
        .validate_ownership("job", &ownership(1, 1))
        .await
        .is_err());
    c.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unconfirmed_external_effect_blocks_takeover() {
    let c = Cluster::new().await;
    c.create().await;
    c.send(
        1,
        "checkpoint",
        Command::ProtectCheckpoint {
            job_id: "job".into(),
            ownership: ownership(1, 1),
            receipts: checkpoint(&c, 1),
        },
    )
    .await;
    c.send(
        1,
        "effect",
        Command::BeginEffect {
            job_id: "job".into(),
            ownership: ownership(1, 1),
            effect_id: "publish".into(),
        },
    )
    .await;
    assert_eq!(
        c.send(
            2,
            "takeover",
            Command::TakeOver {
                job_id: "job".into(),
                expected: ownership(1, 1),
                checkpoint_digest: checkpoint(&c, 1)[0].checkpoint_digest.clone(),
                owner: 2
            }
        )
        .await,
        Receipt::Rejected(Rejection::ReconciliationRequired)
    );
    c.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn state_and_receipts_survive_all_peer_restarts() {
    let mut c = Cluster::new().await;
    let created = c.create().await;
    let copies = checkpoint(&c, 1);
    assert!(matches!(
        c.send(
            1,
            "protect-before-restart",
            Command::ProtectCheckpoint {
                job_id: "job".into(),
                ownership: ownership(1, 1),
                receipts: copies.clone(),
            }
        )
        .await,
        Receipt::Applied(_)
    ));
    for id in 1..=3 {
        c.stop(id).await;
    }
    for id in 1..=3 {
        c.start(id).await;
    }
    for n in c.nodes.values() {
        n.wait_for_leader(Duration::from_secs(10)).await.unwrap();
    }
    assert_eq!(
        c.send(
            1,
            "create",
            Command::Create {
                spec: spec(),
                owner: 1
            }
        )
        .await,
        Receipt::Replayed(created)
    );
    let restored = c.nodes[&1]
        .validate_ownership("job", &ownership(1, 1))
        .await
        .unwrap();
    assert_eq!(restored.checkpoint.unwrap().receipts, copies);
    let mut altered = spec();
    altered.model_id = "different".into();
    assert_eq!(
        c.send(
            1,
            "create",
            Command::Create {
                spec: altered,
                owner: 1
            }
        )
        .await,
        Receipt::Rejected(Rejection::RequestIdConflict)
    );
    c.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn checkpoint_protection_requires_distinct_authentic_matching_durable_copies() {
    let c = Cluster::new().await;
    c.create().await;
    let valid = checkpoint(&c, 1);
    let mut forged = valid.clone();
    forged[1].signature = "0".repeat(128);
    let mut wrong_sequence = valid.clone();
    wrong_sequence[1] = checkpoint_fixture::copy_receipt(
        c.root.path(),
        2,
        &c.keys[&2],
        &spec(),
        &ownership(1, 1),
        2,
    );
    let mut other_job = valid.clone();
    let mut other_spec = spec();
    other_spec.job_id = "another-job".into();
    other_job[1] = checkpoint_fixture::copy_receipt(
        c.root.path(),
        2,
        &c.keys[&2],
        &other_spec,
        &ownership(1, 1),
        1,
    );
    let mut wrong_generation = valid.clone();
    wrong_generation[1] = checkpoint_fixture::copy_receipt(
        c.root.path(),
        2,
        &c.keys[&2],
        &spec(),
        &ownership(2, 1),
        1,
    );
    let mut wrong_signer = valid.clone();
    wrong_signer[1] = checkpoint_fixture::copy_receipt(
        c.root.path(),
        2,
        &c.keys[&1],
        &spec(),
        &ownership(1, 1),
        1,
    );
    for (index, receipts) in [
        vec![],
        vec![valid[0].clone()],
        vec![valid[0].clone(), valid[0].clone()],
        forged,
        wrong_sequence,
        other_job,
        wrong_generation,
        wrong_signer,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            c.send(
                1,
                &format!("invalid-copy-{index}"),
                Command::ProtectCheckpoint {
                    job_id: "job".into(),
                    ownership: ownership(1, 1),
                    receipts,
                }
            )
            .await,
            Receipt::Rejected(Rejection::CheckpointUnavailable)
        );
    }
    // Rejections never publish a partly validated checkpoint.
    assert!(c.nodes[&1]
        .validate_ownership("job", &ownership(1, 1))
        .await
        .unwrap()
        .checkpoint
        .is_none());
    assert!(
        matches!(c.send(1, "valid-copy", Command::ProtectCheckpoint {
        job_id: "job".into(), ownership: ownership(1, 1), receipts: valid,
    }).await, Receipt::Applied(job) if job.checkpoint.as_ref().unwrap().replicas == BTreeSet::from([1, 2]))
    );
    c.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn witness_cannot_claim_execution_or_count_as_data_copy() {
    let c = Cluster::new().await;
    c.create().await;
    let mut invalid = checkpoint(&c, 1);
    invalid[1] = checkpoint_fixture::copy_receipt(
        c.root.path(),
        3,
        &c.keys[&3],
        &spec(),
        &ownership(1, 1),
        1,
    );
    assert_eq!(
        c.send(
            1,
            "checkpoint",
            Command::ProtectCheckpoint {
                job_id: "job".into(),
                ownership: ownership(1, 1),
                receipts: invalid
            }
        )
        .await,
        Receipt::Rejected(Rejection::CheckpointUnavailable)
    );
    assert_eq!(
        c.send(
            3,
            "claim",
            Command::Create {
                spec: spec(),
                owner: 3
            }
        )
        .await,
        Receipt::Rejected(Rejection::UnknownPeer)
    );
    let packet = Packet {
        version: ctox_sync::authority::network::CONTROL_PROTOCOL,
        scope_id: "test-scope".into(),
        from: 1,
        rpc: ctox_sync::authority::network::Rpc::Validate {
            job_id: "job".into(),
            ownership: ownership(1, 1),
        },
    };
    assert!(c.nodes[&2]
        .handle(&c.peers[&3].identity, packet.clone())
        .await
        .is_err());
    let mut old_protocol = packet;
    old_protocol.version = 1;
    assert!(c.nodes[&2]
        .handle(&c.peers[&1].identity, old_protocol)
        .await
        .is_err());
    c.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn additional_worker_requires_committed_membership_but_never_gets_a_vote() {
    use ctox_sync::{
        authority::{
            network::{ControlTransport, Reply, Rpc, CONTROL_PROTOCOL},
            store::SqliteStore,
            WorkerMembership,
        },
        contracts::{SyncIpcOperation, SyncIpcRequest, SyncIpcResult},
        ipc::{AuthorityIpc, IPC_PROTOCOL_VERSION},
    };
    use openraft::{storage::RaftStateMachine, RaftSnapshotBuilder};
    let mut c = Cluster::new().await;
    let key =
        Arc::new(SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap());
    let worker = WorkerMembership {
        node_id: 4,
        identity: key.public_identity(),
        data_replica: true,
        revoked: false,
    };
    let transport = SignedTransport::new(key, "test-scope".into(), c.bus.clone());
    let leader = c.nodes[&1].leader().unwrap();
    let packet = |rpc| Packet {
        version: CONTROL_PROTOCOL,
        scope_id: "test-scope".into(),
        from: 4,
        rpc,
    };
    let create = packet(Rpc::Propose(Request {
        request_id: "worker-create".into(),
        actor: 4,
        command: Command::Create {
            spec: spec(),
            owner: 4,
        },
    }));
    assert!(transport
        .exchange(&c.peers[&leader], create.clone())
        .await
        .is_err());

    // The private native IPC uses its own voter identity; no client-supplied actor.
    let ipc = AuthorityIpc::new(c.nodes[&leader].clone());
    let membership = SyncIpcRequest {
        version: IPC_PROTOCOL_VERSION,
        request_id: "current-membership".into(),
        operation: SyncIpcOperation::WorkerMembership { node_id: 4 },
    };
    assert_eq!(
        ipc.dispatch(membership.clone()).await.result,
        SyncIpcResult::WorkerMembership {
            node_id: 4,
            worker: None
        }
    );
    let admission = SyncIpcRequest {
        version: IPC_PROTOCOL_VERSION,
        request_id: "admit-4".into(),
        operation: SyncIpcOperation::AdmitWorker {
            worker: worker.clone(),
        },
    };
    assert_eq!(
        ipc.dispatch(admission.clone()).await.result,
        SyncIpcResult::WorkerApplied {
            worker: worker.clone()
        }
    );
    assert_eq!(
        ipc.dispatch(admission.clone()).await.result,
        SyncIpcResult::WorkerReplayed {
            worker: worker.clone()
        }
    );
    assert!(
        matches!(transport.exchange(&c.peers[&leader], create.clone()).await.unwrap(), Reply::Propose(Ok(Receipt::Applied(job))) if job.ownership == ownership(1, 4))
    );
    let validate = packet(Rpc::Validate {
        job_id: "job".into(),
        ownership: ownership(1, 4),
    });
    assert!(matches!(
        transport
            .exchange(&c.peers[&leader], validate.clone())
            .await
            .unwrap(),
        Reply::Validate(Ok(_))
    ));

    let escalate = packet(Rpc::Propose(Request {
        request_id: "worker-enrolls-peer".into(),
        actor: 4,
        command: Command::AdmitWorker {
            worker: WorkerMembership {
                node_id: 5,
                ..worker.clone()
            },
        },
    }));
    assert!(matches!(
        transport
            .exchange(&c.peers[&leader], escalate)
            .await
            .unwrap(),
        Reply::Propose(Ok(Receipt::Rejected(Rejection::UnknownPeer)))
    ));
    let vote = packet(Rpc::Vote(openraft::raft::VoteRequest {
        vote: openraft::Vote::new(100, 4),
        last_log_id: None,
    }));
    assert!(transport.exchange(&c.peers[&leader], vote).await.is_err());
    let append = packet(Rpc::Append(openraft::raft::AppendEntriesRequest {
        vote: openraft::Vote::new_committed(100, 4),
        prev_log_id: None,
        entries: vec![],
        leader_commit: None,
    }));
    assert!(transport.exchange(&c.peers[&leader], append).await.is_err());
    assert_eq!(c.nodes[&leader].leader(), Some(leader));

    // The directory is part of the same durable snapshot as jobs and replay receipts.
    let mut store = SqliteStore::open(&c.root.path().join(format!("{leader}.sqlite"))).unwrap();
    let snapshot = store.build_snapshot().await.unwrap();
    assert_eq!(snapshot.meta.last_membership.nodes().count(), 3);
    let mut restored = SqliteStore::open(&c.root.path().join("restored.sqlite")).unwrap();
    restored
        .install_snapshot(&snapshot.meta, snapshot.snapshot)
        .await
        .unwrap();
    assert_eq!(restored.worker(4).await.unwrap(), Some(worker.clone()));
    assert_eq!(
        restored.job("job").await.unwrap().unwrap().ownership,
        ownership(1, 4)
    );

    let revoked = WorkerMembership {
        revoked: true,
        ..worker.clone()
    };
    assert_eq!(
        ipc.dispatch(SyncIpcRequest {
            version: IPC_PROTOCOL_VERSION,
            request_id: "revoke-4".into(),
            operation: SyncIpcOperation::RevokeWorker { node_id: 4 }
        })
        .await
        .result,
        SyncIpcResult::WorkerApplied {
            worker: revoked.clone()
        }
    );
    // The immutable admission receipt stays historical; the same read request
    // must now return the current tombstone, not cache its earlier None/active state.
    assert_eq!(
        ipc.dispatch(admission).await.result,
        SyncIpcResult::WorkerReplayed {
            worker: worker.clone()
        }
    );
    assert_eq!(
        ipc.dispatch(membership).await.result,
        SyncIpcResult::WorkerMembership {
            node_id: 4,
            worker: Some(revoked.clone())
        }
    );
    assert!(matches!(
        transport.exchange(&c.peers[&leader], packet(Rpc::WorkerMembership { node_id: 4 }))
            .await.unwrap(),
        Reply::WorkerMembership(Ok(Some(ref current))) if current == &revoked
    ));
    assert!(transport
        .exchange(
            &c.peers[&leader],
            packet(Rpc::WorkerMembership { node_id: 5 })
        )
        .await
        .is_err());
    assert!(c.nodes[&leader]
        .handle(
            &c.peers[&1].identity,
            packet(Rpc::WorkerMembership { node_id: 4 })
        )
        .await
        .is_err());
    // A pinned revoked worker gets a quorum-confirmed denial, never a job.
    assert!(matches!(
        transport
            .exchange(&c.peers[&leader], validate.clone())
            .await
            .unwrap(),
        Reply::Validate(Err(
            ctox_sync::authority::network::AuthorityFailure::Rejected { .. }
        ))
    ));
    assert!(c.nodes[&leader]
        .handle(&c.peers[&1].identity, validate.clone())
        .await
        .is_err());
    assert!(transport
        .exchange(
            &c.peers[&leader],
            packet(Rpc::Validate {
                job_id: "job".into(),
                ownership: ownership(1, 5),
            })
        )
        .await
        .is_err());
    assert!(transport.exchange(&c.peers[&leader], create).await.is_err());
    assert_eq!(
        c.send(leader, "reuse-revoked-id", Command::AdmitWorker { worker })
            .await,
        Receipt::Rejected(Rejection::AlreadyExists)
    );
    let snapshot = store.build_snapshot().await.unwrap();
    restored
        .install_snapshot(&snapshot.meta, snapshot.snapshot)
        .await
        .unwrap();
    assert_eq!(restored.worker(4).await.unwrap(), Some(revoked.clone()));

    // Closing and reopening the store must not resurrect the revoked executor.
    for id in 1..=3 {
        c.stop(id).await;
    }
    for id in 1..=3 {
        c.start(id).await;
    }
    let leader = c.nodes[&1]
        .wait_for_leader(Duration::from_secs(10))
        .await
        .unwrap();
    assert!(matches!(
        transport
            .exchange(&c.peers[&leader], validate.clone())
            .await
            .unwrap(),
        Reply::Validate(Err(
            ctox_sync::authority::network::AuthorityFailure::Rejected { .. }
        ))
    ));
    let reopened = SqliteStore::open(&c.root.path().join(format!("{leader}.sqlite"))).unwrap();
    assert_eq!(
        c.nodes[&leader].worker_membership(4).await.unwrap(),
        Some(revoked.clone())
    );
    // The same durable tombstone cannot claim a confirmed denial without quorum.
    c.bus.isolated.write().unwrap().insert(leader);
    assert!(matches!(
        c.nodes[&leader]
            .handle(&revoked.identity, validate)
            .await
            .unwrap(),
        Reply::Validate(Err(
            ctox_sync::authority::network::AuthorityFailure::Unavailable { .. }
                | ctox_sync::authority::network::AuthorityFailure::NotLeader { .. }
        ))
    ));
    assert_eq!(reopened.worker(4).await.unwrap(), Some(revoked));
    c.close().await;
}
