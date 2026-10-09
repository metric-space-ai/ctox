// Origin: CTOX
// License: AGPL-3.0-only

//! Durable, owner-bound handoff to Workjet's existing source dispatcher.
//! This queue never creates a harness, worktree or worker itself.
use super::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

pub(super) const TOOL: &str = "business_os.workjet_worker_dispatch";
const CONTRACT: &str = "ctox.workjet.worker-dispatch.v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Registration {
    contract: String,
    registration_id: String,
    revision: u64,
    source_environment_id: String,
    source_supervisor_thread_id: String,
    source_instance_id: String,
    source_workspace_id: String,
    project_id: String,
    owner_user_id: String,
    authority_epoch: i64,
    state: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct SupervisorLease {
    pub(super) task_id: String,
    pub(super) lease_owner: String,
    pub(super) leased_at: String,
    pub(super) lease_worker_id: String,
}

pub(super) fn current_lease(
    core: &Connection,
    command_id: &str,
) -> anyhow::Result<SupervisorLease> {
    let mut statement = core.prepare("SELECT r.message_key,r.lease_owner,r.leased_at,r.lease_worker_id
        FROM business_command_task_links l JOIN communication_routing_state r ON r.message_key=l.task_id
        WHERE l.command_id=?1 AND r.route_status='leased'
        AND length(trim(r.lease_owner))>0 AND length(trim(r.lease_worker_id))>0
        AND julianday(r.leased_at) IS NOT NULL AND julianday(r.lease_expires_at)>julianday('now') LIMIT 2")?;
    let rows = statement
        .query_map([command_id], |row| {
            Ok(SupervisorLease {
                task_id: row.get(0)?,
                lease_owner: row.get(1)?,
                leased_at: row.get(2)?,
                lease_worker_id: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    anyhow::ensure!(
        rows.len() == 1,
        "supervisor has no unique current native execution lease"
    );
    Ok(rows.into_iter().next().unwrap())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Intent {
    intent_id: String,
    registration_id: String,
    registration_revision: u64,
    source_environment_id: String,
    source_supervisor_thread_id: String,
    project_id: String,
    task: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    computer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worker_profile_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    RegisterSource {
        source_environment_id: String,
        source_supervisor_thread_id: String,
        project_id: String,
        expected_revision: Option<u64>,
    },
    RevokeSource {
        registration_id: String,
        revision: u64,
    },
    Poll {
        source_environment_id: String,
    },
    Complete {
        registration_id: String,
        revision: u64,
        intent_id: String,
        result: Value,
    },
    Dispatch {
        dispatch_key: String,
        task: String,
        title: Option<String>,
        computer_id: Option<String>,
        worker_profile_id: Option<String>,
    },
    Observe {
        limit: Option<u32>,
    },
}

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_worker_dispatch_sources (
    owner_user_id TEXT NOT NULL, project_id TEXT NOT NULL,
    registration_id TEXT UNIQUE NOT NULL, record_json TEXT NOT NULL,
    PRIMARY KEY(owner_user_id,project_id));
CREATE TABLE IF NOT EXISTS workjet_worker_dispatch_intents (
    intent_id TEXT PRIMARY KEY, registration_id TEXT NOT NULL, registration_revision INTEGER NOT NULL,
    command_id TEXT NOT NULL, dispatch_key TEXT NOT NULL, request_digest TEXT NOT NULL,
    intent_json TEXT NOT NULL, result_json TEXT,
    UNIQUE(command_id,dispatch_key));";

pub(super) fn descriptor() -> BusinessOsMcpToolDescriptor {
    write_tool(TOOL,
        "Dispatch through the registered Workjet source, or observe persisted dispatch acknowledgements for the native-bound project. Restricted native supervisors may dispatch or observe (limit 1..32). An acknowledgement of dispatched means started, never completed work or a merged PR. Authenticated source Owner/Admin may register_source, poll, complete the exact result, or revoke_source. Replayed intents keep the same UUID; this tool never starts another executor.",
        serde_json::json!({"type":"object","additionalProperties":false,"required":["action"],
            "properties":{
                "action":{"type":"string","enum":["register_source","revoke_source","poll","complete","dispatch","observe"]},
                "limit":{"type":"integer","minimum":1,"maximum":32},
                "source_environment_id":{"type":"string"},"source_supervisor_thread_id":{"type":"string"},
                "project_id":{"type":"string"},"expected_revision":{"type":"integer","minimum":1},
                "registration_id":{"type":"string"},"revision":{"type":"integer","minimum":1},
                "intent_id":{"type":"string"},"result":{"type":"object"},
                "dispatch_key":{"type":"string"},"task":{"type":"string","maxLength":16384},
                "title":{"type":"string","maxLength":200},"computer_id":{"type":"string"},
                "worker_profile_id":{"type":"string"}}}))
}

fn text(value: &str, max: usize) -> anyhow::Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= max
            && value == value.trim()
            && !value.chars().any(|c| c == '\0'),
        "invalid worker dispatch value"
    );
    Ok(())
}

pub(super) fn current_project(
    policy: &Connection,
    context: &McpChannelRequestContext,
    project: &str,
    thread: &str,
) -> anyhow::Result<i64> {
    let (role, epoch) = remote_worker::current_actor(policy, context)?;
    anyhow::ensure!(
        context.trusted_role.as_deref() == Some(role.as_str()),
        "dispatch role changed"
    );
    super::super::project_chats::owned_project(policy, project, &context.actor, true)?;
    let binding = super::super::project_chats::supervisor_binding::for_thread(
        policy,
        &context.actor,
        thread,
    )?
    .context("source supervisor is not registered natively")?;
    anyhow::ensure!(
        binding.project_id == project,
        "source supervisor belongs to another project"
    );
    let record = store::outbound_load_record(policy, "user_threads", thread)?
        .context("native supervisor history missing")?;
    anyhow::ensure!(
        record["is_deleted"] != true
            && record["_deleted"] != true
            && record["owner_user_id"] == context.actor
            && record["source_module"] == "ctox"
            && record["source_record_type"] == "workjet_project"
            && record["source_record_id"] == project,
        "native supervisor provenance changed"
    );
    for (permission, scope, id) in [
        (
            BusinessOsPermission::CtoxTaskCreate,
            BusinessOsScopeType::Record,
            Some(project),
        ),
        (
            BusinessOsPermission::IntegrationsManage,
            BusinessOsScopeType::Workspace,
            None,
        ),
    ] {
        anyhow::ensure!(
            super::super::store_policy::trusted_actor_policy_decision_with_conn(
                policy,
                &context.actor,
                &role,
                permission,
                scope,
                id
            )?
            .allowed,
            "native worker dispatch policy denied"
        );
    }
    Ok(epoch)
}

fn load(core: &Connection, owner: &str, id: &str) -> anyhow::Result<Registration> {
    let raw: String = core.query_row(
        "SELECT record_json FROM workjet_worker_dispatch_sources WHERE owner_user_id=?1 AND registration_id=?2",
        params![owner, id], |row| row.get(0),
    ).optional()?.context("worker source registration unavailable")?;
    let record: Registration = serde_json::from_str(&raw)?;
    anyhow::ensure!(
        record.contract == CONTRACT
            && record.owner_user_id == owner
            && record.registration_id == id
            && record.revision > 0,
        "invalid worker source registration"
    );
    Ok(record)
}
fn check_source(
    policy: &Connection,
    context: &McpChannelRequestContext,
    record: &Registration,
    revision: u64,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        record.state == "active"
            && record.revision == revision
            && record.source_instance_id == context.managed_source_instance()?
            && record.source_workspace_id == context.workspace,
        "worker source is stale or revoked"
    );
    let epoch = current_project(
        policy,
        context,
        &record.project_id,
        &record.source_supervisor_thread_id,
    )?;
    anyhow::ensure!(
        epoch == record.authority_epoch,
        "worker source authority changed; explicit registration required"
    );
    Ok(())
}
fn save(core: &Connection, record: &Registration) -> anyhow::Result<()> {
    core.execute("INSERT INTO workjet_worker_dispatch_sources(owner_user_id,project_id,registration_id,record_json)
        VALUES(?1,?2,?3,?4) ON CONFLICT(owner_user_id,project_id)
        DO UPDATE SET registration_id=excluded.registration_id,record_json=excluded.record_json",
        params![record.owner_user_id,record.project_id,record.registration_id,serde_json::to_string(record)?])?;
    Ok(())
}

/// Service admission uses the canonical native command, never caller metadata.
pub(crate) fn is_supervisor_command(root: &Path, command: &Value) -> anyhow::Result<bool> {
    if command["command_type"] != "business_os.chat.task" || command["module"] != "ctox" {
        return Ok(false);
    }
    let Some(thread) = command
        .pointer("/payload/thread_id")
        .and_then(Value::as_str)
    else {
        return Ok(false);
    };
    let Some(project) = command["record_id"].as_str() else {
        return Ok(false);
    };
    let policy = store::open_store(root)?;
    let bound: Option<String> = if policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_supervisor_bindings')",
        [], |row| row.get::<_,bool>(0))? {
        policy.query_row("SELECT owner_user_id FROM workjet_supervisor_bindings WHERE project_id=?1 AND thread_id=?2",
            params![project,thread], |row| row.get(0)).optional()?
    } else { None };
    Ok(bound.is_some())
}

pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
    trusted: Option<&Value>,
) -> anyhow::Result<Value> {
    let request: Request = serde_json::from_value(arguments.clone())?;
    let internal = context.trusted_role_source.as_deref() == Some(MCP_INTERNAL_SESSION_AUTH_SOURCE);
    if matches!(&request, Request::Dispatch { .. } | Request::Observe { .. }) {
        anyhow::ensure!(
            internal && trusted.is_some_and(|t| t["workjet_supervisor_only"] == true),
            "dispatch/observe requires the restricted native supervisor session"
        );
    } else {
        anyhow::ensure!(
            !internal
                && context.trusted_role_source.as_deref() == Some("ctox_dev_managed_mcp_token")
                && context.channel == "ctox_dev_managed_mcp",
            "source controls require authenticated managed Owner/Admin MCP"
        );
    }
    if let Request::Observe { limit } = &request {
        return observe(
            root,
            context,
            trusted.context("native supervisor session unavailable")?,
            *limit,
        );
    }
    // Worker/core before policy, matching the native execution lock order.
    // The core transaction fences cancellation while the intent commits.
    let mut core = Connection::open_with_flags(
        crate::paths::core_db(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    core.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    core.execute_batch(SCHEMA)?;
    let core_tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut policy = store::open_store(root)?;
    let policy_tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    remote_worker::current_actor(&policy_tx, context)?;
    let response = match request {
        Request::Observe { .. } => unreachable!("observations use read-only snapshots"),
        Request::RegisterSource {
            source_environment_id,
            source_supervisor_thread_id,
            project_id,
            expected_revision,
        } => {
            text(&source_environment_id, 256)?;
            text(&source_supervisor_thread_id, 36)?;
            text(&project_id, 128)?;
            let epoch = current_project(
                &policy_tx,
                context,
                &project_id,
                &source_supervisor_thread_id,
            )?;
            let raw: Option<String> = core_tx.query_row(
                "SELECT record_json FROM workjet_worker_dispatch_sources WHERE owner_user_id=?1 AND project_id=?2",
                params![context.actor,project_id], |row| row.get(0)).optional()?;
            let mut record = if let Some(raw) = raw {
                serde_json::from_str::<Registration>(&raw)?
            } else {
                anyhow::ensure!(
                    expected_revision.is_none(),
                    "source registration does not exist"
                );
                Registration {
                    contract: CONTRACT.into(),
                    registration_id: uuid::Uuid::new_v4().to_string(),
                    revision: 1,
                    source_environment_id: source_environment_id.clone(),
                    source_supervisor_thread_id: source_supervisor_thread_id.clone(),
                    source_instance_id: context.managed_source_instance()?.to_owned(),
                    source_workspace_id: context.workspace.clone(),
                    project_id: project_id.clone(),
                    owner_user_id: context.actor.clone(),
                    authority_epoch: epoch,
                    state: "active".into(),
                }
            };
            let unchanged = record.state == "active"
                && record.source_environment_id == source_environment_id
                && record.source_supervisor_thread_id == source_supervisor_thread_id
                && record.source_instance_id == context.managed_source_instance()?
                && record.source_workspace_id == context.workspace
                && record.authority_epoch == epoch;
            if unchanged {
                anyhow::ensure!(
                    expected_revision.is_none()
                        || expected_revision == Some(record.revision)
                        || expected_revision.and_then(|r| r.checked_add(1))
                            == Some(record.revision),
                    "source registration revision stale"
                );
            } else {
                anyhow::ensure!(
                    expected_revision == Some(record.revision),
                    "source replacement needs exact revision"
                );
                record.revision = record
                    .revision
                    .checked_add(1)
                    .context("source revision exhausted")?;
                record.source_environment_id = source_environment_id;
                record.source_supervisor_thread_id = source_supervisor_thread_id;
                record.source_instance_id = context.managed_source_instance()?.to_owned();
                record.source_workspace_id = context.workspace.clone();
                record.authority_epoch = epoch;
                record.state = "active".into();
            }
            save(&core_tx, &record)?;
            serde_json::to_value(record)?
        }
        Request::RevokeSource {
            registration_id,
            revision,
        } => {
            let mut record = load(&core_tx, &context.actor, &registration_id)?;
            anyhow::ensure!(
                record.source_instance_id == context.managed_source_instance()?
                    && record.source_workspace_id == context.workspace,
                "source instance differs"
            );
            if record.state == "revoked" {
                anyhow::ensure!(
                    revision == record.revision || revision.checked_add(1) == Some(record.revision),
                    "source revision stale"
                );
            } else {
                anyhow::ensure!(
                    revision == record.revision,
                    "source revocation needs exact revision"
                );
                record.revision = revision
                    .checked_add(1)
                    .context("source revision exhausted")?;
                record.state = "revoked".into();
            }
            save(&core_tx, &record)?;
            serde_json::to_value(record)?
        }
        Request::Poll {
            source_environment_id,
        } => {
            text(&source_environment_id, 256)?;
            let (_, epoch) = remote_worker::current_actor(&policy_tx, context)?;
            // One source-wide queue, no per-project polling loops. Replaced,
            // revoked and old-epoch registrations cannot hold its head.
            let raw: Option<(String, String)> = core_tx
                .query_row(
                    "SELECT i.intent_json,s.record_json FROM workjet_worker_dispatch_intents i
                 JOIN workjet_worker_dispatch_sources s ON s.registration_id=i.registration_id
                 WHERE s.owner_user_id=?1 AND json_extract(s.record_json,'$.sourceInstanceId')=?2
                 AND json_extract(s.record_json,'$.sourceEnvironmentId')=?3
                 AND json_extract(s.record_json,'$.state')='active'
                 AND json_extract(s.record_json,'$.revision')=i.registration_revision
                 AND json_extract(s.record_json,'$.authorityEpoch')=?4
                 AND json_extract(s.record_json,'$.sourceWorkspaceId')=?5
                 AND i.result_json IS NULL ORDER BY i.rowid LIMIT 1",
                    params![
                        context.actor,
                        context.managed_source_instance()?,
                        source_environment_id,
                        epoch,
                        context.workspace
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let (intents, registration) = if let Some((raw, registration)) = raw {
                let intent: Intent = serde_json::from_str(&raw)?;
                let registration: Registration = serde_json::from_str(&registration)?;
                check_source(
                    &policy_tx,
                    context,
                    &registration,
                    intent.registration_revision,
                )?;
                (vec![intent], Some(registration))
            } else {
                (Vec::<Intent>::new(), None)
            };
            serde_json::json!({"contract":CONTRACT,"intents":intents,"registration":registration})
        }
        Request::Complete {
            registration_id,
            revision,
            intent_id,
            result,
        } => {
            let record = load(&core_tx, &context.actor, &registration_id)?;
            check_source(&policy_tx, context, &record, revision)?;
            let (raw, previous): (String, Option<String>) = core_tx
                .query_row(
                    "SELECT intent_json,result_json FROM workjet_worker_dispatch_intents
                 WHERE intent_id=?1 AND registration_id=?2 AND registration_revision=?3",
                    params![intent_id, registration_id, revision],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .context("worker dispatch intent unavailable")?;
            let intent: Intent = serde_json::from_str(&raw)?;
            validate_result(&intent, &result)?;
            let serialized = serde_json::to_string(&result)?;
            if let Some(previous) = previous {
                anyhow::ensure!(
                    serde_json::from_str::<Value>(&previous)? == result,
                    "dispatch completion differs from prior result"
                );
            } else {
                core_tx.execute("UPDATE workjet_worker_dispatch_intents SET result_json=?2 WHERE intent_id=?1 AND result_json IS NULL",
                    params![intent_id,serialized])?;
            }
            serde_json::json!({"contract":CONTRACT,"intentId":intent_id,"registrationId":registration_id,"revision":revision,"result":result})
        }
        Request::Dispatch {
            dispatch_key,
            task,
            title,
            computer_id,
            worker_profile_id,
        } => {
            text(&dispatch_key, 128)?;
            text(&task, 16 * 1024)?;
            if let Some(title) = &title {
                text(title, 200)?;
            }
            for value in [&computer_id, &worker_profile_id].into_iter().flatten() {
                text(value, 256)?;
            }
            let trusted = trusted.context("native supervisor session unavailable")?;
            let (project, thread, _) =
                workjet_jour_fixe::bound_project(&core_tx, &policy_tx, context, trusted)?;
            // Existing column name is retained; the source is either an actual
            // business command or the exact native confirmed-plan task key.
            let execution_key = workjet_jour_fixe::execution_key(trusted)?;
            let epoch = current_project(&policy_tx, context, &project, &thread)?;
            let raw: String = core_tx
                .query_row(
                    "SELECT record_json FROM workjet_worker_dispatch_sources
                WHERE owner_user_id=?1 AND project_id=?2",
                    params![context.actor, project],
                    |row| row.get(0),
                )
                .optional()?
                .context("no Workjet source registered for this supervisor")?;
            let record: Registration = serde_json::from_str(&raw)?;
            anyhow::ensure!(
                record.state == "active"
                    && record.authority_epoch == epoch
                    && record.source_supervisor_thread_id == thread,
                "Workjet source binding stale"
            );
            let mut intent = Intent {
                intent_id: uuid::Uuid::new_v4().to_string(),
                registration_id: record.registration_id,
                registration_revision: record.revision,
                source_environment_id: record.source_environment_id,
                source_supervisor_thread_id: thread.to_owned(),
                project_id: project.to_owned(),
                task,
                title,
                computer_id,
                worker_profile_id,
            };
            let digest = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&serde_json::json!({
                "registrationId":intent.registration_id,"revision":intent.registration_revision,"task":intent.task,
                "title":intent.title,"computerId":intent.computer_id,"workerProfileId":intent.worker_profile_id}))?)
            );
            let prior: Option<(String,String,Option<String>)> = core_tx.query_row(
                "SELECT request_digest,intent_json,result_json FROM workjet_worker_dispatch_intents WHERE command_id=?1 AND dispatch_key=?2",
                params![execution_key,dispatch_key],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
            let result = if let Some((old_digest, raw, result)) = prior {
                anyhow::ensure!(
                    old_digest == digest,
                    "dispatch_key reused with different scope or task"
                );
                intent = serde_json::from_str(&raw)?;
                result
                    .map(|raw| serde_json::from_str::<Value>(&raw))
                    .transpose()?
            } else {
                let pending: i64 = core_tx.query_row(
                    "SELECT count(*) FROM workjet_worker_dispatch_intents i JOIN workjet_worker_dispatch_sources s
                     ON s.registration_id=i.registration_id WHERE s.owner_user_id=?1 AND i.result_json IS NULL
                     AND json_extract(s.record_json,'$.state')='active'
                     AND json_extract(s.record_json,'$.revision')=i.registration_revision
                     AND json_extract(s.record_json,'$.authorityEpoch')=?2",
                    params![context.actor,epoch], |row| row.get(0))?;
                anyhow::ensure!(
                    pending < 128,
                    "worker dispatch pending-intent capacity exceeded"
                );
                core_tx.execute("INSERT INTO workjet_worker_dispatch_intents(intent_id,registration_id,registration_revision,
                    command_id,dispatch_key,request_digest,intent_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![intent.intent_id,intent.registration_id,intent.registration_revision,execution_key,dispatch_key,digest,serde_json::to_string(&intent)?])?;
                None
            };
            serde_json::json!({"contract":CONTRACT,"state":if result.is_some(){"completed"}else{"pending"},
                "intent":intent,"result":result})
        }
    };
    // Retain current policy through the durable intent linearization point.
    core_tx.commit()?;
    policy_tx.commit()?;
    Ok(response)
}

/// Observation issues no execution authority and never repairs schema or reserves
/// a writer. As for meeting reads, both snapshots revalidate the actual lease,
/// canonical admission and current Owner/project/Supervisor binding.
fn observe(
    root: &Path,
    context: &McpChannelRequestContext,
    trusted: &Value,
    limit: Option<u32>,
) -> anyhow::Result<Value> {
    let limit = limit.unwrap_or(16);
    anyhow::ensure!((1..=32).contains(&limit), "observe limit must be 1..32");
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY;
    let mut core = Connection::open_with_flags(crate::paths::core_db(root), flags)?;
    core.busy_timeout(std::time::Duration::from_secs(5))?;
    let mut policy = Connection::open_with_flags(store::business_os_store_path(root), flags)?;
    policy.busy_timeout(std::time::Duration::from_secs(5))?;
    let core_tx = core.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let policy_tx = policy.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let (project, thread, _) =
        workjet_jour_fixe::bound_project(&core_tx, &policy_tx, context, trusted)?;
    let epoch = current_project(&policy_tx, context, &project, &thread)?;
    anyhow::ensure!(
        super::super::store_policy::trusted_actor_policy_decision_with_conn(
            &policy_tx,
            &context.actor,
            context
                .trusted_role
                .as_deref()
                .context("native role missing")?,
            BusinessOsPermission::DataRead,
            BusinessOsScopeType::Record,
            Some(&project),
        )?
        .allowed,
        "native dispatch observation policy denied"
    );
    let mut response = serde_json::json!({
        "contract":CONTRACT, "projectId":project, "supervisorThreadId":thread,
        "observations":[], "truncated":false
    });
    let tables: u32 = core_tx.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN
         ('workjet_worker_dispatch_sources','workjet_worker_dispatch_intents')",
        [],
        |row| row.get(0),
    )?;
    if tables != 2 {
        return Ok(response);
    }
    let mut statement = core_tx.prepare(
        "SELECT i.command_id,i.dispatch_key,i.intent_json,i.result_json,s.record_json
         FROM workjet_worker_dispatch_intents i
         JOIN workjet_worker_dispatch_sources s ON s.registration_id=i.registration_id
         WHERE s.owner_user_id=?1 AND s.project_id=?2
         AND json_extract(i.intent_json,'$.sourceSupervisorThreadId')=?3
         ORDER BY i.rowid DESC LIMIT ?4",
    )?;
    let rows = statement.query_map(params![context.actor, project, thread, limit + 1], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut observations = Vec::new();
    for row in rows {
        if observations.len() == limit as usize {
            response["truncated"] = Value::Bool(true);
            break;
        }
        let (execution_key, dispatch_key, intent_raw, result_raw, source_raw) = row?;
        let intent: Intent = serde_json::from_str(&intent_raw)?;
        let source: Registration = serde_json::from_str(&source_raw)?;
        anyhow::ensure!(
            source.contract == CONTRACT
                && source.owner_user_id == context.actor
                && source.project_id == project
                && intent.project_id == project
                && intent.source_supervisor_thread_id == thread
                && intent.registration_id == source.registration_id,
            "persisted dispatch observation identity differs"
        );
        let acknowledgement = if let Some(raw) = result_raw {
            let result: Value = serde_json::from_str(&raw)?;
            validate_result(&intent, &result)?;
            let mut fields = serde_json::Map::new();
            // Selection metadata is not an actual producer witness. The source
            // receipt only attests startup (or a typed startup failure).
            for key in [
                "status",
                "reason",
                "workerThreadId",
                "environmentId",
                "computerId",
                "branch",
            ] {
                if let Some(value) = result.get(key) {
                    fields.insert(key.to_owned(), value.clone());
                }
            }
            Value::Object(fields)
        } else {
            Value::Null
        };
        let observation = serde_json::json!({
            "intentId":intent.intent_id, "executionKey":execution_key, "dispatchKey":dispatch_key,
            "registrationRevision":intent.registration_revision,
            "registrationCurrent":source.state=="active" && source.revision==intent.registration_revision
                && source.authority_epoch==epoch && source.source_supervisor_thread_id==thread,
            "title":intent.title, "taskPreview":intent.task.chars().take(256).collect::<String>(),
            "computerId":intent.computer_id, "workerProfileId":intent.worker_profile_id,
            "acknowledgement":acknowledgement, "execution":null
        });
        observations.push(observation);
        // Bound metadata as well as row count, without claiming completeness.
        if serde_json::to_vec(&observations)?.len() > 60 * 1024 {
            observations.pop();
            response["truncated"] = Value::Bool(true);
            break;
        }
    }
    response["observations"] = serde_json::to_value(observations)?;
    Ok(response)
}

fn validate_result(intent: &Intent, value: &Value) -> anyhow::Result<()> {
    let object = value
        .as_object()
        .context("dispatch result must be an object")?;
    anyhow::ensure!(
        serde_json::to_vec(value)?.len() <= 32 * 1024 && value["schemaVersion"] == 1,
        "invalid dispatch result schema/bound"
    );
    match value["status"].as_str() {
        Some("dispatched") => {
            anyhow::ensure!(
                object.keys().all(|k| matches!(
                    k.as_str(),
                    "schemaVersion"
                        | "status"
                        | "environmentId"
                        | "workerThreadId"
                        | "computerId"
                        | "branch"
                        | "worktreePath"
                        | "parent"
                        | "modelSelection"
                        | "enabledCapabilityIds"
                )),
                "unknown dispatch result field"
            );
            for field in [
                "environmentId",
                "workerThreadId",
                "computerId",
                "branch",
                "worktreePath",
            ] {
                text(
                    value[field]
                        .as_str()
                        .context("dispatch result field missing")?,
                    2048,
                )?;
            }
            anyhow::ensure!(
                value["workerThreadId"] == intent.intent_id
                    && value.pointer("/parent/environmentId")
                        == Some(&Value::String(intent.source_environment_id.clone()))
                    && value.pointer("/parent/threadId")
                        == Some(&Value::String(intent.source_supervisor_thread_id.clone())),
                "dispatch result identity differs from intent"
            );
            if let Some(computer) = &intent.computer_id {
                anyhow::ensure!(
                    value["computerId"] == *computer,
                    "dispatch computer differs"
                );
            }
            let parent = value["parent"]
                .as_object()
                .context("dispatch parent missing")?;
            anyhow::ensure!(parent.len() == 2, "unknown dispatch parent field");
            let model = value["modelSelection"]
                .as_object()
                .context("dispatch model missing")?;
            anyhow::ensure!(
                model
                    .keys()
                    .all(|k| matches!(k.as_str(), "instanceId" | "model" | "options")),
                "unknown model selection field"
            );
            text(
                value["modelSelection"]["instanceId"]
                    .as_str()
                    .context("model instance missing")?,
                256,
            )?;
            text(
                value["modelSelection"]["model"]
                    .as_str()
                    .context("model missing")?,
                256,
            )?;
            if let Some(options) = model.get("options") {
                let options = options
                    .as_array()
                    .context("model options must be a typed array")?;
                anyhow::ensure!(options.len() <= 64, "too many model options");
                let mut ids = std::collections::BTreeSet::new();
                for option in options {
                    let fields = option.as_object().context("invalid model option")?;
                    anyhow::ensure!(
                        fields.len() == 2
                            && fields.contains_key("id")
                            && fields.contains_key("value"),
                        "invalid model option fields"
                    );
                    let id = option["id"].as_str().context("model option id missing")?;
                    text(id, 256)?;
                    anyhow::ensure!(ids.insert(id), "duplicate model option");
                    if let Some(value) = option["value"].as_str() {
                        text(value, 2048)?;
                    } else {
                        anyhow::ensure!(option["value"].is_boolean(), "invalid model option value");
                    }
                }
            }
            let caps = value["enabledCapabilityIds"]
                .as_array()
                .context("dispatch capabilities missing")?;
            anyhow::ensure!(caps.len() <= 64, "too many dispatch capabilities");
            for (i, cap) in caps.iter().enumerate() {
                text(cap.as_str().context("invalid dispatch capability")?, 256)?;
                anyhow::ensure!(!caps[..i].contains(cap), "duplicate dispatch capability");
            }
        }
        Some("failed") => {
            anyhow::ensure!(
                object.len() == 3
                    && matches!(
                        value["reason"].as_str(),
                        Some(
                            "role-not-authorized"
                                | "parent-unavailable"
                                | "parent-not-orchestrator"
                                | "duplicate-capabilities"
                                | "capability-escalation"
                                | "computer-unavailable"
                                | "worker-profile-unavailable"
                                | "remote-dispatch-unavailable"
                                | "remote-dispatch-failed"
                                | "worktree-failed"
                                | "create-failed"
                                | "turn-start-failed"
                                | "rollback-failed"
                        )
                    ),
                "invalid terminal dispatch failure"
            );
        }
        _ => anyhow::bail!("pending or unsupported dispatch result is not terminal"),
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn service_test_fixture() -> anyhow::Result<(tempfile::TempDir, String)> {
    let root = tests::fixture()?;
    let command_id = tests::queued_supervisor(root.path())?;
    Ok((root, command_id))
}

#[cfg(test)]
pub(super) fn meeting_test_fixture() -> anyhow::Result<(tempfile::TempDir, Value)> {
    let root = tests::fixture()?;
    let (_, trusted) = tests::session(root.path())?;
    Ok((root, trusted))
}

#[cfg(test)]
#[path = "mcp_workjet_worker_dispatch_tests.rs"]
mod tests;
