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
