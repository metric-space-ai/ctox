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
    let meta = serde_json::json!({
        "timestamp": "2026-09-20T12:00:00Z",
        "type": "session_meta",
        "payload": {
            "id": spec.session_id,
            "timestamp": "2026-09-20T12:00:00Z",
            "cwd": "/original/workspace",
            "originator": "codex_cli_rs",
            "cli_version": "1.0.0",
            "source": "exec",
            "model_provider": "test-provider",
            "base_instructions": {},
            "capability_profile": "workspace_worker",
        },
    });
    let event = serde_json::json!({
        "timestamp": "2026-09-20T12:00:00Z",
        "type": "event_msg",
        "payload": {"type": "user_message", "message": "ready", "kind": "plain"},
    });
    let data = format!("{meta}\n{event}\n").into_bytes();
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
