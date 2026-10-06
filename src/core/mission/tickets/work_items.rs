// Self-work items and the workflow engine: item lifecycle (publish/
// assign/note/transition), spawn budgets, workflow start/steps/deltas/
// materialization, and the knowledge-entry internals they share. The
// P8a map lists self_work and workflow separately; the code interleaves
// them — the semantic wave may split further.

use super::{
    cached_ticket_self_work_list, cached_ticket_workflow_materialize_result,
    clear_ticket_self_work_list_cache, enforce_core_spawn, enforce_core_transition,
    ensure_core_transition_guard_schema, map_ticket_knowledge_entry_row,
    map_ticket_self_work_assignment_row, map_ticket_self_work_note_row, map_ticket_self_work_row,
    normalize_token, now_iso_string, open_ticket_db, parse_domain_csv, record_audit,
    record_harness_flow_event_lossy, resolve_db_path, stable_digest,
    store_ticket_self_work_list_cache, store_ticket_workflow_materialize_cache,
    ticket_self_work_list_cache_key, ticket_self_work_list_cache_stamp,
    ticket_store_change_stamp_for_path, ticket_workflow_materialize_cache_key, AuditRequest,
    CoreEntityType, CoreEvent, CoreEvidenceRefs, CoreSpawnRequest, CoreState,
    CoreTransitionRequest, RecordHarnessFlowEventRequest, RuntimeLane, TerminalPolicyGrant,
    TicketKnowledgeEntryView, TicketKnowledgeUpsertInput, TicketSelfWorkAssignmentView,
    TicketSelfWorkItemView, TicketSelfWorkNoteView, TicketSelfWorkUpsertInput,
    TicketWorkflowMaterializeResult, TicketWorkflowStartInput, TicketWorkflowStepInput,
    TicketWorkflowStepView, TicketWorkflowView, WorkItemStatus, WorkflowDelta, WORKFLOW_CASE_KIND,
    WORKFLOW_MATERIALIZE_DEFAULT_LIMIT, WORKFLOW_MAX_STEPS_PER_WORKFLOW,
    WORKFLOW_ORCHESTRATOR_SKILL, WORKFLOW_ROLE_CASE, WORKFLOW_ROLE_LEAF, WORKFLOW_ROLE_REDUCER,
    WORKFLOW_STEP_KIND,
};
#[cfg(test)]
use super::{
    record_ticket_self_work_assignment_batch_hydration_for_tests,
    record_ticket_self_work_list_cache_miss_for_tests,
    record_ticket_workflow_materialize_cache_miss_for_tests,
};
use crate::mission::plan;
use crate::mission::ticket_adapters;
use crate::mission::ticket_protocol;
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

fn parse_work_item_status(raw: &str) -> Result<WorkItemStatus> {
    WorkItemStatus::parse(raw)
        .ok_or_else(|| anyhow!("unsupported ticket work-item status `{}`", raw.trim()))
}

pub(crate) fn put_ticket_self_work_item(
    root: &Path,
    input: TicketSelfWorkUpsertInput,
    publish: bool,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let requested_status = parse_work_item_status(&input.state)?;
    let status = if publish {
        WorkItemStatus::Publishing
    } else {
        requested_status
    };
    let item = upsert_ticket_self_work_item_internal(
        root,
        &mut conn,
        TicketSelfWorkUpsertInput {
            source_system: input.source_system,
            kind: input.kind,
            title: input.title,
            body_text: input.body_text,
            state: status.as_str().to_string(),
            metadata: input.metadata,
        },
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_item_upsert",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "kind": item.kind,
                "state": item.state,
                "remote_ticket_id": item.remote_ticket_id,
            }),
        },
    )?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "ticket.self_work_created",
            title: "Ticket internal work item created",
            body_text: &item.title,
            message_key: self_work_message_key(&item),
            work_id: Some(&item.work_id),
            ticket_key: None,
            attempt_index: None,
            metadata: json!({
                "source_system": item.source_system,
                "kind": item.kind,
                "state": item.state,
                "remote_ticket_id": item.remote_ticket_id,
            }),
        },
    );
    if let Err(err) = enforce_ticket_self_work_spawn(&conn, &item) {
        let now = Utc::now().to_rfc3339();
        let fallback_status = if item.kind.to_ascii_lowercase().contains("review") {
            WorkItemStatus::Failed
        } else {
            WorkItemStatus::Blocked
        };
        let fallback_state = fallback_status.as_str();
        let fallback_reason = if fallback_status == WorkItemStatus::Failed {
            "ticket_self_work_spawn_rejected_terminal"
        } else {
            "ticket_self_work_spawn_rejected"
        };
        let failure_reason = err.to_string();
        let transition_result = enforce_ticket_self_work_state_transition(
            &conn,
            &item.work_id,
            &item.state,
            fallback_state,
            "ctox-core-spawn-gate",
            fallback_reason,
            if fallback_status == WorkItemStatus::Failed {
                Some(failure_reason.as_str())
            } else {
                None
            },
            None,
        );
        if let Err(transition_err) = transition_result {
            anyhow::bail!(
                "core spawn gate rejected ticket internal work `{}` ({}), and core state guard rejected fallback `{}` transition: {}; original spawn rejection: {}",
                item.work_id,
                item.kind,
                fallback_state,
                transition_err,
                err
            );
        }
        let _ = conn.execute(
            r#"
            UPDATE ticket_self_work_items
            SET state = ?2, updated_at = ?3
            WHERE work_id = ?1
            "#,
            params![&item.work_id, fallback_state, now],
        );
        clear_ticket_self_work_list_cache();
        anyhow::bail!(
            "core spawn gate rejected ticket internal work `{}` ({}): {}",
            item.work_id,
            item.kind,
            err
        );
    }
    if publish {
        publish_ticket_self_work_item(root, &item.work_id)
    } else {
        Ok(item)
    }
}

pub(super) fn enforce_ticket_self_work_spawn(
    conn: &Connection,
    item: &TicketSelfWorkItemView,
) -> Result<()> {
    let thread_key = metadata_string_value(&item.metadata, "thread_key")
        .or_else(|| metadata_string_value(&item.metadata, "queue_thread_key"))
        .unwrap_or_else(|| item.source_system.clone());
    let (parent_entity_type, parent_entity_id) = if let Some(parent_work_id) =
        metadata_string_value(&item.metadata, "parent_work_id")
            .or_else(|| metadata_string_value(&item.metadata, "ticket_self_work_id"))
    {
        ("WorkItem".to_string(), parent_work_id)
    } else if let Some(queue_message_key) =
        metadata_string_value(&item.metadata, "queue_message_key")
    {
        ("QueueTask".to_string(), queue_message_key)
    } else if let Some(parent_message_key) =
        metadata_string_value(&item.metadata, "parent_message_key")
            .or_else(|| metadata_string_value(&item.metadata, "inbound_message_key"))
    {
        ("Message".to_string(), parent_message_key)
    } else if !thread_key.trim().is_empty() {
        ("Thread".to_string(), thread_key.clone())
    } else {
        ("ControlPlane".to_string(), "ticket-self-work".to_string())
    };
    let (budget_key, max_attempts) =
        ticket_self_work_spawn_budget(&item.kind, &thread_key, &item.metadata);
    let mut edge_metadata = BTreeMap::new();
    edge_metadata.insert("thread_key".to_string(), thread_key);
    edge_metadata.insert("self_work_kind".to_string(), item.kind.clone());
    edge_metadata.insert("source_system".to_string(), item.source_system.clone());
    if let Some(source_label) = metadata_string_value(&item.metadata, "source_label") {
        edge_metadata.insert("source_label".to_string(), source_label);
    }
    if let Some(queue_message_key) = metadata_string_value(&item.metadata, "queue_message_key") {
        edge_metadata.insert("queue_message_key".to_string(), queue_message_key);
    }
    if let Some(workspace_root) = metadata_string_value(&item.metadata, "workspace_root") {
        edge_metadata.insert("workspace_root".to_string(), workspace_root);
    }
    if let Some(run_class) = metadata_string_value(&item.metadata, "core_run_class")
        .or_else(|| metadata_string_value(&item.metadata, "run_class"))
    {
        edge_metadata.insert("core_run_class".to_string(), run_class);
    }
    if let Some(dedupe_key) = metadata_string_value(&item.metadata, "dedupe_key") {
        edge_metadata.insert("dedupe_key".to_string(), dedupe_key);
    }

    enforce_core_spawn(
        conn,
        &CoreSpawnRequest {
            parent_entity_type,
            parent_entity_id,
            child_entity_type: "WorkItem".to_string(),
            child_entity_id: item.work_id.clone(),
            spawn_kind: format!("self-work:{}", item.kind),
            spawn_reason: "ticket_self_work_put".to_string(),
            actor: "ctox-ticket".to_string(),
            checkpoint_key: metadata_string_value(&item.metadata, "dedupe_key"),
            budget_key: Some(budget_key),
            max_attempts: Some(max_attempts),
            metadata: edge_metadata,
        },
    )?;
    Ok(())
}

pub(super) fn ticket_self_work_spawn_budget(
    kind: &str,
    thread_key: &str,
    metadata: &Value,
) -> (String, i64) {
    let lowered = kind.to_ascii_lowercase();
    if lowered.contains("review") {
        // A communication thread can carry many independent durable work
        // episodes over its lifetime. Spend the finite review budget against
        // the current parent episode, while leaving all historical spawn
        // edges untouched for audit.
        let episode = metadata_string_value(metadata, "work_episode_id")
            .or_else(|| metadata_string_value(metadata, "parent_work_id"))
            .or_else(|| metadata_string_value(metadata, "ticket_self_work_id"))
            .or_else(|| metadata_string_value(metadata, "queue_message_key"))
            .or_else(|| metadata_string_value(metadata, "parent_message_key"))
            .or_else(|| metadata_string_value(metadata, "inbound_message_key"))
            .or_else(|| metadata_string_value(metadata, "dedupe_key"))
            .unwrap_or_else(|| thread_key.to_string());
        return (format!("review-spawn:{kind}:episode:{episode}"), 5);
    }
    if kind == "founder-communication-rework" {
        let key = metadata_string_value(metadata, "inbound_message_key")
            .or_else(|| metadata_string_value(metadata, "parent_message_key"))
            .unwrap_or_else(|| thread_key.to_string());
        return (format!("founder-rework-spawn:{key}"), 2);
    }
    let key = metadata_string_value(metadata, "dedupe_key").unwrap_or_else(|| {
        format!(
            "{}:{}",
            thread_key,
            item_title_budget_component(metadata).unwrap_or_default()
        )
    });
    (format!("service-self-work-spawn:{kind}:{key}"), 64)
}

pub(super) fn item_title_budget_component(metadata: &Value) -> Option<String> {
    metadata_string_value(metadata, "title").map(|value| value.chars().take(80).collect())
}

pub(super) fn metadata_string_value(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn publish_ticket_self_work_item(
    root: &Path,
    work_id: &str,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let item = conn
        .query_row(
            r#"
            SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
            FROM ticket_self_work_items
            WHERE work_id = ?1
            LIMIT 1
            "#,
            params![work_id],
            map_ticket_self_work_row,
        )
        .optional()?
        .context("ticket internal work item not found")?;
    let adapter = ticket_adapters::adapter_for_system(&item.source_system)
        .context("no adapter available to publish ticket internal work item")?;
    if !adapter.capabilities().can_create_self_work_items {
        anyhow::bail!(
            "ticket adapter {} cannot publish internal work items",
            item.source_system
        );
    }
    if item.remote_ticket_id.is_some() {
        return Ok(item);
    }
    let published = adapter.publish_self_work_item(
        root,
        ticket_protocol::TicketSelfWorkPublishRequest {
            title: &item.title,
            body: &item.body_text,
        },
    )?;
    let published_item = mark_ticket_self_work_published(
        &mut conn,
        &item.work_id,
        published.remote_ticket_id.as_deref(),
        published.remote_locator.as_deref(),
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", published_item.source_system),
            case_id: None,
            actor_type: "adapter",
            action_type: "self_work_item_published",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": published_item.work_id,
                "kind": published_item.kind,
                "remote_ticket_id": published_item.remote_ticket_id,
                "remote_locator": published_item.remote_locator,
            }),
        },
    )?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "ticket.self_work_published",
            title: "Ticket internal work item published",
            body_text: &published_item.title,
            message_key: self_work_message_key(&published_item),
            work_id: Some(&published_item.work_id),
            ticket_key: None,
            attempt_index: None,
            metadata: json!({
                "source_system": published_item.source_system,
                "kind": published_item.kind,
                "state": published_item.state,
                "remote_ticket_id": published_item.remote_ticket_id,
                "remote_locator": published_item.remote_locator,
            }),
        },
    );
    Ok(published_item)
}

pub(crate) fn assign_ticket_self_work_item(
    root: &Path,
    work_id: &str,
    assignee: &str,
    assigned_by: &str,
    rationale: Option<&str>,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let item = conn
        .query_row(
            r#"
            SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
            FROM ticket_self_work_items
            WHERE work_id = ?1
            LIMIT 1
            "#,
            params![work_id],
            map_ticket_self_work_row,
        )
        .optional()?
        .context("ticket internal work item not found")?;
    let mut remote_event_ids = Vec::new();
    if let Some(remote_ticket_id) = item.remote_ticket_id.as_deref() {
        let adapter = ticket_adapters::adapter_for_system(&item.source_system)
            .context("no adapter available to assign ticket internal work item")?;
        if !adapter.capabilities().can_assign_self_work_items {
            anyhow::bail!(
                "ticket adapter {} cannot assign internal work items",
                item.source_system
            );
        }
        let result = adapter.assign_self_work_item(
            root,
            ticket_protocol::TicketSelfWorkAssignRequest {
                remote_ticket_id,
                assignee,
            },
        )?;
        remote_event_ids = result.remote_event_ids;
    }
    let assignment = insert_ticket_self_work_assignment(
        &mut conn,
        work_id,
        assignee,
        assigned_by,
        rationale,
        remote_event_ids.first().map(String::as_str),
    )?;
    touch_ticket_self_work_item(&mut conn, work_id)?;
    let item = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("ticket internal work item not found after assignment")?;
    let item = hydrate_ticket_self_work_item(&conn, item)?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_assigned",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "assigned_to": assignment.assigned_to,
                "assigned_by": assignment.assigned_by,
                "rationale": assignment.rationale,
            }),
        },
    )?;
    Ok(item)
}

pub(crate) fn append_ticket_self_work_note(
    root: &Path,
    work_id: &str,
    body: &str,
    authored_by: &str,
    visibility: &str,
) -> Result<TicketSelfWorkNoteView> {
    let mut conn = open_ticket_db(root)?;
    let item = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("ticket internal work item not found")?;
    let mut remote_event_ids = Vec::new();
    if let Some(remote_ticket_id) = item.remote_ticket_id.as_deref() {
        let adapter = ticket_adapters::adapter_for_system(&item.source_system)
            .context("no adapter available to note ticket internal work item")?;
        if !adapter.capabilities().can_append_self_work_notes {
            anyhow::bail!(
                "ticket adapter {} cannot append internal work notes",
                item.source_system
            );
        }
        let result = adapter.append_self_work_note(
            root,
            ticket_protocol::TicketSelfWorkNoteRequest {
                remote_ticket_id,
                body,
                internal: visibility != "public",
            },
        )?;
        remote_event_ids = result.remote_event_ids;
    }
    let note = insert_ticket_self_work_note(
        &mut conn,
        work_id,
        body,
        visibility,
        authored_by,
        remote_event_ids.first().map(String::as_str),
    )?;
    touch_ticket_self_work_item(&mut conn, work_id)?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_note_appended",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "visibility": note.visibility,
                "authored_by": note.authored_by,
            }),
        },
    )?;
    Ok(note)
}

pub(crate) fn transition_ticket_self_work_item(
    root: &Path,
    work_id: &str,
    state: &str,
    transitioned_by: &str,
    note: Option<&str>,
    visibility: &str,
) -> Result<TicketSelfWorkItemView> {
    let status = parse_work_item_status(state)?;
    let state = status.as_str();
    let mut conn = open_ticket_db(root)?;
    let item = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("ticket internal work item not found")?;
    // Evaluate the state guard before any side effect. The real enforcement
    // below runs inside a transaction that rolls back on rejection, so without
    // this preflight a rejected transition still wrote the remote ticket
    // transition and the note first; callers that retry every router tick
    // (stale founder-rework cleanup) then appended a comment, a status event
    // and a note every 12 s forever. The preflight transaction is rolled back,
    // so it records nothing; the accepted transition is recorded once below.
    preflight_ticket_self_work_state_transition(&mut conn, work_id, &item.state, state, note)?;
    let mut remote_event_ids = Vec::new();
    if let Some(remote_ticket_id) = item.remote_ticket_id.as_deref() {
        let adapter = ticket_adapters::adapter_for_system(&item.source_system)
            .context("no adapter available to transition ticket internal work item")?;
        if !adapter.capabilities().can_transition_self_work_items {
            anyhow::bail!(
                "ticket adapter {} cannot transition internal work items",
                item.source_system
            );
        }
        let result = adapter.transition_self_work_item(
            root,
            ticket_protocol::TicketSelfWorkTransitionRequest {
                remote_ticket_id,
                state,
                note_body: note,
                internal_note: visibility != "public",
            },
        )?;
        remote_event_ids = result.remote_event_ids;
    }
    if let Some(note) = note.map(str::trim).filter(|value| !value.is_empty()) {
        let _ = insert_ticket_self_work_note(
            &mut conn,
            work_id,
            note,
            visibility,
            transitioned_by,
            remote_event_ids.first().map(String::as_str),
        )?;
    }
    let item = set_ticket_self_work_state_internal(root, &mut conn, work_id, state, note)?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_transitioned",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "state": item.state,
                "transitioned_by": transitioned_by,
                "visibility": visibility,
            }),
        },
    )?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "ticket.self_work_transitioned",
            title: "Ticket internal work state changed",
            body_text: note.unwrap_or(state),
            message_key: self_work_message_key(&item),
            work_id: Some(&item.work_id),
            ticket_key: None,
            attempt_index: None,
            metadata: json!({
                "source_system": item.source_system,
                "kind": item.kind,
                "state": item.state,
                "transitioned_by": transitioned_by,
                "visibility": visibility,
            }),
        },
    );
    Ok(item)
}

pub(crate) fn list_ticket_self_work_items(
    root: &Path,
    system: Option<&str>,
    state: Option<&str>,
    limit: usize,
) -> Result<Vec<TicketSelfWorkItemView>> {
    let state = state.map(parse_work_item_status).transpose()?;
    let state = state.map(WorkItemStatus::as_str);
    let db_path = resolve_db_path(root);
    let cache_key = ticket_self_work_list_cache_key(&db_path, system, state, limit);
    let initial_stamp = ticket_self_work_list_cache_stamp(&db_path);
    if let Some(items) = cached_ticket_self_work_list(&cache_key, &initial_stamp) {
        return Ok(items);
    }

    let conn = open_ticket_db(root)?;
    let items = list_ticket_self_work_items_on_conn(&conn, system, state, limit)?;
    drop(conn);
    let cache_key = ticket_self_work_list_cache_key(&db_path, system, state, limit);
    let stamp = ticket_self_work_list_cache_stamp(&db_path);
    #[cfg(test)]
    record_ticket_self_work_list_cache_miss_for_tests(&cache_key);
    if stamp == initial_stamp {
        store_ticket_self_work_list_cache(cache_key, stamp, items.clone());
    }
    Ok(items)
}

pub(super) fn list_ticket_self_work_items_on_conn(
    conn: &Connection,
    system: Option<&str>,
    state: Option<&str>,
    limit: usize,
) -> Result<Vec<TicketSelfWorkItemView>> {
    let state = state.map(parse_work_item_status).transpose()?;
    let query_limit = if state.is_some() {
        i64::MAX
    } else {
        limit as i64
    };
    let mut statement = conn.prepare(
        r#"
        SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items
        WHERE (?1 IS NULL OR source_system = ?1)
        ORDER BY updated_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![system, query_limit], map_ticket_self_work_row)?;
    let items = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?
        .into_iter()
        .filter(|item| {
            state
                .map(|expected| item.state == expected.as_str())
                .unwrap_or(true)
        })
        .take(limit)
        .collect();
    hydrate_ticket_self_work_items_with_latest_assignments(conn, items)
}

pub(crate) fn load_ticket_self_work_item(
    root: &Path,
    work_id: &str,
) -> Result<Option<TicketSelfWorkItemView>> {
    let conn = open_ticket_db(root)?;
    let item = conn.query_row(
        r#"
        SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items
        WHERE work_id = ?1
        LIMIT 1
        "#,
        params![work_id],
        map_ticket_self_work_row,
    )
    .optional()
    .map_err(anyhow::Error::from)?;
    item.map(|item| hydrate_ticket_self_work_item(&conn, item))
        .transpose()
}

pub(crate) fn start_ticket_workflow(
    root: &Path,
    input: TicketWorkflowStartInput,
) -> Result<TicketWorkflowView> {
    let title = input.title.trim();
    let goal = input.goal.trim();
    if title.is_empty() || goal.is_empty() {
        anyhow::bail!("workflow title and goal must be non-empty");
    }
    let scope = input
        .thread_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(input.source_system.trim());
    let normalized_scope = normalize_token(scope);
    let workflow_scope = if normalized_scope.is_empty() {
        "ctox".to_string()
    } else {
        normalized_scope
    };
    let workflow_id = format!(
        "workflow:{}:{}",
        workflow_scope,
        stable_digest(&format!("{}:{}:{}", input.source_system, title, goal))
    );
    let mut metadata = json!({"workflow_id": workflow_id, "workflow_role": WORKFLOW_ROLE_CASE, "workflow_status": "active", "workflow_goal": goal, "dedupe_key": format!("ticket-workflow-case:{}", workflow_id), "source_label": "ticket-workflow", "skill": WORKFLOW_ORCHESTRATOR_SKILL});
    if let Some(object) = metadata.as_object_mut() {
        if let Some(thread_key) = input
            .thread_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("thread_key".to_string(), json!(thread_key));
        }
        if let Some(workspace_root) = input
            .workspace_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("workspace_root".to_string(), json!(workspace_root));
        }
        if let Some(priority) = input
            .priority
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("priority".to_string(), json!(priority));
        }
    }
    let case = put_ticket_self_work_item(
        root,
        TicketSelfWorkUpsertInput {
            source_system: input.source_system.clone(),
            kind: WORKFLOW_CASE_KIND.to_string(),
            title: title.to_string(),
            body_text: goal.to_string(),
            state: WorkItemStatus::Open.as_str().to_string(),
            metadata,
        },
        false,
    )?;
    let first_step_title = input
        .first_step_title
        .unwrap_or_else(|| format!("Plan workflow phase: {}", input.first_phase.trim()));
    let first_step_prompt = input.first_step_prompt.unwrap_or_else(|| format!("Create the first executable CTOX workflow delta for this long-running goal.\n\nGoal:\n{}\n\nReturn or apply a workflow delta with bounded follow-up tickets. Do not execute the whole goal in one turn.", goal));
    let first_step = put_ticket_workflow_step(
        root,
        TicketWorkflowStepInput {
            workflow_id: workflow_id.clone(),
            role: WORKFLOW_ROLE_REDUCER.to_string(),
            phase: input.first_phase,
            step_id: Some("phase-0-reducer".to_string()),
            title: first_step_title,
            body_text: first_step_prompt,
            phase_goal: input.first_phase_goal,
            exit_gate: input.first_exit_gate,
            predecessor_work_ids: Vec::new(),
            predecessor_step_ids: Vec::new(),
            skill: input
                .skill
                .or_else(|| Some(WORKFLOW_ORCHESTRATOR_SKILL.to_string())),
            priority: input.priority,
            metadata: json!({ "workflow_created_by": case.work_id }),
        },
    )?;
    if input.queue_now {
        let _ = workflow_mark_step_queue_ready(root, &first_step.work_id)?;
    }
    load_ticket_workflow(root, &workflow_id)?.context("ticket workflow not found after creation")
}

pub(crate) fn put_ticket_workflow_step(
    root: &Path,
    input: TicketWorkflowStepInput,
) -> Result<TicketSelfWorkItemView> {
    let conn = open_ticket_db(root)?;
    let case = load_ticket_workflow_case_internal(&conn, &input.workflow_id)?
        .context("workflow case not found")?;
    drop(conn);
    let role = normalize_workflow_role(&input.role)?;
    let phase = input.phase.trim();
    let title = input.title.trim();
    let body_text = input.body_text.trim();
    if phase.is_empty() || title.is_empty() || body_text.is_empty() {
        anyhow::bail!("workflow step phase, title, and body must be non-empty");
    }
    let (mut predecessor_work_ids, inferred_step_ids) =
        split_workflow_predecessor_refs(input.predecessor_work_ids);
    let mut predecessor_step_ids = input.predecessor_step_ids;
    predecessor_step_ids.extend(inferred_step_ids);
    dedupe_strings(&mut predecessor_work_ids);
    dedupe_strings(&mut predecessor_step_ids);
    let step_id = input.step_id.unwrap_or_else(|| {
        format!(
            "{}:{}",
            normalize_token(phase),
            stable_digest(&format!("{}:{}", phase, title))
        )
    });
    let workflow_id = input.workflow_id;
    let case_priority = metadata_string_value(&case.metadata, "priority");
    let status = if predecessor_work_ids.is_empty() && predecessor_step_ids.is_empty() {
        WorkItemStatus::Ready
    } else {
        WorkItemStatus::Waiting
    };
    let mut metadata = if input.metadata.is_object() {
        input.metadata
    } else {
        json!({})
    };
    if let Some(object) = metadata.as_object_mut() {
        object.insert("workflow_id".to_string(), json!(workflow_id.clone()));
        object.insert("workflow_role".to_string(), json!(role.clone()));
        object.insert("workflow_step_id".to_string(), json!(step_id.clone()));
        object.insert("workflow_phase".to_string(), json!(phase));
        object.insert("workflow_step_status".to_string(), json!(status.as_str()));
        object.insert(
            "workflow_predecessor_work_ids".to_string(),
            json!(predecessor_work_ids),
        );
        object.insert(
            "workflow_predecessor_step_ids".to_string(),
            json!(predecessor_step_ids),
        );
        object.insert("parent_work_id".to_string(), json!(case.work_id));
        object.insert("source_label".to_string(), json!("ticket-workflow"));
        object.insert(
            "dedupe_key".to_string(),
            json!(format!("ticket-workflow-step:{}:{}", workflow_id, step_id)),
        );
        if let Some(thread_key) = metadata_string_value(&case.metadata, "thread_key") {
            object.insert("thread_key".to_string(), json!(thread_key));
        }
        if let Some(workspace_root) = metadata_string_value(&case.metadata, "workspace_root") {
            object.insert("workspace_root".to_string(), json!(workspace_root));
        }
        if let Some(priority) = input
            .priority
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| case_priority.as_deref().map(str::trim))
        {
            object.insert("priority".to_string(), json!(priority));
        }
        if let Some(phase_goal) = input
            .phase_goal
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("workflow_phase_goal".to_string(), json!(phase_goal));
        }
        if let Some(exit_gate) = input
            .exit_gate
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("workflow_exit_gate".to_string(), json!(exit_gate));
        }
        let skill = input.skill.or_else(|| {
            if role == WORKFLOW_ROLE_REDUCER {
                Some(WORKFLOW_ORCHESTRATOR_SKILL.to_string())
            } else {
                None
            }
        });
        if let Some(skill) = skill
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("skill".to_string(), json!(skill));
        }
    }
    let step_dedupe_key = format!("ticket-workflow-step:{}:{}", workflow_id, step_id);
    let prospective_work_id = format!(
        "self-work:{}:{}",
        case.source_system,
        stable_digest(&step_dedupe_key)
    );
    {
        let conn = open_ticket_db(root)?;
        let is_new_step = load_ticket_self_work_item_raw(&conn, &prospective_work_id)?.is_none();
        if is_new_step {
            let existing_count = count_ticket_workflow_steps_internal(&conn, &workflow_id)?;
            if existing_count as usize + 1 > WORKFLOW_MAX_STEPS_PER_WORKFLOW {
                anyhow::bail!(
                    "workflow `{}` already has {} steps, at the per-workflow ceiling of {}; refusing to materialize step `{}`",
                    workflow_id,
                    existing_count,
                    WORKFLOW_MAX_STEPS_PER_WORKFLOW,
                    step_id
                );
            }
        }
    }
    put_ticket_self_work_item(
        root,
        TicketSelfWorkUpsertInput {
            source_system: case.source_system,
            kind: WORKFLOW_STEP_KIND.to_string(),
            title: title.to_string(),
            body_text: body_text.to_string(),
            state: WorkItemStatus::Open.as_str().to_string(),
            metadata,
        },
        false,
    )
}

pub(crate) fn apply_ticket_workflow_delta(
    root: &Path,
    workflow_id: &str,
    delta_value: Value,
    queue_now: bool,
) -> Result<Value> {
    let delta: WorkflowDelta = serde_json::from_value(delta_value)
        .context("workflow delta must match the CTOX workflow delta schema")?;
    if delta.create_steps.len() > WORKFLOW_MATERIALIZE_DEFAULT_LIMIT {
        anyhow::bail!(
            "workflow delta creates too many steps at once ({} > {})",
            delta.create_steps.len(),
            WORKFLOW_MATERIALIZE_DEFAULT_LIMIT
        );
    }
    let mut updated = Vec::new();
    for update in delta.update_steps {
        let item = locate_workflow_step(
            root,
            workflow_id,
            update.work_id.as_deref(),
            update.step_id.as_deref(),
        )?
        .context("workflow delta update references an unknown step")?;
        let mut metadata_update = json!({});
        if let Some(object) = metadata_update.as_object_mut() {
            if let Some(status) = update
                .workflow_step_status
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                let status = parse_work_item_status(status)?;
                object.insert("workflow_step_status".to_string(), json!(status.as_str()));
            }
            if !update.evidence.is_null() {
                object.insert("workflow_step_evidence".to_string(), update.evidence);
            }
            if let Some(extra) = update.metadata.as_object() {
                for (key, value) in extra {
                    object.insert(key.clone(), value.clone());
                }
            }
        }
        let merged = merge_ticket_self_work_metadata(root, &item.work_id, metadata_update)?;
        if let Some(note) = update
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let _ = append_ticket_self_work_note(
                root,
                &item.work_id,
                note,
                WORKFLOW_ORCHESTRATOR_SKILL,
                "internal",
            );
        }
        updated.push(merged.work_id);
    }
    let queue_now_refs = delta.queue_now.clone();
    let mut created_items = Vec::new();
    for create in delta.create_steps {
        if create.prompt.trim().is_empty() {
            anyhow::bail!(
                "workflow delta create_steps entry `{}` has an empty prompt",
                create.title
            );
        }
        let mut predecessor_refs = create.predecessors;
        predecessor_refs.extend(create.predecessor_work_ids);
        let (predecessor_work_ids, inferred_step_ids) =
            split_workflow_predecessor_refs(predecessor_refs);
        let mut predecessor_step_ids = create.predecessor_steps;
        predecessor_step_ids.extend(create.predecessor_step_ids);
        predecessor_step_ids.extend(inferred_step_ids);
        dedupe_strings(&mut predecessor_step_ids);
        let item = put_ticket_workflow_step(
            root,
            TicketWorkflowStepInput {
                workflow_id: workflow_id.to_string(),
                role: create
                    .role
                    .unwrap_or_else(|| WORKFLOW_ROLE_LEAF.to_string()),
                phase: create.phase,
                step_id: create.step_id,
                title: create.title,
                body_text: create.prompt,
                phase_goal: create.phase_goal,
                exit_gate: create.exit_gate,
                predecessor_work_ids,
                predecessor_step_ids,
                skill: create.skill,
                priority: create.priority,
                metadata: create.metadata,
            },
        )?;
        created_items.push(item);
    }
    let mut explicitly_queued = Vec::new();
    for reference in queue_now_refs {
        if let Some(item) =
            locate_workflow_step(root, workflow_id, Some(&reference), Some(&reference))?
        {
            if workflow_step_ready_for_queue(root, &item.work_id)? {
                explicitly_queued.push(workflow_mark_step_queue_ready(root, &item.work_id)?);
            }
        }
    }
    let materialized = if queue_now || !explicitly_queued.is_empty() {
        materialize_ready_workflow_steps_for_workflow(
            root,
            Some(workflow_id),
            WORKFLOW_MATERIALIZE_DEFAULT_LIMIT,
        )?
    } else {
        TicketWorkflowMaterializeResult {
            workflow_id: Some(workflow_id.to_string()),
            materialized_count: 0,
            materialized: Vec::new(),
            skipped_count: 0,
        }
    };
    Ok(
        json!({ "workflow_id": workflow_id, "phase_decision": delta.phase_decision, "updated_work_ids": updated, "created_work_ids": created_items.iter().map(|item| item.work_id.clone()).collect::<Vec<_>>(), "explicitly_queued_work_ids": explicitly_queued.iter().map(|item| item.work_id.clone()).collect::<Vec<_>>(), "materialized": materialized }),
    )
}

pub(crate) fn materialize_ready_workflow_steps(
    root: &Path,
    limit: usize,
) -> Result<TicketWorkflowMaterializeResult> {
    materialize_ready_workflow_steps_for_workflow(root, None, limit)
}

pub(crate) fn materialize_ready_workflow_steps_for_workflow(
    root: &Path,
    workflow_id: Option<&str>,
    limit: usize,
) -> Result<TicketWorkflowMaterializeResult> {
    if limit == 0 {
        return Ok(TicketWorkflowMaterializeResult {
            workflow_id: workflow_id.map(ToOwned::to_owned),
            materialized_count: 0,
            materialized: Vec::new(),
            skipped_count: 0,
        });
    }
    let db_path = resolve_db_path(root);
    let cache_key = ticket_workflow_materialize_cache_key(&db_path, workflow_id, limit);
    let initial_stamp = ticket_store_change_stamp_for_path(&db_path);
    if let Some(result) = cached_ticket_workflow_materialize_result(&cache_key, &initial_stamp) {
        return Ok(result);
    }

    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND (?2 IS NULL OR json_extract(metadata_json, '$.workflow_id') = ?2) ORDER BY created_at ASC LIMIT 512"#)?;
    let rows = statement.query_map(
        params![WORKFLOW_STEP_KIND, workflow_id],
        map_ticket_self_work_row,
    )?;
    let candidates = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    drop(statement);
    let mut ready_work_ids = Vec::new();
    let mut skipped_count = 0usize;
    for candidate in candidates {
        let item = hydrate_ticket_self_work_item(&conn, candidate)?;
        if workflow_step_is_runnable_state(&item.state)?
            && workflow_step_ready_internal(&conn, &item)?
        {
            ready_work_ids.push(item.work_id.clone());
            if ready_work_ids.len() >= limit {
                break;
            }
        } else {
            skipped_count += 1;
        }
    }
    drop(conn);
    let mut materialized = Vec::new();
    for work_id in ready_work_ids {
        materialized.push(workflow_mark_step_queue_ready(root, &work_id)?);
    }
    let result = TicketWorkflowMaterializeResult {
        workflow_id: workflow_id.map(ToOwned::to_owned),
        materialized_count: materialized.len(),
        materialized,
        skipped_count,
    };
    let final_stamp = ticket_store_change_stamp_for_path(&db_path);
    #[cfg(test)]
    record_ticket_workflow_materialize_cache_miss_for_tests(&cache_key);
    if result.materialized.is_empty() && final_stamp == initial_stamp {
        store_ticket_workflow_materialize_cache(cache_key, final_stamp, result.clone());
    }
    Ok(result)
}

pub(crate) fn workflow_prompt_block(root: &Path, limit: usize) -> Result<Option<String>> {
    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 ORDER BY updated_at DESC"#)?;
    let rows = statement.query_map(params![WORKFLOW_CASE_KIND], map_ticket_self_work_row)?;
    let mut cases = Vec::new();
    for case in rows {
        let case = case?;
        let status = parse_work_item_status(&case.state)?;
        if !status.is_workflow_case_terminal() {
            cases.push(case);
            if cases.len() >= limit.max(1) {
                break;
            }
        }
    }
    drop(statement);
    if cases.is_empty() {
        return Ok(None);
    }
    let mut lines = vec!["ticket_workflows:".to_string()];
    for case in cases {
        let case = hydrate_ticket_self_work_item(&conn, case)?;
        let Some(workflow_id) = metadata_string_value(&case.metadata, "workflow_id") else {
            continue;
        };
        let steps = list_ticket_workflow_steps_internal(&conn, &workflow_id, 64)?;
        let mut ready = Vec::new();
        let mut waiting = Vec::new();
        let mut running = Vec::new();
        for step in &steps {
            let status = parse_work_item_status(&step.state)?;
            let satisfied = workflow_step_satisfied(step)?;
            if status.is_active() {
                running.push(step);
            } else if status.is_workflow_runnable() && workflow_step_ready_internal(&conn, step)? {
                ready.push(step);
            } else if !satisfied {
                waiting.push(step);
            }
        }
        lines.push(format!(
            "- {} [{}] state={} ready={} running={} waiting={} goal={}",
            workflow_id,
            workflow_clip(&case.title, 80),
            case.state,
            ready.len(),
            running.len(),
            waiting.len(),
            workflow_clip(
                &metadata_string_value(&case.metadata, "workflow_goal")
                    .unwrap_or_else(|| case.body_text.clone()),
                120
            )
        ));
        for step in ready.iter().take(3) {
            lines.push(format!(
                "  ready: {} phase={} work_id={} title={}",
                workflow_step_id(step),
                workflow_step_phase(step),
                step.work_id,
                workflow_clip(&step.title, 90)
            ));
        }
        for step in running.iter().take(3) {
            lines.push(format!(
                "  active: {} phase={} state={} work_id={} title={}",
                workflow_step_id(step),
                workflow_step_phase(step),
                step.state,
                step.work_id,
                workflow_clip(&step.title, 90)
            ));
        }
    }
    if lines.len() == 1 {
        Ok(None)
    } else {
        Ok(Some(lines.join("\n")))
    }
}

pub(crate) fn load_ticket_workflow(
    root: &Path,
    workflow_id: &str,
) -> Result<Option<TicketWorkflowView>> {
    let conn = open_ticket_db(root)?;
    let case = load_ticket_workflow_case_internal(&conn, workflow_id)?;
    let steps = list_ticket_workflow_steps_internal(&conn, workflow_id, 512)?;
    if case.is_none() && steps.is_empty() {
        return Ok(None);
    }
    let mut step_views = Vec::new();
    let mut ready_steps = Vec::new();
    let mut waiting_steps = Vec::new();
    for step in steps {
        let ready = workflow_step_is_runnable_state(&step.state)?
            && workflow_step_ready_internal(&conn, &step)?;
        let step_id = workflow_step_id(&step);
        if ready {
            ready_steps.push(step_id.clone());
        } else if !workflow_step_satisfied(&step)? {
            waiting_steps.push(step_id.clone());
        }
        step_views.push(TicketWorkflowStepView {
            work_id: step.work_id.clone(),
            step_id,
            role: workflow_step_role(&step),
            phase: workflow_step_phase(&step),
            title: step.title.clone(),
            state: step.state.clone(),
            status: workflow_step_status(&step)?.as_str().to_string(),
            predecessor_work_ids: workflow_predecessor_work_ids(&step.metadata),
            predecessor_step_ids: workflow_predecessor_step_ids(&step.metadata),
            ready,
            suggested_skill: step.suggested_skill.clone(),
            updated_at: step.updated_at.clone(),
        });
    }
    let (title, goal, state, case_work_id) = if let Some(case) = case {
        (
            case.title,
            metadata_string_value(&case.metadata, "workflow_goal").or(Some(case.body_text)),
            case.state,
            Some(case.work_id),
        )
    } else {
        (
            workflow_id.to_string(),
            None,
            "missing-case".to_string(),
            None,
        )
    };
    Ok(Some(TicketWorkflowView {
        workflow_id: workflow_id.to_string(),
        title,
        goal,
        state,
        case_work_id,
        steps: step_views,
        ready_steps,
        waiting_steps,
    }))
}

pub(super) fn load_ticket_workflow_case_internal(
    conn: &Connection,
    workflow_id: &str,
) -> Result<Option<TicketSelfWorkItemView>> {
    let item = conn.query_row(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND json_extract(metadata_json, '$.workflow_id') = ?2 ORDER BY created_at ASC LIMIT 1"#, params![WORKFLOW_CASE_KIND, workflow_id], map_ticket_self_work_row).optional().map_err(anyhow::Error::from)?;
    item.map(|item| hydrate_ticket_self_work_item(conn, item))
        .transpose()
}
pub(super) fn list_ticket_workflow_steps_internal(
    conn: &Connection,
    workflow_id: &str,
    limit: usize,
) -> Result<Vec<TicketSelfWorkItemView>> {
    let mut statement = conn.prepare(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND json_extract(metadata_json, '$.workflow_id') = ?2 ORDER BY created_at ASC LIMIT ?3"#)?;
    let rows = statement.query_map(
        params![WORKFLOW_STEP_KIND, workflow_id, limit as i64],
        map_ticket_self_work_row,
    )?;
    let steps = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    steps
        .into_iter()
        .map(|item| hydrate_ticket_self_work_item(conn, item))
        .collect()
}
pub(super) fn count_ticket_workflow_steps_internal(
    conn: &Connection,
    workflow_id: &str,
) -> Result<i64> {
    conn.query_row(
        r#"SELECT COUNT(*) FROM ticket_self_work_items WHERE kind = ?1 AND json_extract(metadata_json, '$.workflow_id') = ?2"#,
        params![WORKFLOW_STEP_KIND, workflow_id],
        |row| row.get::<_, i64>(0),
    )
    .map_err(anyhow::Error::from)
}
pub(super) fn locate_workflow_step(
    root: &Path,
    workflow_id: &str,
    work_id: Option<&str>,
    step_id: Option<&str>,
) -> Result<Option<TicketSelfWorkItemView>> {
    let conn = open_ticket_db(root)?;
    if let Some(work_id) = work_id.map(str::trim).filter(|value| !value.is_empty()) {
        if let Some(item) = load_ticket_self_work_item_raw(&conn, work_id)? {
            if item.kind == WORKFLOW_STEP_KIND
                && metadata_string_value(&item.metadata, "workflow_id").as_deref()
                    == Some(workflow_id)
            {
                return hydrate_ticket_self_work_item(&conn, item).map(Some);
            }
        }
    }
    let Some(step_id) = step_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let item = conn.query_row(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND json_extract(metadata_json, '$.workflow_id') = ?2 AND json_extract(metadata_json, '$.workflow_step_id') = ?3 ORDER BY created_at DESC LIMIT 1"#, params![WORKFLOW_STEP_KIND, workflow_id, step_id], map_ticket_self_work_row).optional().map_err(anyhow::Error::from)?;
    item.map(|item| hydrate_ticket_self_work_item(&conn, item))
        .transpose()
}
pub(crate) fn workflow_mark_step_queue_ready(
    root: &Path,
    work_id: &str,
) -> Result<TicketSelfWorkItemView> {
    if !workflow_step_ready_for_queue(root, work_id)? {
        anyhow::bail!("workflow step `{work_id}` is not ready for queue materialization");
    }
    let mut item = merge_ticket_self_work_metadata(
        root,
        work_id,
        json!({"workflow_step_status": WorkItemStatus::Ready.as_str()}),
    )?;
    if item.assigned_to.as_deref() != Some("self") {
        item = assign_ticket_self_work_item(
            root,
            work_id,
            "self",
            WORKFLOW_ORCHESTRATOR_SKILL,
            Some("workflow predecessor conditions are satisfied"),
        )?;
    }
    if parse_work_item_status(&item.state)?.is_active() {
        Ok(item)
    } else {
        transition_ticket_self_work_item(
            root,
            work_id,
            WorkItemStatus::Queued.as_str(),
            WORKFLOW_ORCHESTRATOR_SKILL,
            Some("Workflow predecessor conditions are satisfied; queued as the next bounded step."),
            "internal",
        )
    }
}
pub(super) fn workflow_step_ready_for_queue(root: &Path, work_id: &str) -> Result<bool> {
    let conn = open_ticket_db(root)?;
    let Some(item) = load_ticket_self_work_item_raw(&conn, work_id)? else {
        return Ok(false);
    };
    Ok(
        workflow_step_is_runnable_state(&item.state)?
            && workflow_step_ready_internal(&conn, &item)?,
    )
}
pub(super) fn workflow_step_ready_internal(
    conn: &Connection,
    item: &TicketSelfWorkItemView,
) -> Result<bool> {
    if item.kind != WORKFLOW_STEP_KIND || workflow_step_satisfied(item)? {
        return Ok(false);
    }
    for work_id in workflow_predecessor_work_ids(&item.metadata) {
        let Some(predecessor) = load_ticket_self_work_item_raw(conn, &work_id)? else {
            return Ok(false);
        };
        if !workflow_step_satisfied(&predecessor)? {
            return Ok(false);
        }
    }
    let workflow_id = metadata_string_value(&item.metadata, "workflow_id").unwrap_or_default();
    for step_id in workflow_predecessor_step_ids(&item.metadata) {
        let Some(predecessor) = locate_workflow_step_in_conn(conn, &workflow_id, &step_id)? else {
            return Ok(false);
        };
        if !workflow_step_satisfied(&predecessor)? {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) fn locate_workflow_step_in_conn(
    conn: &Connection,
    workflow_id: &str,
    step_id: &str,
) -> Result<Option<TicketSelfWorkItemView>> {
    let item = conn.query_row(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND json_extract(metadata_json, '$.workflow_id') = ?2 AND json_extract(metadata_json, '$.workflow_step_id') = ?3 ORDER BY created_at DESC LIMIT 1"#, params![WORKFLOW_STEP_KIND, workflow_id, step_id], map_ticket_self_work_row).optional().map_err(anyhow::Error::from)?;
    item.map(|item| hydrate_ticket_self_work_item(conn, item))
        .transpose()
}
pub(super) fn merge_ticket_self_work_metadata(
    root: &Path,
    work_id: &str,
    update: Value,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let item = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("ticket internal work item not found")?;
    let mut metadata = if item.metadata.is_object() {
        item.metadata
    } else {
        json!({})
    };
    let updated_keys = update
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    merge_json_object_values(&mut metadata, &update);
    conn.execute(r#"UPDATE ticket_self_work_items SET metadata_json = ?2, updated_at = ?3 WHERE work_id = ?1"#, params![work_id, serde_json::to_string(&metadata)?, now_iso_string()])?;
    clear_ticket_self_work_list_cache();
    let item = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("ticket internal work item not found after metadata update")?;
    let item = hydrate_ticket_self_work_item(&conn, item)?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_metadata_updated",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "kind": item.kind,
                "updated_keys": updated_keys,
                "workflow_id": metadata_string_value(&item.metadata, "workflow_id"),
                "workflow_step_id": metadata_string_value(&item.metadata, "workflow_step_id"),
            }),
        },
    )?;
    Ok(item)
}
pub(super) fn merge_json_object_values(target: &mut Value, update: &Value) {
    let (Some(target), Some(update)) = (target.as_object_mut(), update.as_object()) else {
        return;
    };
    for (key, value) in update {
        target.insert(key.clone(), value.clone());
    }
}
pub(super) fn workflow_step_satisfied(item: &TicketSelfWorkItemView) -> Result<bool> {
    let state = parse_work_item_status(&item.state)?;
    let status = workflow_step_status(item)?;
    Ok(state.is_workflow_item_satisfied() || status.is_workflow_status_satisfied())
}
pub(super) fn workflow_step_is_runnable_state(state: &str) -> Result<bool> {
    Ok(parse_work_item_status(state)?.is_workflow_runnable())
}
pub(super) fn workflow_step_id(item: &TicketSelfWorkItemView) -> String {
    metadata_string_value(&item.metadata, "workflow_step_id")
        .unwrap_or_else(|| item.work_id.clone())
}
pub(super) fn workflow_step_role(item: &TicketSelfWorkItemView) -> String {
    metadata_string_value(&item.metadata, "workflow_role")
        .unwrap_or_else(|| WORKFLOW_ROLE_LEAF.to_string())
}
pub(super) fn workflow_step_phase(item: &TicketSelfWorkItemView) -> String {
    metadata_string_value(&item.metadata, "workflow_phase")
        .unwrap_or_else(|| "unspecified".to_string())
}
pub(super) fn workflow_step_status(item: &TicketSelfWorkItemView) -> Result<WorkItemStatus> {
    let raw = metadata_string_value(&item.metadata, "workflow_step_status")
        .unwrap_or_else(|| item.state.clone());
    parse_work_item_status(&raw).with_context(|| {
        format!(
            "workflow step `{}` has an unknown persisted status",
            item.work_id
        )
    })
}
pub(super) fn workflow_predecessor_work_ids(metadata: &Value) -> Vec<String> {
    workflow_metadata_strings(metadata, "workflow_predecessor_work_ids")
}
pub(super) fn workflow_predecessor_step_ids(metadata: &Value) -> Vec<String> {
    workflow_metadata_strings(metadata, "workflow_predecessor_step_ids")
}
pub(super) fn workflow_metadata_strings(metadata: &Value, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    match metadata.get(key) {
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(text) = item
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    values.push(text.to_string());
                }
            }
        }
        Some(Value::String(text)) => values.extend(parse_domain_csv(text)),
        _ => {}
    }
    dedupe_strings(&mut values);
    values
}
pub(super) fn normalize_workflow_role(role: &str) -> Result<String> {
    match normalize_token(role).as_str() {
        "" | "leaf" => Ok(WORKFLOW_ROLE_LEAF.to_string()),
        "reducer" | "planner" | "orchestrator" => Ok(WORKFLOW_ROLE_REDUCER.to_string()),
        other => anyhow::bail!("unsupported workflow role `{other}`"),
    }
}
pub(super) fn split_workflow_predecessor_refs(values: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut work_ids = Vec::new();
    let mut step_ids = Vec::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.starts_with("self-work:") {
            work_ids.push(value.to_string());
        } else {
            step_ids.push(value.to_string());
        }
    }
    dedupe_strings(&mut work_ids);
    dedupe_strings(&mut step_ids);
    (work_ids, step_ids)
}
pub(super) fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() || seen.contains(trimmed) {
            return false;
        }
        seen.insert(trimmed.to_string());
        true
    });
}
pub(super) fn workflow_clip(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let clipped: String = trimmed.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{clipped}...")
}

pub(crate) fn set_ticket_self_work_state(
    root: &Path,
    work_id: &str,
    state: &str,
) -> Result<TicketSelfWorkItemView> {
    set_ticket_self_work_state_with_failure_reason(root, work_id, state, None)
}

pub(crate) fn set_ticket_self_work_state_with_failure_reason(
    root: &Path,
    work_id: &str,
    state: &str,
    failure_reason: Option<&str>,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let item =
        set_ticket_self_work_state_internal(root, &mut conn, work_id, state, failure_reason)?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_state_set",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "kind": item.kind,
                "state": item.state,
            }),
        },
    )?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "ticket.self_work_state_set",
            title: "Ticket internal work state set",
            body_text: state,
            message_key: self_work_message_key(&item),
            work_id: Some(&item.work_id),
            ticket_key: None,
            attempt_index: None,
            metadata: json!({
                "source_system": item.source_system,
                "kind": item.kind,
                "state": item.state,
            }),
        },
    );
    Ok(item)
}

pub(crate) fn set_ticket_approval_gate_state_from_authorized_reply(
    root: &Path,
    work_id: &str,
    state: &str,
    approval_message_key: &str,
) -> Result<TicketSelfWorkItemView> {
    let status = parse_work_item_status(state)?;
    let state = status.as_str();
    let mut conn = open_ticket_db(root)?;
    let existing = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("approval gate internal work item not found")?;
    if existing.kind != "approval-gate" {
        anyhow::bail!("authorized approval reply target is not an approval gate");
    }
    let expected_action = match status {
        WorkItemStatus::Closed => "approve",
        WorkItemStatus::Failed => "reject",
        _ => anyhow::bail!("authorized approval reply target state must be closed or failed"),
    };
    let ledger: Option<(String, String, String)> = conn
        .query_row(
            r#"
            SELECT action, sender_address, body_sha256
            FROM ticket_approval_reply_ledger
            WHERE message_key=?1 AND work_id=?2
              AND decision_status IN ('observed', 'applied')
            LIMIT 1
            "#,
            params![approval_message_key, work_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let (action, sender_address, body_sha256) =
        ledger.context("authorized approval reply is missing its durable ledger proof")?;
    if action != expected_action
        || sender_address.trim().is_empty()
        || body_sha256.trim().is_empty()
    {
        anyhow::bail!("authorized approval reply ledger proof does not match the requested state");
    }
    let terminal_policy_grant =
        TerminalPolicyGrant::authorized_approval_reply(approval_message_key, &body_sha256);
    let terminal_policy_proof = terminal_policy_grant.proof();
    let failure_reason = (status == WorkItemStatus::Failed)
        .then_some("authorized approval reply rejected the internal work item");
    let item = set_ticket_self_work_state_internal_with_policy(
        root,
        &mut conn,
        work_id,
        state,
        failure_reason,
        Some(&terminal_policy_grant),
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "approval_reply_state_set",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "state": item.state,
                "approval_message_key": approval_message_key,
                "terminal_policy_proof": terminal_policy_proof,
            }),
        },
    )?;
    Ok(item)
}

pub(crate) fn list_ticket_self_work_assignments(
    root: &Path,
    work_id: &str,
    limit: usize,
) -> Result<Vec<TicketSelfWorkAssignmentView>> {
    let conn = open_ticket_db(root)?;
    list_ticket_self_work_assignments_internal(&conn, work_id, limit)
}

pub(super) fn list_ticket_self_work_assignments_internal(
    conn: &Connection,
    work_id: &str,
    limit: usize,
) -> Result<Vec<TicketSelfWorkAssignmentView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT assignment_id, work_id, assigned_to, assigned_by, rationale, remote_event_id, created_at
        FROM ticket_self_work_assignments
        WHERE work_id = ?1
        ORDER BY created_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(
        params![work_id, limit as i64],
        map_ticket_self_work_assignment_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub(crate) fn list_ticket_self_work_notes(
    root: &Path,
    work_id: &str,
    limit: usize,
) -> Result<Vec<TicketSelfWorkNoteView>> {
    let conn = open_ticket_db(root)?;
    list_ticket_self_work_notes_internal(&conn, work_id, limit)
}

pub(super) fn list_ticket_self_work_notes_internal(
    conn: &Connection,
    work_id: &str,
    limit: usize,
) -> Result<Vec<TicketSelfWorkNoteView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT note_id, work_id, body_text, visibility, authored_by, remote_event_id, created_at
        FROM ticket_self_work_notes
        WHERE work_id = ?1
        ORDER BY created_at ASC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(
        params![work_id, limit as i64],
        map_ticket_self_work_note_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub(super) fn load_ticket_self_work_item_raw(
    conn: &Connection,
    work_id: &str,
) -> Result<Option<TicketSelfWorkItemView>> {
    conn.query_row(
        r#"
        SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items
        WHERE work_id = ?1
        LIMIT 1
        "#,
        params![work_id],
        map_ticket_self_work_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

pub(crate) fn load_ticket_self_work_items_by_work_id_from_conn(
    conn: &Connection,
    work_ids: &[String],
) -> Result<BTreeMap<String, TicketSelfWorkItemView>> {
    let mut items_by_work_id = BTreeMap::new();
    if work_ids.is_empty() {
        return Ok(items_by_work_id);
    }
    for chunk in work_ids.chunks(500) {
        let placeholders = std::iter::repeat("?")
            .take(chunk.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            r#"
            SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
            FROM ticket_self_work_items
            WHERE work_id IN ({placeholders})
            "#
        );
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(chunk.iter()), map_ticket_self_work_row)?;
        let items = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)?;
        for item in hydrate_ticket_self_work_items_with_latest_assignments(conn, items)? {
            items_by_work_id.insert(item.work_id.clone(), item);
        }
    }
    Ok(items_by_work_id)
}

pub(super) fn hydrate_ticket_self_work_item(
    conn: &Connection,
    mut item: TicketSelfWorkItemView,
) -> Result<TicketSelfWorkItemView> {
    if let Some(assignment) = list_ticket_self_work_assignments_internal(conn, &item.work_id, 1)?
        .into_iter()
        .next()
    {
        item.assigned_to = Some(assignment.assigned_to);
        item.assigned_by = Some(assignment.assigned_by);
        item.assigned_at = Some(assignment.created_at);
    }
    Ok(item)
}

pub(super) fn hydrate_ticket_self_work_items_with_latest_assignments(
    conn: &Connection,
    mut items: Vec<TicketSelfWorkItemView>,
) -> Result<Vec<TicketSelfWorkItemView>> {
    if items.is_empty() {
        return Ok(items);
    }
    let work_ids = items
        .iter()
        .map(|item| item.work_id.clone())
        .collect::<Vec<_>>();
    let placeholders = std::iter::repeat("?")
        .take(work_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        r#"
        SELECT assignment_id, work_id, assigned_to, assigned_by, rationale, remote_event_id, created_at
        FROM (
            SELECT assignment_id, work_id, assigned_to, assigned_by, rationale, remote_event_id, created_at,
                   ROW_NUMBER() OVER (
                       PARTITION BY work_id
                       ORDER BY created_at DESC, assignment_id DESC
                   ) AS assignment_rank
            FROM ticket_self_work_assignments
            WHERE work_id IN ({placeholders})
        )
        WHERE assignment_rank = 1
        "#
    );
    #[cfg(test)]
    record_ticket_self_work_assignment_batch_hydration_for_tests();
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(work_ids.iter()), |row| {
        map_ticket_self_work_assignment_row(row)
    })?;
    let latest = rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|assignment| (assignment.work_id.clone(), assignment))
        .collect::<BTreeMap<_, _>>();
    for item in &mut items {
        if let Some(assignment) = latest.get(&item.work_id) {
            item.assigned_to = Some(assignment.assigned_to.clone());
            item.assigned_by = Some(assignment.assigned_by.clone());
            item.assigned_at = Some(assignment.created_at.clone());
        }
    }
    Ok(items)
}

pub(super) fn self_work_message_key(item: &TicketSelfWorkItemView) -> Option<&str> {
    ["queue_message_key", "parent_message_key", "message_key"]
        .iter()
        .find_map(|key| {
            item.metadata
                .get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

pub(super) fn enforce_ticket_self_work_state_transition(
    conn: &Connection,
    work_id: &str,
    from_state: &str,
    to_state: &str,
    actor: &str,
    reason: &str,
    failure_reason: Option<&str>,
    terminal_policy_grant: Option<&TerminalPolicyGrant>,
) -> Result<()> {
    let from_core = ticket_self_work_core_state(from_state)?;
    let to_core = ticket_self_work_core_state(to_state)?;
    if to_core == CoreState::Closed && work_item_has_terminal_success_proof(conn, work_id)? {
        return Ok(());
    }
    if to_core == CoreState::ReworkRequired && work_item_has_rework_witness_proof(conn, work_id)? {
        return Ok(());
    }
    let mut metadata = BTreeMap::new();
    metadata.insert("from_state".to_string(), from_state.to_string());
    metadata.insert("to_state".to_string(), to_state.to_string());
    metadata.insert("reason".to_string(), reason.to_string());
    if let Some(policy_proof) = terminal_policy_grant.map(TerminalPolicyGrant::proof) {
        metadata.insert("terminal_policy_proof".to_string(), policy_proof);
    }
    if to_core == CoreState::Failed {
        let failure_reason = failure_reason
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context(
                "ticket internal work failed transition requires a non-empty failure reason",
            )?;
        metadata.insert("failure_reason".to_string(), failure_reason.to_string());
        metadata.insert(
            "failure_class".to_string(),
            "ticket_self_work_failure".to_string(),
        );
    }
    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::WorkItem,
            entity_id: work_id.to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: from_core,
            to_state: to_core,
            event: ticket_self_work_core_event(to_state)?,
            actor: actor.to_string(),
            evidence: CoreEvidenceRefs {
                verification_id: if to_core == CoreState::Closed {
                    Some(format!("ticket-self-work-state-close:{work_id}"))
                } else {
                    None
                },
                ..CoreEvidenceRefs::default()
            },
            metadata,
        },
    )?;
    Ok(())
}

pub(super) fn work_item_has_rework_witness_proof(conn: &Connection, work_id: &str) -> Result<bool> {
    ensure_core_transition_guard_schema(conn)?;
    let count = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_transition_proofs
        WHERE entity_type = 'WorkItem'
          AND entity_id = ?1
          AND to_state = 'ReworkRequired'
          AND accepted = 1
          AND json_valid(request_json) = 1
          AND (
                json_extract(request_json, '$.metadata.review_checkpoint') = 'true'
             OR json_extract(request_json, '$.metadata.validator_rework') = 'true'
          )
        "#,
        params![work_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count > 0)
}

pub(super) fn work_item_has_terminal_success_proof(
    conn: &Connection,
    work_id: &str,
) -> Result<bool> {
    ensure_core_transition_guard_schema(conn)?;
    let count = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_transition_proofs
        WHERE entity_type = 'WorkItem'
          AND entity_id = ?1
          AND to_state = 'Closed'
          AND accepted = 1
          AND json_valid(request_json) = 1
          AND (
                json_extract(request_json, '$.metadata.reviewed_work_terminal_success') = 'true'
             OR (
                    json_type(request_json, '$.metadata.terminal_policy_proof') = 'text'
                AND TRIM(json_extract(request_json, '$.metadata.terminal_policy_proof')) <> ''
             )
          )
        "#,
        params![work_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count > 0)
}

pub(super) fn ticket_self_work_core_state(raw: &str) -> Result<CoreState> {
    let status = parse_work_item_status(raw)?;
    match status {
        WorkItemStatus::Created => Ok(CoreState::Created),
        WorkItemStatus::Open
        | WorkItemStatus::Queued
        | WorkItemStatus::Restored
        | WorkItemStatus::Publishing => Ok(CoreState::Planned),
        WorkItemStatus::Published | WorkItemStatus::Executing => Ok(CoreState::Executing),
        WorkItemStatus::AwaitingReview => Ok(CoreState::AwaitingReview),
        WorkItemStatus::ReworkRequired => Ok(CoreState::ReworkRequired),
        WorkItemStatus::AwaitingVerification => Ok(CoreState::AwaitingVerification),
        WorkItemStatus::Verified => Ok(CoreState::Verified),
        WorkItemStatus::Blocked | WorkItemStatus::Spilled => Ok(CoreState::Blocked),
        WorkItemStatus::Failed => Ok(CoreState::Failed),
        WorkItemStatus::Closed | WorkItemStatus::Handled => Ok(CoreState::Closed),
        WorkItemStatus::Cancelled | WorkItemStatus::Superseded => Ok(CoreState::Superseded),
        WorkItemStatus::Ready | WorkItemStatus::Waiting | WorkItemStatus::Satisfied => {
            anyhow::bail!(
                "ticket internal work status `{}` is not a state in the core state machine",
                status.as_str()
            )
        }
    }
}

pub(super) fn ticket_self_work_core_event(state: &str) -> Result<CoreEvent> {
    let status = parse_work_item_status(state)?;
    match status {
        WorkItemStatus::Created => Ok(CoreEvent::CreateTicket),
        WorkItemStatus::Open
        | WorkItemStatus::Queued
        | WorkItemStatus::Restored
        | WorkItemStatus::Publishing => Ok(CoreEvent::Plan),
        WorkItemStatus::Published | WorkItemStatus::Executing => Ok(CoreEvent::Execute),
        WorkItemStatus::AwaitingReview => Ok(CoreEvent::RequestReview),
        WorkItemStatus::ReworkRequired => Ok(CoreEvent::RequireRework),
        WorkItemStatus::AwaitingVerification | WorkItemStatus::Verified => Ok(CoreEvent::Verify),
        WorkItemStatus::Blocked | WorkItemStatus::Spilled => Ok(CoreEvent::Block),
        WorkItemStatus::Failed => Ok(CoreEvent::Fail),
        WorkItemStatus::Closed | WorkItemStatus::Handled => Ok(CoreEvent::Close),
        WorkItemStatus::Cancelled | WorkItemStatus::Superseded => Ok(CoreEvent::Supersede),
        WorkItemStatus::Ready | WorkItemStatus::Waiting | WorkItemStatus::Satisfied => {
            anyhow::bail!(
                "ticket internal work status `{}` has no core transition event",
                status.as_str()
            )
        }
    }
}

fn preflight_ticket_self_work_state_transition(
    conn: &mut Connection,
    work_id: &str,
    from_state: &str,
    to_state: &str,
    failure_reason: Option<&str>,
) -> Result<()> {
    let tx = conn.transaction()?;
    let verdict = enforce_ticket_self_work_state_transition(
        &tx,
        work_id,
        from_state,
        to_state,
        "ctox-ticket",
        "set_ticket_self_work_state",
        failure_reason,
        None,
    );
    tx.rollback()?;
    verdict
}

pub(super) fn set_ticket_self_work_state_internal(
    root: &Path,
    conn: &mut Connection,
    work_id: &str,
    state: &str,
    failure_reason: Option<&str>,
) -> Result<TicketSelfWorkItemView> {
    set_ticket_self_work_state_internal_with_policy(
        root,
        conn,
        work_id,
        state,
        failure_reason,
        None,
    )
}

pub(super) fn set_ticket_self_work_state_internal_with_policy(
    root: &Path,
    conn: &mut Connection,
    work_id: &str,
    state: &str,
    failure_reason: Option<&str>,
    terminal_policy_grant: Option<&TerminalPolicyGrant>,
) -> Result<TicketSelfWorkItemView> {
    let status = parse_work_item_status(state)?;
    let state = status.as_str();
    plan::ensure_wait_resolution_schema(root, conn)?;
    let tx = conn.transaction()?;
    let existing = load_ticket_self_work_item_raw(&tx, work_id)?
        .context("ticket internal work item not found")?;
    enforce_ticket_self_work_state_transition(
        &tx,
        work_id,
        &existing.state,
        state,
        "ctox-ticket",
        "set_ticket_self_work_state",
        failure_reason,
        terminal_policy_grant,
    )?;
    let now = now_iso_string();
    tx.execute(
        r#"
        UPDATE ticket_self_work_items
        SET state = ?2,
            updated_at = ?3
        WHERE work_id = ?1
        "#,
        params![work_id, state, now],
    )?;
    let satisfied_waits = plan::satisfy_wait_for_work_item_tx(&tx, work_id, state)?;
    tx.commit()?;
    clear_ticket_self_work_list_cache();
    plan::finish_satisfied_wait_write(root, satisfied_waits)?;
    let item = load_ticket_self_work_item_raw(conn, work_id)?
        .context("ticket internal work item not found")?;
    hydrate_ticket_self_work_item(conn, item)
}

pub(super) fn touch_ticket_self_work_item(conn: &mut Connection, work_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE ticket_self_work_items SET updated_at = ?2 WHERE work_id = ?1",
        params![work_id, now_iso_string()],
    )?;
    clear_ticket_self_work_list_cache();
    Ok(())
}

pub(super) fn insert_ticket_self_work_assignment(
    conn: &mut Connection,
    work_id: &str,
    assigned_to: &str,
    assigned_by: &str,
    rationale: Option<&str>,
    remote_event_id: Option<&str>,
) -> Result<TicketSelfWorkAssignmentView> {
    let now = now_iso_string();
    let assignment_id = format!(
        "swa:{}:{}",
        work_id,
        stable_digest(&(assigned_to.to_string() + now.as_str()))
    );
    conn.execute(
        r#"
        INSERT INTO ticket_self_work_assignments (
            assignment_id, work_id, assigned_to, assigned_by, rationale, remote_event_id, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            assignment_id,
            work_id,
            assigned_to.trim(),
            assigned_by.trim(),
            rationale,
            remote_event_id,
            now
        ],
    )?;
    clear_ticket_self_work_list_cache();
    conn.query_row(
        r#"
        SELECT assignment_id, work_id, assigned_to, assigned_by, rationale, remote_event_id, created_at
        FROM ticket_self_work_assignments
        WHERE assignment_id = ?1
        LIMIT 1
        "#,
        params![assignment_id],
        map_ticket_self_work_assignment_row,
    ).map_err(anyhow::Error::from)
}

pub(super) fn insert_ticket_self_work_note(
    conn: &mut Connection,
    work_id: &str,
    body: &str,
    visibility: &str,
    authored_by: &str,
    remote_event_id: Option<&str>,
) -> Result<TicketSelfWorkNoteView> {
    let now = now_iso_string();
    let note_id = format!(
        "swn:{}:{}",
        work_id,
        stable_digest(&(body.to_string() + now.as_str()))
    );
    conn.execute(
        r#"
        INSERT INTO ticket_self_work_notes (
            note_id, work_id, body_text, visibility, authored_by, remote_event_id, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            note_id,
            work_id,
            body.trim(),
            visibility.trim(),
            authored_by.trim(),
            remote_event_id,
            now
        ],
    )?;
    clear_ticket_self_work_list_cache();
    conn.query_row(
        r#"
        SELECT note_id, work_id, body_text, visibility, authored_by, remote_event_id, created_at
        FROM ticket_self_work_notes
        WHERE note_id = ?1
        LIMIT 1
        "#,
        params![note_id],
        map_ticket_self_work_note_row,
    )
    .map_err(anyhow::Error::from)
}

pub(super) fn put_ticket_knowledge_entry_internal(
    conn: &mut Connection,
    input: TicketKnowledgeUpsertInput,
) -> Result<TicketKnowledgeEntryView> {
    let now = now_iso_string();
    let entry_id = format!(
        "knowledge:{}:{}:{}",
        input.source_system,
        input.domain,
        stable_digest(&input.knowledge_key)
    );
    conn.execute(
        r#"
        INSERT INTO ticket_knowledge_entries (
            entry_id, source_system, domain, knowledge_key, title, summary, status,
            content_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
        ON CONFLICT(source_system, domain, knowledge_key) DO UPDATE SET
            title=excluded.title,
            summary=excluded.summary,
            status=excluded.status,
            content_json=excluded.content_json,
            updated_at=excluded.updated_at
        "#,
        params![
            entry_id,
            input.source_system,
            input.domain,
            input.knowledge_key,
            input.title,
            input.summary,
            input.status,
            serde_json::to_string(&input.content)?,
            now,
        ],
    )?;
    conn.query_row(
        r#"
        SELECT entry_id, source_system, domain, knowledge_key, title, summary, status, content_json, created_at, updated_at
        FROM ticket_knowledge_entries
        WHERE source_system = ?1 AND domain = ?2 AND knowledge_key = ?3
        LIMIT 1
        "#,
        params![input.source_system, input.domain, input.knowledge_key],
        map_ticket_knowledge_entry_row,
    )
    .map_err(anyhow::Error::from)
}

pub(crate) fn put_ticket_knowledge_entry(
    root: &Path,
    input: TicketKnowledgeUpsertInput,
) -> Result<TicketKnowledgeEntryView> {
    let mut conn = open_ticket_db(root)?;
    put_ticket_knowledge_entry_internal(&mut conn, input)
}

pub(super) fn upsert_ticket_self_work_item_internal(
    root: &Path,
    conn: &mut Connection,
    input: TicketSelfWorkUpsertInput,
) -> Result<TicketSelfWorkItemView> {
    let status = parse_work_item_status(&input.state)?;
    let state = status.as_str();
    let now = now_iso_string();
    let dedupe_key = input
        .metadata
        .get("dedupe_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let work_id = format!(
        "self-work:{}:{}",
        input.source_system,
        stable_digest(dedupe_key.as_deref().unwrap_or(&format!(
            "{}:{}:{}:{}",
            input.kind, input.title, input.body_text, now
        )),)
    );
    plan::ensure_wait_resolution_schema(root, conn)?;
    let tx = conn.transaction()?;
    if let Some(existing) = load_ticket_self_work_item_raw(&tx, &work_id)? {
        enforce_ticket_self_work_state_transition(
            &tx,
            &existing.work_id,
            &existing.state,
            state,
            "ctox-ticket",
            "self_work_item_upsert",
            None,
            None,
        )?;
    }
    tx.execute(
        r#"
        INSERT INTO ticket_self_work_items (
            work_id, source_system, kind, title, body_text, state, metadata_json,
            remote_ticket_id, remote_locator, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8, ?8)
        ON CONFLICT(work_id) DO UPDATE SET
            title=excluded.title,
            body_text=excluded.body_text,
            state=CASE
                WHEN ticket_self_work_items.state = ?9 THEN ticket_self_work_items.state
                ELSE excluded.state
            END,
            metadata_json=excluded.metadata_json,
            updated_at=excluded.updated_at
        "#,
        params![
            work_id,
            input.source_system,
            input.kind,
            input.title,
            input.body_text,
            state,
            serde_json::to_string(&input.metadata)?,
            now,
            WorkItemStatus::Published.as_str(),
        ],
    )?;
    let item = tx.query_row(
        r#"
        SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items
        WHERE work_id = ?1
        LIMIT 1
        "#,
        params![work_id],
        map_ticket_self_work_row,
    )?;
    let satisfied_waits = plan::satisfy_wait_for_work_item_tx(&tx, &item.work_id, &item.state)?;
    tx.commit()?;
    clear_ticket_self_work_list_cache();
    plan::finish_satisfied_wait_write(root, satisfied_waits)?;
    Ok(item)
}

pub(super) fn mark_ticket_self_work_published(
    conn: &mut Connection,
    work_id: &str,
    remote_ticket_id: Option<&str>,
    remote_locator: Option<&str>,
) -> Result<TicketSelfWorkItemView> {
    let existing = load_ticket_self_work_item_raw(conn, work_id)?
        .context("ticket internal work item not found")?;
    let published = WorkItemStatus::Published.as_str();
    enforce_ticket_self_work_state_transition(
        conn,
        work_id,
        &existing.state,
        published,
        "ctox-ticket",
        "mark_ticket_self_work_published",
        None,
        None,
    )?;
    let now = now_iso_string();
    conn.execute(
        r#"
        UPDATE ticket_self_work_items
        SET state = ?2,
            remote_ticket_id = ?3,
            remote_locator = ?4,
            updated_at = ?5
        WHERE work_id = ?1
        "#,
        params![work_id, published, remote_ticket_id, remote_locator, now],
    )?;
    clear_ticket_self_work_list_cache();
    conn.query_row(
        r#"
        SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items
        WHERE work_id = ?1
        LIMIT 1
        "#,
        params![work_id],
        map_ticket_self_work_row,
    )
    .map_err(anyhow::Error::from)
}
