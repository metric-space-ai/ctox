// Origin: CTOX
// License: AGPL-3.0-only

//! Versioned Owner intent, not provider/tool admission. Execution consumers must
//! revalidate the native project/team/worktree and this revision before acting.

use super::workjet_project_execution_policy_contract::{
    ProjectExecutionPolicy, ProjectExecutionPolicyMode, ProjectExecutionPolicySchema,
    ProjectExecutionPolicyUpdate, WireValidate,
};
use anyhow::Context;
use serde_json::Value;

pub(super) fn current(project: Option<&Value>) -> anyhow::Result<ProjectExecutionPolicy> {
    let Some(value) = project.and_then(|project| project.get("execution_policy")) else {
        return Ok(ProjectExecutionPolicy {
            schema: ProjectExecutionPolicySchema::CtoxWorkjetProjectExecutionPolicyV1,
            mode: ProjectExecutionPolicyMode::Default,
            revision: 0,
        });
    };
    let policy: ProjectExecutionPolicy = serde_json::from_value(value.clone())
        .context("invalid stored Workjet project execution_policy")?;
    policy.validate().map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        policy.mode != ProjectExecutionPolicyMode::AutonomousWorktree || policy.revision > 0,
        "invalid stored autonomous_worktree policy without a native revision"
    );
    Ok(policy)
}

/// Called only inside the existing Owner-checked domain writer transaction.
pub(super) fn apply(
    existing: Option<&Value>,
    requested: &ProjectExecutionPolicyUpdate,
) -> anyhow::Result<Value> {
    requested.validate().map_err(anyhow::Error::msg)?;
    let mut policy = current(existing)?;
    anyhow::ensure!(
        requested.expected_revision == policy.revision,
        "workjet_project_execution_policy_revision_conflict: expected {}, current {}",
        requested.expected_revision,
        policy.revision
    );
    if requested.mode != policy.mode {
        policy.revision = policy
            .revision
            .checked_add(1)
            .filter(|revision| *revision <= 9_007_199_254_740_991)
            .context("Workjet project execution_policy revision exhausted")?;
        policy.mode = requested.mode;
    }
    Ok(serde_json::to_value(policy)?)
}
