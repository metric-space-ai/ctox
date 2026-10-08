// Origin: CTOX
// License: AGPL-3.0-only

//! Private durable input from the actual retired native producer.
//! A journal receipt is neither a complete checkpoint nor a transfer/resume grant.
use super::*;
use ctox_protocol::portable_journal::{
    artifact_ref_for, validate_portable_journal, PortableJournalExpectation, PortableJournalFormat,
    PortableJournalLimits,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSourceJournalReceipt {
    pub capture_id: String,
    pub job_id: String,
    pub session_id: String,
    pub journal_sha256: String,
    pub journal_size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<source_checkpoint::NativeSourceCheckpointReceipt>,
}

pub(super) fn source_store(
    parent: &Path,
) -> Result<(
    ctox_sync::checkpoint::CheckpointStore,
    PathBuf,
    FileIdentity,
)> {
    private_directory(parent)?;
    ensure!(
        std::fs::canonicalize(parent)? == parent,
        "native source parent is not canonical"
    );
    let root = parent.join("source-journals");
    for path in [&root, &root.join("blobs"), &root.join("manifests")] {
        use std::os::unix::fs::DirBuilderExt;
        match std::fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
        private_directory(path)?;
        // The blob fsync cannot make a newly created store directory durable.
        // Persist each directory and its parent before policy metadata commits.
        std::fs::File::open(path)?.sync_all()?;
        std::fs::File::open(
            path.parent()
                .context("native source directory has no parent")?,
        )?
        .sync_all()?;
    }
    let identity = private_directory(&root)?;
    let store = ctox_sync::checkpoint::CheckpointStore::open(
        root.clone(),
        PortableJournalLimits::default().max_bytes,
    )?;
    ensure!(
        private_directory(&root)? == identity,
        "native source artifact store changed; reconcile"
    );
    Ok((store, root, identity))
}

pub(super) fn persist(
    policy: &Connection,
    store: &ctox_sync::checkpoint::CheckpointStore,
    store_root: &Path,
    destination: &GuestRestoreDestination,
    spec: &ExecutionSpec,
    ownership: &Ownership,
    policy_revision: &str,
    bytes: &[u8],
) -> Result<NativeSourceJournalReceipt> {
    ensure!(
        spec.harness == ctox_core::native_harness_name()
            && spec.harness_version == ctox_core::native_harness_version()
            && spec.model_route_id == "openai",
        "unsupported native source journal producer"
    );
    let artifact = artifact_ref_for(bytes);
    let expected = PortableJournalExpectation {
        format: PortableJournalFormat::current(),
        session_id: ctox_protocol::ThreadId::from_string(&spec.session_id)
            .context("native source journal session is invalid")?,
    };
    let validated = validate_portable_journal(
        bytes,
        &artifact,
        &expected,
        &PortableJournalLimits::default(),
    )?;
    let stored_artifact = ctox_sync::contracts::ArtifactRef {
        sha256: artifact.sha256.clone(),
        size_bytes: artifact.size_bytes,
    };
    store.ingest_blob(&stored_artifact, std::io::Cursor::new(bytes))?;
    let store_path = store_root
        .to_str()
        .context("native source artifact path is not UTF-8")?;
    let spec_json = serde_json::to_string(spec)?;
    let ownership_json = serde_json::to_string(ownership)?;
    let capture_id = format!("capture_{}", uuid::Uuid::new_v4());
    policy.execute(
        "INSERT INTO business_native_source_journals
         (capture_id,guest_id,controller_id,controller_generation,owner_user_id,
          worker_profile_id,project_id,thread_id,source_instance_id,job_id,session_id,
          ownership_generation,spec_json,ownership_json,policy_revision,
          journal_format,journal_version,journal_sha256,journal_size_bytes,
          journal_record_count,artifact_store_path,created_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)
         ON CONFLICT(job_id,session_id,ownership_generation) DO NOTHING",
        rusqlite::params![
            capture_id,
            destination.guest_id,
            destination.controller_id,
            i64::try_from(destination.controller_generation)?,
            destination.human_owner_id,
            destination.worker_profile_id,
            destination.project_id,
            destination.thread_id,
            destination.instance_id,
            spec.job_id,
            spec.session_id,
            i64::try_from(ownership.generation)?,
            spec_json,
            ownership_json,
            policy_revision,
            validated.format,
            i64::from(validated.format_version),
            artifact.sha256,
            i64::try_from(artifact.size_bytes)?,
            i64::try_from(validated.record_count)?,
            store_path,
            super::super::store::now_ms() as i64,
        ],
    )?;
    // Repeated publication may recover the same receipt, never overwrite an
    // earlier journal or silently bind it to another controller/actor/policy.
    let stored: (String, String, i64) = policy
        .query_row(
            "SELECT capture_id,journal_sha256,journal_size_bytes
         FROM business_native_source_journals
         WHERE job_id=?1 AND session_id=?2 AND ownership_generation=?3
         AND guest_id=?4 AND controller_id=?5 AND controller_generation=?6
         AND owner_user_id=?7 AND worker_profile_id=?8 AND project_id=?9 AND thread_id=?10
         AND source_instance_id=?11 AND spec_json=?12 AND ownership_json=?13
         AND policy_revision=?14 AND journal_sha256=?15 AND journal_size_bytes=?16
         AND journal_record_count=?17 AND artifact_store_path=?18",
            rusqlite::params![
                spec.job_id,
                spec.session_id,
                i64::try_from(ownership.generation)?,
                destination.guest_id,
                destination.controller_id,
                i64::try_from(destination.controller_generation)?,
                destination.human_owner_id,
                destination.worker_profile_id,
                destination.project_id,
                destination.thread_id,
                destination.instance_id,
                spec_json,
                ownership_json,
                policy_revision,
                artifact.sha256,
                i64::try_from(artifact.size_bytes)?,
                i64::try_from(validated.record_count)?,
                store_path,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .context("native source journal publication conflicts; reconcile")?;
    Ok(NativeSourceJournalReceipt {
        capture_id: stored.0,
        job_id: spec.job_id.clone(),
        session_id: spec.session_id.clone(),
        journal_sha256: stored.1,
        journal_size_bytes: u64::try_from(stored.2)?,
        checkpoint: None,
    })
}

/// Actual Core configuration captured after checked producer shutdown. This
/// exports settings, not credentials, remote provider state or resume permission.
pub(super) fn core_configuration_bytes(
    spec: &ExecutionSpec,
    configuration: &ctox_core::ThreadConfigSnapshot,
) -> Result<Vec<u8>> {
    ensure!(
        spec.harness == ctox_core::native_harness_name()
            && spec.harness_version == ctox_core::native_harness_version()
            && spec.model_route_id == "openai"
            && configuration.model_provider_id == spec.model_route_id
            && configuration.model == spec.model_id
            && !configuration.ephemeral,
        "native Core configuration differs from admitted producer"
    );
    ensure!(
        configuration.cwd.is_absolute()
            && configuration.cwd.is_dir()
            && std::fs::canonicalize(&configuration.cwd)? == configuration.cwd,
        "native Core workspace is unavailable or not canonical"
    );
    let bytes = serde_json::to_vec(&serde_json::json!({
        "format": "ctox-native-core-configuration",
        "version": 1,
        "sessionId": spec.session_id,
        "harness": spec.harness,
        "harnessVersion": spec.harness_version,
        "modelRouteId": configuration.model_provider_id,
        "gatewayAccountId": spec.gateway_account_id,
        "modelId": configuration.model,
        "sourceWorkspace": configuration.cwd,
        "serviceTier": configuration.service_tier,
        "approvalPolicy": configuration.approval_policy,
        "approvalsReviewer": configuration.approvals_reviewer,
        "sandboxPolicy": configuration.sandbox_policy,
        "reasoningEffort": configuration.reasoning_effort,
        "personality": configuration.personality,
        "sessionSource": configuration.session_source,
        "providerContinuation": "unresolved",
        "externalEffects": "unknown"
    }))?;
    ensure!(
        bytes.len() <= 65536,
        "native Core configuration exceeds capture budget"
    );
    Ok(bytes)
}

pub(super) fn persist_core_configuration(
    policy: &Connection,
    store: &ctox_sync::checkpoint::CheckpointStore,
    spec: &ExecutionSpec,
    receipt: &NativeSourceJournalReceipt,
    configuration: &ctox_core::ThreadConfigSnapshot,
) -> Result<ctox_sync::contracts::ArtifactRef> {
    let bytes = core_configuration_bytes(spec, configuration)?;
    let matches: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_native_source_journals
         WHERE capture_id=?1 AND job_id=?2 AND session_id=?3 AND spec_json=?4)",
        rusqlite::params![
            receipt.capture_id,
            spec.job_id,
            spec.session_id,
            serde_json::to_string(spec)?
        ],
        |row| row.get(0),
    )?;
    ensure!(
        matches && receipt.job_id == spec.job_id && receipt.session_id == spec.session_id,
        "native Core configuration belongs to another capture"
    );
    let artifact = ctox_sync::contracts::ArtifactRef {
        sha256: artifact_ref_for(&bytes).sha256,
        size_bytes: bytes.len() as u64,
    };
    store.ingest_blob(&artifact, std::io::Cursor::new(bytes))?;
    policy.execute(
        "INSERT INTO business_native_source_core_configurations
         (capture_id,format_version,artifact_sha256,artifact_size_bytes)
         VALUES (?1,1,?2,?3) ON CONFLICT(capture_id) DO NOTHING",
        rusqlite::params![
            receipt.capture_id,
            artifact.sha256,
            i64::try_from(artifact.size_bytes)?
        ],
    )?;
    let exact: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_native_source_core_configurations
         WHERE capture_id=?1 AND format_version=1 AND artifact_sha256=?2 AND artifact_size_bytes=?3)",
        rusqlite::params![receipt.capture_id, artifact.sha256, i64::try_from(artifact.size_bytes)?],
        |row| row.get(0),
    )?;
    ensure!(
        exact,
        "native Core configuration publication conflicts; reconcile"
    );
    Ok(artifact)
}

pub(super) fn validate_session_state(
    spec: &ExecutionSpec,
    state: &ctox_core::NativeSessionState,
) -> Result<()> {
    ensure!(
        spec.harness == ctox_core::native_harness_name()
            && spec.harness_version == ctox_core::native_harness_version()
            && spec.session_id == state.session_id().to_string()
            && spec.model_id == state.model()
            && spec.model_route_id == "openai"
            && spec.model_route_id == state.provider_id()
            && !state.as_bytes().is_empty()
            && state.as_bytes().len() <= 64 * 1024 * 1024,
        "native session state differs from admitted producer"
    );
    Ok(())
}

pub(super) fn persist_session_state(
    policy: &Connection,
    store: &ctox_sync::checkpoint::CheckpointStore,
    spec: &ExecutionSpec,
    receipt: &NativeSourceJournalReceipt,
    state: &ctox_core::NativeSessionState,
) -> Result<ctox_sync::contracts::ArtifactRef> {
    validate_session_state(spec, state)?;
    let matches: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_native_source_journals
         WHERE capture_id=?1 AND job_id=?2 AND session_id=?3 AND spec_json=?4)",
        rusqlite::params![
            receipt.capture_id,
            spec.job_id,
            spec.session_id,
            serde_json::to_string(spec)?
        ],
        |row| row.get(0),
    )?;
    ensure!(
        matches && receipt.job_id == spec.job_id && receipt.session_id == spec.session_id,
        "native session state belongs to another capture"
    );
    let artifact = ctox_sync::contracts::ArtifactRef {
        sha256: artifact_ref_for(state.as_bytes()).sha256,
        size_bytes: state.as_bytes().len() as u64,
    };
    store.ingest_blob(&artifact, std::io::Cursor::new(state.as_bytes()))?;
    policy.execute(
        "INSERT INTO business_native_source_session_states
         (capture_id,format_version,artifact_sha256,artifact_size_bytes)
         VALUES (?1,1,?2,?3) ON CONFLICT(capture_id) DO NOTHING",
        rusqlite::params![
            receipt.capture_id,
            artifact.sha256,
            i64::try_from(artifact.size_bytes)?
        ],
    )?;
    let exact: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_native_source_session_states
         WHERE capture_id=?1 AND format_version=1 AND artifact_sha256=?2 AND artifact_size_bytes=?3)",
        rusqlite::params![receipt.capture_id, artifact.sha256, i64::try_from(artifact.size_bytes)?],
        |row| row.get(0),
    )?;
    ensure!(
        exact,
        "native session state publication conflicts; reconcile"
    );
    Ok(artifact)
}

impl NativeGuestExecution {
    /// Only an actual Core journal reader and the exact retired producer can
    /// enter this worker -> account -> policy -> controller publication path.
    pub(crate) fn persist_source_journal(
        &self,
        source: &crate::channels::NativeProviderCaptureOwner,
        journal: &ctox_core::NativeJournalReader,
        configuration: &ctox_core::ThreadConfigSnapshot,
        session_state: &ctox_core::NativeSessionState,
    ) -> Result<NativeSourceJournalReceipt> {
        ensure!(
            source.matches_provider(&self.provider),
            "foreign native capture owner"
        );
        // Verify local capture authority before querying quorum. No worker,
        // issuer or policy lock survives the remote authority await.
        self.registry.require_live_transport()?;
        self.with_capture_authority(source, |_, _| Ok(()))?;
        #[cfg(target_os = "linux")]
        self.export_source_machine(source)?;
        let mut effects = source_effects::SourceEffects::observe(self)?;
        let (store, store_root, store_identity, bytes, mut receipt, workspace) = source
            .with_current_capture_transaction(|worker, facts| {
                self.with_held_worker_policy(worker, facts, |entry, verify, policy| {
                    verify()?;
                    effects.verify_controller(entry)?;
                    core_configuration_bytes(&self.binding.spec, configuration)?;
                    validate_session_state(&self.binding.spec, session_state)?;
                    let bytes = journal.read_bytes(PortableJournalLimits::default().max_bytes)?;
                    let (store, store_root, store_identity) =
                        source_store(&entry.assignment.destination.import_parent)?;
                    let receipt = persist(
                        policy,
                        &store,
                        &store_root,
                        &entry.assignment.destination,
                        &self.binding.spec,
                        &self.binding.ownership,
                        &self.binding.admission.policy_revision,
                        &bytes,
                    )?;
                    persist_core_configuration(
                        policy,
                        &store,
                        &self.binding.spec,
                        &receipt,
                        configuration,
                    )?;
                    persist_session_state(
                        policy,
                        &store,
                        &self.binding.spec,
                        &receipt,
                        session_state,
                    )?;
                    let workspace =
                        if workspaces::snapshot(policy, &entry.assignment.destination)?.is_some() {
                            Some(workspaces::require(
                                policy,
                                &entry.assignment.destination,
                                &configuration.cwd,
                            )?)
                        } else {
                            None
                        };
                    Ok((store, store_root, store_identity, bytes, receipt, workspace))
                })
            })?;
        // Actual Core is shut down; the registry retains the working-copy lease
        // and source child/export owner. Large Git/RAM/disk/hash IO holds no
        // account/issuer/worker/SQLite/controller publication guard.
        let prepared = workspace
            .map(|workspace| {
                source_checkpoint::prepare(
                    &store,
                    &store_root,
                    workspace,
                    &self.binding.spec,
                    &self.binding.ownership,
                    &receipt,
                    configuration,
                    session_state,
                    &bytes,
                    &effects,
                )
            })
            .transpose()?;
        let mut current = source_effects::SourceEffects::observe(self)?;
        source.with_current_capture_transaction(|worker, facts| {
            self.with_held_worker_policy(worker, facts, |entry, verify, policy| {
                self.registry.require_live_transport()?;
                verify()?;
                current.verify_controller(entry)?;
                ensure!(
                    effects.same_observation(&current),
                    "native source effects changed during capture; reconcile"
                );
                ensure!(
                    private_directory(&store_root)? == store_identity,
                    "native source artifact store changed; reconcile"
                );
                if let Some(prepared) = prepared {
                    receipt.checkpoint = Some(source_checkpoint::commit(
                        policy,
                        &store_root,
                        &entry.assignment.destination,
                        &self.binding.spec,
                        &self.binding.ownership,
                        &receipt,
                        prepared,
                    )?);
                }
                verify()?;
                // Private staged manifests are not authority. Only this fresh
                // commit publishes the checkpoint and releases its writer lease.
                entry.workspace_lease.take();
                Ok(receipt)
            })
        })
    }
}
