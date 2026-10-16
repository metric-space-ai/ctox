//! E49 OS task/execution adapters. The native dispatcher retains policy and fence.
//! No client-selected actor, session claims, Workjet project or private path.
use super::*;
use crate::business_os::{
    policy::{BusinessOsPermission, BusinessOsScope},
    session::BusinessOsSession,
};
use ctox_store::change_log::subscription::write_contract::routes;
use sha2::Digest;

fn rejected(code: &str, reason: &str) -> anyhow::Error {
    anyhow::anyhow!(serde_json::to_string(&routes::error(code, reason)).expect("serializable error"))
}
fn fields(args: &Value, allowed: &[&str]) -> anyhow::Result<()> {
    let object = args
        .as_object()
        .ok_or_else(|| rejected("invalid", "crew_arguments"))?;
    anyhow::ensure!(
        object.keys().all(|k| allowed.contains(&k.as_str())),
        rejected("invalid", "crew_arguments")
    );
    Ok(())
}
fn text(args: &Value, name: &str) -> anyhow::Result<String> {
    let value = required_arg(args, name).map_err(|_| rejected("invalid", "crew_arguments"))?;
    anyhow::ensure!(
        value.len() <= 256 && !value.chars().any(char::is_control),
        rejected("invalid", "crew_arguments")
    );
    Ok(value)
}
fn permission(
    root: &Path,
    session: &BusinessOsSession,
    kind: &str,
    task: bool,
) -> anyhow::Result<()> {
    use crate::business_os::store_policy::{scoped_policy_decision, workspace_policy_decision};
    let decision = if task {
        workspace_policy_decision(root, session, BusinessOsPermission::CtoxTaskCreate)?
    } else {
        scoped_policy_decision(
            root,
            session,
            BusinessOsPermission::CrewManage,
            BusinessOsScope::record(kind),
        )?
    };
    if !decision.allowed {
        return Err(rejected("forbidden", "native_task_permission_denied"));
    }
    Ok(())
}

/// Called only with the session supplied by the central native authorization.
/// V4 Core/P9 adapters hold their actual authority fence around this effect.
pub(crate) fn execute(
    root: &Path,
    invocation_id: &str,
    kind: &str,
    args: &Value,
    session: &BusinessOsSession,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session.ok && session.authenticated && !session.auth_required,
        rejected("forbidden", "native_actor_required")
    );
    let user = session
        .user
        .as_ref()
        .ok_or_else(|| rejected("forbidden", "native_actor_required"))?;
    permission(root, session, kind, kind == "ctox.task.create")?;
    if kind == "ctox.task.create" {
        return create(root, args, session);
    }
    let context = McpChannelRequestContext {
        channel: "native_v4".into(),
        surface: "native".into(),
        actor: user.id.clone(),
        workspace: "native_v4".into(),
        tool: kind.into(),
        request_id: invocation_id.into(),
        confirmation_state: McpConfirmationState::NotRequired,
        trusted_role: Some(normalize_role(&user.role)),
        trusted_role_source: Some("retained_native_authority".into()),
        trusted_managed_read_scope: None,
        trusted_managed_instance_id: None,
    };
    let allowed: &[&str] = match kind {
        "ctox.crew.execution.list" => &["command_id", "executor_id"],
        "ctox.crew.execution.claim" => &["command_id", "executor_id", "attempt_id"],
        "ctox.crew.execution.plan.update" => &[
            "command_id",
            "executor_id",
            "attempt_id",
            "steps",
            "explanation",
        ],
        "ctox.crew.execution.report" => {
            &["command_id", "executor_id", "attempt_id", "reply", "error"]
        }
        _ => return Err(rejected("not_found", "unknown_command")),
    };
    fields(args, allowed)?;
    let command_id = text(args, "command_id")?;
    text(args, "executor_id")?;
    // Existing owner/current-role check also covers listing and strict claims.
    crew_execution::authorize_owner(root, &context, &command_id)
        .map_err(|_| rejected("forbidden", "crew_execution_owner"))?;
    match kind {
        "ctox.crew.execution.list" => crew_execution::list(root, &context, args),
        "ctox.crew.execution.claim" => {
            text(args, "attempt_id")?;
            let mut result = crew_execution::claim_once(root, &context, args)
                .map_err(|_| rejected("conflict", "crew_offer_not_available"))?;
            if let Some(object) = result.as_object_mut() {
                // Native plan/report derive authority from the retained server-side offer.
                // Never persist a legacy bearer in a V4 result document.
                object.remove("command_session");
                object.remove("instructions");
            }
            Ok(result)
        }
        _ => {
            let trusted = trusted_attempt(root, &context, args)?;
            let mut payload = args.as_object().expect("validated object").clone();
            for key in ["command_id", "executor_id", "attempt_id"] {
                payload.remove(key);
            }
            let payload = Value::Object(payload);
            if kind == "ctox.crew.execution.plan.update" {
                crew_plan::update(root, &context, &payload, Some(&trusted))
            } else {
                crew_execution::report(root, &context, &payload, Some(&trusted))
            }
        }
    }
}

fn trusted_attempt(
    root: &Path,
    context: &McpChannelRequestContext,
    args: &Value,
) -> anyhow::Result<Value> {
    let command = text(args, "command_id")?;
    let executor = text(args, "executor_id")?;
    let attempt = text(args, "attempt_id")?;
    let conn = crew_context::open_read_connection(root)?;
    let raw: Option<String> = conn.query_row(
        "SELECT claims_json FROM workjet_external_crew_attempts WHERE attempt_id=?3 AND executor_id=?2 AND command_id=?1 AND state IN ('claimed','reported') AND deadline_ms>?4",
        params![command, executor, attempt, now_ms()], |r| r.get(0),
    ).optional()?;
    let claims: BusinessOsMcpInternalSessionClaims =
        serde_json::from_str(&raw.ok_or_else(|| rejected("forbidden", "crew_execution_binding"))?)?;
    anyhow::ensure!(
        claims.actor == context.actor && Some(normalize_role(&claims.role)) == context.trusted_role,
        rejected("forbidden", "crew_execution_binding")
    );
    // Use the same verifier as native MCP execution; no trusted fields from args.
    let trusted = verify_internal_command_session_token(
        root,
        &sign_internal_command_session_claims(root, &claims)?,
    )
    .map_err(|_| rejected("forbidden", "crew_execution_binding"))?;
    crew_context::read(
        root,
        context,
        &serde_json::json!({"attempt_id": attempt}),
        Some(&trusted),
    )
    .map_err(|_| rejected("forbidden", "crew_execution_binding"))?;
    Ok(trusted)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    title: String,
    instruction: String,
    idempotency_key: String,
    target: Target,
    #[serde(default)]
    payload: Value,
    thread_key: Option<String>,
    priority: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Target {
    Native,
    Crew {
        member_id: String,
        executor_id: String,
        harness: String,
        timeout_seconds: u64,
    },
}
fn create(root: &Path, args: &Value, session: &BusinessOsSession) -> anyhow::Result<Value> {
    let request: Create =
        serde_json::from_value(args.clone()).map_err(|_| rejected("invalid", "task_arguments"))?;
    anyhow::ensure!(
        !request.title.trim().is_empty()
            && request.title.len() <= 256
            && !request.instruction.trim().is_empty()
            && request.instruction.len() <= 65536,
        rejected("invalid", "task_text")
    );
    let key = request.idempotency_key.as_bytes();
    anyhow::ensure!(
        !key.is_empty()
            && key.len() <= 256
            && key[0].is_ascii_alphanumeric()
            && key
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(b)),
        rejected("invalid", "task_idempotency_key")
    );
    if let Some(thread) = &request.thread_key {
        anyhow::ensure!(
            !thread.is_empty() && thread.len() <= 256,
            rejected("invalid", "task_thread")
        );
    }
    if let Some(priority) = &request.priority {
        anyhow::ensure!(
            ["low", "normal", "high", "urgent"].contains(&priority.as_str()),
            rejected("invalid", "task_priority")
        );
    }
    let user = session.user.as_ref().expect("authenticated caller");
    let target = serde_json::to_value(&request.target)?;
    let mut payload = serde_json::json!({"title":request.title,"instruction":request.instruction,"task_payload":request.payload,"thread_key":request.thread_key,"priority":request.priority,"mode":"data","native_target":target});
    let member = if let Target::Crew {
        member_id,
        executor_id,
        harness,
        timeout_seconds,
    } = &request.target
    {
        permission(root, session, "ctox.crew.execution.claim", false)?;
        let external = crew_execution::validated_target(
            serde_json::json!({"executor_id":executor_id,"harness":harness,"timeout_seconds":timeout_seconds}),
        )?;
        payload["external_executor"] = serde_json::to_value(external)?;
        Some(member_id.as_str())
    } else {
        None
    };
    // CAP-4's native atomic command/queue primitive, through the existing prompt/auth adapter.
    let identity = serde_json::to_vec(&(&user.id, &request.idempotency_key))?;
    let id = format!("native-task:{:x}", sha2::Sha256::digest(identity));
    let command = store::BusinessCommand {
        id: Some(id.clone()),
        module: "ctox".into(),
        command_type: "business_os.chat.task".into(),
        record_id: None,
        payload,
        client_context: serde_json::json!({"actor":{"id":user.id,"role":user.role,"active":true},"channel":"native_v4","request_id":id}),
        origin: store::CommandOrigin::ReplicatedPeer,
    };
    let authorization = store::queue_command_native_authorization(root, &command, Some(session))?;
    let claim = store::create_native_target_queue_task(
        root,
        &id,
        &command,
        authorization.as_ref(),
        member,
    )?
    .ok_or_else(|| rejected("unavailable", "task_not_created"))?;
    Ok(
        serde_json::json!({"task_id":claim.task.message_key,"command_id":id,"target":target,"status":claim.task.route_status}),
    )
}

#[cfg(test)]
#[path = "mcp_native_tasks_tests.rs"]
pub(crate) mod tests;
