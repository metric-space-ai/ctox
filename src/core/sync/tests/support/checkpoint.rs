use ctox_sync::contracts::{
    SessionHandoffPermit, SessionHandoffPhase, CTOX_SYNC_SESSION_HANDOFF_PERMIT_VERSION,
};
use ctox_sync::{
    authority::{auth::SigningIdentity, ExecutionSpec, Ownership},
    checkpoint::CheckpointStore,
    contracts::{
        ArtifactRef, CheckpointCopyReceipt, CheckpointManifest, GitWorkspaceState, SessionManifest,
        WorkspaceEntry, WorkspaceEntryKind,
    },
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Cursor, path::Path};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Mint quorum-evidence permits with the real signing path, exactly as the
/// native policy adapter does after a policy decision: audience is the job
/// scope, nonce is the command request id.
pub fn handoff_permit(
    key: &SigningIdentity,
    phase: SessionHandoffPhase,
    spec: &ExecutionSpec,
    checkpoint_digest: &str,
    sequence: u64,
    ownership: &Ownership,
    request_id: &str,
) -> SessionHandoffPermit {
    let now = now_ms();
    key.sign_session_handoff_permit(&SessionHandoffPermit {
        version: CTOX_SYNC_SESSION_HANDOFF_PERMIT_VERSION,
        binding_digest: "b".repeat(64),
        phase,
        audience: spec.scope_id.clone(),
        nonce: request_id.to_owned(),
        job_id: spec.job_id.clone(),
        session_id: spec.session_id.clone(),
        scope_id: spec.scope_id.clone(),
        checkpoint_digest: checkpoint_digest.to_owned(),
        checkpoint_sequence: sequence,
        ownership_generation: ownership.generation,
        principal_epoch: 0,
        binding_revision: 1,
        issued_at_ms: now,
        expires_at_ms: now + 60_000,
        signature: String::new(),
    })
    .unwrap()
}

/// Even consensus-only fixtures obtain receipts from independently persisted data.
pub fn copy_receipt(
    root: &Path,
    id: u64,
    key: &SigningIdentity,
    spec: &ExecutionSpec,
    ownership: &Ownership,
    sequence: u64,
) -> CheckpointCopyReceipt {
    let store = CheckpointStore::open(root.join(format!("copy-{id}")), 4096).unwrap();
    let data = b"complete synthetic journal for authority fixture";
    let journal = ArtifactRef {
        sha256: format!("{:x}", Sha256::digest(data)),
        size_bytes: data.len() as u64,
    };
    store.ingest_blob(&journal, Cursor::new(data)).unwrap();
    let manifest = CheckpointManifest {
        version: 2,
        session: SessionManifest {
            version: 1,
            scope_id: spec.scope_id.clone(),
            session_id: spec.session_id.clone(),
            harness: spec.harness.clone(),
            harness_version: spec.harness_version.clone(),
            model_route_id: spec.model_route_id.clone(),
            gateway_account_id: spec.gateway_account_id.clone(),
            model_id: spec.model_id.clone(),
            required_capabilities: spec.required_capabilities.clone(),
            credential_references: BTreeSet::new(),
        },
        sequence,
        workspace_state: GitWorkspaceState {
            base_commit: "a".repeat(40),
            index_patch: journal.clone(),
            worktree_patch: journal.clone(),
            required_untracked: vec![],
            deleted_paths: BTreeSet::new(),
        },
        history: vec![journal.clone()],
        attachments: vec![],
        workspace: vec![],
        provider_state: vec![WorkspaceEntry {
            path: "synthetic-harness.jsonl".into(),
            kind: WorkspaceEntryKind::File,
            artifact: journal,
            executable: false,
        }],
        pending_effects: vec![],
    };
    let digest = store.publish(&manifest).unwrap();
    key.acknowledge_checkpoint(&store, id, spec, ownership, &digest)
        .unwrap()
}
