// Origin: CTOX
// License: AGPL-3.0-only

//! Source-native admission for a Workjet leaf worker. An opaque permit is a
//! locator, never a target bearer credential: every operation returns through
//! the source's authenticated managed MCP connection and current policy store.
use super::*;
use rusqlite::{Connection, TransactionBehavior};
use sha2::{Digest, Sha256};

pub(super) const TOOL: &str = "business_os.remote_worker_admission";
const CONTRACT: &str = "ctox.workjet.remote-worker-admission.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CredentialRef {
    environment_id: String,
    account_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderRef {
    environment_id: String,
    provider: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelRef {
    environment_id: String,
    provider: String,
    model_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Capability {
    RepositoryRead,
    RepositoryWrite,
    RunChecks,
    OpenPullRequest,
}

/// External Workjet identities are exact bindings, not native identities. The
/// source Broker verifies the supervisor and Git HEAD; the target Receiver
/// verifies its paired environment and confines workspaceKey to its private
/// worker-worktree directory. Native authority is the current project owner,
/// task policy and assigned computer, not a presentation chip or SSH access.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding {
    request_id: String,
    request_digest: String,
    source_environment_id: String,
    source_supervisor_thread_id: String,
    source_instance_id: String,
    project_id: String,
    target_environment_id: String,
    target_connection_id: String,
    target_instance_id: String,
    target_computer_id: String,
    repository_url: String,
    repository_head: String,
    workspace_key: String,
    credential_ref: CredentialRef,
    provider_ref: ProviderRef,
    model_ref: ModelRef,
    capabilities: Vec<Capability>,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Issue {
        binding: Binding,
        ttl_seconds: u16,
    },
    Claim {
        permit_id: String,
        binding: Binding,
        execution_id: String,
    },
    Revalidate {
        permit_id: String,
        binding: Binding,
        execution_id: String,
    },
    Revoke {
        permit_id: String,
        binding: Binding,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    contract: String,
    permit_id: String,
    owner_user_id: String,
    authority_epoch: i64,
    authority_fingerprint: String,
    expires_at_ms: i64,
    binding: Binding,
    state: String,
    execution_id: Option<String>,
}

pub(super) fn descriptor() -> BusinessOsMcpToolDescriptor {
    write_tool(TOOL,
        "Issue, claim, revalidate or revoke one source-native remote Workjet leaf-worker admission. Requires the current authenticated source Owner/Admin, owned active project and assigned computer. Revalidation returns through the source; the target receives no Owner bearer or model secret. Model references bind scope but do not replace the source gateway's current account grant.",
        serde_json::json!({"type":"object", "additionalProperties":false,
            "required":["action","binding"],
            "properties": {
                "action":{"type":"string","enum":["issue","claim","revalidate","revoke"]},
                "permit_id":{"type":"string"}, "execution_id":{"type":"string"},
                "ttl_seconds":{"type":"integer","minimum":1,"maximum":300},
                "binding":{"type":"object","additionalProperties":false,
                    "required":["requestId","requestDigest","sourceEnvironmentId","sourceSupervisorThreadId","sourceInstanceId","projectId","targetEnvironmentId","targetConnectionId","targetInstanceId","targetComputerId","repositoryUrl","repositoryHead","workspaceKey","credentialRef","providerRef","modelRef","capabilities"],
                    "properties": {
                        "requestId":{"type":"string"},"requestDigest":{"type":"string"},
                        "sourceEnvironmentId":{"type":"string"},"sourceSupervisorThreadId":{"type":"string"},
                        "sourceInstanceId":{"type":"string"},"projectId":{"type":"string"},
                        "targetEnvironmentId":{"type":"string"},"targetConnectionId":{"type":"string"},
                        "targetInstanceId":{"type":"string"},"targetComputerId":{"type":"string"},
                        "repositoryUrl":{"type":"string"},"repositoryHead":{"type":"string"},"workspaceKey":{"type":"string"},
                        "credentialRef":ref_schema(&["environmentId","accountId"]),
                        "providerRef":ref_schema(&["environmentId","provider"]),
                        "modelRef":ref_schema(&["environmentId","provider","modelId"]),
                        "capabilities":{"type":"array","minItems":1,"maxItems":4,"uniqueItems":true,
                            "items":{"type":"string","enum":["repository_read","repository_write","run_checks","open_pull_request"]}}
                    }}
            }}))
}

fn ref_schema(fields: &[&str]) -> Value {
    let properties = fields
        .iter()
        .map(|field| ((*field).to_owned(), serde_json::json!({"type":"string"})))
        .collect::<serde_json::Map<_, _>>();
    serde_json::json!({"type":"object","additionalProperties":false,"required":fields,"properties":properties})
}

pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    // A caller-supplied _context cannot manufacture this marker. Internal
    // command sessions cannot mint independent work, even when owner-backed.
    anyhow::ensure!(
        context.trusted_role_source.as_deref() == Some("ctox_dev_managed_mcp_token")
            && context.channel == "ctox_dev_managed_mcp"
            && matches!(context.trusted_role.as_deref(), Some("chef" | "admin")),
        "remote worker admission requires authenticated source Owner/Admin MCP authority"
    );
    let request: Request = serde_json::from_value(arguments.clone())
        .context("invalid remote worker admission request")?;
    let binding = match &request {
        Request::Issue { binding, .. }
        | Request::Claim { binding, .. }
        | Request::Revalidate { binding, .. }
        | Request::Revoke { binding, .. } => binding,
    };
    validate_binding(binding)?;
    anyhow::ensure!(
        binding.source_instance_id == context.workspace,
        "remote worker source instance differs from authenticated source"
    );
    let mut conn = store::open_store(root)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workjet_remote_worker_admissions (
        permit_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, source_instance_id TEXT NOT NULL,
        request_id TEXT NOT NULL, receipt_json TEXT NOT NULL,
        UNIQUE(owner_user_id,source_instance_id,request_id));",
    )?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (epoch, fingerprint) = current_authority(&tx, context, binding)?;
    let now = now_ms();
    let mut receipt = match &request {
        Request::Issue { ttl_seconds, .. } => {
            anyhow::ensure!(
                (1..=300).contains(ttl_seconds),
                "worker permit TTL must be 1..300 seconds"
            );
            let existing: Option<String> = tx.query_row(
                "SELECT receipt_json FROM workjet_remote_worker_admissions WHERE owner_user_id=?1 AND source_instance_id=?2 AND request_id=?3",
                params![context.actor, binding.source_instance_id, binding.request_id], |row| row.get(0)).optional()?;
            if let Some(existing) = existing {
                let receipt: Receipt = serde_json::from_str(&existing)?;
                verify_receipt(&receipt, context, binding, epoch, &fingerprint, now)?;
                receipt
            } else {
                Receipt {
                    contract: CONTRACT.to_owned(),
                    permit_id: uuid::Uuid::new_v4().to_string(),
                    owner_user_id: context.actor.clone(),
                    authority_epoch: epoch,
                    authority_fingerprint: fingerprint.clone(),
                    expires_at_ms: now + i64::from(*ttl_seconds) * 1000,
                    binding: binding.clone(),
                    state: "issued".to_owned(),
                    execution_id: None,
                }
            }
        }
        Request::Claim { permit_id, .. }
        | Request::Revalidate { permit_id, .. }
        | Request::Revoke { permit_id, .. } => {
            label(permit_id)?;
            let raw: String = tx.query_row(
                "SELECT receipt_json FROM workjet_remote_worker_admissions WHERE permit_id=?1 AND owner_user_id=?2 AND source_instance_id=?3",
                params![permit_id, context.actor, context.workspace], |row| row.get(0))
                .context("worker permit is unavailable to this source owner")?;
            let receipt: Receipt = serde_json::from_str(&raw)?;
            verify_receipt(&receipt, context, binding, epoch, &fingerprint, now)?;
            receipt
        }
    };
    match request {
        Request::Claim { execution_id, .. } => {
            label(&execution_id)?;
            anyhow::ensure!(
                receipt
                    .execution_id
                    .as_ref()
                    .is_none_or(|id| id == &execution_id),
                "worker permit was claimed by a different execution"
            );
            receipt.execution_id = Some(execution_id);
            receipt.state = "claimed".to_owned();
        }
        Request::Revalidate { execution_id, .. } => {
            label(&execution_id)?;
            anyhow::ensure!(
                receipt.state == "claimed" && receipt.execution_id.as_ref() == Some(&execution_id),
                "worker permit is not claimed by this execution"
            );
        }
        Request::Revoke { .. } => receipt.state = "revoked".to_owned(),
        Request::Issue { .. } => {}
    }
    let response = serde_json::to_value(&receipt)?;
    tx.execute(
        "INSERT INTO workjet_remote_worker_admissions
        (permit_id,owner_user_id,source_instance_id,request_id,receipt_json) VALUES(?1,?2,?3,?4,?5)
        ON CONFLICT(permit_id) DO UPDATE SET receipt_json=excluded.receipt_json",
        params![
            receipt.permit_id,
            receipt.owner_user_id,
            receipt.binding.source_instance_id,
            receipt.binding.request_id,
            serde_json::to_string(&receipt)?
        ],
    )?;
    // All native authority reads and the single claim linearize in this policy
    // transaction. This response is not an offline authorization or a fence
    // over a remote spawn; Receiver/gateway must revalidate at their boundaries.
    tx.commit()?;
    Ok(response)
}

fn verify_receipt(
    receipt: &Receipt,
    context: &McpChannelRequestContext,
    binding: &Binding,
    epoch: i64,
    fingerprint: &str,
    now: i64,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        receipt.contract == CONTRACT
            && receipt.owner_user_id == context.actor
            && &receipt.binding == binding,
        "worker permit immutable binding differs"
    );
    anyhow::ensure!(
        receipt.authority_epoch == epoch && receipt.authority_fingerprint == fingerprint,
        "worker source authority changed; stale permit rejected"
    );
    anyhow::ensure!(
        now < receipt.expires_at_ms && receipt.state != "revoked",
        "worker permit expired or revoked"
    );
    Ok(())
}

fn current_authority(
    conn: &Connection,
    context: &McpChannelRequestContext,
    binding: &Binding,
) -> anyhow::Result<(i64, String)> {
    let (role, active, epoch): (String, bool, i64) = conn
        .query_row(
            "SELECT role,active,capability_epoch FROM business_users WHERE user_id=?1",
            params![context.actor],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .context("worker source actor is not a persisted native user")?;
    anyhow::ensure!(
        active
            && matches!(role.as_str(), "chef" | "admin")
            && context.trusted_role.as_deref() == Some(role.as_str()),
        "worker source actor authority is no longer current"
    );
    for (permission, scope, id) in [
        (
            BusinessOsPermission::CtoxTaskCreate,
            BusinessOsScopeType::Record,
            Some(binding.project_id.as_str()),
        ),
        (
            BusinessOsPermission::IntegrationsManage,
            BusinessOsScopeType::Workspace,
            None,
        ),
    ] {
        anyhow::ensure!(
            super::super::store_policy::trusted_actor_policy_decision_with_conn(
                conn,
                &context.actor,
                &role,
                permission,
                scope,
                id
            )?
            .allowed,
            "native worker admission policy denied"
        );
    }
    let project = super::super::project_chats::owned_project(
        conn,
        &binding.project_id,
        &context.actor,
        true,
    )?;
    let native_repo = project["repo_url"]
        .as_str()
        .context("owned project has no repository binding")?;
    anyhow::ensure!(
        repository_key(native_repo)? == repository_key(&binding.repository_url)?,
        "worker repository differs from native owned project"
    );
    let computer =
        store::outbound_load_record(conn, "workjet_computers", &binding.target_computer_id)?
            .context("worker target computer is not registered")?;
    anyhow::ensure!(
        computer["owner_user_id"] == context.actor
            && computer["status"] == "assigned"
            && computer["is_deleted"] != true
            && computer["_deleted"] != true
            && computer["agentless"] != true
            && matches!(
                computer["hosting_mode"].as_str(),
                Some("workstation" | "self_hosted")
            ),
        "worker target is not a current assigned computer of this owner"
    );
    let fingerprint = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&serde_json::json!({
            "owner":context.actor,"epoch":epoch,"instance":context.workspace,
            "project":binding.project_id,"repository":repository_key(native_repo)?,
            "computer":binding.target_computer_id,"hostingMode":computer["hosting_mode"],
            "capabilityEpoch":computer["capability_epoch"],"capabilityConfig":computer["capability_config"]
        }))?)
    );
    Ok((epoch, fingerprint))
}

fn label(value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 256
            && value == value.trim()
            && !value.chars().any(char::is_control),
        "invalid worker binding identifier"
    );
    Ok(())
}
fn hex_digest(value: &str, sizes: &[usize]) -> bool {
    sizes.contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn repository_key(value: &str) -> anyhow::Result<String> {
    let url =
        url::Url::parse(value).context("worker repository must be a credential-free HTTPS URL")?;
    anyhow::ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "invalid worker repository URL"
    );
    let path = url.path().trim_end_matches('/').trim_end_matches(".git");
    anyhow::ensure!(
        !path.is_empty() && path != "/",
        "worker repository path is missing"
    );
    Ok(format!(
        "https://{}{}{}",
        url.host_str().unwrap(),
        url.port().map(|p| format!(":{p}")).unwrap_or_default(),
        path
    ))
}
fn validate_binding(binding: &Binding) -> anyhow::Result<()> {
    for value in [
        &binding.request_id,
        &binding.source_environment_id,
        &binding.source_supervisor_thread_id,
        &binding.source_instance_id,
        &binding.project_id,
        &binding.target_environment_id,
        &binding.target_connection_id,
        &binding.target_instance_id,
        &binding.target_computer_id,
        &binding.credential_ref.environment_id,
        &binding.credential_ref.account_id,
        &binding.provider_ref.environment_id,
        &binding.provider_ref.provider,
        &binding.model_ref.environment_id,
        &binding.model_ref.provider,
        &binding.model_ref.model_id,
    ] {
        label(value)?;
    }
    anyhow::ensure!(
        binding.source_environment_id != binding.target_environment_id,
        "remote worker target must be a different environment"
    );
    anyhow::ensure!(
        hex_digest(&binding.request_digest, &[64])
            && hex_digest(&binding.repository_head, &[40, 64]),
        "worker request digest or Git HEAD is invalid"
    );
    // No client may turn a worker workspace key into a path, symlink or shell
    // fragment. The target owns the actual private worktree root and checks it.
    anyhow::ensure!(
        binding.workspace_key == binding.request_id
            && binding.workspace_key.len() <= 128
            && binding
                .workspace_key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "worker workspace key must be the path-free request identity"
    );
    repository_key(&binding.repository_url)?;
    anyhow::ensure!(
        binding.credential_ref.environment_id == binding.source_environment_id
            && binding.provider_ref.environment_id == binding.source_environment_id
            && binding.model_ref.environment_id == binding.source_environment_id
            && binding.provider_ref.provider == binding.model_ref.provider,
        "worker model and credential references must name the same source gateway"
    );
    let caps = serde_json::to_value(&binding.capabilities)?;
    let caps = caps.as_array().unwrap();
    anyhow::ensure!(
        !caps.is_empty()
            && caps.len() <= 4
            && caps
                .iter()
                .enumerate()
                .all(|(i, cap)| !caps[..i].contains(cap)),
        "invalid worker delegated capabilities"
    );
    Ok(())
}

#[cfg(test)]
#[path = "mcp_remote_worker_tests.rs"]
mod tests;
