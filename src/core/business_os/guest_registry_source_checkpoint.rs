// Origin: CTOX
// License: AGPL-3.0-only
//! Stage complete native artifacts without publication locks; commit only under fresh authority.
use super::*;
use ctox_sync::{
    capture::{CaptureEntry, CaptureRequest, CaptureResult},
    checkpoint::CheckpointStore,
    contracts::{SessionManifest, WorkspaceEntryKind},
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSourceCheckpointReceipt {
    pub digest: String,
    pub sequence: u64,
}
pub(super) struct PreparedSourceCheckpoint {
    captured: CaptureResult,
    workspace: workspaces::AssignedWorkspace,
}

pub(super) fn prepare(
    store: &CheckpointStore,
    store_root: &Path,
    workspace: workspaces::AssignedWorkspace,
    spec: &ExecutionSpec,
    ownership: &Ownership,
    receipt: &source_journal::NativeSourceJournalReceipt,
    configuration: &ctox_core::ThreadConfigSnapshot,
    state: &ctox_core::NativeSessionState,
    journal: &[u8],
    effects: &source_effects::SourceEffects,
) -> Result<PreparedSourceCheckpoint> {
    ensure!(
        !store_root.starts_with(&workspace.path),
        "native artifact store cannot be inside its captured workspace"
    );
    source_journal::validate_session_state(spec, state)?;
    let configuration_bytes = source_journal::core_configuration_bytes(spec, configuration)?;
    let effect_bytes = effects.bytes(spec, ownership)?;
    let pending_effects = effects.pending(&receipt.capture_id)?;
    ensure!(
        receipt.job_id == spec.job_id
            && receipt.session_id == spec.session_id
            && receipt.journal_sha256 == format!("{:x}", Sha256::digest(journal))
            && receipt.journal_size_bytes == journal.len() as u64,
        "native checkpoint differs from its actual source journal"
    );
    ensure!(
        tokio::runtime::Handle::try_current().is_err(),
        "native checkpoint requires the synchronous quiescent owner"
    );
    workspace.verify()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let captured = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(90), async {
            let repository = store.capture_git_bundle(&workspace.path).await?;
            let mut captured = store
                .capture(CaptureRequest {
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
                        credential_references: BTreeSet::from([spec.gateway_account_id.clone()]),
                    },
                    sequence: 1,
                    workspace_root: workspace.path.clone(),
                    history: vec![journal.to_vec()],
                    attachments: Vec::new(),
                    workspace: Vec::new(),
                    provider_state: vec![
                        CaptureEntry {
                            path: "native-core-configuration.json".into(),
                            kind: WorkspaceEntryKind::File,
                            bytes: configuration_bytes,
                            executable: false,
                        },
                        CaptureEntry {
                            path: "native-session-state.json".into(),
                            kind: WorkspaceEntryKind::File,
                            bytes: state.as_bytes().to_vec(),
                            executable: false,
                        },
                        CaptureEntry {
                            path: "native-workspace.bundle".into(),
                            kind: WorkspaceEntryKind::File,
                            bytes: repository,
                            executable: false,
                        },
                        CaptureEntry {
                            path: "native-effect-state.json".into(),
                            kind: WorkspaceEntryKind::File,
                            bytes: effect_bytes,
                            executable: false,
                        },
                    ],
                    pending_effects,
                })
                .await?;
            // VM420 chunks/metadata join the SAME protected Core/history/effect
            // manifest. publish rechecks every hash, path and total byte bound.
            if !effects.machine_entries().is_empty() {
                captured
                    .manifest
                    .provider_state
                    .extend_from_slice(effects.machine_entries());
                captured.digest = store.publish(&captured.manifest)?;
            }
            Ok::<_, anyhow::Error>(captured)
        })
        .await
        .context("native workspace capture timed out")?
        .context("native workspace checkpoint capture failed")
    })?;
    workspace.verify()?;
    Ok(PreparedSourceCheckpoint {
        captured,
        workspace,
    })
}

pub(super) fn commit(
    policy: &Connection,
    store_root: &Path,
    destination: &GuestRestoreDestination,
    spec: &ExecutionSpec,
    ownership: &Ownership,
    receipt: &source_journal::NativeSourceJournalReceipt,
    prepared: PreparedSourceCheckpoint,
) -> Result<NativeSourceCheckpointReceipt> {
    let current = workspaces::require(policy, destination, &prepared.workspace.path)?;
    prepared.workspace.verify()?;
    ensure!(
        current.working_copy_id == prepared.workspace.working_copy_id
            && current.revision == prepared.workspace.revision,
        "native workspace assignment changed during capture"
    );
    let workspace = prepared.workspace;
    let result = prepared.captured;
    let exact_source: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_native_source_journals
        WHERE capture_id=?1 AND spec_json=?2 AND ownership_json=?3
        AND artifact_store_path=?4 AND guest_id=?5 AND controller_id=?6
        AND controller_generation=?7 AND owner_user_id=?8 AND worker_profile_id=?9
        AND project_id=?10 AND thread_id=?11 AND source_instance_id=?12)",
        rusqlite::params![
            receipt.capture_id,
            serde_json::to_string(spec)?,
            serde_json::to_string(ownership)?,
            store_root
                .to_str()
                .context("native store path is not UTF-8")?,
            destination.guest_id,
            destination.controller_id,
            i64::try_from(destination.controller_generation)?,
            destination.human_owner_id,
            destination.worker_profile_id,
            destination.project_id,
            destination.thread_id,
            destination.instance_id,
        ],
        |row| row.get(0),
    )?;
    ensure!(
        exact_source,
        "native checkpoint has a foreign source capture"
    );
    source_policy::persist(policy, destination, &receipt.capture_id)?;
    policy.execute(
        "INSERT INTO business_native_source_checkpoints
        (capture_id,checkpoint_digest,checkpoint_sequence,working_copy_id,workspace_revision)
        VALUES (?1,?2,?3,?4,?5) ON CONFLICT(capture_id) DO NOTHING",
        rusqlite::params![
            receipt.capture_id,
            result.digest,
            i64::try_from(result.manifest.sequence)?,
            workspace.working_copy_id,
            i64::try_from(workspace.revision)?
        ],
    )?;
    let exact: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_native_source_checkpoints WHERE capture_id=?1
        AND checkpoint_digest=?2 AND checkpoint_sequence=?3 AND working_copy_id=?4 AND workspace_revision=?5)",
        rusqlite::params![receipt.capture_id,result.digest,i64::try_from(result.manifest.sequence)?,
            workspace.working_copy_id,i64::try_from(workspace.revision)?],
        |row| row.get(0),
    )?;
    ensure!(exact, "native source checkpoint conflicts; reconcile");
    Ok(NativeSourceCheckpointReceipt {
        digest: result.digest,
        sequence: result.manifest.sequence,
    })
}
