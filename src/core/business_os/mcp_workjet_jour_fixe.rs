// Origin: CTOX
// License: AGPL-3.0-only
//! Meeting tools for the actual, currently leased registered Supervisor.
//! Read-only snapshots do not take the issuer fence or a database writer lock.
use super::super::{project_chats::jour_fixe_owner, workjet_jour_fixe_contract as wire};
use super::*;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde_json::json;
use sha2::{Digest, Sha256};
use wire::WireValidate;

pub(super) const READ_TOOL: &str = "business_os.jour_fixe_read";
pub(super) const WRITE_TOOL: &str = "business_os.jour_fixe_update";
const MAX_METADATA_BYTES: usize = 1024 * 1024;
const OPERATIONS: &str = "CREATE TABLE IF NOT EXISTS workjet_jour_fixe_supervisor_operations (
 operation_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, meeting_id TEXT NOT NULL,
 intent_hash TEXT NOT NULL, receipt_json TEXT NOT NULL, command_id TEXT NOT NULL);";

#[derive(Deserialize)]
#[serde(
    tag = "action",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Update {
    PrepareDeck(wire::PublishDeckRequest),
    ProposeTodos(wire::ProposeTodosRequest),
}
#[derive(Deserialize)]
#[serde(
    tag = "action",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Read {
    ReadMeeting(wire::ReadMeetingRequest),
    ReadComments(wire::ReadMeetingRequest),
    ReadTranscript(wire::ReadMeetingRequest),
    ReadReport(wire::ReadMeetingRequest),
    ReadConfirmedGoal(wire::ReadConfirmedGoalRequest),
}

pub(super) fn allows(tool: &str, args: &Value) -> bool {
    matches!(
        (tool, args["action"].as_str()),
        (
            READ_TOOL,
            Some(
                "read_meeting"
                    | "read_comments"
                    | "read_transcript"
                    | "read_confirmed_goal"
                    | "read_report"
            )
        ) | (
            WRITE_TOOL,
            Some("prepare_deck" | "propose_todos" | "narrate" | "save_report")
        )
    )
}

// Describe the very same bounded DTOs used by both wire consumers, rather than
// maintaining a third, permissive copy of the meeting schema.
pub(super) fn schema(spec: &Value, kind: &str) -> Value {
    match kind {
        "String" => json!({"type":"string"}),
        "u64" => json!({"type":"integer","minimum":0,"maximum":9_007_199_254_740_991_u64}),
        "i64" => {
            json!({"type":"integer","minimum":-9_007_199_254_740_991_i64,"maximum":9_007_199_254_740_991_i64})
        }
        "f64" => json!({"type":"number"}),
        "bool" => json!({"type":"boolean"}),
        _ if kind.starts_with("Vec<") => {
            json!({"type":"array","items":schema(spec,&kind[4..kind.len()-1])})
        }
        _ => {
            let ty = &spec["types"][kind];
            if let Some(values) = ty.get("enum") {
                return json!({"type":"string","enum":values});
            }
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for (name, field) in ty["fields"].as_object().expect("fixture fields") {
                let mut value = schema(spec, field["type"].as_str().expect("fixture type"));
                for (source, target) in [
                    ("min_chars", "minLength"),
                    ("max_chars", "maxLength"),
                    ("min_items", "minItems"),
                    ("max_items", "maxItems"),
                    ("minimum", "minimum"),
                    ("maximum", "maximum"),
                ] {
                    if let Some(bound) = field.get(source) {
                        value[target] = bound.clone();
                    }
                }
                if field["optional"] == true {
                    value = json!({"anyOf":[value,{"type":"null"}]});
                } else {
                    required.push(name.clone());
                }
                properties.insert(name.clone(), value);
            }
            json!({"type":"object","additionalProperties":false,"properties":properties,"required":required})
        }
    }
}
fn descriptor_schema(actions: &[(&str, &str)]) -> Value {
    let spec: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"
    ))
    .expect("shared meeting fixture");
    json!({"type":"object","additionalProperties":false,"required":["action","request"],
        "properties":{"action":{"type":"string"},"request":{"type":"object"}},
        "oneOf":actions.iter().map(|(action,kind)| {
        let mut request = schema(&spec,kind);
        if *kind == "ReadMeetingRequest" {
            request["required"] = json!(["project_id","meeting_id"]);
            request["properties"]["meeting_id"] = json!({"type":"string","minLength":1,"maxLength":128});
        }
        if *action == "read_meeting" {
            request["properties"]["section"] = json!({"const":"report"});
        }
        json!({"type":"object","additionalProperties":false,"required":["action","request"],
            "properties":{"action":{"type":"string","const":action},"request":request}})
    }).collect::<Vec<_>>()})
}
pub(super) fn read_descriptor() -> BusinessOsMcpToolDescriptor {
    read_tool(READ_TOOL,
        "Read this registered Supervisor's current meeting plus bounded project configuration, retained comments/final transcript, or latest stored report (read_report; also read_meeting request section=report). Requires its signed, current native execution session and an explicit meeting_id for meeting sections; read_confirmed_goal takes an empty request and returns the actual current confirmed plan/progress or null. Returns stored evidence, never inferred speech or cross-project data.",
        descriptor_schema(&[("read_meeting","ReadMeetingRequest"),("read_comments","ReadMeetingRequest"),("read_transcript","ReadMeetingRequest"),("read_confirmed_goal","ReadConfirmedGoalRequest"),("read_report","ReadMeetingRequest")]))
}
pub(super) fn write_descriptor() -> BusinessOsMcpToolDescriptor {
    let mut descriptor = descriptor_schema(&[
        ("prepare_deck", "PublishDeckRequest"),
        ("propose_todos", "ProposeTodosRequest"),
        ("narrate", "NarrateRequest"),
    ]);
    let mut report_schema = super::super::meeting_report::report_schema();
    // Local JSON-schema refs resolve within the tool's outer schema.
    descriptor["$defs"] = report_schema
        .as_object_mut()
        .expect("report schema")
        .remove("$defs")
        .expect("report definitions");
    report_schema
        .as_object_mut()
        .expect("report schema")
        .remove("$id");
    descriptor["oneOf"].as_array_mut().expect("actions").push(json!({
        "type":"object","additionalProperties":false,"required":["action","request"],
        "properties":{"action":{"const":"save_report"},"request":{
            "type":"object","additionalProperties":false,"required":["operation_id","meeting_id","report"],
            "properties":{"operation_id":{"type":"string","minLength":1,"maxLength":128},
                "meeting_id":{"type":"string","minLength":1,"maxLength":128},"report":report_schema}}}
    }));
    write_tool(WRITE_TOOL,
        "Persist a deck draft (prepare_deck), review todo proposal (propose_todos), or schema-validated status report (save_report with request operation_id, meeting_id, report; schema ctox.workjet.meeting_report.v1). Reports retain independent revisions and idempotent receipts. Requires this registered Supervisor's signed current execution, exact meeting revision and stable operation_id. Publication queues native sequential narration outside the agent turn; drafts stay preparing until all audio is retained. Todos require an explicit owner and remain proposed. Compatibility action narrate synthesizes the exact stored slide via the configured native gateway and freezes authorized WAV bytes, hashes and native provenance before setting ready. No caller text/audio/model, goal confirmation, SQL or independent work admission.",
        descriptor)
}

/// Durable source identity, never a synthetic business command.
pub(super) fn execution_key(trusted: &Value) -> anyhow::Result<String> {
    if let Some(plan) = trusted
        .get("workjet_confirmed_plan")
        .filter(|v| !v.is_null())
    {
        return plan
            .pointer("/lease/task_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .context("native confirmed plan task missing");
    }
    required_arg(trusted, "command_id")
}

pub(super) fn bound_project(
    core: &Connection,
    policy: &Connection,
    context: &McpChannelRequestContext,
    trusted: &Value,
) -> anyhow::Result<(String, String, String)> {
    anyhow::ensure!(
        context.trusted_role_source.as_deref() == Some(MCP_INTERNAL_SESSION_AUTH_SOURCE)
            && trusted["workjet_supervisor_only"] == true,
        "meeting tool requires the restricted native supervisor session"
    );
    if trusted
        .get("workjet_confirmed_plan")
        .is_some_and(|v| !v.is_null())
    {
        return workjet_confirmed_plan::bound_project(core, policy, context, trusted);
    }
    let command_id = required_arg(trusted, "command_id")?;
    let expected: workjet_worker_dispatch::SupervisorLease =
        serde_json::from_value(trusted["workjet_supervisor_lease"].clone())?;
    anyhow::ensure!(
        workjet_worker_dispatch::current_lease(core, &command_id)? == expected,
        "supervisor execution lease replaced or expired"
    );
    let command = crate::channels::business_command_projection_from_conn(core, &command_id)?;
    anyhow::ensure!(
        command["payload_hash"] == required_arg(trusted, "payload_hash")?
            && command["execution_phase"] != "terminal",
        "supervisor command changed or completed"
    );
    if trusted.get("crew_binding").is_some_and(|v| !v.is_null()) {
        current_guest_crew_command(core, trusted)?;
    }
    let admitted = store::load_business_command(policy, &command_id)?;
    anyhow::ensure!(
        admitted.module == "ctox"
            && admitted.command_type == "business_os.chat.task"
            && admitted.payload == command["payload"]
            && admitted.payload["risk_class"] == "internal"
            && admitted
                .client_context
                .pointer("/actor/id")
                .and_then(Value::as_str)
                == Some(context.actor.as_str()),
        "native supervisor command provenance differs"
    );
    let project = admitted.record_id.context("supervisor project missing")?;
    let thread = admitted.payload["thread_id"]
        .as_str()
        .context("supervisor thread missing")?
        .to_owned();
    let thread_key = admitted.payload["thread_key"]
        .as_str()
        .context("supervisor thread key missing")?
        .to_owned();
    let epoch = workjet_worker_dispatch::current_project(policy, context, &project, &thread)?;
    anyhow::ensure!(
        trusted["workjet_supervisor_epoch"] == epoch,
        "supervisor authority changed"
    );
    Ok((project, thread, thread_key))
}
pub(super) fn current_meeting(
    core: &Connection,
    policy: &Connection,
    context: &McpChannelRequestContext,
    trusted: &Value,
    id: &str,
    writing: bool,
) -> anyhow::Result<wire::Meeting> {
    let (project, thread, thread_key) = bound_project(core, policy, context, trusted)?;
    let meeting = super::super::project_chats::jour_fixe_confirmed_goal::overlay_from_core(
        core,
        jour_fixe_owner::owned(policy, &context.actor, Some(&project), id)?,
    )?;
    anyhow::ensure!(
        meeting.supervisor.workjet_thread_id == thread
            && meeting.supervisor.ctox_thread_key == thread_key,
        "meeting belongs to another supervisor execution"
    );
    let role = context
        .trusted_role
        .as_deref()
        .context("native role missing")?;
    let permission = if writing {
        BusinessOsPermission::DataWrite
    } else {
        BusinessOsPermission::DataRead
    };
    anyhow::ensure!(
        super::super::store_policy::trusted_actor_policy_decision_with_conn(
            policy,
            &context.actor,
            role,
            permission,
            BusinessOsScopeType::Record,
            Some(&meeting.project_id)
        )?
        .allowed,
        "native meeting data policy denied"
    );
    Ok(meeting)
}

pub(super) fn read_confirmed_goal_in_native_scope(
    core: &Connection,
    policy: &Connection,
    context: &McpChannelRequestContext,
    trusted: &Value,
) -> anyhow::Result<Value> {
    let (project, thread, thread_key) = bound_project(core, policy, context, trusted)?;
    let role = context
        .trusted_role
        .as_deref()
        .context("native role missing")?;
    anyhow::ensure!(
        super::super::store_policy::trusted_actor_policy_decision_with_conn(
            policy,
            &context.actor,
            role,
            BusinessOsPermission::DataRead,
            BusinessOsScopeType::Record,
            Some(&project)
        )?
        .allowed,
        "native project goal data policy denied"
    );
    let goal = super::super::project_chats::jour_fixe_confirmed_goal::current_goal_for_supervisor(
        core,
        &context.actor,
        &project,
        &thread_key,
    )?;
    Ok(
        json!({"contract":wire::CONTRACT_SCHEMA,"project_id":project,
        "supervisor_thread_id":thread,"confirmed_goal":goal}),
    )
}

pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    tool: &str,
    arguments: &Value,
    trusted: Option<&Value>,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        allows(tool, arguments),
        "unsupported supervisor meeting action"
    );
    let trusted = trusted.context("native supervisor session unavailable")?;
    anyhow::ensure!(
        context.trusted_role_source.as_deref() == Some(MCP_INTERNAL_SESSION_AUTH_SOURCE)
            && trusted["workjet_supervisor_only"] == true,
        "meeting tool requires the restricted native supervisor session"
    );
    anyhow::ensure!(
        serde_json::to_vec(arguments)?.len() <= MAX_METADATA_BYTES,
        "meeting request exceeds native write budget"
    );
    if tool == WRITE_TOOL && arguments["action"] == "narrate" {
        return workjet_narration::execute(root, context, arguments, trusted);
    }
    let writing = tool == WRITE_TOOL;
    // Core before Policy, matching the native execution/cancellation lock order.
    // Only a mutation holds a Core writer reservation while the Policy edit
    // linearizes. Reads use existing read-only snapshots, with no schema repair.
    let flags = if writing {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let mut core = Connection::open_with_flags(crate::paths::core_db(root), flags)?;
    core.busy_timeout(std::time::Duration::from_secs(5))?;
    let mut policy = Connection::open_with_flags(store::business_os_store_path(root), flags)?;
    policy.busy_timeout(std::time::Duration::from_secs(5))?;
    let behavior = if writing {
        TransactionBehavior::Immediate
    } else {
        TransactionBehavior::Deferred
    };
    let core_tx = core.transaction_with_behavior(behavior)?;
    let policy_tx = policy.transaction_with_behavior(behavior)?;
    if !writing {
        let mut read_arguments = arguments.clone();
        if read_arguments["action"] == "read_meeting" {
            if let Some(section) = read_arguments["request"]
                .as_object_mut()
                .and_then(|v| v.remove("section"))
            {
                anyhow::ensure!(section == "report", "unsupported meeting read section");
                read_arguments["action"] = json!("read_report");
            }
        }
        let read: Read = serde_json::from_value(read_arguments)?;
        let (request, section) = match read {
            Read::ReadMeeting(v) => (v, "meeting"),
            Read::ReadComments(v) => (v, "comments"),
            Read::ReadTranscript(v) => (v, "transcript"),
            Read::ReadReport(v) => (v, "report"),
            Read::ReadConfirmedGoal(v) => {
                v.validate().map_err(anyhow::Error::msg)?;
                return read_confirmed_goal_in_native_scope(&core_tx, &policy_tx, context, trusted);
            }
        };
        request.validate().map_err(anyhow::Error::msg)?;
        let id = request
            .meeting_id
            .as_deref()
            .context("explicit meeting_id is required")?;
        let meeting = current_meeting(&core_tx, &policy_tx, context, trusted, id, false)?;
        anyhow::ensure!(
            request.project_id == meeting.project_id,
            "meeting read project differs"
        );
        if section == "report" {
            return Ok(json!({"contract":super::super::meeting_report::SCHEMA,
                "meeting_id":meeting.id,"project_id":meeting.project_id,
                "report":super::super::meeting_report::latest(&policy_tx, &meeting.id)?}));
        }
        if section == "meeting" {
            let record =
                store::outbound_load_record(&policy_tx, "workjet_projects", &meeting.project_id)?
                    .context("native project configuration unavailable")?;
            let project = json!({"id":meeting.project_id,"name":record["name"],"repo_url":record["repo_url"],
                "public_url":record["public_url"],"info":{"summary":record["info"]["summary"],
                "goal":record["info"]["goal"],"phase":record["info"]["phase"]},
                "jour_fixe":{"weekday":record["jour_fixe"]["weekday"],"time":record["jour_fixe"]["time"],
                "timezone":record["jour_fixe"]["timezone"]}});
            anyhow::ensure!(
                serde_json::to_vec(&project)?.len() <= 64 * 1024,
                "project configuration exceeds meeting read budget"
            );
            let previous_goal_definition =
                super::super::project_chats::jour_fixe_confirmed_goal::goal_for_deck(
                    &core_tx, &meeting,
                )?;
            return Ok(
                json!({"contract":wire::CONTRACT_SCHEMA,"meeting":meeting,"project":project,
                "previous_goal_definition":previous_goal_definition,
                "narration_inputs":workjet_narration::inputs(&meeting),
                "worker_outcomes":workjet_worker_dispatch::outcomes_for_deck(&core_tx,&meeting)?}),
            );
        }
        return Ok(if section == "comments" {
            json!({"contract":wire::CONTRACT_SCHEMA,"meeting_id":meeting.id,"project_id":meeting.project_id,
                "revision":meeting.revision,"state":meeting.state,"comments":meeting.comments})
        } else {
            json!({"contract":wire::CONTRACT_SCHEMA,"meeting_id":meeting.id,"project_id":meeting.project_id,
                "revision":meeting.revision,"state":meeting.state,"transcript":meeting.transcript})
        });
    }
    if arguments["action"] == "save_report" {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            action: String,
            request: super::super::meeting_report::SaveReport,
        }
        let input: Input = serde_json::from_value(arguments.clone())?;
        anyhow::ensure!(input.action == "save_report", "unsupported report action");
        let meeting = current_meeting(
            &core_tx,
            &policy_tx,
            context,
            trusted,
            &input.request.meeting_id,
            true,
        )?;
        let report = super::super::meeting_report::save(
            &policy_tx,
            &input.request,
            &meeting.id,
            &meeting.project_id,
            &meeting.supervisor.workjet_thread_id,
        )?;
        policy_tx.commit()?;
        core_tx.commit()?;
        return Ok(
            json!({"ok":true,"contract":super::super::meeting_report::SCHEMA,"report":report}),
        );
    }
    let update: Update = serde_json::from_value(arguments.clone())?;
    let (operation, id, expected) = match &update {
        Update::PrepareDeck(v) => {
            v.validate().map_err(anyhow::Error::msg)?;
            (&v.operation_id, &v.meeting_id, v.expected_revision)
        }
        Update::ProposeTodos(v) => {
            v.validate().map_err(anyhow::Error::msg)?;
            (&v.operation_id, &v.meeting_id, v.expected_revision)
        }
    };
    anyhow::ensure!(
        operation.trim() == operation && id.trim() == id,
        "meeting identity must be canonical"
    );
    let mut meeting = current_meeting(&core_tx, &policy_tx, context, trusted, id, true)?;
    policy_tx.execute_batch(OPERATIONS)?;
    let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(arguments)?));
    let prior: Option<(String,String,String,String)> = policy_tx.query_row(
        "SELECT owner_user_id,meeting_id,intent_hash,receipt_json FROM workjet_jour_fixe_supervisor_operations WHERE operation_id=?1",
        [operation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    if let Some((owner, prior_meeting, intent, raw)) = prior {
        anyhow::ensure!(
            owner == meeting.owner_user_id && prior_meeting == meeting.id && intent == hash,
            "meeting operation intent conflicts"
        );
        let receipt = serde_json::from_str(&raw)?;
        if let Update::PrepareDeck(request) = &update {
            if meeting.deck_revision == request.deck_revision
                && matches!(
                    meeting.state,
                    wire::MeetingState::Preparing | wire::MeetingState::Ready
                )
            {
                // A pre-upgrade publication receipt can admit its native job
                // on authenticated replay; existing authority remains pinned.
                deck_narration::enqueue(&policy_tx, context, &meeting, operation)?;
                deck_narration::retry_publication(&policy_tx, operation)?;
            }
        }
        policy_tx.commit()?;
        core_tx.commit()?;
        #[cfg(not(test))]
        if matches!(update, Update::PrepareDeck(_)) {
            if let Err(error) = deck_narration::resume(root) {
                eprintln!("[jour-fixe] native narration deferred: {error}");
            }
        }
        return Ok(receipt);
    }
    anyhow::ensure!(
        meeting.revision == expected,
        "meeting revision changed; read the current meeting"
    );
    let mut todos_revision = None;
    match &update {
        Update::PrepareDeck(request) => {
            anyhow::ensure!(
                matches!(
                    meeting.state,
                    wire::MeetingState::Planned | wire::MeetingState::Preparing
                ),
                "only an unpublished meeting accepts a deck draft"
            );
            anyhow::ensure!(
                meeting.comments.is_empty()
                    && meeting.transcript.is_empty()
                    && meeting.todos.is_none(),
                "deck replacement would discard retained meeting evidence"
            );
            anyhow::ensure!(
                request.deck_revision
                    == meeting
                        .deck_revision
                        .checked_add(1)
                        .context("deck revision overflow")?,
                "deck revision changed"
            );
            let mut ids = BTreeSet::new();
            for (position, slide) in request.slides.iter().enumerate() {
                anyhow::ensure!(
                    slide.meeting_id == meeting.id
                        && slide.position == position as u64
                        && ids.insert(&slide.id)
                        && !slide.id.trim().is_empty()
                        && !slide.title.trim().is_empty()
                        && !slide.body_markdown.trim().is_empty(),
                    "deck slide identity, order or content conflicts"
                );
                anyhow::ensure!(
                    slide.audio.is_none(),
                    "draft cannot claim unverified narration"
                );
            }
            meeting.slides = request.slides.clone();
            meeting.deck_revision = request.deck_revision;
            meeting.state = wire::MeetingState::Preparing;
        }
        Update::ProposeTodos(request) => {
            anyhow::ensure!(
                meeting.state == wire::MeetingState::Review,
                "meeting is not in review"
            );
            let next = if let Some(current) = &meeting.todos {
                anyhow::ensure!(
                    current.status == wire::TodoState::Proposed,
                    "confirmed goals cannot be replaced by a proposal"
                );
                current
                    .revision
                    .checked_add(1)
                    .context("todo revision overflow")?
            } else {
                1
            };
            anyhow::ensure!(
                request.proposal_revision == next,
                "todo proposal revision changed"
            );
            let evidence: BTreeSet<&str> = meeting
                .slides
                .iter()
                .map(|v| v.id.as_str())
                .chain(meeting.comments.iter().map(|v| v.id.as_str()))
                .chain(meeting.transcript.iter().map(|v| v.id.as_str()))
                .collect();
            let mut ids = BTreeSet::new();
            for todo in &request.items {
                anyhow::ensure!(
                    ids.insert(&todo.id)
                        && !evidence.contains(todo.id.as_str())
                        && todo
                            .evidence_ids
                            .iter()
                            .all(|id| evidence.contains(id.as_str()))
                        && todo.owner.as_deref().is_some_and(|v| !v.trim().is_empty())
                        && !todo.title.trim().is_empty()
                        && !todo.acceptance.trim().is_empty(),
                    "todo identity, owner or meeting evidence conflicts"
                );
            }
            todos_revision = Some(next);
            meeting.todos = Some(wire::TodoList {
                revision: next,
                status: wire::TodoState::Proposed,
                items: request.items.clone(),
                confirmed_by_user_id: None,
                confirmed_at_ms: None,
                goal: None,
                meeting_id: meeting.id.clone(),
            });
        }
    }
    meeting.revision = meeting
        .revision
        .checked_add(1)
        .context("meeting revision overflow")?;
    meeting.validate().map_err(anyhow::Error::msg)?;
    let raw = serde_json::to_string(&meeting)?;
    anyhow::ensure!(
        raw.len() <= MAX_METADATA_BYTES,
        "meeting metadata exceeds native write budget"
    );
    let receipt = json!({"ok":true,"contract":wire::CONTRACT_SCHEMA,"mutation":wire::MeetingMutationReceipt{
        operation_id:operation.clone(),meeting_id:meeting.id.clone(),project_id:meeting.project_id.clone(),
        revision:meeting.revision,state:meeting.state,changed_id:None,todos_revision}});
    anyhow::ensure!(policy_tx.execute(
        "UPDATE workjet_jour_fixe_meetings SET metadata_json=?2 WHERE meeting_id=?1 AND owner_user_id=?3",
        params![meeting.id,raw,meeting.owner_user_id])? == 1,"meeting disappeared");
    policy_tx.execute(
        "INSERT INTO workjet_jour_fixe_supervisor_operations(operation_id,owner_user_id,meeting_id,intent_hash,receipt_json,command_id) VALUES(?1,?2,?3,?4,?5,?6)",
        params![operation,meeting.owner_user_id,meeting.id,hash,serde_json::to_string(&receipt)?,execution_key(trusted)?])?;
    if matches!(update, Update::PrepareDeck(_)) {
        deck_narration::enqueue(&policy_tx, context, &meeting, operation)?;
    }
    policy_tx.commit()?;
    core_tx.commit()?;
    #[cfg(not(test))]
    if matches!(update, Update::PrepareDeck(_)) {
        if let Err(error) = deck_narration::resume(root) {
            eprintln!("[jour-fixe] native narration deferred: {error}");
        }
    }
    Ok(receipt)
}

#[cfg(test)]
#[path = "mcp_workjet_jour_fixe_tests.rs"]
mod tests;
