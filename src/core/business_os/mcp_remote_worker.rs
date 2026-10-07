// Origin: CTOX
// License: AGPL-3.0-only

//! Source-native admission for a Workjet leaf worker. An opaque permit is a
//! locator, never a target bearer credential: every operation returns through
//! the source's authenticated managed MCP connection and current policy store.
use super::*;
use rusqlite::{Connection, TransactionBehavior};
use sha2::{Digest, Sha256};

#[path = "mcp_remote_worker_target.rs"]
mod target;

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
    Renew {
        permit_id: String,
        binding: Binding,
        execution_id: String,
        renewal_sequence: u64,
        ttl_seconds: u16,
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
    #[serde(default)]
    renewal_sequence: u64,
}

/// Preserve only the typed, secret-free account locator in our own receipt.
/// Generic records (including similarly named objects) retain normal redaction.
pub(super) fn redact_receipt(value: Value) -> anyhow::Result<Value> {
    let receipt: Receipt = serde_json::from_value(value)?;
    anyhow::ensure!(
        receipt.contract == CONTRACT,
        "unexpected worker receipt contract"
    );
    let account_ref = serde_json::to_value(&receipt.binding.credential_ref)?;
    let mut result = redact_mcp_response(serde_json::to_value(receipt)?);
    result["binding"]["credentialRef"] = account_ref;
    Ok(result)
}

pub(super) fn descriptor() -> BusinessOsMcpToolDescriptor {
    write_tool(TOOL,
        "Issue, claim, revalidate, renew or revoke one source-native remote Workjet leaf-worker admission. Requires the current authenticated source Owner/Admin, owned active project and assigned computer. Revalidation returns through the source; the target receives no Owner bearer or model secret. Model references bind scope but do not replace the source gateway's current account grant.",
        serde_json::json!({"type":"object", "additionalProperties":false,
            "required":["action"],
            "properties": {
                "action":{"type":"string","enum":["issue","claim","revalidate","renew","revoke","enroll_target","register_target","resolve_target","revoke_target"]},
                "target_environment_id":{"type":"string"},
                "expected_revision":{"type":"integer","minimum":1},
                "target":{"oneOf":[
                    ref_schema(&["sourceEnvironmentId","targetEnvironmentId","targetConnectionId","targetInstanceId","targetComputerId"]),
                    ref_schema(&["sourceEnvironmentId","targetEnvironmentId","targetConnectionId","targetInstanceId"])]},
                "computer":{"type":"object","additionalProperties":false,
                    "required":["displayName","hostingMode","buildCapability"],
                    "properties":{"displayName":{"type":"string"},
                        "hostingMode":{"type":"string","enum":["workstation","self_hosted"]},
                        "buildCapability":{"type":"object","additionalProperties":false,
                            "required":["ssh_endpoint_ref","slots","jobs","lane_root","disk_floor_gib","toolchains"],
                            "properties":{"ssh_endpoint_ref":{"type":"string"},
                                "slots":{"type":"integer","minimum":1,"maximum":32},
                                "jobs":{"type":"integer","minimum":1,"maximum":64},
                                "lane_root":{"type":"string"},
                                "disk_floor_gib":{"type":"integer","minimum":1},
                                "toolchains":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"string"}}}}}},
                "permit_id":{"type":"string"}, "execution_id":{"type":"string"},
                "renewal_sequence":{"type":"integer","minimum":1},
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
    execute_checked(root, context, arguments).map_err(|error| {
        if error.downcast_ref::<BusinessOsMcpError>().is_some() {
            error
        } else if error.downcast_ref::<rusqlite::Error>().is_some() {
            anyhow::Error::new(BusinessOsMcpError {
                code: BusinessOsMcpErrorCode::RuntimeUnavailable,
                message: "native worker admission store is unavailable".to_owned(),
                field: Some("remote_worker_admission".to_owned()),
            })
        } else {
            policy_denied(&error.to_string(), "remote_worker_admission")
        }
    })
}

fn execute_checked(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    // Caller context and command-scoped sessions cannot mint independent work.
    anyhow::ensure!(
        context.trusted_role_source.as_deref() == Some("ctox_dev_managed_mcp_token")
            && context.channel == "ctox_dev_managed_mcp"
            && matches!(context.trusted_role.as_deref(), Some("chef" | "admin")),
        "remote worker admission requires authenticated source Owner/Admin MCP authority"
    );
    if target::handles(arguments) {
        return target::execute(root, context, arguments);
    }
    let request: Request = serde_json::from_value(arguments.clone()).map_err(|_| {
        BusinessOsMcpError::validation(
            "remote_worker_admission",
            "invalid remote worker admission request",
        )
    })?;
    let binding = match &request {
        Request::Issue { binding, .. }
        | Request::Claim { binding, .. }
        | Request::Revalidate { binding, .. }
        | Request::Renew { binding, .. }
        | Request::Revoke { binding, .. } => binding,
    };
    validate_binding(binding)?;
    anyhow::ensure!(
        binding.source_instance_id == context.workspace,
        "remote worker source instance differs from authenticated source"
    );
    let mut conn = store::open_store(root)?;
    conn.execute_batch(target::SCHEMA)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workjet_remote_worker_admissions (
        permit_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, source_instance_id TEXT NOT NULL,
        request_id TEXT NOT NULL, receipt_json TEXT NOT NULL,
        UNIQUE(owner_user_id,source_instance_id,request_id));",
    )?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let retiring = matches!(&request, Request::Revoke { .. });
    let (epoch, fingerprint) = if retiring {
        // Cancelling reduces authority; an expired permit or retired project/
        // computer cannot obstruct it. The actual source actor and receipt
        // owner/binding still have to be authenticated and current.
        (current_actor(&tx, context)?.1, String::new())
    } else {
        current_authority(&tx, context, binding)?
    };
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
                verify_receipt(&receipt, context, binding, epoch, &fingerprint, now, false)?;
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
                    renewal_sequence: 0,
                }
            }
        }
        Request::Claim { permit_id, .. }
        | Request::Revalidate { permit_id, .. }
        | Request::Renew { permit_id, .. }
        | Request::Revoke { permit_id, .. } => {
            label(permit_id)?;
            let raw: String = tx.query_row(
                "SELECT receipt_json FROM workjet_remote_worker_admissions WHERE permit_id=?1 AND owner_user_id=?2 AND source_instance_id=?3",
                params![permit_id, context.actor, context.workspace], |row| row.get(0)).optional()?
                .context("worker permit is unavailable to this source owner")?;
            let receipt: Receipt = serde_json::from_str(&raw)?;
            verify_receipt(
                &receipt,
                context,
                binding,
                epoch,
                &fingerprint,
                now,
                retiring,
            )?;
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
        Request::Renew {
            execution_id,
            renewal_sequence,
            ttl_seconds,
            ..
        } => {
            label(&execution_id)?;
            anyhow::ensure!(
                receipt.state == "claimed" && receipt.execution_id.as_ref() == Some(&execution_id),
                "worker permit is not claimed by this execution"
            );
            anyhow::ensure!(
                (1..=300).contains(&ttl_seconds) && renewal_sequence > 0,
                "worker renewal requires a positive sequence and TTL of 1..300 seconds"
            );
            if renewal_sequence != receipt.renewal_sequence {
                anyhow::ensure!(
                    receipt.renewal_sequence.checked_add(1) == Some(renewal_sequence),
                    "worker renewal sequence is stale or skips a lease revision"
                );
                receipt.renewal_sequence = renewal_sequence;
                receipt.expires_at_ms = now.saturating_add(i64::from(ttl_seconds) * 1000);
            }
            // Same revision is a lost-ACK replay, never a fresh extension.
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
    // This source transaction is the native admission linearization point,
    // not an offline permit or a lock around a subsequent remote await/spawn.
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
    retiring: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        receipt.contract == CONTRACT
            && receipt.owner_user_id == context.actor
            && &receipt.binding == binding,
        "worker permit immutable binding differs"
    );
    anyhow::ensure!(
        matches!(receipt.state.as_str(), "issued" | "claimed" | "revoked")
            && (receipt.state != "issued" || receipt.execution_id.is_none())
            && (receipt.state != "claimed" || receipt.execution_id.is_some()),
        "worker permit has an unknown or inconsistent lifecycle state"
    );
    if retiring {
        return Ok(());
    }
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

fn current_actor(
    conn: &Connection,
    context: &McpChannelRequestContext,
) -> anyhow::Result<(String, i64)> {
    let (role, active, epoch): (String, bool, i64) = conn
        .query_row(
            "SELECT role,active,capability_epoch FROM business_users WHERE user_id=?1",
            params![context.actor],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .context("worker source actor is not a persisted native user")?;
    anyhow::ensure!(
        active && matches!(role.as_str(), "chef" | "admin"),
        "worker source actor authority is no longer current"
    );
    Ok((role, epoch))
}

fn current_authority(
    conn: &Connection,
    context: &McpChannelRequestContext,
    binding: &Binding,
) -> anyhow::Result<(i64, String)> {
    let (role, epoch) = current_actor(conn, context)?;
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
    let computer = current_computer(conn, context, &binding.target_computer_id)?;
    let (target_binding_id, target_revision) = target::current_binding(conn, context, binding)?;
    let fingerprint = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&serde_json::json!({
            "owner":context.actor,"epoch":epoch,"instance":context.workspace,
            "project":binding.project_id,"repository":repository_key(native_repo)?,
            "computer":binding.target_computer_id,"hostingMode":computer["hosting_mode"],
            "capabilityEpoch":computer["capability_epoch"],"capabilityConfig":computer["capability_config"],
            "targetBindingId":target_binding_id,"targetRevision":target_revision
        }))?)
    );
    Ok((epoch, fingerprint))
}

fn current_computer(
    conn: &Connection,
    context: &McpChannelRequestContext,
    computer_id: &str,
) -> anyhow::Result<Value> {
    let computer = store::outbound_load_record(conn, "workjet_computers", computer_id)?
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
    // Use the operational native contract, never the browser's capability
    // chips. Runtime availability and slot acquisition remain the build
    // adapter's responsibility; this is the durable eligibility check.
    use super::super::computer_capabilities::{validate_capabilities, ComputerCapability};
    let mut capabilities: Vec<ComputerCapability> =
        serde_json::from_value(computer["capability_config"].clone())
            .context("worker target has no valid native build capability configuration")?;
    validate_capabilities(&mut capabilities, false)?;
    anyhow::ensure!(
        capabilities
            .iter()
            .any(|capability| matches!(capability, ComputerCapability::Build(_))),
        "worker target has no current native build capability"
    );
    Ok(computer)
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
