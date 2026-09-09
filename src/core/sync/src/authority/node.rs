//! A node authorizes execution only after a quorum-backed read, never from a local projection.
use super::{
    diagnostics::{OperationTiming, Phase, Timings},
    network::{AuthorityFailure, ControlTransport, Factory, Packet, Reply, Rpc, CONTROL_PROTOCOL},
    store::SqliteStore,
    Job, NodeId, Ownership, Peer, Raft, Receipt, Request, WorkerMembership,
};
use openraft::Config;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub struct AuthorityNode {
    id: NodeId,
    scope_id: String,
    peers: BTreeMap<NodeId, Peer>,
    raft: Raft,
    store: SqliteStore,
    transport: Arc<dyn ControlTransport>,
    deadline: Duration,
    stopped: AtomicBool,
    operations: Timings,
}
/// Read-only local observations, not a quorum or permission decision.
/// Keep the library-specific metrics behind this adapter.
#[derive(Debug, serde::Serialize)]
pub struct AuthorityDiagnostics {
    pub leader: Option<NodeId>,
    pub term: u64,
    pub running: bool,
    pub last_log_index: Option<u64>,
    pub last_applied_index: Option<u64>,
    pub millis_since_quorum_ack: Option<u64>,
    pub operations: BTreeMap<&'static str, OperationTiming>,
    pub storage: BTreeMap<&'static str, OperationTiming>,
}
fn error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
impl AuthorityNode {
    pub async fn open(
        id: NodeId,
        scope_id: String,
        peers: BTreeMap<NodeId, Peer>,
        path: &Path,
        transport: Arc<dyn ControlTransport>,
        config: Config,
    ) -> io::Result<Self> {
        let identities: BTreeSet<_> = peers.values().map(|p| p.identity.as_str()).collect();
        for identity in &identities {
            super::auth::public_key(identity)?;
        }
        if scope_id.is_empty()
            || peers.len() != 3
            || !peers.contains_key(&id)
            || identities.len() != 3
            || identities.contains("")
            || peers
                .values()
                .filter(|p| p.executor && p.data_replica)
                .count()
                < 2
        {
            return Err(error("authority requires a scope and three distinct trusted peers, including two execution/data peers"));
        }
        // Bound encoded AppendEntries and snapshot chunks below the signed
        // envelope limit, including JSON byte-array expansion of snapshot data.
        let config = Config {
            max_payload_entries: config.max_payload_entries.min(32),
            snapshot_max_chunk_size: config.snapshot_max_chunk_size.min(64 * 1024),
            ..config
        };
        let config = config.validate().map_err(|e| error(e.to_string()))?;
        let path = path.to_path_buf();
        let store = tokio::task::spawn_blocking(move || SqliteStore::open(&path))
            .await
            .map_err(|error| io::Error::other(error.to_string()))??;

        store
            .bind_identity(id, scope_id.clone(), peers.clone())
            .await
            .map_err(|e| error(e.to_string()))?;
        let raft = Raft::new(
            id,
            Arc::new(config),
            Factory {
                from: id,
                scope_id: scope_id.clone(),
                transport: transport.clone(),
            },
            store.clone(),
            store.clone(),
        )
        .await
        .map_err(|e| error(e.to_string()))?;
        Ok(Self {
            id,
            scope_id,
            peers,
            raft,
            store,
            transport,
            deadline: Duration::from_secs(5),
            stopped: AtomicBool::new(false),
            operations: Timings::default(),
        })
    }
    pub async fn bootstrap(&self) -> io::Result<()> {
        if self
            .raft
            .is_initialized()
            .await
            .map_err(|e| error(e.to_string()))?
        {
            return Ok(());
        }
        if Some(&self.id) != self.peers.keys().next() {
            return Err(error(
                "only the first configured peer initializes a new authority group",
            ));
        }
        self.raft
            .initialize(self.peers.clone())
            .await
            .map_err(|e| error(e.to_string()))
    }
    pub fn node_id(&self) -> NodeId {
        self.id
    }
    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }
    pub fn matches_local_identity(&self, scope: &str, identity: &str) -> bool {
        self.scope_id == scope
            && self
                .peers
                .get(&self.id)
                .is_some_and(|peer| peer.identity == identity)
    }
    pub fn leader(&self) -> Option<NodeId> {
        self.raft.metrics().borrow().current_leader
    }
    pub fn diagnostics(&self) -> AuthorityDiagnostics {
        let metrics = self.raft.metrics().borrow().clone();
        AuthorityDiagnostics {
            leader: metrics.current_leader,
            term: metrics.current_term,
            running: metrics.running_state.is_ok() && !self.stopped.load(Ordering::Acquire),
            last_log_index: metrics.last_log_index,
            last_applied_index: metrics.last_applied.map(|log| log.index),
            millis_since_quorum_ack: metrics.millis_since_quorum_ack,
            operations: self.operations.snapshot(),
            storage: self.store.diagnostics(),
        }
    }
    pub async fn wait_for_leader(&self, timeout: Duration) -> io::Result<NodeId> {
        self.raft
            .wait(Some(timeout))
            .metrics(|m| m.current_leader.is_some(), "authority leader")
            .await
            .map_err(|e| error(e.to_string()))?
            .current_leader
            .ok_or_else(|| error("no authority leader"))
    }
    pub async fn wait_for_other_leader(
        &self,
        previous: NodeId,
        timeout: Duration,
    ) -> io::Result<NodeId> {
        self.raft
            .wait(Some(timeout))
            .metrics(
                |m| m.current_leader.is_some_and(|id| id != previous),
                "replacement authority leader",
            )
            .await
            .map_err(|e| error(e.to_string()))?
            .current_leader
            .ok_or_else(|| error("no replacement leader"))
    }
    pub async fn shutdown(&self) -> io::Result<()> {
        self.stopped.store(true, Ordering::Release);
        self.raft.shutdown().await.map_err(|e| error(e.to_string()))
    }
    pub async fn local_job(&self, id: &str) -> io::Result<Option<Job>> {
        self.store.job(id).await.map_err(|e| error(e.to_string()))
    }

    async fn local_submit(&self, request: Request) -> Result<Receipt, AuthorityFailure> {
        if serde_json::to_vec(&request)
            .map_err(AuthorityFailure::unavailable)?
            .len()
            > 64 * 1024
        {
            return Err(AuthorityFailure::rejected(
                "execution command exceeds the 64 KiB control budget",
            ));
        }
        if let super::Command::Create { spec, .. } = &request.command {
            if spec.scope_id != self.scope_id {
                return Err(AuthorityFailure::rejected(
                    "execution scope does not match authority",
                ));
            }
        }
        let observation = self.operations.start("client_write", Phase::Running);
        let result = tokio::time::timeout(self.deadline, self.raft.client_write(request)).await;
        observation.finish(result.as_ref().is_ok_and(|result| result.is_ok()));
        let result = result.map_err(|_| {
            AuthorityFailure::unavailable(
                "authority write has no confirmed outcome; retain its request ID",
            )
        })?;
        result.map(|r| r.data).map_err(|error| {
            if let Some(redirect) = error.forward_to_leader::<Peer>() {
                AuthorityFailure::NotLeader {
                    leader: redirect.leader_id,
                }
            } else {
                AuthorityFailure::unavailable(error)
            }
        })
    }
    async fn confirm_quorum(&self) -> Result<(), AuthorityFailure> {
        let observation = self.operations.start("linearizable_read", Phase::Running);
        let result = tokio::time::timeout(self.deadline, self.raft.ensure_linearizable()).await;
        observation.finish(result.as_ref().is_ok_and(|result| result.is_ok()));
        result
            .map_err(|_| AuthorityFailure::unavailable("execution paused: no authority quorum"))?
            .map_err(|error| {
                if let Some(redirect) = error.forward_to_leader::<Peer>() {
                    AuthorityFailure::NotLeader {
                        leader: redirect.leader_id,
                    }
                } else {
                    AuthorityFailure::unavailable(error)
                }
            })?;
        Ok(())
    }
    async fn local_worker_membership(
        &self,
        node_id: NodeId,
    ) -> Result<Option<WorkerMembership>, AuthorityFailure> {
        if node_id == 0 || node_id > 9_007_199_254_740_991 {
            return Err(AuthorityFailure::rejected("invalid worker node ID"));
        }
        self.confirm_quorum().await?;
        self.store
            .worker(node_id)
            .await
            .map_err(AuthorityFailure::unavailable)
    }
    async fn local_validate(
        &self,
        id: &str,
        ownership: &Ownership,
    ) -> Result<Job, AuthorityFailure> {
        self.confirm_quorum().await?;
        if self
            .execution_peer(ownership.node_id)
            .await
            .map_err(AuthorityFailure::unavailable)?
            .is_none()
        {
            return Err(AuthorityFailure::rejected(
                "executor membership is unknown or revoked",
            ));
        }
        let job = self
            .local_job(id)
            .await
            .map_err(AuthorityFailure::unavailable)?
            .ok_or_else(|| AuthorityFailure::rejected("unknown execution"))?;
        if job.ownership != *ownership || job.stopped {
            return Err(AuthorityFailure::rejected(
                "execution ownership is stale or stopped",
            ));
        }
        Ok(job)
    }
    async fn route_execution(&self, rpc: Rpc) -> io::Result<Reply> {
        let (_, reply) = super::routing::route(
            rpc,
            &self.peers,
            self.leader(),
            || self.stopped.load(Ordering::Acquire),
            |id, peer, rpc| async move {
                if id == self.id {
                    return match rpc {
                        Rpc::Propose(request) => {
                            Ok(Reply::Propose(self.local_submit(request).await))
                        }
                        Rpc::WorkerMembership { node_id } => Ok(Reply::WorkerMembership(
                            self.local_worker_membership(node_id).await,
                        )),
                        Rpc::Validate { job_id, ownership } => Ok(Reply::Validate(
                            self.local_validate(&job_id, &ownership).await,
                        )),
                        _ => Err(error("invalid local execution operation")),
                    };
                }
                self.transport
                    .exchange(
                        &peer,
                        Packet {
                            version: CONTROL_PROTOCOL,
                            scope_id: self.scope_id.clone(),
                            from: self.id,
                            rpc,
                        },
                    )
                    .await
            },
        )
        .await?;
        Ok(reply)
    }
    pub async fn submit(&self, request: Request) -> io::Result<Receipt> {
        if request.actor != self.id {
            return Err(error("local execution request impersonates another peer"));
        }
        match self.route_execution(Rpc::Propose(request)).await? {
            Reply::Propose(Ok(receipt)) => Ok(receipt),
            _ => Err(error("unexpected authority reply")),
        }
    }
    pub async fn validate_ownership(&self, id: &str, ownership: &Ownership) -> io::Result<Job> {
        if ownership.node_id != self.id {
            return Err(error("cannot authorize another executor"));
        }
        match self
            .route_execution(Rpc::Validate {
                job_id: id.into(),
                ownership: ownership.clone(),
            })
            .await?
        {
            Reply::Validate(Ok(job)) => Ok(job),
            _ => Err(error("unexpected authority reply")),
        }
    }
    /// Current member state after a quorum read, distinct from immutable command receipts.
    pub async fn worker_membership(&self, node_id: NodeId) -> io::Result<Option<WorkerMembership>> {
        match self
            .route_execution(Rpc::WorkerMembership { node_id })
            .await?
        {
            Reply::WorkerMembership(Ok(worker)) => Ok(worker),
            _ => Err(error("unexpected membership reply")),
        }
    }
    async fn execution_peer(&self, id: NodeId) -> io::Result<Option<Peer>> {
        if let Some(peer) = self.peers.get(&id) {
            return Ok(peer.executor.then(|| peer.clone()));
        }
        Ok(self
            .store
            .worker(id)
            .await
            .map_err(|e| error(e.to_string()))?
            .filter(|worker| !worker.revoked)
            .map(|worker| Peer {
                identity: worker.identity,
                executor: true,
                data_replica: worker.data_replica,
            }))
    }
    /// authenticated_identity comes from the verified transport session, never Packet.from.
    pub async fn handle(&self, authenticated_identity: &str, packet: Packet) -> io::Result<Reply> {
        if packet.version != CONTROL_PROTOCOL || packet.scope_id != self.scope_id {
            return Err(error("untrusted or incompatible authority control packet"));
        }
        let voter = self
            .peers
            .get(&packet.from)
            .is_some_and(|peer| peer.identity == authenticated_identity);
        let execution_rpc = matches!(&packet.rpc, Rpc::Propose(_) | Rpc::Validate { .. });
        // Identity admits only an own-status read, never execution. In particular,
        // a revoked worker's validation must reach local_validate's quorum read
        // and membership check so its denial is typed and confirmed, not retried
        // as an unknown transport outcome. Proposals remain blocked below.
        let own_status_read = !voter
            && (matches!(&packet.rpc, Rpc::WorkerMembership { node_id } if *node_id == packet.from)
                || matches!(&packet.rpc, Rpc::Validate { ownership, .. } if ownership.node_id == packet.from))
            && self
                .store
                .worker(packet.from)
                .await
                .map_err(io::Error::other)?
                .is_some_and(|worker| worker.identity == authenticated_identity);
        if !voter
            && !own_status_read
            && (!execution_rpc
                || self
                    .execution_peer(packet.from)
                    .await?
                    .is_none_or(|peer| peer.identity != authenticated_identity))
        {
            return Err(error(
                "sender is not a confirmed peer for this control operation",
            ));
        }
        match packet.rpc {
            Rpc::WorkerMembership { node_id } => Ok(Reply::WorkerMembership(
                self.local_worker_membership(node_id).await,
            )),
            Rpc::Append(rpc) => {
                if rpc.vote.leader_id.node_id != packet.from {
                    return Err(error("append sender is not its declared leader"));
                }
                Ok(Reply::Append(self.raft.append_entries(rpc).await))
            }
            Rpc::Vote(rpc) => {
                if rpc.vote.leader_id.node_id != packet.from {
                    return Err(error("vote sender is not its declared candidate"));
                }
                Ok(Reply::Vote(self.raft.vote(rpc).await))
            }
            Rpc::Snapshot(rpc) => {
                if rpc.vote.leader_id.node_id != packet.from {
                    return Err(error("snapshot sender is not its declared leader"));
                }
                Ok(Reply::Snapshot(self.raft.install_snapshot(rpc).await))
            }
            Rpc::Propose(request) => {
                if request.actor != packet.from {
                    return Err(error("execution request impersonates another peer"));
                }
                Ok(Reply::Propose(self.local_submit(request).await))
            }
            Rpc::Validate { job_id, ownership } => {
                if ownership.node_id != packet.from {
                    return Err(error("ownership validation impersonates another peer"));
                }
                Ok(Reply::Validate(
                    self.local_validate(&job_id, &ownership).await,
                ))
            }
        }
    }
}
