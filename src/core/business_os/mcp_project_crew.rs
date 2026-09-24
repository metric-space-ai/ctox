//! Project execution ingress. Existing command admission owns queueing and retries.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartRequest {
    thread_id: String,
    title: String,
    instruction: String,
    harness: String,
    timeout_seconds: u64,
    idempotency_key: String,
    #[serde(default, rename = "_context")]
    _context: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartNativeProjectRequest {
    project_id: String,
    title: String,
    instruction: String,
    idempotency_key: String,
    #[serde(default, rename = "_context")]
    _context: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelNativeProjectRequest {
    target_command_id: String,
    idempotency_key: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default, rename = "_context")]
    _context: Option<Value>,
}

pub(super) fn native_project_cancel_descriptor() -> BusinessOsMcpToolDescriptor {
    write_tool(
        "business_os.cancel_project_task",
        "Cancel an owned Workjet native task admitted by start_project_task, start_crew_execution, or a keyed ctox.delegate_task action. A repeated key returns the same cancellation command and native task result; cancellation may not undo side effects already started.",
        serde_json::json!({"type":"object","additionalProperties":false,
            "required":["target_command_id","idempotency_key"],
            "properties":{
                "target_command_id":{"type":"string","minLength":1,"maxLength":256},
                "idempotency_key":{"type":"string","pattern":"^[A-Za-z0-9][A-Za-z0-9._:-]{0,255}$"},
                "reason":{"type":"string","maxLength":512}
            }}),
    )
}

pub(super) fn cancel_native_project(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    let request: CancelNativeProjectRequest = serde_json::from_value(arguments.clone())?;
    let target_command_id = request.target_command_id.trim();
    anyhow::ensure!(
        (target_command_id.starts_with("workjet_project_native_")
            || target_command_id.starts_with("workjet_crew_")
            || target_command_id.starts_with("ctox_delegate_"))
            && target_command_id.len() <= 256,
        "native project target command is required"
    );
    let key = request.idempotency_key.as_bytes();
    anyhow::ensure!(
        !key.is_empty()
            && key.len() <= 256
            && key[0].is_ascii_alphanumeric()
            && key
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b':' | b'-')),
        "invalid project cancellation idempotency key"
    );
    let reason = request
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("cancelled by user");
    anyhow::ensure!(
        reason.len() <= 512,
        "project cancellation reason exceeds limit"
    );
    let actor = resolved_mcp_actor_context(root, context)?;
    anyhow::ensure!(
        actor.get("active").and_then(Value::as_bool) == Some(true),
        "project task actor is inactive"
    );
    let owner = required_arg(&actor, "id")?;
    enforce_collection_policy(root, "business_commands")?;
    enforce_module_policy(root, "ctox")?;
    let target = crate::mission::channels::inspect_business_command(root, target_command_id)?
        .context("native project target command was not found")?;
    let canonical = crate::mission::channels::business_command_projection(root, target_command_id)?;
    // The core command projection deliberately redacts actor identity for audit.
    // Read ownership from the admitted native command, then bind it back to the
    // projected command before authorizing cancellation.
    let admitted = store::load_business_command(&store::open_store(root)?, target_command_id)?;
    let native_project = target_command_id.starts_with("workjet_project_native_")
        && canonical
            .pointer("/payload/project_id")
            .and_then(Value::as_str)
            .is_some()
        && canonical.pointer("/payload/external_executor").is_none();
    let project_crew = target_command_id.starts_with("workjet_crew_")
        && canonical
            .pointer("/payload/thread_id")
            .and_then(Value::as_str)
            .is_some_and(|thread| thread.starts_with("workjet_private_"))
        && canonical
            .pointer("/payload/workjet_crew_member_id")
            .and_then(Value::as_str)
            .is_some()
        && canonical
            .pointer("/payload/external_executor")
            .is_some_and(Value::is_object);
    let app_linked = if target_command_id.starts_with("ctox_delegate_")
        && canonical["command_type"] == "ctox.delegate_task"
        && canonical["module"]
            .as_str()
            .is_some_and(|module| !module.is_empty())
    {
        delegate_action_command_id(
            &admitted.module,
            &admitted.command_type,
            &admitted.client_context,
            &admitted.client_context["actor"],
        )?
        .as_deref()
            == Some(target_command_id)
    } else {
        false
    };
    let native_chat = (native_project || project_crew)
        && canonical["module"] == "ctox"
        && canonical["command_type"] == "business_os.chat.task"
        && canonical
            .pointer("/payload/workjet_request_fingerprint")
            .and_then(Value::as_str)
            .is_some();
    anyhow::ensure!(
        (native_chat || app_linked)
            && canonical["module"].as_str() == Some(admitted.module.as_str())
            && canonical["command_type"].as_str() == Some(admitted.command_type.as_str())
            && canonical["record_id"].as_str().filter(|id| !id.is_empty())
                == admitted.record_id.as_deref()
            && admitted.payload == canonical["payload"]
            && admitted
                .client_context
                .pointer("/actor/id")
                .and_then(Value::as_str)
                == Some(owner.as_str()),
        "native project command is not owned by this actor"
    );
    let task_id = target
        .get("execution_task_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .context("native project command has no task")?;
    let mut authorized = context.clone();
    authorized.actor = owner.clone();
    authorized.trusted_role = actor
        .get("role")
        .and_then(Value::as_str)
        .map(normalize_role);
    let decision = trusted_mcp_actor_policy_decision(
        root,
        &authorized,
        BusinessOsPermission::CtoxTaskManage,
        BusinessOsScopeType::Task,
        Some(task_id),
    )?;
    anyhow::ensure!(
        decision.allowed,
        "project task management denied: {}",
        decision.display_reason
    );
    let identity = serde_json::to_vec(&(&owner, target_command_id, &request.idempotency_key))?;
    let command_id = format!(
        "workjet_project_cancel_{}",
        URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, &identity).as_ref())
    );
    let payload = serde_json::json!({"target_command_id":target_command_id,"reason":reason});
    let replay = crate::mission::channels::inspect_business_command(root, &command_id)?.is_some();
    if replay {
        let existing = crate::mission::channels::business_command_projection(root, &command_id)?;
        anyhow::ensure!(
            existing["payload"] == payload,
            "project cancellation key conflicts with existing intent"
        );
    } else {
        anyhow::ensure!(
            !matches!(
                canonical["status"].as_str(),
                Some("completed" | "failed" | "cancelled")
            ),
            "native project task is already terminal"
        );
    }
    let accepted = store::accept_rxdb_business_command(
        root,
        serde_json::json!({
            "id":command_id,"command_id":command_id,"module":"ctox",
            "command_type":"ctox.command.cancel","payload":payload,
            "client_context":{"actor":actor,"channel":context.channel,"surface":context.surface,
                "workspace":context.workspace,"mcp_actor":context.actor,"request_id":command_id}
        }),
    )?;
    anyhow::ensure!(
        accepted.get("ok").and_then(Value::as_bool) != Some(false),
        "native project cancellation was rejected: {}",
        accepted.get("status").unwrap_or(&Value::Null)
    );
    let cancellation = crate::mission::channels::business_command_projection(root, &command_id)?;
    anyhow::ensure!(
        cancellation["payload"] == payload
            && cancellation["result"]["target_command_id"] == target_command_id
            && cancellation["result"]["execution_task_id"] == task_id
            && cancellation["status"] == "completed",
        "project cancellation has no matching native receipt"
    );
    let target_after =
        crate::mission::channels::business_command_projection(root, target_command_id)?;
    anyhow::ensure!(
        target_after["status"] == "cancelled",
        "native project task is not cancelled"
    );
    Ok(serde_json::json!({
        "schema":"ctox.native_project_cancel.v1",
        "command_id":command_id,
        "target_command_id":target_command_id,
        "task_id":task_id,
        "status":cancellation["status"],
        "target_status":target_after["status"],
        "side_effects_may_have_started":cancellation["result"]["side_effects_may_have_started"]
    }))
}

pub(super) fn native_project_descriptor() -> BusinessOsMcpToolDescriptor {
    write_tool(
        "business_os.start_project_task",
        "Start one durable native CTOX task for an owned Workjet project without requiring a Business OS app, Crew member, execution computer or external harness. Repeating the same request key returns the same command and task.",
        serde_json::json!({"type":"object","additionalProperties":false,
            "required":["project_id","title","instruction","idempotency_key"],
            "properties":{
                "project_id":{"type":"string","minLength":1,"maxLength":128},
                "title":{"type":"string","minLength":1,"maxLength":256},
                "instruction":{"type":"string","minLength":1,"maxLength":16000},
                "idempotency_key":{"type":"string","pattern":"^[A-Za-z0-9][A-Za-z0-9._:-]{0,255}$"}
            }}),
    )
}

pub(super) fn start_native_project(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    let request: StartNativeProjectRequest = serde_json::from_value(arguments.clone())?;
    anyhow::ensure!(
        !request.title.trim().is_empty()
            && request.title.len() <= 256
            && !request.instruction.trim().is_empty()
            && request.instruction.len() <= 16000,
        "project task text is empty or exceeds limits"
    );
    let key = request.idempotency_key.as_bytes();
    anyhow::ensure!(
        !key.is_empty()
            && key.len() <= 256
            && key[0].is_ascii_alphanumeric()
            && key
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b':' | b'-')),
        "invalid project request idempotency key"
    );
    let actor = resolved_mcp_actor_context(root, context)?;
    anyhow::ensure!(
        actor.get("active").and_then(Value::as_bool) == Some(true),
        "project task actor is inactive"
    );
    let owner = required_arg(&actor, "id")?;
    enforce_collection_policy(root, "workjet_projects")?;
    enforce_module_policy(root, "ctox")?;
    let mut conn = rusqlite::Connection::open_with_flags(
        store::business_os_store_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let tx = conn.transaction()?;
    let project =
        super::super::project_chats::owned_project(&tx, &request.project_id, &owner, true)?;
    tx.commit()?;
    let project_id = required_arg(&project, "id")?;
    let mut authorized = context.clone();
    authorized.actor = owner.clone();
    authorized.trusted_role = actor
        .get("role")
        .and_then(Value::as_str)
        .map(normalize_role);
    enforce_business_os_mcp_policy(
        root,
        &authorized,
        "business_os.start_project_task",
        arguments,
    )?;
    let identity = serde_json::to_vec(&(&owner, &project_id, &request.idempotency_key))?;
    let command_id = format!(
        "workjet_project_native_{}",
        URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, &identity).as_ref())
    );
    let mut payload = serde_json::json!({
        "project_id":project_id,
        "title":request.title,
        "instruction":request.instruction,
        "mode":"data"
    });
    let fingerprint = URL_SAFE_NO_PAD
        .encode(digest::digest(&digest::SHA256, &serde_json::to_vec(&payload)?).as_ref());
    payload["workjet_request_fingerprint"] = Value::String(fingerprint.clone());
    if crate::mission::channels::inspect_business_command(root, &command_id)?.is_some() {
        let existing = crate::mission::channels::business_command_projection(root, &command_id)?;
        anyhow::ensure!(
            existing
                .pointer("/payload/workjet_request_fingerprint")
                .and_then(Value::as_str)
                == Some(fingerprint.as_str()),
            "project request key conflicts with existing intent"
        );
    }
    let accepted = store::accept_rxdb_business_command(
        root,
        serde_json::json!({
            "id":command_id,"command_id":command_id,"module":"ctox",
            "command_type":"business_os.chat.task","payload":payload,
            "client_context":{"actor":actor,"channel":context.channel,"surface":context.surface,
                "workspace":context.workspace,"mcp_actor":context.actor,"request_id":command_id}
        }),
    )?;
    anyhow::ensure!(
        accepted.get("ok").and_then(Value::as_bool) != Some(false),
        "native project command was rejected: {}",
        accepted.get("status").unwrap_or(&Value::Null)
    );
    let canonical = crate::mission::channels::business_command_projection(root, &command_id)?;
    anyhow::ensure!(
        canonical
            .pointer("/payload/workjet_request_fingerprint")
            .and_then(Value::as_str)
            == Some(fingerprint.as_str()),
        "project request key conflicts with admitted intent"
    );
    Ok(serde_json::json!({
        "schema":"ctox.native_project_task.v1",
        "project_id":project_id,
        "command_id":command_id,
        "task_id":accepted.get("task_id"),
        "status":accepted.get("status")
    }))
}

pub(super) fn descriptor() -> BusinessOsMcpToolDescriptor {
    write_tool("business_os.start_crew_execution",
        "Start an idempotent external Crew task in an existing private Workjet project chat. Crew identity and executor computer come from native bindings. This enqueues work; native admission and review own execution and completion.",
        serde_json::json!({"type":"object","additionalProperties":false,
            "required":["thread_id","title","instruction","harness","timeout_seconds","idempotency_key"],
            "properties":{
                "thread_id":{"type":"string","minLength":1,"maxLength":256},
                "title":{"type":"string","minLength":1,"maxLength":256},
                "instruction":{"type":"string","minLength":1,"maxLength":16000},
                "harness":{"type":"string","enum":["codex","claude","opencode","grok","cursor"]},
                "timeout_seconds":{"type":"integer","minimum":1,"maximum":600},
                "idempotency_key":{"type":"string","pattern":"^[A-Za-z0-9][A-Za-z0-9._:-]{0,255}$"}
            }}))
}

pub(super) fn start(
    root: &Path,
    context: &McpChannelRequestContext,
    arguments: &Value,
) -> anyhow::Result<Value> {
    let request: StartRequest = serde_json::from_value(arguments.clone())?;
    let bounded = |value: &str, max: usize| !value.trim().is_empty() && value.len() <= max;
    anyhow::ensure!(
        bounded(&request.thread_id, 256) && request.thread_id.starts_with("workjet_private_"),
        "an existing private project chat is required"
    );
    anyhow::ensure!(
        bounded(&request.title, 256) && bounded(&request.instruction, 16000),
        "project task text is empty or exceeds limits"
    );
    let key = request.idempotency_key.as_bytes();
    anyhow::ensure!(
        !key.is_empty()
            && key.len() <= 256
            && key[0].is_ascii_alphanumeric()
            && key
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b':' | b'-')),
        "invalid project request idempotency key"
    );
    let actor = resolved_mcp_actor_context(root, context)?;
    anyhow::ensure!(
        actor.get("active").and_then(Value::as_bool) == Some(true),
        "project task actor is inactive"
    );
    let owner = required_arg(&actor, "id")?;
    let mut authorized = context.clone();
    authorized.trusted_role = actor
        .get("role")
        .and_then(Value::as_str)
        .map(normalize_role);
    for collection in [
        "workjet_projects",
        "workjet_project_chats",
        "workjet_project_workers",
        "workjet_worker_profile_bindings",
        "workjet_computers",
        "ctox_crew_members",
    ] {
        enforce_collection_policy(root, collection)?;
    }
    enforce_module_policy(root, "ctox")?;
    anyhow::ensure!(
        !crew_read_is_public(root, &authorized, "ctox_crew_members")?,
        "project execution requires private Crew access"
    );
    let mut conn = rusqlite::Connection::open_with_flags(
        store::business_os_store_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let tx = conn.transaction()?;
    let target = super::super::project_crew::target_for_chat(&tx, &owner, &request.thread_id)?;
    tx.commit()?;
    let core = crew_context::open_read_connection(root)?;
    anyhow::ensure!(
        crate::crew::members(&core)?
            .iter()
            .any(|m| m.id == target.member_id && !m.archived),
        "project Crew member is unavailable or archived"
    );
    let executor = crew_execution::validated_target(serde_json::json!({
        "executor_id":target.computer_id,"harness":request.harness,"timeout_seconds":request.timeout_seconds
    }))?;
    let identity = serde_json::to_vec(&(&owner, &request.thread_id, &request.idempotency_key))?;
    let command_id = format!(
        "workjet_crew_{}",
        URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, &identity).as_ref())
    );
    let mut payload = serde_json::json!({"thread_id":request.thread_id,"title":request.title,
        "instruction":request.instruction,"mode":"data","workjet_crew_member_id":target.member_id,
        "external_executor":executor});
    let fingerprint = URL_SAFE_NO_PAD
        .encode(digest::digest(&digest::SHA256, &serde_json::to_vec(&payload)?).as_ref());
    payload["workjet_request_fingerprint"] = Value::String(fingerprint.clone());
    if crate::mission::channels::inspect_business_command(root, &command_id)?.is_some() {
        let existing = crate::mission::channels::business_command_projection(root, &command_id)?;
        anyhow::ensure!(
            existing
                .pointer("/payload/workjet_request_fingerprint")
                .and_then(Value::as_str)
                == Some(fingerprint.as_str()),
            "project request key conflicts with existing intent"
        );
    }
    let accepted = store::accept_rxdb_business_command(
        root,
        serde_json::json!({
            "id":command_id,"command_id":command_id,"module":"ctox","command_type":"business_os.chat.task",
            "payload":payload,
            "client_context":{"actor":actor,"channel":context.channel,"surface":context.surface,
                "workspace":context.workspace,"mcp_actor":context.actor,"request_id":command_id}
        }),
    )?;
    anyhow::ensure!(
        accepted.get("ok").and_then(Value::as_bool) != Some(false),
        "project Crew command was rejected: {}",
        accepted.get("status").unwrap_or(&Value::Null)
    );
    // Another caller can win admission after the preflight lookup. The command
    // plane may then return its existing receipt without comparing our payload.
    // Only acknowledge the immutable canonical intent that actually won.
    let canonical = crate::mission::channels::business_command_projection(root, &command_id)?;
    anyhow::ensure!(
        canonical
            .pointer("/payload/workjet_request_fingerprint")
            .and_then(Value::as_str)
            == Some(fingerprint.as_str()),
        "project request key conflicts with admitted intent"
    );
    Ok(
        serde_json::json!({"schema":"ctox.project_crew_request.v1","command_id":command_id,
        "thread_id":request.thread_id,"crew_member_id":target.member_id,"executor_id":target.computer_id,
        "status":accepted.get("status"),"task_id":accepted.get("task_id")}),
    )
}
