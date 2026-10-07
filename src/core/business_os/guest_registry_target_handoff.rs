// Origin: CTOX
// License: AGPL-3.0-only
//! Native target policy scopes contain logical chat IDs, never guest/process claims.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TargetRepositoryAssignment {
    pub owner_user_id: String,
    pub worker_profile_id: String,
    pub project_id: String,
    pub working_copy_id: String,
    pub repository_id: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TargetPolicyScope {
    pub owner_user_id: String,
    pub worker_profile_id: String,
    pub project_id: String,
    pub thread_id: String,
    pub working_copy_id: String,
    pub repository_id: String,
    pub policy_revision: String,
}

/// Separate trusted local provisioning. A target intake cannot issue this mapping.
pub(crate) fn configure_repository(root: &Path, encoded: &str) -> Result<()> {
    let input: TargetRepositoryAssignment = serde_json::from_str(encoded)?;
    for id in [
        &input.owner_user_id,
        &input.worker_profile_id,
        &input.project_id,
        &input.working_copy_id,
        &input.repository_id,
    ] {
        ensure!(
            identifier(id),
            "invalid native target repository assignment"
        );
    }
    let mut policy = super::super::store::open_store(root)?;
    let tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let workspace = workspaces::snapshot_scope(
        &tx,
        &input.owner_user_id,
        &input.worker_profile_id,
        &input.project_id,
    )?
    .context("target workspace is not assigned")?;
    let path = workspace["nativeWorkspace"]
        .as_str()
        .context("native workspace missing")?;
    let assigned = workspaces::require_scope(
        &tx,
        &input.owner_user_id,
        &input.worker_profile_id,
        &input.project_id,
        Path::new(path),
    )?;
    ensure!(
        assigned.working_copy_id == input.working_copy_id,
        "target repository working copy mismatched"
    );
    let epoch: i64 = tx.query_row(
        "SELECT capability_epoch FROM business_users WHERE user_id=?1 AND active=1",
        [&input.owner_user_id],
        |r| r.get(0),
    )?;
    tx.execute("INSERT INTO business_native_target_repository_assignments
        (owner_user_id,worker_profile_id,project_id,working_copy_id,repository_id,principal_epoch,revision)
        VALUES (?1,?2,?3,?4,?5,?6,1)
        ON CONFLICT(owner_user_id,worker_profile_id,project_id) DO UPDATE SET
        working_copy_id=excluded.working_copy_id,repository_id=excluded.repository_id,
        principal_epoch=excluded.principal_epoch,
        revision=business_native_target_repository_assignments.revision+1",
        rusqlite::params![input.owner_user_id,input.worker_profile_id,input.project_id,
            input.working_copy_id,input.repository_id,epoch])?;
    super::super::store::insert_business_event(
        &tx,
        "business_native_target_repository_assignments",
        &input.project_id,
        "business_os.session_handoff.target_repository_configured",
        serde_json::json!({"version":1,"owner_user_id":input.owner_user_id,
            "worker_profile_id":input.worker_profile_id,"working_copy_id":input.working_copy_id,
            "repository_id":input.repository_id}),
        super::super::store::now_ms() as i64,
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn resolve(
    root: &Path,
    policy: &Connection,
    requested: &TargetPolicyScope,
    spec: &ExecutionSpec,
) -> Result<TargetPolicyScope> {
    for id in [
        &requested.owner_user_id,
        &requested.worker_profile_id,
        &requested.project_id,
        &requested.thread_id,
        &requested.working_copy_id,
        &requested.repository_id,
    ] {
        ensure!(identifier(id), "invalid target scope");
    }
    let snapshot = policy_snapshot_scope(
        policy,
        &requested.owner_user_id,
        &requested.worker_profile_id,
        &requested.project_id,
        &requested.thread_id,
    )?;
    let settings = Connection::open_with_flags(
        crate::inference::runtime_env::runtime_config_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let encoded: String = settings.query_row(
        "SELECT env_value FROM runtime_env_kv WHERE env_key='native_guest_host_config'",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        encoded.len() <= 64 * 1024,
        "native target configuration exceeds its bound"
    );
    let config: serde_json::Value = serde_json::from_str(&encoded)?;
    let configured_capabilities: BTreeSet<String> =
        serde_json::from_value(config["requiredCapabilities"].clone())?;
    ensure!(
        config["version"] == 1
            && snapshot[1]["computer_id"] == config["computerId"]
            && !configured_capabilities.is_empty()
            && spec
                .required_capabilities
                .is_subset(&configured_capabilities),
        "target profile computer or capabilities are not locally configured"
    );
    let provider = accounts::require_scope(
        policy,
        &requested.owner_user_id,
        &requested.worker_profile_id,
    )?;
    ensure!(
        provider["gateway_account_id"] == spec.gateway_account_id
            && provider["model_id"] == spec.model_id
            && provider["model_route_id"] == spec.model_route_id
            && provider["harness"] == spec.harness
            && provider["harness_version"] == spec.harness_version,
        "target does not have the captured account/model/harness entitlement"
    );
    let workspace = workspaces::snapshot_scope(
        policy,
        &requested.owner_user_id,
        &requested.worker_profile_id,
        &requested.project_id,
    )?
    .context("target workspace missing")?;
    let path = Path::new(
        workspace["nativeWorkspace"]
            .as_str()
            .context("target native workspace missing")?,
    );
    let assigned = workspaces::require_scope(
        policy,
        &requested.owner_user_id,
        &requested.worker_profile_id,
        &requested.project_id,
        path,
    )?;
    ensure!(
        assigned.working_copy_id == requested.working_copy_id,
        "target working copy is not assigned"
    );
    let repository:(String,String,i64,i64,i64)=policy.query_row(
        "SELECT r.working_copy_id,r.repository_id,r.principal_epoch,r.revision,u.capability_epoch
        FROM business_native_target_repository_assignments r JOIN business_users u ON u.user_id=r.owner_user_id
        WHERE r.owner_user_id=?1 AND r.worker_profile_id=?2 AND r.project_id=?3 AND u.active=1",
        rusqlite::params![requested.owner_user_id,requested.worker_profile_id,requested.project_id],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    ensure!(
        repository.0 == requested.working_copy_id
            && repository.1 == requested.repository_id
            && repository.2 == repository.4
            && repository.3 > 0,
        "target repository mapping is absent or stale"
    );
    let mut current = requested.clone();
    current.policy_revision =
        source_policy::revision(&serde_json::json!([snapshot, config, repository]))?;
    assigned.verify()?;
    Ok(current)
}
