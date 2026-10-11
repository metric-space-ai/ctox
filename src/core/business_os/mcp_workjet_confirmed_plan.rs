// Origin: CTOX
// License: AGPL-3.0-only
//! Restricted Supervisor continuity for an actual Owner-confirmed Core plan.
//! No synthetic business command, caller identity, new task, or general grant.
use super::super::workjet_jour_fixe_contract as wire;
use super::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use wire::WireValidate;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    project_id: String,
    owner_user_id: String,
    supervisor_thread_id: String,
    supervisor_thread_key: String,
    goal_id: String,
    goal_revision: u64,
    meeting_id: String,
    step_id: String,
    source_sha256: String,
    lease: workjet_worker_dispatch::SupervisorLease,
}

fn hash(value: &Value) -> anyhow::Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn load(core: &Connection, task_id: &str) -> anyhow::Result<Option<Binding>> {
    // Ordinary Core plans remain ordinary. Never initialize/repair schema here.
    let exists: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master
        WHERE type='table' AND name='workjet_jour_fixe_confirmations')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let mut query = core.prepare(
        "SELECT c.project_id,c.owner_user_id,c.goal_id,c.goal_revision,c.meeting_id,
        c.metadata_json,g.source_prompt,g.thread_key,g.status,s.step_id,s.instruction,s.status,
        m.body_text,m.metadata_json,m.thread_key,m.channel,m.account_key,
        r.route_status,r.lease_owner,r.leased_at,r.lease_worker_id,
        julianday(r.lease_expires_at)>julianday('now')
        FROM planned_steps s JOIN planned_goals g ON g.goal_id=s.goal_id
        JOIN workjet_jour_fixe_confirmations c ON c.goal_id=g.goal_id
        JOIN communication_messages m ON m.message_key=s.last_message_key
        JOIN communication_routing_state r ON r.message_key=m.message_key
        WHERE s.last_message_key=?1 AND m.direction='inbound' LIMIT 2",
    )?;
    let rows = query
        .query_map([task_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, u64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
                r.get::<_, String>(10)?,
                r.get::<_, String>(11)?,
                r.get::<_, String>(12)?,
                r.get::<_, String>(13)?,
                r.get::<_, String>(14)?,
                r.get::<_, String>(15)?,
                r.get::<_, String>(16)?,
                r.get::<_, String>(17)?,
                r.get::<_, Option<String>>(18)?,
                r.get::<_, Option<String>>(19)?,
                r.get::<_, Option<String>>(20)?,
                r.get::<_, Option<bool>>(21)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(None);
    }
    anyhow::ensure!(rows.len() == 1, "confirmed plan has no unique native step");
    let (
        project,
        owner,
        goal,
        revision,
        id,
        raw,
        source,
        thread,
        goal_status,
        step,
        instruction,
        step_status,
        body,
        metadata,
        message_thread,
        channel,
        account,
        route_status,
        lease_owner,
        leased_at,
        worker,
        unexpired,
    ) = rows.into_iter().next().unwrap();
    let meeting: wire::Meeting = serde_json::from_str(&raw)?;
    meeting.validate().map_err(anyhow::Error::msg)?;
    let todos = meeting
        .todos
        .as_ref()
        .context("confirmed plan has no accepted todo list")?;
    let accepted = todos
        .goal
        .as_ref()
        .context("confirmed plan goal receipt missing")?;
    anyhow::ensure!(
        meeting.id == id
            && meeting.project_id == project
            && meeting.owner_user_id == owner
            && meeting.state == wire::MeetingState::Confirmed
            && todos.status == wire::TodoState::Confirmed
            && todos.confirmed_by_user_id.as_deref() == Some(owner.as_str())
            && accepted.goal_id == goal
            && accepted.revision == revision
            && thread == meeting.supervisor.ctox_thread_key
            && message_thread == thread
            && goal_status == "active"
            && step_status == "queued",
        "confirmed plan provenance is no longer active"
    );
    let head:Option<(String,String,String,u64)>=core.query_row("SELECT owner_user_id,
        supervisor_thread_key,goal_id,revision FROM workjet_project_goal_definitions WHERE project_id=?1",
        [&project],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    anyhow::ensure!(
        head == Some((owner.clone(), thread.clone(), goal.clone(), revision)),
        "confirmed project definition replaced"
    );
    let todo = todos
        .items
        .iter()
        .find(|v| format!("{goal}::{}", v.id) == step)
        .context("native step is not in the explicitly confirmed todo list")?;
    let expected=format!("Complete this explicitly confirmed JourFix task. Preserve its owner, due date and acceptance criteria. Report evidence through the existing Core completion path. {}",serde_json::to_string(todo)?);
    let source: Value = serde_json::from_str(&source)?;
    let metadata: Value = serde_json::from_str(&metadata)?;
    anyhow::ensure!(
        instruction == expected
            && source
                == serde_json::json!({"schema":"ctox.workjet.confirmed_goal.v1","project_id":project,
            "meeting_id":id,"proposal_revision":todos.revision,"items":todos.items})
            && channel == "plan"
            && account == "plan:system"
            && task_id == format!("plan:system::{goal}::{step}")
            && metadata["goal_id"] == goal
            && metadata["step_id"] == step
            && route_status == "leased"
            && unexpired == Some(true),
        "confirmed plan message or execution changed"
    );
    let lease = workjet_worker_dispatch::SupervisorLease {
        task_id: task_id.to_owned(),
        lease_owner: lease_owner
            .filter(|s| !s.trim().is_empty())
            .context("native plan lease owner missing")?,
        leased_at: leased_at
            .filter(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok())
            .context("native plan lease timestamp missing")?,
        lease_worker_id: worker
            .filter(|s| !s.trim().is_empty())
            .context("native plan worker missing")?,
    };
    Ok(Some(Binding {
        project_id: project,
        owner_user_id: owner,
        supervisor_thread_id: meeting.supervisor.workjet_thread_id,
        supervisor_thread_key: thread,
        goal_id: goal,
        goal_revision: revision,
        meeting_id: id,
        step_id: step,
        source_sha256: hash(&serde_json::json!([
            raw,
            source,
            instruction,
            body,
            metadata
        ]))?,
        lease,
    }))
}

pub(super) fn bound_project(
    core: &Connection,
    policy: &Connection,
    context: &McpChannelRequestContext,
    trusted: &Value,
) -> anyhow::Result<(String, String, String)> {
    let expected: Binding = serde_json::from_value(trusted["workjet_confirmed_plan"].clone())?;
    let current =
        load(core, &expected.lease.task_id)?.context("confirmed plan source unavailable")?;
    anyhow::ensure!(
        current == expected
            && trusted["workjet_supervisor_lease"] == serde_json::to_value(&current.lease)?
            && context.actor == current.owner_user_id
            && trusted["command_id"].as_str() == Some("")
            && trusted["payload_hash"].as_str() == Some(""),
        "confirmed plan identity, lease or source changed"
    );
    let epoch = workjet_worker_dispatch::current_project(
        policy,
        context,
        &current.project_id,
        &current.supervisor_thread_id,
    )?;
    let native = super::super::project_chats::supervisor_binding::for_thread(
        policy,
        &context.actor,
        &current.supervisor_thread_id,
    )?
    .context("confirmed plan supervisor unavailable")?;
    anyhow::ensure!(
        native.thread_key == current.supervisor_thread_key
            && trusted["workjet_supervisor_epoch"] == epoch,
        "confirmed plan supervisor authority changed"
    );
    Ok((
        current.project_id,
        current.supervisor_thread_id,
        current.supervisor_thread_key,
    ))
}

pub(crate) fn issue(
    root: &Path,
    task: &str,
    worker: &str,
    workspace: &str,
) -> anyhow::Result<Option<String>> {
    let mut core = crew_context::open_read_connection(root)?;
    let mut policy = Connection::open_with_flags(
        store::business_os_store_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    policy.busy_timeout(std::time::Duration::from_secs(5))?;
    let core_tx = core.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let policy_tx = policy.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let Some(binding) = load(&core_tx, task)? else {
        return Ok(None);
    };
    anyhow::ensure!(
        binding.lease.lease_worker_id == worker && !workspace.trim().is_empty(),
        "confirmed plan does not belong to this admitted worker"
    );
    let role: String = policy_tx.query_row(
        "SELECT role FROM business_users WHERE user_id=?1",
        [&binding.owner_user_id],
        |r| r.get(0),
    )?;
    let mut trusted = serde_json::json!({"auth_source":MCP_INTERNAL_SESSION_AUTH_SOURCE,
        "channel":"ctox_internal_business_command","surface":"business_os_command_session",
        "actor":binding.owner_user_id,"role":role,"workspace":workspace,
        "workjet_confirmed_plan":binding,"workjet_supervisor_lease":binding.lease,"command_id":"","payload_hash":""});
    let context = context_from_arguments_with_trusted_gateway_context(
        workjet_worker_dispatch::TOOL,
        &serde_json::json!({}),
        Some(&trusted),
    )?;
    trusted["workjet_supervisor_epoch"] =
        serde_json::json!(workjet_worker_dispatch::current_project(
            &policy_tx,
            &context,
            &binding.project_id,
            &binding.supervisor_thread_id
        )?);
    bound_project(&core_tx, &policy_tx, &context, &trusted)?;
    let issued = now_ms();
    let claims = BusinessOsMcpInternalSessionClaims {
        schema: "ctox.business_os.mcp_command_session.v1".to_owned(),
        actor: binding.owner_user_id.clone(),
        role,
        workspace: workspace.to_owned(),
        command_id: String::new(),
        payload_hash: String::new(),
        allowed_actions: vec![],
        allowed_collections: vec![],
        metadata_read_contract: None,
        crew_binding: None,
        crew_work_key: None,
        crew_only: false,
        workjet_supervisor_only: true,
        workjet_supervisor_epoch: trusted["workjet_supervisor_epoch"].as_i64(),
        workjet_supervisor_lease: Some(binding.lease.clone()),
        workjet_confirmed_plan: Some(binding),
        communication_binding: None,
        issued_at_ms: issued,
        expires_at_ms: issued.saturating_add(MCP_INTERNAL_SESSION_TTL_MS),
    };
    // Release both snapshots before accessing the existing secret-store signer.
    drop(policy_tx);
    drop(core_tx);
    drop(policy);
    drop(core);
    sign_internal_command_session_claims(root, &claims).map(Some)
}

pub(super) fn verify(
    root: &Path,
    claims: &BusinessOsMcpInternalSessionClaims,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        claims.workjet_supervisor_only
            && claims.command_id.is_empty()
            && claims.payload_hash.is_empty()
            && claims.allowed_actions.is_empty()
            && claims.allowed_collections.is_empty()
            && claims.metadata_read_contract.is_none()
            && claims.crew_binding.is_none()
            && !claims.crew_only,
        "confirmed plan cannot carry another grant"
    );
    let mut core = crew_context::open_read_connection(root)?;
    let mut policy = Connection::open_with_flags(
        store::business_os_store_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    policy.busy_timeout(std::time::Duration::from_secs(5))?;
    let core_tx = core.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let policy_tx = policy.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let trusted = serde_json::json!({"auth_source":MCP_INTERNAL_SESSION_AUTH_SOURCE,
        "channel":"ctox_internal_business_command","surface":"business_os_command_session",
        "actor":claims.actor,"role":claims.role,"workspace":claims.workspace,"command_id":"","payload_hash":"",
        "workjet_confirmed_plan":claims.workjet_confirmed_plan,"workjet_supervisor_lease":claims.workjet_supervisor_lease,"workjet_supervisor_epoch":claims.workjet_supervisor_epoch});
    let context = context_from_arguments_with_trusted_gateway_context(
        workjet_worker_dispatch::TOOL,
        &serde_json::json!({}),
        Some(&trusted),
    )?;
    bound_project(&core_tx, &policy_tx, &context, &trusted)?;
    Ok(())
}

#[cfg(test)]
pub(super) fn service_fixture() -> anyhow::Result<(tempfile::TempDir, String)> {
    tests::fixture()
}

#[cfg(test)]
#[path = "mcp_workjet_confirmed_plan_tests.rs"]
mod tests;
