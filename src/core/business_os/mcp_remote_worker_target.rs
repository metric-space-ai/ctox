// Origin: CTOX
// License: AGPL-3.0-only
//! Explicit source-owner pairing of Workjet registry identities and native computers.
//! The authenticated source Broker supplies its current registry facts. Native
//! computer IDs stay opaque; neither labels nor environment IDs invent them.
use super::super::super::computer_capabilities::{
    validate_capabilities, BuildCapability, ComputerCapability,
};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enrollment_digest: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnrollmentTarget {
    source_environment_id: String,
    target_environment_id: String,
    target_connection_id: String,
    target_instance_id: String,
}
impl EnrollmentTarget {
    fn validate(&self) -> anyhow::Result<()> {
        for value in [
            &self.source_environment_id,
            &self.target_environment_id,
            &self.target_connection_id,
            &self.target_instance_id,
        ] {
            label(value)?;
        }
        anyhow::ensure!(
            self.source_environment_id != self.target_environment_id,
            "remote target must differ from the source environment"
        );
        Ok(())
    }
    fn bind(self, computer: String) -> Target {
        Target {
            source_environment_id: self.source_environment_id,
            target_environment_id: self.target_environment_id,
            target_connection_id: self.target_connection_id,
            target_instance_id: self.target_instance_id,
            target_computer_id: computer,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnrollmentComputer {
    display_name: String,
    hosting_mode: String,
    build_capability: BuildCapability,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    EnrollTarget {
        target: EnrollmentTarget,
        computer: EnrollmentComputer,
    },
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
        Some("enroll_target" | "register_target" | "resolve_target" | "revoke_target")
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
    let (role, epoch) = current_actor(&tx, context)?;
    let enrolling = matches!(&request, Request::EnrollTarget { .. });
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
        Request::EnrollTarget { target, .. } => {
            label(&target.target_environment_id)?;
            target.target_environment_id.as_str()
        }
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
        Request::EnrollTarget { target, computer } => {
            target.validate()?;
            let intent = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&(&target, &computer))?)
            );
            let mut config = vec![ComputerCapability::Build(computer.build_capability.clone())];
            validate_capabilities(&mut config, false)?;
            let mut target = target.bind(String::new());
            if let Some(old) = existing {
                target.target_computer_id = old.target.target_computer_id.clone();
                target.validate()?;
                anyhow::ensure!(old.state == "active" && old.target == target
                    && old.enrollment_digest.as_deref() == Some(intent.as_str()),
                    "worker enrollment differs from the original request; explicit assignment required");
                let current = current_computer(&tx, context, &target.target_computer_id)?;
                anyhow::ensure!(
                    current["capability_config"] == serde_json::to_value(&config)?
                        && current["hosting_mode"] == computer.hosting_mode,
                    "native worker enrollment settings changed; reconcile"
                );
                old
            } else {
                // Validate registry scope before creating either durable record.
                let native = super::super::super::store_workjet_computers::enroll_worker_computer(
                    &tx,
                    &context.actor,
                    &computer.display_name,
                    &computer.hosting_mode,
                    computer.build_capability,
                )?;
                target.target_computer_id = native["id"]
                    .as_str()
                    .context("native enrollment did not issue a computer identity")?
                    .to_owned();
                target.validate()?;
                Registration {
                    contract: CONTRACT.into(),
                    binding_id: uuid::Uuid::new_v4().to_string(),
                    owner_user_id: context.actor.clone(),
                    source_instance_id: context.workspace.clone(),
                    target,
                    revision: 1,
                    state: "active".into(),
                    enrollment_digest: Some(intent),
                }
            }
        }
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
                    old.enrollment_digest = None;
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
                    enrollment_digest: None,
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
    if enrolling {
        // Projection follows committed authority. Recheck under a fresh native
        // transaction so rollback/unpair/revocation cannot publish a phantom.
        let projection = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (current_role, current_epoch) = current_actor(&projection, context)?;
        anyhow::ensure!(
            current_role == role && current_epoch == epoch,
            "native worker enrollment authority changed before projection"
        );
        anyhow::ensure!(
            super::super::super::store_policy::trusted_actor_policy_decision_with_conn(
                &projection,
                &context.actor,
                &current_role,
                BusinessOsPermission::IntegrationsManage,
                BusinessOsScopeType::Workspace,
                None
            )?
            .allowed,
            "native enrollment projection denied"
        );
        let current = load(&projection, context, &record.target.target_environment_id)?
            .context("native enrollment disappeared before projection")?;
        anyhow::ensure!(
            current.state == "active"
                && current.binding_id == record.binding_id
                && current.revision == record.revision
                && current.target == record.target,
            "native enrollment changed before projection"
        );
        let computer = current_computer(&projection, context, &record.target.target_computer_id)?;
        anyhow::ensure!(
            computer["capability_config"]
                .as_array()
                .and_then(|items| items.iter().find(|item| item["kind"] == "build"))
                == Some(&response["buildCapability"]),
            "native build settings changed before projection"
        );
        super::super::super::store_workjet_computers::project_computer_record(root, &computer)?;
        projection.commit()?;
    }
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
