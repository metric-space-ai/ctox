//! Nonvoting executor access to the same committed authority used by native voters.
//! No local ownership writer, election, HTTP mailbox or unsigned routing shortcut.
use super::{
    auth::{ControlChannel, SignedTransport, SigningIdentity},
    network::{ControlTransport, Packet, Reply, Rpc, CONTROL_PROTOCOL},
    node::AuthorityNode,
    Command, Job, NodeId, Ownership, Peer, Receipt, Request, WorkerMembership,
};
use async_trait::async_trait;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

/// One local IPC contract for voters and additional workers. Implementations
/// confirm authority remotely or through Raft, never from a local projection.
#[async_trait]
pub trait ExecutionAuthority: Send + Sync {
    fn node_id(&self) -> NodeId;
    fn scope_id(&self) -> &str;
    /// Current quorum-confirmed membership, including tombstones; never a replay receipt.
    async fn worker_membership(&self, node_id: NodeId) -> io::Result<Option<WorkerMembership>>;
    async fn submit(&self, request: Request) -> io::Result<Receipt>;
    async fn validate_ownership(&self, id: &str, ownership: &Ownership) -> io::Result<Job>;
    async fn shutdown(&self) -> io::Result<()>;
}

#[async_trait]
impl ExecutionAuthority for AuthorityNode {
    fn node_id(&self) -> NodeId {
        AuthorityNode::node_id(self)
    }
    fn scope_id(&self) -> &str {
        AuthorityNode::scope_id(self)
    }
    async fn submit(&self, request: Request) -> io::Result<Receipt> {
        AuthorityNode::submit(self, request).await
    }
    async fn worker_membership(&self, node_id: NodeId) -> io::Result<Option<WorkerMembership>> {
        AuthorityNode::worker_membership(self, node_id).await
    }
    async fn validate_ownership(&self, id: &str, ownership: &Ownership) -> io::Result<Job> {
        AuthorityNode::validate_ownership(self, id, ownership).await
    }
    async fn shutdown(&self) -> io::Result<()> {
        AuthorityNode::shutdown(self).await
    }
}

pub struct WorkerAuthorityClient {
    member: WorkerMembership,
    scope_id: String,
    voters: BTreeMap<NodeId, Peer>,
    transport: SignedTransport,
    preferred: Mutex<Option<NodeId>>,
    stopped: AtomicBool,
}
impl WorkerAuthorityClient {
    /// The supplied member record is an identity pin, not permission to execute.
    /// Every operation must still be confirmed by the configured voting group.
    pub fn new(
        member: WorkerMembership,
        scope_id: String,
        voters: BTreeMap<NodeId, Peer>,
        key: Arc<SigningIdentity>,
        channel: Arc<dyn ControlChannel>,
    ) -> io::Result<Self> {
        let identities: BTreeSet<_> = voters.values().map(|peer| &peer.identity).collect();
        for identity in &identities {
            super::auth::public_key(identity)?;
        }
        if scope_id.trim().is_empty()
            || member.revoked
            || member.node_id == 0
            || member.node_id > 9_007_199_254_740_991
            || member.identity != key.public_identity()
            || voters.len() != 3
            || identities.len() != 3
            || voters.contains_key(&member.node_id)
            || identities.contains(&member.identity)
            || voters
                .values()
                .filter(|peer| peer.executor && peer.data_replica)
                .count()
                < 2
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "worker requires its own pinned identity and three distinct authority voters",
            ));
        }
        Ok(Self {
            member,
            transport: SignedTransport::new(key, scope_id.clone(), channel),
            scope_id,
            voters,
            preferred: Mutex::new(None),
            stopped: AtomicBool::new(false),
        })
    }

    async fn exchange(&self, rpc: Rpc) -> io::Result<Reply> {
        let preferred = *self
            .preferred
            .lock()
            .map_err(|_| io::Error::other("worker route lock poisoned"))?;
        let (id, reply) = super::routing::route(
            rpc,
            &self.voters,
            preferred,
            || self.stopped.load(Ordering::Acquire),
            |_, peer, rpc| async move {
                self.transport
                    .exchange(
                        &peer,
                        Packet {
                            version: CONTROL_PROTOCOL,
                            scope_id: self.scope_id.clone(),
                            from: self.member.node_id,
                            rpc,
                        },
                    )
                    .await
            },
        )
        .await?;
        *self
            .preferred
            .lock()
            .map_err(|_| io::Error::other("worker route lock poisoned"))? = Some(id);
        Ok(reply)
    }
}
#[async_trait]
impl ExecutionAuthority for WorkerAuthorityClient {
    async fn worker_membership(&self, node_id: NodeId) -> io::Result<Option<WorkerMembership>> {
        if node_id != self.member.node_id {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "worker cannot inspect another member",
            ));
        }
        match self.exchange(Rpc::WorkerMembership { node_id }).await? {
            Reply::WorkerMembership(Ok(Some(worker)))
                if worker.node_id == node_id && worker.identity == self.member.identity =>
            {
                Ok(Some(worker))
            }
            Reply::WorkerMembership(Ok(None)) => Ok(None),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected worker membership response",
            )),
        }
    }
    fn node_id(&self) -> NodeId {
        self.member.node_id
    }
    fn scope_id(&self) -> &str {
        &self.scope_id
    }
    async fn submit(&self, request: Request) -> io::Result<Receipt> {
        if request.actor != self.member.node_id
            || matches!(
                request.command,
                Command::AdmitWorker { .. } | Command::RevokeWorker { .. }
            )
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "worker cannot impersonate another peer or change membership",
            ));
        }
        if let Command::Create { spec, .. } = &request.command {
            if spec.scope_id != self.scope_id {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "worker execution scope mismatch",
                ));
            }
        }
        match self.exchange(Rpc::Propose(request)).await? {
            Reply::Propose(Ok(receipt)) => Ok(receipt),
            _ => Err(io::Error::other("unexpected execution proposal response")),
        }
    }
    async fn validate_ownership(&self, id: &str, ownership: &Ownership) -> io::Result<Job> {
        if ownership.node_id != self.member.node_id {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "worker cannot authorize another executor",
            ));
        }
        match self
            .exchange(Rpc::Validate {
                job_id: id.into(),
                ownership: ownership.clone(),
            })
            .await?
        {
            Reply::Validate(Ok(job))
                if job.spec.job_id == id
                    && job.spec.scope_id == self.scope_id
                    && job.ownership == *ownership
                    && !job.stopped =>
            {
                Ok(job)
            }
            _ => Err(io::Error::other("unexpected execution validation response")),
        }
    }
    async fn shutdown(&self) -> io::Result<()> {
        self.stopped.store(true, Ordering::Release);
        Ok(())
    }
}

/// Transfer an existing protected job to this executor. Source ownership is
/// checked by the committed TakeOver command, not by a foreign Validate read:
/// nonvoting workers deliberately cannot authorize another executor.
/// The lifecycle caller owns native policy fences and durable Pending evidence.
pub async fn take_over_checkpoint(
    authority: &dyn ExecutionAuthority,
    request: Request,
    spec: &crate::contracts::ExecutionSpec,
    sequence: u64,
) -> io::Result<Job> {
    let Command::TakeOver {
        job_id,
        expected,
        checkpoint_digest,
        owner,
        ..
    } = &request.command
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "checkpoint takeover requires TakeOver",
        ));
    };
    let next = Ownership {
        node_id: authority.node_id(),
        generation: expected
            .generation
            .checked_add(1)
            .ok_or_else(|| io::Error::other("checkpoint ownership generation exhausted"))?,
    };
    if request.actor != next.node_id
        || *owner != next.node_id
        || expected.node_id == next.node_id
        || expected.node_id == 0
        || expected.generation == 0
        || next.node_id == 0
        || job_id != &spec.job_id
        || authority.scope_id() != spec.scope_id
        || sequence == 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "checkpoint takeover destination differs",
        ));
    }
    let digest = checkpoint_digest.clone();
    let applied = match authority.submit(request).await? {
        Receipt::Applied(job) => job,
        _ => {
            return Err(io::Error::other(
                "checkpoint takeover needs a fresh applied receipt; reconcile",
            ))
        }
    };
    let matches = |job: &Job| {
        job.spec == *spec
            && job.ownership == next
            && !job.stopped
            && job.pending_effects.is_empty()
            && !job.checkpoint_requires_refresh
            && job.checkpoint.as_ref().is_some_and(|checkpoint| {
                checkpoint.digest == digest
                    && checkpoint.sequence == sequence
                    && checkpoint.replicas.contains(&next.node_id)
            })
    };
    if !matches(&applied) {
        return Err(io::Error::other(
            "checkpoint takeover applied a different execution; reconcile",
        ));
    }
    let current = authority.validate_ownership(&spec.job_id, &next).await?;
    if !matches(&current) || current != applied {
        return Err(io::Error::other(
            "checkpoint job changed after takeover; reconcile",
        ));
    }
    Ok(current)
}
