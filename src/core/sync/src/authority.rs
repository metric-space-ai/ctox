//! Majority-confirmed execution ownership, independent of RxDB master/fork election.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub mod auth;
pub mod client;
pub mod diagnostics;
pub mod handoff;
pub mod network;
pub mod node;
mod routing;
pub mod store;
pub mod timing;
#[cfg(feature = "webrtc")]
pub mod webrtc;

pub type NodeId = u64;

pub use crate::contracts::ExecutionPeer as Peer;

pub use crate::contracts::{
    CheckpointCopyReceipt, ExecutionOwnership as Ownership, ExecutionSpec, SessionHandoffPermit,
    SessionHandoffPhase, WorkerMembership,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProtectedCheckpoint {
    pub digest: String,
    pub sequence: u64,
    pub replicas: BTreeSet<NodeId>,
    /// Retain the authenticated evidence across Raft log compaction and snapshots.
    pub receipts: Vec<CheckpointCopyReceipt>,
    /// Legacy snapshots remain readable but cannot authorize takeover without
    /// freshly protected disclosure evidence.
    #[serde(default)]
    pub disclosure: Option<SessionHandoffPermit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Job {
    pub spec: ExecutionSpec,
    pub ownership: Ownership,
    pub checkpoint: Option<ProtectedCheckpoint>,
    pub pending_effects: BTreeSet<String>,
    pub completed_effects: BTreeSet<String>,
    pub stopped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Command {
    AdmitWorker {
        worker: WorkerMembership,
    },
    RevokeWorker {
        node_id: NodeId,
    },
    Create {
        spec: ExecutionSpec,
        owner: NodeId,
    },
    ProtectCheckpoint {
        job_id: String,
        ownership: Ownership,
        receipts: Vec<CheckpointCopyReceipt>,
        /// Current signed source-disclosure decision, verified deterministically
        /// against the enrolled owner key, this scope and the request id.
        disclosure: SessionHandoffPermit,
    },
    TakeOver {
        job_id: String,
        expected: Ownership,
        checkpoint_digest: String,
        owner: NodeId,
        /// Current signed target-resume decision from the new owner instance.
        resume: SessionHandoffPermit,
    },
    BeginEffect {
        job_id: String,
        ownership: Ownership,
        effect_id: String,
    },
    CompleteEffect {
        job_id: String,
        ownership: Ownership,
        effect_id: String,
    },
    Stop {
        job_id: String,
        ownership: Ownership,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub request_id: String,
    pub actor: NodeId,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "camelCase")]
pub enum Receipt {
    WorkerApplied(WorkerMembership),
    WorkerReplayed(WorkerMembership),
    Applied(Job),
    /// A previous committed result, never a fresh permission to dispatch an effect.
    Replayed(Job),
    Rejected(Rejection),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rejection {
    InvalidRequest,
    UnknownPeer,
    UnknownJob,
    AlreadyExists,
    StaleOwner,
    CheckpointUnavailable,
    CheckpointRegressed,
    /// The embedded session-handoff permit is missing, forged, mismatched or stale.
    PolicyDenied,
    ReconciliationRequired,
    EffectAlreadyCompleted,
    EffectNotStarted,
    RequestIdConflict,
    Stopped,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub jobs: BTreeMap<String, Job>,
    /// Confirmed executors, independent of Raft voters. Revoked IDs are never reused.
    #[serde(default)]
    pub workers: BTreeMap<NodeId, WorkerMembership>,
    receipts: BTreeMap<String, (String, Receipt)>,
}

impl State {
    /// Pure deterministic transition; only a committed Raft entry may call this.
    pub fn apply(&mut self, request: &Request, peers: &BTreeMap<NodeId, Peer>) -> Receipt {
        let bytes = serde_json::to_vec(request).expect("typed request serializes");
        let fingerprint = format!("{:x}", Sha256::digest(bytes));
        if let Some((previous, receipt)) = self.receipts.get(&request.request_id) {
            return if previous == &fingerprint {
                match receipt {
                    Receipt::WorkerApplied(worker) | Receipt::WorkerReplayed(worker) => {
                        Receipt::WorkerReplayed(worker.clone())
                    }
                    Receipt::Applied(job) | Receipt::Replayed(job) => {
                        Receipt::Replayed(job.clone())
                    }
                    Receipt::Rejected(reason) => Receipt::Rejected(reason.clone()),
                }
            } else {
                Receipt::Rejected(Rejection::RequestIdConflict)
            };
        }
        let receipt = match self.apply_command(request, peers) {
            Ok(receipt) => receipt,
            Err(error) => Receipt::Rejected(error),
        };
        // Empty IDs are invalid, and must not reserve the shared empty receipt key.
        if !request.request_id.is_empty() {
            self.receipts
                .insert(request.request_id.clone(), (fingerprint, receipt.clone()));
        }
        receipt
    }

    fn apply_command(
        &mut self,
        request: &Request,
        peers: &BTreeMap<NodeId, Peer>,
    ) -> Result<Receipt, Rejection> {
        use Rejection::*;
        if request.request_id.is_empty() || request.request_id.len() > 256 {
            return Err(InvalidRequest);
        }
        match &request.command {
            Command::AdmitWorker { worker } => {
                // Only an existing voter can commit membership. Workers cannot enroll peers.
                if !peers.contains_key(&request.actor) {
                    return Err(UnknownPeer);
                }
                if worker.revoked
                    || worker.node_id == 0
                    || worker.node_id > 9_007_199_254_740_991
                    || auth::public_key(&worker.identity).is_err()
                {
                    return Err(InvalidRequest);
                }
                if peers.contains_key(&worker.node_id)
                    || self.workers.contains_key(&worker.node_id)
                    || peers.values().any(|peer| peer.identity == worker.identity)
                    || self
                        .workers
                        .values()
                        .any(|peer| !peer.revoked && peer.identity == worker.identity)
                {
                    return Err(AlreadyExists);
                }
                self.workers.insert(worker.node_id, worker.clone());
                return Ok(Receipt::WorkerApplied(worker.clone()));
            }
            Command::RevokeWorker { node_id } => {
                if !peers.contains_key(&request.actor) {
                    return Err(UnknownPeer);
                }
                let worker = self.workers.get_mut(node_id).ok_or(UnknownPeer)?;
                worker.revoked = true;
                return Ok(Receipt::WorkerApplied(worker.clone()));
            }
            _ => {}
        }
        let mut peers = peers.clone();
        for worker in self.workers.values().filter(|worker| !worker.revoked) {
            peers.insert(
                worker.node_id,
                Peer {
                    identity: worker.identity.clone(),
                    executor: true,
                    data_replica: worker.data_replica,
                },
            );
        }
        let actor = peers.get(&request.actor).ok_or(UnknownPeer)?;
        if !actor.executor {
            return Err(UnknownPeer);
        }
        if let Command::Create { spec, owner } = &request.command {
            if *owner != request.actor
                || spec.job_id.is_empty()
                || spec.session_id.is_empty()
                || spec.scope_id.is_empty()
                || spec.harness.is_empty()
                || spec.harness_version.is_empty()
                || spec.model_route_id.is_empty()
                || spec.gateway_account_id.is_empty()
                || spec.model_id.is_empty()
            {
                return Err(InvalidRequest);
            }
            if self.jobs.contains_key(&spec.job_id) {
                return Err(AlreadyExists);
            }
            let job = Job {
                spec: spec.clone(),
                ownership: Ownership {
                    node_id: *owner,
                    generation: 1,
                },
                checkpoint: None,
                pending_effects: BTreeSet::new(),
                completed_effects: BTreeSet::new(),
                stopped: false,
            };
            self.jobs.insert(spec.job_id.clone(), job.clone());
            return Ok(Receipt::Applied(job));
        }
        let (job_id, expected) = match &request.command {
            Command::ProtectCheckpoint {
                job_id, ownership, ..
            }
            | Command::BeginEffect {
                job_id, ownership, ..
            }
            | Command::CompleteEffect {
                job_id, ownership, ..
            }
            | Command::Stop { job_id, ownership } => (job_id, ownership),
            Command::TakeOver {
                job_id, expected, ..
            } => (job_id, expected),
            Command::Create { .. } | Command::AdmitWorker { .. } | Command::RevokeWorker { .. } => {
                unreachable!()
            }
        };
        let job = self.jobs.get_mut(job_id).ok_or(UnknownJob)?;
        if &job.ownership != expected {
            return Err(StaleOwner);
        }
        if job.stopped {
            return Err(Stopped);
        }
        if !matches!(request.command, Command::TakeOver { .. }) && request.actor != expected.node_id
        {
            return Err(StaleOwner);
        }
        match &request.command {
            Command::ProtectCheckpoint {
                receipts,
                disclosure,
                ..
            } => {
                if !job.pending_effects.is_empty() {
                    return Err(ReconciliationRequired);
                }
                if receipts.len() < 2 || receipts.len() > peers.len() {
                    return Err(CheckpointUnavailable);
                }
                let first = &receipts[0];
                // Disclosure evidence is part of the deterministic transition:
                // signed by the enrolled owner key, bound to this exact job,
                // checkpoint, ownership generation, scope and request id.
                let owner_peer = peers.get(&expected.node_id).ok_or(PolicyDenied)?;
                auth::session_handoff::verify_permit_evidence(
                    disclosure,
                    &owner_peer.identity,
                    SessionHandoffPhase::Disclose,
                    &job.spec,
                    &first.checkpoint_digest,
                    first.sequence,
                    expected.generation,
                    &request.request_id,
                )
                .map_err(|_| PolicyDenied)?;
                let mut replicas = BTreeSet::new();
                for receipt in receipts {
                    let peer = peers.get(&receipt.node_id).ok_or(CheckpointUnavailable)?;
                    if receipt.spec != job.spec
                        || &receipt.ownership != expected
                        || receipt.checkpoint_digest != first.checkpoint_digest
                        || receipt.sequence != first.sequence
                        || !replicas.insert(receipt.node_id)
                        || auth::verify_checkpoint_copy(receipt, peer).is_err()
                    {
                        return Err(CheckpointUnavailable);
                    }
                }
                let checkpoint = ProtectedCheckpoint {
                    digest: first.checkpoint_digest.clone(),
                    sequence: first.sequence,
                    replicas,
                    receipts: receipts.clone(),
                    disclosure: Some(disclosure.clone()),
                };
                if checkpoint.digest.len() != 64
                    || !checkpoint
                        .digest
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                    || checkpoint.replicas.len() < 2
                    || !checkpoint.replicas.contains(&expected.node_id)
                    || checkpoint
                        .replicas
                        .iter()
                        .any(|id| !peers.get(id).is_some_and(|p| p.data_replica))
                {
                    return Err(CheckpointUnavailable);
                }
                if let Some(previous) = &job.checkpoint {
                    // Upgrade an unchanged legacy checkpoint only after the
                    // current owner supplied newly verified disclosure evidence.
                    // No checkpoint bytes, copy evidence or ownership can change.
                    let authorizes_legacy = previous.disclosure.is_none()
                        && checkpoint.sequence == previous.sequence
                        && checkpoint.digest == previous.digest
                        && checkpoint.replicas == previous.replicas
                        && checkpoint.receipts == previous.receipts;
                    if checkpoint.sequence <= previous.sequence && !authorizes_legacy {
                        return Err(CheckpointRegressed);
                    }
                }
                job.checkpoint = Some(checkpoint);
            }
            Command::TakeOver {
                owner,
                checkpoint_digest,
                resume,
                ..
            } => {
                if *owner != request.actor || *owner == expected.node_id {
                    return Err(InvalidRequest);
                }
                if !job.pending_effects.is_empty() {
                    return Err(ReconciliationRequired);
                }
                let checkpoint = job.checkpoint.as_ref().ok_or(CheckpointUnavailable)?;
                if &checkpoint.digest != checkpoint_digest || !checkpoint.replicas.contains(owner) {
                    return Err(CheckpointUnavailable);
                }
                // The new owner must carry its own current, signed resume
                // decision: target execution starts only with target policy.
                let disclosure = checkpoint.disclosure.as_ref().ok_or(PolicyDenied)?;
                if resume.binding_digest != disclosure.binding_digest {
                    return Err(PolicyDenied);
                }
                let new_owner_peer = peers.get(owner).ok_or(PolicyDenied)?;
                auth::session_handoff::verify_permit_evidence(
                    resume,
                    &new_owner_peer.identity,
                    SessionHandoffPhase::Resume,
                    &job.spec,
                    &checkpoint.digest,
                    checkpoint.sequence,
                    expected.generation,
                    &request.request_id,
                )
                .map_err(|_| PolicyDenied)?;
                let generation = expected.generation.checked_add(1).ok_or(InvalidRequest)?;
                job.ownership = Ownership {
                    node_id: *owner,
                    generation,
                };
            }
            Command::BeginEffect { effect_id, .. } => {
                if effect_id.is_empty() {
                    return Err(InvalidRequest);
                }
                if job.completed_effects.contains(effect_id) {
                    return Err(EffectAlreadyCompleted);
                }
                // A new request must not execute an already-started effect again.
                if !job.pending_effects.insert(effect_id.clone()) {
                    return Err(ReconciliationRequired);
                }
            }
            Command::CompleteEffect { effect_id, .. } => {
                if !job.pending_effects.remove(effect_id) {
                    return Err(EffectNotStarted);
                }
                job.completed_effects.insert(effect_id.clone());
            }
            Command::Stop { .. } => {
                if !job.pending_effects.is_empty() {
                    return Err(ReconciliationRequired);
                }
                job.stopped = true;
            }
            Command::Create { .. } | Command::AdmitWorker { .. } | Command::RevokeWorker { .. } => {
                unreachable!()
            }
        }
        Ok(Receipt::Applied(job.clone()))
    }
}

openraft::declare_raft_types!(
    pub TypeConfig:
        D = Request,
        R = Receipt,
        NodeId = NodeId,
        Node = Peer,
        Entry = openraft::Entry<TypeConfig>,
        SnapshotData = std::io::Cursor<Vec<u8>>,
        AsyncRuntime = openraft::TokioRuntime,
);
pub type Raft = openraft::Raft<TypeConfig>;
