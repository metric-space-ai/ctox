// Origin: CTOX
// License: AGPL-3.0-only

//! Bounded controls for the existing Workjet supervisor. The native Threads
//! producer owns execution; this adapter does not create a transfer session.
use super::super::session::{session_user_id, BusinessOsSession};
use super::super::store::{self, CommandOrigin};
use super::*;
use crate::mission::channels;

const CONTRACT: &str = "ctox.workjet.supervisor_turn.v1";
const CAPABILITIES_CONTRACT: &str = "ctox.workjet.supervisor_turn_capabilities.v1";

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TurnKind {
    #[default]
    Work,
    Conversation,
}

impl TurnKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Conversation => "conversation",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteReadPayload {
    project_id: String,
    thread_id: String,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitPayload {
    project_id: String,
    thread_id: String,
    goal: String,
    #[serde(default)]
    turn_kind: TurnKind,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplyProvenance {
    kind: TurnKind,
    submit_command_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilitiesPayload {
    project_id: String,
    thread_id: String,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservePayload {
    project_id: String,
    thread_id: String,
    target_command_id: String,
    #[serde(default)]
    execution_page:
        Option<super::super::workjet_supervisor_execution_contract::ExecutionPageRequest>,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelPayload {
    project_id: String,
    thread_id: String,
    target_command_id: String,
    reason: String,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

pub(in crate::business_os) fn is_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "ctox.workjet.project.supervisor.turn.submit"
            | "ctox.workjet.project.supervisor.turn.watch"
            | "ctox.workjet.project.supervisor.turn.history"
            | "ctox.workjet.project.supervisor.turn.cancel"
            | "ctox.workjet.project.supervisor.turn.capabilities"
            | "ctox.workjet.project.supervisor.route.read.v1"
    )
}

pub(super) fn binding(
    root: &Path,
    owner: &str,
    project_id: &str,
    thread_id: &str,
    active: bool,
) -> anyhow::Result<supervisor_binding::SupervisorBinding> {
    let conn = open_store(root)?;
    binding_from_connection(&conn, owner, project_id, thread_id, active)
}

pub(in crate::business_os) fn binding_from_connection(
    conn: &Connection,
    owner: &str,
    project_id: &str,
    thread_id: &str,
    active: bool,
) -> anyhow::Result<supervisor_binding::SupervisorBinding> {
    let project_id = required(project_id, "project_id", 128)?;
    let thread_id = required(thread_id, "thread_id", 36)?;
    owned_project(conn, &project_id, owner, active)?;
    let binding = supervisor_binding::for_thread(&conn, owner, &thread_id)?
        .context("register this project's existing supervisor before submitting a turn")?;
    ensure!(
        binding.project_id == project_id,
        "supervisor belongs to another project"
    );
    let thread = outbound_load_record(&conn, THREADS, &thread_id)?
        .context("registered supervisor history is unavailable")?;
    ensure!(
        thread["is_deleted"] != true
            && thread["owner_user_id"] == owner
            && thread["source_module"] == "ctox"
            && thread["source_record_type"] == "workjet_project"
            && thread["source_record_id"] == project_id,
        "supervisor history conflicts with its native binding"
    );
    Ok(binding)
}

fn owned_turn(
    root: &Path,
    owner: &str,
    binding: &supervisor_binding::SupervisorBinding,
    command_id: &str,
) -> anyhow::Result<Value> {
    let id = required(command_id, "target_command_id", 256)?;
    let context =
        channels::inspect_business_command(root, &id)?.context("supervisor turn was not found")?;
    let canonical = &context["command"];
    // Audit projections redact actor identity. Ownership comes from the
    // admitted native envelope and must agree with the canonical ledger.
    let admitted = store::load_business_command(&open_store(root)?, &id)?;
    ensure!(
        admitted.module == "ctox"
            && admitted.command_type == "business_os.chat.task"
            && admitted.record_id.as_deref() == Some(binding.project_id.as_str())
            && admitted.payload["thread_id"] == binding.thread_id
            && admitted.payload["thread_key"] == binding.thread_key
            && admitted.payload["risk_class"] == "internal"
            && admitted
                .client_context
                .pointer("/actor/id")
                .and_then(Value::as_str)
                == Some(owner)
            && canonical["module"] == admitted.module
            && canonical["command_type"] == admitted.command_type
            && canonical["record_id"] == binding.project_id
            && canonical["payload"] == admitted.payload,
        "turn does not belong to this owner's registered supervisor"
    );
    let queued = channels::load_queue_task_for_business_os_command(root, &id)?
        .context("supervisor turn has no durable queue task")?;
    ensure!(
        queued.thread_key == binding.thread_key
            && context["execution_task_id"] == queued.message_key,
        "supervisor turn has a conflicting native queue link"
    );
    let result = canonical.get("result").cloned().unwrap_or(Value::Null);
    let result_truncated = serde_json::to_vec(&result)?.len() > 64 * 1024;
    Ok(json!({
        "command_id": id, "task_id": queued.message_key,
        "thread_id": binding.thread_id, "thread_key": binding.thread_key,
        "execution_phase": canonical["execution_phase"], "status": canonical["status"],
        "queue_status": queued.route_status, "attempt": queued.attempt,
        "terminal": canonical["execution_phase"] == "terminal",
        "error_code": canonical["error_code"], "error_message": canonical["error_message"],
        "result": if result_truncated { Value::Null } else { result },
        "result_truncated": result_truncated,
    }))
}

/// Native policy for completing a dialogue reply, not the project's work.
/// Prompt text and a model's claimed status never confer this exemption.
pub(crate) fn reply_completion_allowed(root: &Path, canonical: &Value) -> anyhow::Result<bool> {
    let payload = &canonical["payload"];
    let empty_array = |field: &str| {
        payload
            .get(field)
            .is_none_or(|value| value.as_array().is_some_and(Vec::is_empty))
    };
    if canonical["command_type"] != "business_os.chat.task"
        || canonical["module"] != "ctox"
        || payload["risk_class"] != "internal"
        || payload.get("mode").is_some()
        || payload.get("writeback_contract").is_some()
        || payload.get("external_executor").is_some()
        || !empty_array("attachments")
        || !empty_array("dependencies")
    {
        return Ok(false);
    }
    let Some(provenance) = payload.get("supervisor_turn") else {
        return Ok(false);
    };
    let Ok(provenance) = serde_json::from_value::<ReplyProvenance>(provenance.clone()) else {
        return Ok(false);
    };
    if provenance.kind != TurnKind::Conversation {
        return Ok(false);
    }
    let Some(id) = canonical["command_id"].as_str() else {
        return Ok(false);
    };
    let conn = open_store(root)?;
    let admitted = store::load_business_command(&conn, id)?;
    let owner = admitted
        .client_context
        .pointer("/actor/id")
        .and_then(Value::as_str)
        .context("Supervisor reply has no admitted owner")?;
    let Some(thread_id) = admitted.payload["thread_id"].as_str() else {
        return Ok(false);
    };
    // Ordinary Business OS chats and coding workers have no Supervisor binding.
    let Some(binding) = supervisor_binding::for_thread(&conn, owner, thread_id)? else {
        return Ok(false);
    };
    let binding = binding_from_connection(&conn, owner, &binding.project_id, thread_id, true)?;
    ensure!(
        canonical["module"] == admitted.module
            && canonical["command_type"] == admitted.command_type
            && canonical["record_id"].as_str() == admitted.record_id.as_deref()
            && canonical["payload"] == admitted.payload,
        "Supervisor reply conflicts with its admitted native command"
    );
    let submitted = store::load_business_command(&conn, &provenance.submit_command_id)?;
    let request: SubmitPayload = serde_json::from_value(submitted.payload.clone())?;
    ensure!(
        submitted.module == "ctox"
            && submitted.command_type == "ctox.workjet.project.supervisor.turn.submit"
            && submitted
                .client_context
                .pointer("/actor/id")
                .and_then(Value::as_str)
                == Some(owner)
            && submitted
                .record_id
                .as_deref()
                .is_none_or(|id| id == binding.project_id)
            && request.project_id == binding.project_id
            && request.thread_id == binding.thread_id
            && request.turn_kind == TurnKind::Conversation
            && admitted.payload["user_message"].as_str() == Some(request.goal.trim()),
        "Supervisor conversation kind conflicts with its admitted Owner submit"
    );
    drop(conn);
    let turn = owned_turn(root, owner, &binding, id)?;
    let submission = channels::inspect_business_command(root, &provenance.submit_command_id)?
        .context("Supervisor conversation has no native submission receipt")?;
    let receipt = &submission["command"]["result"];
    ensure!(
        receipt["contract"] == CONTRACT
            && receipt["binding"]["project_id"] == binding.project_id
            && receipt["binding"]["thread_id"] == binding.thread_id
            && receipt["turn"]["command_id"] == id
            && receipt["turn"]["task_id"] == turn["task_id"],
        "Supervisor conversation provenance conflicts with its actual native submission receipt"
    );
    let result = &turn["result"];
    ensure!(
        matches!(
            turn["execution_phase"].as_str(),
            Some("awaiting_review" | "validating")
        ) && result["command_id"] == id
            && result["execution_task_id"] == turn["task_id"]
            && result["attempt"] == turn["attempt"]
            && result["user_reply"]
                .as_str()
                .is_some_and(|reply| !reply.trim().is_empty()),
        "Supervisor reply lacks a persisted response for this exact command, task and attempt"
    );
    Ok(true)
}

pub(in crate::business_os) fn control(
    root: &Path,
    command: &BusinessCommand,
    session: &BusinessOsSession,
) -> anyhow::Result<Value> {
    let owner = session_user_id(session).context("authenticated supervisor owner is required")?;
    match command.command_type.as_str() {
        "ctox.workjet.project.supervisor.route.read.v1" => {
            let request: RouteReadPayload = serde_json::from_value(command.payload.clone())?;
            super::super::mcp_channel::read_supervisor_configured_route(
                root,
                owner,
                &request.project_id,
                &request.thread_id,
            )
        }
        "ctox.workjet.project.supervisor.turn.capabilities" => {
            let request: CapabilitiesPayload = serde_json::from_value(command.payload.clone())?;
            let binding = binding(root, owner, &request.project_id, &request.thread_id, false)?;
            Ok(
                json!({"ok":true, "contract":CAPABILITIES_CONTRACT, "binding":binding,
                "turn_kinds":["work","conversation"], "default_turn_kind":"work"}),
            )
        }
        "ctox.workjet.project.supervisor.turn.history" => {
            super::supervisor_history::history(root, owner, command.payload.clone())
        }
        "ctox.workjet.project.supervisor.turn.submit" => {
            let request: SubmitPayload = serde_json::from_value(command.payload.clone())?;
            let binding = binding(root, owner, &request.project_id, &request.thread_id, true)?;
            let goal = required(&request.goal, "goal", 4096)?;
            let operation = command.id.as_deref().context("command id is required")?;
            let mut delegated = command.clone();
            delegated.module = "threads".to_owned();
            delegated.command_type = "threads.ai.request".to_owned();
            delegated.record_id = Some(binding.thread_id.clone());
            delegated.payload = json!({
                "thread_id": binding.thread_id, "goal": goal, "risk_class": "internal",
                "message_id": stable_id("workjet_supervisor_message", &[owner, operation]),
            });
            let result = super::super::threads::create_supervisor_ai_request(
                root,
                session,
                &delegated,
                request.turn_kind.as_str(),
                operation,
            )?;
            let ai_id = result["ai_command"]["command_id"]
                .as_str()
                .or_else(|| result["ai_command"]["id"].as_str())
                .context("native Threads producer did not return an accepted command")?;
            let turn = owned_turn(root, owner, &binding, ai_id)?;
            Ok(json!({"ok": true, "contract": CONTRACT, "binding": binding,
                "message_id": result["message"]["message_id"], "turn": turn}))
        }
        "ctox.workjet.project.supervisor.turn.watch" => {
            let request: ObservePayload = serde_json::from_value(command.payload.clone())?;
            let binding = binding(root, owner, &request.project_id, &request.thread_id, false)?;
            let turn = owned_turn(root, owner, &binding, &request.target_command_id)?;
            let mut response =
                json!({"ok": true, "contract": CONTRACT, "binding": binding, "turn": turn});
            // Installed v1 decoders reject excess properties. Add observer facts
            // only when the caller explicitly requests this separately versioned page.
            if let Some(page) = request.execution_page {
                let execution =
                    super::supervisor_observation::page(root, &response["turn"], &page)?;
                response["execution_contract"] =
                    json!(super::super::workjet_supervisor_execution_contract::CONTRACT_SCHEMA);
                response["execution_page"] = serde_json::to_value(execution)?;
            }
            Ok(response)
        }
        "ctox.workjet.project.supervisor.turn.cancel" => {
            let request: CancelPayload = serde_json::from_value(command.payload.clone())?;
            let binding = binding(root, owner, &request.project_id, &request.thread_id, false)?;
            let before = owned_turn(root, owner, &binding, &request.target_command_id)?;
            let reason = required(&request.reason, "reason", 512)?;
            let operation = command.id.as_deref().context("command id is required")?;
            let cancellation_id = stable_id("workjet_project_cancel", &[owner, operation]);
            let payload = json!({"target_command_id": request.target_command_id, "reason": reason});
            if channels::inspect_business_command(root, &cancellation_id)?.is_none() {
                ensure!(
                    before["terminal"] != true,
                    "supervisor turn is already terminal"
                );
            }
            let accepted = store::accept_rxdb_business_command_with_origin(
                root,
                json!({
                    "id": cancellation_id, "command_id": cancellation_id, "module": "ctox",
                    "command_type": "ctox.command.cancel", "payload": payload,
                    "client_context": {"actor": super::super::threads::actor_payload(session),
                        "source": "workjet-supervisor-control"},
                }),
                CommandOrigin::TrustedLocal,
            )?;
            let cancellation = channels::business_command_projection(root, &cancellation_id)?;
            ensure!(
                accepted["ok"] != false
                    && cancellation["status"] == "completed"
                    && cancellation["payload"] == payload
                    && cancellation["result"]["target_command_id"] == request.target_command_id
                    && cancellation["result"]["execution_task_id"] == before["task_id"],
                "native cancellation has no matching successful receipt"
            );
            let turn = owned_turn(root, owner, &binding, &request.target_command_id)?;
            ensure!(
                turn["status"] == "cancelled",
                "native supervisor turn is not cancelled"
            );
            Ok(
                json!({"ok": true, "contract": CONTRACT, "binding": binding, "turn": turn,
                "cancellation": {"command_id": cancellation_id,
                    "side_effects_may_have_started": cancellation["result"]["side_effects_may_have_started"],
                    "worker_interrupt_acknowledged": false}}),
            )
        }
        _ => anyhow::bail!("unsupported supervisor turn command"),
    }
}
