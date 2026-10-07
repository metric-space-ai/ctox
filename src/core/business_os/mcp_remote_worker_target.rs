// Origin: CTOX
// License: AGPL-3.0-only
//! Explicit source-owner pairing of Workjet registry identities and native computers.
//! The authenticated source Broker supplies its current registry facts. Native
//! computer IDs stay opaque; neither labels nor environment IDs invent them.
use super::*;

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_remote_worker_targets (
    owner_user_id TEXT NOT NULL, source_instance_id TEXT NOT NULL,
    target_environment_id TEXT NOT NULL, record_json TEXT NOT NULL,
    PRIMARY KEY(owner_user_id,source_instance_id,target_environment_id));";
const CONTRACT: &str = "ctox.workjet.remote-worker-target.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Target {
    source_environment_id: String,
    target_environment_id: String,
    target_connection_id: String,
    target_instance_id: String,
    target_computer_id: String,
}
impl Target {
    fn validate(&self) -> anyhow::Result<()> {
        for value in [
            &self.source_environment_id,
            &self.target_environment_id,
            &self.target_connection_id,
            &self.target_instance_id,
            &self.target_computer_id,
        ] {
            label(value)?;
        }
        anyhow::ensure!(
            self.source_environment_id != self.target_environment_id,
            "remote target must differ from the source environment"
        );
        Ok(())
    }
    fn matches(&self, binding: &Binding) -> bool {
        self.source_environment_id == binding.source_environment_id
            && self.target_environment_id == binding.target_environment_id
            && self.target_connection_id == binding.target_connection_id
            && self.target_instance_id == binding.target_instance_id
            && self.target_computer_id == binding.target_computer_id
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Registration {
    contract: String,
    binding_id: String,
    owner_user_id: String,
    source_instance_id: String,
    target: Target,
    revision: u64,
    state: String,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    RegisterTarget {
        target: Target,
        expected_revision: Option<u64>,
    },
    ResolveTarget {
        target_environment_id: String,
    },
    RevokeTarget {
        target_environment_id: String,
        expected_revision: u64,
    },
}

pub(super) fn handles(arguments: &Value) -> bool {
    matches!(
        arguments["action"].as_str(),
        Some("register_target" | "resolve_target" | "revoke_target")
    )
}

pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    let request: Request = serde_json::from_value(arguments.clone()).map_err(|_| {
        BusinessOsMcpError::validation(
            "remote_worker_target",
            "invalid worker target registration request",
        )
    })?;
    let mut conn = store::open_store(root)?;
    conn.execute_batch(SCHEMA)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (role, _) = current_actor(&tx, context)?;
    let retiring = matches!(&request, Request::RevokeTarget { .. });
    if !retiring {
        anyhow::ensure!(
            super::super::super::store_policy::trusted_actor_policy_decision_with_conn(
                &tx,
                &context.actor,
                &role,
                BusinessOsPermission::IntegrationsManage,
                BusinessOsScopeType::Workspace,
                None
            )?
            .allowed,
            "native worker target registration policy denied"
        );
    }
    let environment = match &request {
        Request::RegisterTarget { target, .. } => {
            target.validate()?;
            target.target_environment_id.as_str()
        }
        Request::ResolveTarget {
            target_environment_id,
        }
        | Request::RevokeTarget {
            target_environment_id,
            ..
        } => {
            label(target_environment_id)?;
            target_environment_id.as_str()
        }
    };
    let existing = load(&tx, context, environment)?;
    let mut record = match request {
        Request::RegisterTarget {
            target,
            expected_revision,
        } => {
            current_computer(&tx, context, &target.target_computer_id)?;
            if let Some(mut old) = existing {
                // Same live tuple is a lost-ACK retry, never a revision change.
                if old.state == "active" && old.target == target {
                    anyhow::ensure!(
                        expected_revision.is_none()
                            || expected_revision == Some(old.revision)
                            || expected_revision.and_then(|r| r.checked_add(1))
                                == Some(old.revision),
                        "worker target registration revision is stale"
                    );
                    old
                } else {
                    anyhow::ensure!(
                        expected_revision == Some(old.revision),
                        "worker target replacement requires its exact current revision"
                    );
                    old.revision = old
                        .revision
                        .checked_add(1)
                        .context("worker target revision exhausted")?;
                    old.target = target;
                    old.state = "active".into();
                    old
                }
            } else {
                anyhow::ensure!(
                    expected_revision.is_none(),
                    "worker target registration does not exist"
                );
                Registration {
                    contract: CONTRACT.into(),
                    binding_id: uuid::Uuid::new_v4().to_string(),
                    owner_user_id: context.actor.clone(),
                    source_instance_id: context.workspace.clone(),
                    target,
                    revision: 1,
                    state: "active".into(),
                }
            }
        }
        Request::ResolveTarget { .. } => {
            let record =
                existing.context("worker target is not registered for this source owner")?;
            anyhow::ensure!(
                record.state == "active",
                "worker target registration is revoked"
            );
            current_computer(&tx, context, &record.target.target_computer_id)?;
            record
        }
        Request::RevokeTarget {
            expected_revision, ..
        } => {
            let mut record =
                existing.context("worker target is not registered for this source owner")?;
            if record.state == "revoked" {
                anyhow::ensure!(
                    expected_revision == record.revision
                        || expected_revision.checked_add(1) == Some(record.revision),
                    "worker target revocation revision is stale"
                );
            } else {
                anyhow::ensure!(
                    expected_revision == record.revision,
                    "worker target revocation requires its exact current revision"
                );
                record.revision = record
                    .revision
                    .checked_add(1)
                    .context("worker target revision exhausted")?;
                record.state = "revoked".into();
            }
            record
        }
    };
    // No payload can select a foreign native owner or source instance.
    record.owner_user_id = context.actor.clone();
    record.source_instance_id = context.workspace.clone();
    tx.execute(
        "INSERT INTO workjet_remote_worker_targets
        (owner_user_id,source_instance_id,target_environment_id,record_json)
        VALUES(?1,?2,?3,?4)
        ON CONFLICT(owner_user_id,source_instance_id,target_environment_id)
        DO UPDATE SET record_json=excluded.record_json",
        params![
            context.actor,
            context.workspace,
            record.target.target_environment_id,
            serde_json::to_string(&record)?
        ],
    )?;
    let mut response = serde_json::to_value(&record)?;
    if record.state == "active" {
        let computer = current_computer(&tx, context, &record.target.target_computer_id)?;
        response["capabilityEpoch"] = computer["capability_epoch"].clone();
        // Typed operational eligibility, not a free-slot/readiness observation.
        response["buildCapability"] = computer["capability_config"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["kind"] == "build"))
            .cloned()
            .context("current native build capability missing")?;
    }
    tx.commit()?;
    Ok(response)
}

fn load(
    conn: &Connection,
    context: &McpChannelRequestContext,
    environment: &str,
) -> anyhow::Result<Option<Registration>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT record_json FROM workjet_remote_worker_targets
        WHERE owner_user_id=?1 AND source_instance_id=?2 AND target_environment_id=?3",
            params![context.actor, context.workspace, environment],
            |row| row.get(0),
        )
        .optional()?;
    raw.map(|raw| {
        let record: Registration = serde_json::from_str(&raw)?;
        anyhow::ensure!(
            record.contract == CONTRACT
                && record.owner_user_id == context.actor
                && record.source_instance_id == context.workspace
                && record.target.target_environment_id == environment
                && record.revision > 0
                && matches!(record.state.as_str(), "active" | "revoked"),
            "native worker target registration is inconsistent"
        );
        record.target.validate()?;
        Ok(record)
    })
    .transpose()
}

/// Called inside the same native policy transaction as permit publication.
pub(super) fn current_binding(
    conn: &Connection,
    context: &McpChannelRequestContext,
    binding: &Binding,
) -> anyhow::Result<(String, u64)> {
    let record = load(conn, context, &binding.target_environment_id)?
        .context("worker target has no explicit native registration")?;
    anyhow::ensure!(
        record.state == "active" && record.target.matches(binding),
        "worker target tuple differs from its current native registration"
    );
    Ok((record.binding_id, record.revision))
}
