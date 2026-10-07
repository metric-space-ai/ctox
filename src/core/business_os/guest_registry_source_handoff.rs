// Origin: CTOX
// License: AGPL-3.0-only

//! Resolve only an immutable capture published by the stopped native owner.
use super::*;

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SourceHandoffFacts {
    pub capture_id: String,
    pub source_instance_id: String,
    pub owner_user_id: String,
    pub project_id: String,
    pub worker_profile_id: String,
    pub policy_revision: String,
    pub spec: ExecutionSpec,
    pub ownership: Ownership,
    pub checkpoint_digest: String,
    pub checkpoint_sequence: u64,
    pub source_working_copy_id: String,
    pub workspace_revision: u64,
}

/// Policy transaction must remain held through enrollment/decision. This
/// validates current native assignments, not current provider credentials or
/// target entitlement; those guards belong to the physical transfer owner.
pub(crate) fn resolve_source_handoff(
    root: &Path,
    policy: &Connection,
    capture_id: &str,
) -> Result<SourceHandoffFacts> {
    ensure!(identifier(capture_id), "invalid native capture ID");
    let row = policy
        .query_row(
            "SELECT j.guest_id,j.controller_id,j.controller_generation,j.owner_user_id,
        j.worker_profile_id,j.project_id,j.thread_id,j.source_instance_id,
        j.spec_json,j.ownership_json,j.policy_revision,j.artifact_store_path,
        c.checkpoint_digest,c.checkpoint_sequence,c.working_copy_id,c.workspace_revision
        FROM business_native_source_journals j
        JOIN business_native_source_checkpoints c ON c.capture_id=j.capture_id
        WHERE j.capture_id=?1 AND j.job_id=json_extract(j.spec_json,'$.jobId')
        AND j.session_id=json_extract(j.spec_json,'$.sessionId')
        AND j.ownership_generation=json_extract(j.ownership_json,'$.generation')",
            [capture_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, String>(10)?,
                    r.get::<_, String>(11)?,
                    r.get::<_, String>(12)?,
                    r.get::<_, i64>(13)?,
                    r.get::<_, String>(14)?,
                    r.get::<_, i64>(15)?,
                ))
            },
        )
        .optional()?
        .context("native capture has no complete checkpoint")?;
    let store_path = PathBuf::from(&row.11);
    let parent = store_path
        .parent()
        .context("native source parent missing")?;
    ensure!(
        parent.starts_with(root) && store_path == parent.join("source-journals"),
        "native source store belongs to another root"
    );
    let store_identity = private_directory(&store_path)?;
    private_directory(parent)?;
    // Never create a replacement instance identity during a decision.
    identity(&root.join("runtime/business-os-instance-id"))?;
    ensure!(
        stable_instance_id(root)? == row.7,
        "native source instance changed"
    );
    let d = GuestRestoreDestination {
        instance_id: row.7.clone(),
        guest_id: row.0,
        controller_id: row.1,
        controller_generation: u64::try_from(row.2)?,
        human_owner_id: row.3.clone(),
        worker_profile_id: row.4.clone(),
        project_id: row.5.clone(),
        thread_id: row.6,
        import_parent: parent.into(),
    };
    ensure!(
        validate_policy(policy, &d)? == row.10,
        "native source policy changed since capture; reconcile"
    );
    let spec: ExecutionSpec = serde_json::from_str(&row.8)?;
    let ownership: Ownership = serde_json::from_str(&row.9)?;
    ensure!(
        spec.harness == ctox_core::native_harness_name()
            && spec.harness_version == ctox_core::native_harness_version()
            && spec.model_route_id == "openai"
            && ownership.generation > 0,
        "native source producer contract is unsupported"
    );
    accounts::validate_provider(
        policy,
        &d,
        &spec.model_id,
        &crate::channels::NativeProviderCheckpointContract {
            harness: spec.harness.clone(),
            harness_version: spec.harness_version.clone(),
            model_route_id: spec.model_route_id.clone(),
            gateway_account_id: spec.gateway_account_id.clone(),
        },
    )?;
    let a = workspaces::snapshot(policy, &d)?.context("native source workspace missing")?;
    let path = PathBuf::from(
        a["nativeWorkspace"]
            .as_str()
            .context("native workspace path missing")?,
    );
    let workspace = workspaces::require(policy, &d, &path)?;
    ensure!(
        workspace.working_copy_id == row.14 && workspace.revision == u64::try_from(row.15)?,
        "native source workspace assignment changed"
    );
    let store = ctox_sync::checkpoint::CheckpointStore::open(store_path.clone(), 64 * 1024 * 1024)?;
    let manifest = store.load(&row.12)?;
    ensure!(
        manifest.sequence == u64::try_from(row.13)?
            && manifest.session.scope_id == spec.scope_id
            && manifest.session.session_id == spec.session_id
            && manifest.session.harness == spec.harness
            && manifest.session.harness_version == spec.harness_version
            && manifest.session.model_route_id == spec.model_route_id
            && manifest.session.gateway_account_id == spec.gateway_account_id
            && manifest.session.model_id == spec.model_id
            && manifest.session.required_capabilities == spec.required_capabilities,
        "native checkpoint differs from the captured producer"
    );
    workspace.verify()?;
    ensure!(
        private_directory(&store_path)? == store_identity,
        "native source store replaced"
    );
    Ok(SourceHandoffFacts {
        capture_id: capture_id.into(),
        source_instance_id: row.7,
        owner_user_id: row.3,
        project_id: row.5,
        worker_profile_id: row.4,
        policy_revision: row.10,
        spec,
        ownership,
        checkpoint_digest: row.12,
        checkpoint_sequence: u64::try_from(row.13)?,
        source_working_copy_id: row.14,
        workspace_revision: u64::try_from(row.15)?,
    })
}
