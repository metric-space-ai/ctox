// Origin: CTOX
// License: Apache-2.0

use super::policy::{BusinessOsPermission, BusinessOsRole};
use super::store::{
    self, BusinessCommand, BusinessOsSession, BusinessOsSessionUser, CommandOrigin,
};
use anyhow::Context;
use rusqlite::Connection;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct ProjectionRef {
    collection: &'static str,
    record_id: String,
}

#[derive(Debug, Clone)]
struct ProjectionActor {
    id: String,
    display_name: String,
    role: String,
}

#[derive(Debug, Clone, Copy)]
struct AppRelevanceSpec {
    collection: &'static str,
    module: &'static str,
    record_type: &'static str,
    link_type: &'static str,
    kind: &'static str,
}

const APP_RELEVANCE_SPECS: &[AppRelevanceSpec] = &[
    AppRelevanceSpec {
        collection: "ctox_ticket_approvals",
        module: "tickets",
        record_type: "ticket_case",
        link_type: "ticket_case",
        kind: "approval",
    },
    AppRelevanceSpec {
        collection: "ctox_ticket_clarification_requests",
        module: "tickets",
        record_type: "ticket_case",
        link_type: "ticket_case",
        kind: "app_event",
    },
    AppRelevanceSpec {
        collection: "ctox_ticket_self_work_items",
        module: "tickets",
        record_type: "ticket_self_work",
        link_type: "ticket_self_work",
        kind: "app_event",
    },
    AppRelevanceSpec {
        collection: "ctox_ticket_self_work_notes",
        module: "tickets",
        record_type: "ticket_self_work",
        link_type: "ticket_self_work",
        kind: "app_event",
    },
    AppRelevanceSpec {
        collection: "support_conversations",
        module: "support",
        record_type: "conversation",
        link_type: "support_conversation",
        kind: "app_event",
    },
    AppRelevanceSpec {
        collection: "support_agent_requests",
        module: "support",
        record_type: "conversation",
        link_type: "support_conversation",
        kind: "ctox_task",
    },
    AppRelevanceSpec {
        collection: "support_notes",
        module: "support",
        record_type: "conversation",
        link_type: "support_conversation",
        kind: "note",
    },
    AppRelevanceSpec {
        collection: "outbound_approvals",
        module: "outbound",
        record_type: "engagement",
        link_type: "outbound_engagement",
        kind: "approval",
    },
    AppRelevanceSpec {
        collection: "outbound_research_runs",
        module: "outbound",
        record_type: "research_run",
        link_type: "research_run",
        kind: "ctox_task",
    },
    AppRelevanceSpec {
        collection: "research_tasks",
        module: "research",
        record_type: "research_task",
        link_type: "research_run",
        kind: "app_event",
    },
    AppRelevanceSpec {
        collection: "research_runs",
        module: "research",
        record_type: "research_task",
        link_type: "research_run",
        kind: "ctox_task",
    },
    AppRelevanceSpec {
        collection: "documents",
        module: "documents",
        record_type: "document",
        link_type: "document",
        kind: "app_event",
    },
    AppRelevanceSpec {
        collection: "notes",
        module: "notes",
        record_type: "note",
        link_type: "app_record",
        kind: "note",
    },
];

#[derive(Debug, Clone, Default)]
pub(super) struct RelevanceProjectionOutcome {
    pub changed_count: usize,
    pub source_cursors: Vec<(&'static str, i64)>,
    pub projections: Vec<(&'static str, String)>,
}

pub(super) fn is_threads_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "threads.note.create"
            | "threads.note.update"
            | "threads.note.delete"
            | "threads.message.create"
            | "threads.thread.watch"
            | "threads.thread.unwatch"
            | "threads.thread.archive"
            | "threads.thread.snooze"
            | "threads.ctox_approval.request"
            | "threads.ctox_approval.edit"
            | "threads.ctox_approval.approve"
            | "threads.ctox_approval.reject"
            | "threads.ctox_approval.cancel"
            | "threads.link.create"
            | "threads.link.remove"
            | "threads.notification.mark_read"
            | "threads.notification.dismiss"
    )
}

pub(super) fn requires_external_approval(command_type: &str) -> bool {
    matches!(
        command_type,
        "threads.ctox_approval.approve" | "threads.ctox_approval.reject"
    )
}

pub(super) fn is_threads_owned_collection(collection: &str) -> bool {
    matches!(
        collection,
        "user_threads"
            | "user_thread_messages"
            | "user_thread_links"
            | "user_notifications"
            | "ctox_task_approval_requests"
    )
}

pub(super) fn may_accept_peer_write(_root: &Path, _token: &str, collection: &str) -> bool {
    if is_threads_owned_collection(collection) {
        return false;
    }
    if collection == "business_commands" {
        // Command documents carry their own authenticated client context and
        // are admitted by `accept_rxdb_business_command`; do not pre-filter
        // them by the peer-session token here.
        return true;
    }
    if collection == "ctox_queue_tasks" {
        return false;
    }
    true
}

pub(super) fn may_replicate_document(
    root: &Path,
    token: &str,
    collection: &str,
    document: &Value,
) -> bool {
    let Some((user_id, role)) = store::verify_capability_actor(root, token) else {
        return !is_threads_owned_collection(collection)
            && collection != "business_commands"
            && collection != "ctox_queue_tasks";
    };
    if matches!(
        super::policy::parse_role(&role),
        BusinessOsRole::Chef | BusinessOsRole::Admin
    ) {
        return true;
    }
    if collection == "business_commands" {
        return ctox_command_document_visible_to_user(document, &user_id);
    }
    if collection == "ctox_queue_tasks" {
        return ctox_task_document_visible_to_user(root, document, &user_id);
    }
    if !is_threads_owned_collection(collection) {
        return true;
    }
    match collection {
        "user_notifications" => value_string(document, "user_id") == user_id,
        "ctox_task_approval_requests" => {
            value_string(document, "requester_user_id") == user_id
                || value_string(document, "reviewer_user_id") == user_id
                || value_string(document, "decision_by_id") == user_id
                || thread_document_visible_to_user(root, document, &user_id)
        }
        "user_threads" => thread_record_visible_to_user(document, &user_id),
        "user_thread_messages" => {
            value_string(document, "author_user_id") == user_id
                || array_strings(document.get("target_user_ids")).contains(&user_id)
                || thread_document_visible_to_user(root, document, &user_id)
        }
        "user_thread_links" => thread_document_visible_to_user(root, document, &user_id),
        _ => false,
    }
}

pub(super) fn project_ctox_relevance(
    root: &Path,
    command_since_ms: i64,
    task_since_ms: i64,
    limit: usize,
) -> anyhow::Result<RelevanceProjectionOutcome> {
    let limit = limit.clamp(1, 2_000);
    let commands = pull_projection_documents(root, "business_commands", command_since_ms, limit)?;
    let tasks = pull_projection_documents(root, "ctox_queue_tasks", task_since_ms, limit)?;
    let mut command_by_id = BTreeMap::new();
    let mut max_command_updated_at_ms = command_since_ms;
    for command in &commands {
        max_command_updated_at_ms = max_command_updated_at_ms.max(document_updated_at_ms(command));
        if document_is_deleted(command) {
            continue;
        }
        let command_id = first_non_empty_owned([
            value_string(command, "command_id"),
            value_string(command, "id"),
        ]);
        if !command_id.is_empty() {
            command_by_id.insert(command_id, command.clone());
        }
    }
    let mut max_task_updated_at_ms = task_since_ms;
    for task in &tasks {
        max_task_updated_at_ms = max_task_updated_at_ms.max(document_updated_at_ms(task));
    }

    let conn = store::open_store(root)?;
    let mut projections = Vec::new();
    for command in commands
        .iter()
        .filter(|document| !document_is_deleted(document))
    {
        project_ctox_command_document(root, &conn, command, None, &mut projections)?;
    }
    for task in tasks
        .iter()
        .filter(|document| !document_is_deleted(document))
    {
        let command_id = value_string(task, "command_id");
        let command = command_by_id.get(&command_id).cloned().or_else(|| {
            load_record(root, "business_commands", &command_id)
                .ok()
                .flatten()
        });
        project_ctox_task_document(root, &conn, task, command.as_ref(), &mut projections)?;
    }

    let projection_pairs = projection_pairs(projections);
    Ok(RelevanceProjectionOutcome {
        changed_count: projection_pairs.len(),
        source_cursors: vec![
            ("business_commands", max_command_updated_at_ms),
            ("ctox_queue_tasks", max_task_updated_at_ms),
        ],
        projections: projection_pairs,
    })
}

pub(super) fn app_relevance_source_collections() -> &'static [&'static str] {
    static COLLECTIONS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    COLLECTIONS
        .get_or_init(|| {
            APP_RELEVANCE_SPECS
                .iter()
                .map(|spec| spec.collection)
                .collect()
        })
        .as_slice()
}

pub(super) fn project_app_relevance(
    root: &Path,
    source_cursors: &[(&'static str, i64)],
    limit: usize,
) -> anyhow::Result<RelevanceProjectionOutcome> {
    let limit = limit.clamp(1, 2_000);
    let conn = store::open_store(root)?;
    let mut projections = Vec::new();
    let mut cursors = Vec::new();

    for (collection, since_ms) in source_cursors {
        if app_relevance_spec(collection).is_none() {
            continue;
        }
        let documents = pull_projection_documents(root, collection, *since_ms, limit)?;
        let mut max_updated_at_ms = *since_ms;
        for document in &documents {
            max_updated_at_ms = max_updated_at_ms.max(document_updated_at_ms(document));
            if document_is_deleted(document) {
                continue;
            }
            project_app_record_document(root, &conn, collection, document, &mut projections)?;
        }
        cursors.push((*collection, max_updated_at_ms));
    }

    let projection_pairs = projection_pairs(projections);
    Ok(RelevanceProjectionOutcome {
        changed_count: projection_pairs.len(),
        source_cursors: cursors,
        projections: projection_pairs,
    })
}

pub(super) fn handle_business_command(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        command.module == "threads",
        "threads commands require module=threads"
    );
    match command.command_type.as_str() {
        "threads.note.create" => create_note(root, session, command),
        "threads.note.update" => update_note(root, session, command),
        "threads.note.delete" => delete_note(root, session, command),
        "threads.message.create" => create_message(root, session, command),
        "threads.thread.watch" => update_thread_watch(root, session, command, true),
        "threads.thread.unwatch" => update_thread_watch(root, session, command, false),
        "threads.thread.archive" => archive_thread(root, session, command),
        "threads.thread.snooze" => snooze_thread(root, session, command),
        "threads.ctox_approval.request" => request_approval(root, session, command),
        "threads.ctox_approval.edit" => edit_approval(root, session, command),
        "threads.ctox_approval.approve" => approve_approval(root, session, command),
        "threads.ctox_approval.reject" => reject_approval(root, session, command),
        "threads.ctox_approval.cancel" => cancel_approval(root, session, command),
        "threads.link.create" => create_link(root, session, command),
        "threads.link.remove" => remove_link(root, session, command),
        "threads.notification.mark_read" => {
            update_notification_status(root, session, command, "read")
        }
        "threads.notification.dismiss" => {
            update_notification_status(root, session, command, "dismissed")
        }
        other => anyhow::bail!("unsupported threads command type: {other}"),
    }
}

fn create_note(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let body = required_string(&command.payload, &["body", "message", "note"])?;
    let source = source_context(command);
    ensure_source_context_read_policy(root, session, &source)?;
    let message_kind = first_string_field(&command.payload, &["message_type", "kind"])
        .filter(|kind| matches!(kind.as_str(), "note" | "mention"))
        .unwrap_or_else(|| "note".to_owned());
    let thread_id = thread_id_for_command(command, &source);
    let title = first_string_field(&command.payload, &["title", "subject"])
        .or_else(|| source_string(&source, "label"))
        .unwrap_or_else(|| "Notiz".to_owned());
    let target_user_ids = target_user_ids(&command.payload);
    let actor = actor_id(session);
    let participants = participant_set(root, &thread_id, [actor.as_str()], target_user_ids.iter());
    let message_id = first_string_field(&command.payload, &["message_id", "note_id"])
        .unwrap_or_else(|| format!("msg_{}", Uuid::new_v4()));
    let command_id = command.id.as_deref().context("command id is required")?;
    let conn = store::open_store(root)?;
    let mut projections = Vec::new();

    upsert_thread(
        root,
        &conn,
        &thread_id,
        &title,
        if message_kind == "mention" {
            "mention"
        } else {
            "note"
        },
        "open",
        &participants,
        &source,
        &session,
        target_user_ids
            .first()
            .map(String::as_str)
            .unwrap_or_default(),
        Some(&message_id),
        now,
        0,
        &mut projections,
    )?;
    upsert_source_link(&conn, &thread_id, &source, now, &mut projections)?;
    upsert_message(
        &conn,
        &thread_id,
        &message_id,
        &message_kind,
        &session,
        &target_user_ids,
        &body,
        &source,
        "",
        command_id,
        now,
        &mut projections,
    )?;
    upsert_notifications(
        &conn,
        &thread_id,
        &message_id,
        "",
        if message_kind == "mention" {
            "mention"
        } else {
            "note"
        },
        &target_user_ids,
        &title,
        &body,
        &source,
        now,
        &mut projections,
    )?;

    Ok(json!({
        "ok": true,
        "thread_id": thread_id,
        "message_id": message_id,
        "projections": projection_values(projections),
    }))
}

fn update_note(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let message_id = first_string_field(&command.payload, &["message_id", "note_id", "id"])
        .or_else(|| command.record_id.clone())
        .context("message_id is required")?;
    let body = required_string(&command.payload, &["body", "message", "note"])?;
    let mut message = load_record(root, "user_thread_messages", &message_id)?
        .with_context(|| format!("thread message {message_id} not found"))?;
    ensure_message_author_or_admin(session, &message)?;
    set_object_string(&mut message, "body", &body);
    set_object_i64(&mut message, "updated_at_ms", now);
    let next_targets = target_user_ids(&command.payload);
    if !next_targets.is_empty() {
        set_object_array_strings(&mut message, "target_user_ids", &next_targets);
    }
    let thread_id = value_string(&message, "thread_id");
    let source = thread_source_context(&message).unwrap_or_else(|| source_context(command));
    let conn = store::open_store(root)?;
    let mut projections = Vec::new();
    store::upsert_business_record(&conn, "user_thread_messages", &message_id, now, message)?;
    projections.push(ProjectionRef {
        collection: "user_thread_messages",
        record_id: message_id.clone(),
    });
    if !thread_id.is_empty() {
        upsert_thread_status_delta(
            root,
            &conn,
            &thread_id,
            "open",
            Some(&message_id),
            now,
            0,
            &mut projections,
        )?;
    }
    if !next_targets.is_empty() {
        upsert_notifications(
            &conn,
            &thread_id,
            &message_id,
            "",
            "mention",
            &next_targets,
            "Thread aktualisiert",
            &body,
            &source,
            now,
            &mut projections,
        )?;
    }
    Ok(json!({
        "ok": true,
        "message_id": message_id,
        "thread_id": thread_id,
        "projections": projection_values(projections),
    }))
}

fn delete_note(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let message_id = first_string_field(&command.payload, &["message_id", "note_id", "id"])
        .or_else(|| command.record_id.clone())
        .context("message_id is required")?;
    let mut message = load_record(root, "user_thread_messages", &message_id)?
        .with_context(|| format!("thread message {message_id} not found"))?;
    ensure_message_author_or_admin(session, &message)?;
    let thread_id = value_string(&message, "thread_id");
    soft_delete_payload(&mut message, now);
    let conn = store::open_store(root)?;
    store::upsert_business_record(&conn, "user_thread_messages", &message_id, now, message)?;
    Ok(json!({
        "ok": true,
        "message_id": message_id,
        "thread_id": thread_id,
        "projections": [{ "collection": "user_thread_messages", "record_id": message_id }],
    }))
}

fn create_message(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let thread_id = first_string_field(&command.payload, &["thread_id"])
        .or_else(|| command.record_id.clone())
        .context("thread_id is required")?;
    let body = required_string(&command.payload, &["body", "message", "note"])?;
    let target_user_ids = target_user_ids(&command.payload);
    let actor = actor_id(session);
    let thread = load_record(root, "user_threads", &thread_id)?.unwrap_or_else(|| json!({}));
    let source = thread_source_context(&thread).unwrap_or_else(|| source_context(command));
    ensure_source_context_read_policy(root, session, &source)?;
    let title = thread
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| "Thread".to_owned());
    let participants = participant_set(root, &thread_id, [actor.as_str()], target_user_ids.iter());
    let message_id = first_string_field(&command.payload, &["message_id"])
        .unwrap_or_else(|| format!("msg_{}", Uuid::new_v4()));
    let command_id = command.id.as_deref().context("command id is required")?;
    let conn = store::open_store(root)?;
    let mut projections = Vec::new();
    upsert_thread(
        root,
        &conn,
        &thread_id,
        &title,
        "thread",
        "open",
        &participants,
        &source,
        session,
        target_user_ids
            .first()
            .map(String::as_str)
            .unwrap_or_default(),
        Some(&message_id),
        now,
        0,
        &mut projections,
    )?;
    upsert_message(
        &conn,
        &thread_id,
        &message_id,
        "message",
        session,
        &target_user_ids,
        &body,
        &source,
        "",
        command_id,
        now,
        &mut projections,
    )?;
    upsert_notifications(
        &conn,
        &thread_id,
        &message_id,
        "",
        "message",
        &target_user_ids,
        &title,
        &body,
        &source,
        now,
        &mut projections,
    )?;
    Ok(json!({
        "ok": true,
        "thread_id": thread_id,
        "message_id": message_id,
        "projections": projection_values(projections),
    }))
}

fn request_approval(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let prompt = required_string(&command.payload, &["prompt", "instruction", "body"])?;
    let reviewer_user_id = required_string(&command.payload, &["reviewer_user_id", "reviewer"])?;
    let source = source_context(command);
    ensure_source_context_read_policy(root, session, &source)?;
    let thread_id = thread_id_for_command(command, &source);
    let approval_id = first_string_field(&command.payload, &["approval_request_id", "id"])
        .unwrap_or_else(|| format!("approval_{}", Uuid::new_v4()));
    let message_id = format!("msg_{}", Uuid::new_v4());
    let title = first_string_field(&command.payload, &["title", "subject"])
        .or_else(|| source_string(&source, "label"))
        .unwrap_or_else(|| "CTOX Freigabe".to_owned());
    let actor = actor_id(session);
    let participants = participant_set(
        root,
        &thread_id,
        [actor.as_str(), reviewer_user_id.as_str()],
        std::iter::empty::<&String>(),
    );
    let command_id = command.id.as_deref().context("command id is required")?;
    let conn = store::open_store(root)?;
    let mut projections = Vec::new();

    upsert_thread(
        root,
        &conn,
        &thread_id,
        &title,
        "approval",
        "open",
        &participants,
        &source,
        session,
        &reviewer_user_id,
        Some(&message_id),
        now,
        1,
        &mut projections,
    )?;
    upsert_source_link(&conn, &thread_id, &source, now, &mut projections)?;
    let approval = approval_record(
        &approval_id,
        &thread_id,
        "pending",
        session,
        &reviewer_user_id,
        "",
        "",
        command,
        &source,
        &prompt,
        now,
        None,
    );
    store::upsert_business_record(
        &conn,
        "ctox_task_approval_requests",
        &approval_id,
        now,
        approval.clone(),
    )?;
    projections.push(ProjectionRef {
        collection: "ctox_task_approval_requests",
        record_id: approval_id.clone(),
    });
    upsert_message(
        &conn,
        &thread_id,
        &message_id,
        "approval_request",
        session,
        std::slice::from_ref(&reviewer_user_id),
        &prompt,
        &source,
        &approval_id,
        command_id,
        now,
        &mut projections,
    )?;
    upsert_notifications(
        &conn,
        &thread_id,
        &message_id,
        &approval_id,
        "approval_request",
        std::slice::from_ref(&reviewer_user_id),
        &title,
        &prompt,
        &source,
        now,
        &mut projections,
    )?;

    Ok(json!({
        "ok": true,
        "thread_id": thread_id,
        "approval_request_id": approval_id,
        "status": "pending",
        "projections": projection_values(projections),
    }))
}

fn edit_approval(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let approval_id = approval_id_from_command(command)?;
    let approval = load_record(root, "ctox_task_approval_requests", &approval_id)?
        .with_context(|| format!("approval request {approval_id} not found"))?;
    ensure_pending_approval(&approval)?;
    ensure_approval_editor(session, &approval)?;

    let mut next = approval.clone();
    let prompt = first_string_field(&command.payload, &["prompt", "instruction", "body"])
        .unwrap_or_else(|| value_string(&approval, "prompt"));
    set_object_string(&mut next, "prompt", &prompt);
    set_object_string(
        &mut next,
        "instruction",
        &first_string_field(&command.payload, &["instruction"]).unwrap_or_else(|| prompt.clone()),
    );
    if let Some(command_type) = first_string_field(&command.payload, &["target_command_type"]) {
        set_object_string(&mut next, "target_command_type", &command_type);
    }
    if let Some(target_module) = first_string_field(&command.payload, &["target_module"]) {
        set_object_string(&mut next, "target_module", &target_module);
    }
    if let Some(target_record_id) = first_string_field(&command.payload, &["target_record_id"]) {
        set_object_string(&mut next, "target_record_id", &target_record_id);
    }
    if let Some(target_payload) = command
        .payload
        .get("target_payload")
        .filter(|value| value.is_object())
    {
        set_object_value(&mut next, "target_payload", target_payload.clone());
    }
    set_object_i64(&mut next, "updated_at_ms", now);

    let thread_id = value_string(&approval, "thread_id");
    let source = approval
        .get("source_context")
        .cloned()
        .unwrap_or_else(|| thread_source_context(&approval).unwrap_or_else(|| json!({})));
    let requester = value_string(&approval, "requester_user_id");
    let reviewer = value_string(&approval, "reviewer_user_id");
    let actor = actor_id(session);
    let notify_user = if actor == requester {
        reviewer
    } else {
        requester
    };
    let message_id = format!("msg_{}", Uuid::new_v4());
    let conn = store::open_store(root)?;
    let mut projections = Vec::new();
    store::upsert_business_record(
        &conn,
        "ctox_task_approval_requests",
        &approval_id,
        now,
        next,
    )?;
    projections.push(ProjectionRef {
        collection: "ctox_task_approval_requests",
        record_id: approval_id.clone(),
    });
    upsert_thread_status_delta(
        root,
        &conn,
        &thread_id,
        "open",
        Some(&message_id),
        now,
        0,
        &mut projections,
    )?;
    let target_users = if notify_user.is_empty() {
        Vec::new()
    } else {
        vec![notify_user]
    };
    upsert_message(
        &conn,
        &thread_id,
        &message_id,
        "approval_edited",
        session,
        &target_users,
        &prompt,
        &source,
        &approval_id,
        command.id.as_deref().unwrap_or_default(),
        now,
        &mut projections,
    )?;
    upsert_notifications(
        &conn,
        &thread_id,
        &message_id,
        &approval_id,
        "approval_edited",
        &target_users,
        "CTOX Freigabe aktualisiert",
        &prompt,
        &source,
        now,
        &mut projections,
    )?;

    Ok(json!({
        "ok": true,
        "approval_request_id": approval_id,
        "thread_id": thread_id,
        "status": "pending",
        "projections": projection_values(projections),
    }))
}

fn approve_approval(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let approval_id = approval_id_from_command(command)?;
    let approval = load_record(root, "ctox_task_approval_requests", &approval_id)?
        .with_context(|| format!("approval request {approval_id} not found"))?;
    ensure_pending_approval(&approval)?;
    ensure_reviewer_or_admin(session, &approval)?;
    ensure_approval_target_policy(root, session, &approval)?;

    let approved = enqueue_approved_ctox_command(root, session, &approval, command)?;
    let approved_command_id = approved.command_id.clone();
    let approved_task_id = approved.task_id.clone().unwrap_or_default();

    let thread_id = value_string(&approval, "thread_id");
    let source = approval
        .get("source_context")
        .cloned()
        .unwrap_or_else(|| thread_source_context(&approval).unwrap_or_else(|| json!({})));
    let prompt = value_string(&approval, "prompt");
    let decision_note =
        first_string_field(&command.payload, &["decision_note", "note"]).unwrap_or_default();
    let message_id = format!("msg_{}", Uuid::new_v4());
    let conn = store::open_store(root)?;
    let mut projections = Vec::new();
    let requester = value_string(&approval, "requester_user_id");
    let mut next = approval.clone();
    set_object_string(&mut next, "status", "approved");
    set_object_i64(&mut next, "decided_at_ms", now);
    set_object_string(&mut next, "decision_by_id", &actor_id(session));
    set_object_string(&mut next, "decision_note", &decision_note);
    set_object_string(&mut next, "approved_command_id", &approved_command_id);
    set_object_string(&mut next, "approved_task_id", &approved_task_id);
    store::upsert_business_record(
        &conn,
        "ctox_task_approval_requests",
        &approval_id,
        now,
        next,
    )?;
    record_approval_decision_event(
        &conn,
        session,
        command,
        &approval,
        "approved",
        &approved_command_id,
        &approved_task_id,
        &decision_note,
        now,
    )?;
    projections.push(ProjectionRef {
        collection: "ctox_task_approval_requests",
        record_id: approval_id.clone(),
    });
    upsert_thread_status_delta(
        root,
        &conn,
        &thread_id,
        "open",
        Some(&message_id),
        now,
        -1,
        &mut projections,
    )?;
    upsert_message(
        &conn,
        &thread_id,
        &message_id,
        "approval_approved",
        session,
        std::slice::from_ref(&requester),
        &format!("Freigegeben: {prompt}"),
        &source,
        &approval_id,
        &approved_command_id,
        now,
        &mut projections,
    )?;
    upsert_notifications(
        &conn,
        &thread_id,
        &message_id,
        &approval_id,
        "approval_approved",
        std::slice::from_ref(&requester),
        "CTOX Freigabe erteilt",
        &prompt,
        &source,
        now,
        &mut projections,
    )?;

    Ok(json!({
        "ok": true,
        "approval_request_id": approval_id,
        "thread_id": thread_id,
        "status": "approved",
        "approved_command_id": approved_command_id,
        "approved_task_id": approved_task_id,
        "approved_command": approved,
        "projections": projection_values(projections),
    }))
}

fn reject_approval(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    decide_without_queue(root, session, command, "rejected", "approval_rejected")
}

fn cancel_approval(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let approval_id = approval_id_from_command(command)?;
    let approval = load_record(root, "ctox_task_approval_requests", &approval_id)?
        .with_context(|| format!("approval request {approval_id} not found"))?;
    let actor = actor_id(session);
    let requester = value_string(&approval, "requester_user_id");
    anyhow::ensure!(
        actor == requester || is_admin_session(session),
        "only requester or admin can cancel this approval request"
    );
    decide_without_queue(root, session, command, "cancelled", "approval_cancelled")
}

fn decide_without_queue(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    status: &str,
    message_kind: &str,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let approval_id = approval_id_from_command(command)?;
    let approval = load_record(root, "ctox_task_approval_requests", &approval_id)?
        .with_context(|| format!("approval request {approval_id} not found"))?;
    ensure_pending_approval(&approval)?;
    if status == "rejected" {
        ensure_reviewer_or_admin(session, &approval)?;
    }
    let thread_id = value_string(&approval, "thread_id");
    let source = approval
        .get("source_context")
        .cloned()
        .unwrap_or_else(|| thread_source_context(&approval).unwrap_or_else(|| json!({})));
    let requester = value_string(&approval, "requester_user_id");
    let prompt = value_string(&approval, "prompt");
    let decision_note =
        first_string_field(&command.payload, &["decision_note", "note"]).unwrap_or_default();
    let message_body = if decision_note.is_empty() {
        format!("{status}: {prompt}")
    } else {
        format!("{status}: {decision_note}\n\n{prompt}")
    };
    let message_id = format!("msg_{}", Uuid::new_v4());
    let conn = store::open_store(root)?;
    let mut projections = Vec::new();
    let mut next = approval.clone();
    set_object_string(&mut next, "status", status);
    set_object_i64(&mut next, "decided_at_ms", now);
    set_object_string(&mut next, "decision_by_id", &actor_id(session));
    set_object_string(&mut next, "decision_note", &decision_note);
    store::upsert_business_record(
        &conn,
        "ctox_task_approval_requests",
        &approval_id,
        now,
        next,
    )?;
    record_approval_decision_event(
        &conn,
        session,
        command,
        &approval,
        status,
        "",
        "",
        &decision_note,
        now,
    )?;
    projections.push(ProjectionRef {
        collection: "ctox_task_approval_requests",
        record_id: approval_id.clone(),
    });
    upsert_thread_status_delta(
        root,
        &conn,
        &thread_id,
        "open",
        Some(&message_id),
        now,
        -1,
        &mut projections,
    )?;
    upsert_message(
        &conn,
        &thread_id,
        &message_id,
        message_kind,
        session,
        std::slice::from_ref(&requester),
        &message_body,
        &source,
        &approval_id,
        command.id.as_deref().unwrap_or_default(),
        now,
        &mut projections,
    )?;
    upsert_notifications(
        &conn,
        &thread_id,
        &message_id,
        &approval_id,
        message_kind,
        std::slice::from_ref(&requester),
        if status == "rejected" {
            "CTOX Freigabe abgelehnt"
        } else {
            "CTOX Freigabe storniert"
        },
        &message_body,
        &source,
        now,
        &mut projections,
    )?;
    Ok(json!({
        "ok": true,
        "approval_request_id": approval_id,
        "thread_id": thread_id,
        "status": status,
        "projections": projection_values(projections),
    }))
}

fn update_thread_watch(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    watch: bool,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let thread_id = first_string_field(&command.payload, &["thread_id"])
        .or_else(|| command.record_id.clone())
        .context("thread_id is required")?;
    let mut thread = load_record(root, "user_threads", &thread_id)?
        .with_context(|| format!("thread {thread_id} not found"))?;
    ensure_thread_participant_or_admin(session, &thread)?;
    let actor = actor_id(session);
    let mut watchers = array_strings(thread.get("watcher_user_ids"));
    if watch {
        if !watchers.iter().any(|id| id == &actor) {
            watchers.push(actor.clone());
        }
        let mut participants = array_strings(thread.get("participant_ids"));
        if !participants.iter().any(|id| id == &actor) {
            participants.push(actor);
        }
        participants.sort();
        participants.dedup();
        set_object_array_strings(&mut thread, "participant_ids", &participants);
    } else {
        watchers.retain(|id| id != &actor);
    }
    watchers.sort();
    watchers.dedup();
    set_object_array_strings(&mut thread, "watcher_user_ids", &watchers);
    set_object_i64(&mut thread, "updated_at_ms", now);
    let conn = store::open_store(root)?;
    store::upsert_business_record(&conn, "user_threads", &thread_id, now, thread)?;
    Ok(json!({
        "ok": true,
        "thread_id": thread_id,
        "watching": watch,
        "projections": [{ "collection": "user_threads", "record_id": thread_id }],
    }))
}

fn archive_thread(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let thread_id = first_string_field(&command.payload, &["thread_id"])
        .or_else(|| command.record_id.clone())
        .context("thread_id is required")?;
    let thread = load_record(root, "user_threads", &thread_id)?
        .with_context(|| format!("thread {thread_id} not found"))?;
    let actor = actor_id(session);
    let participants = array_strings(thread.get("participant_ids"));
    anyhow::ensure!(
        is_admin_session(session) || participants.iter().any(|id| id == &actor),
        "only participants or admins can archive this thread"
    );
    let conn = store::open_store(root)?;
    let mut next = thread.clone();
    set_object_string(&mut next, "status", "archived");
    set_object_i64(&mut next, "archived_at_ms", now);
    store::upsert_business_record(&conn, "user_threads", &thread_id, now, next)?;
    Ok(json!({
        "ok": true,
        "thread_id": thread_id,
        "status": "archived",
        "projections": [{ "collection": "user_threads", "record_id": thread_id }],
    }))
}

fn snooze_thread(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let thread_id = first_string_field(&command.payload, &["thread_id"])
        .or_else(|| command.record_id.clone())
        .context("thread_id is required")?;
    let until = command
        .payload
        .get("snoozed_until_ms")
        .or_else(|| command.payload.get("until_ms"))
        .and_then(Value::as_i64)
        .filter(|value| *value > now)
        .context("future snoozed_until_ms is required")?;
    let mut thread = load_record(root, "user_threads", &thread_id)?
        .with_context(|| format!("thread {thread_id} not found"))?;
    ensure_thread_participant_or_admin(session, &thread)?;
    set_object_string(&mut thread, "status", "snoozed");
    set_object_i64(&mut thread, "snoozed_until_ms", until);
    set_object_i64(&mut thread, "updated_at_ms", now);
    let conn = store::open_store(root)?;
    store::upsert_business_record(&conn, "user_threads", &thread_id, now, thread)?;
    Ok(json!({
        "ok": true,
        "thread_id": thread_id,
        "status": "snoozed",
        "snoozed_until_ms": until,
        "projections": [{ "collection": "user_threads", "record_id": thread_id }],
    }))
}

fn create_link(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let thread_id = first_string_field(&command.payload, &["thread_id"])
        .or_else(|| command.record_id.clone())
        .context("thread_id is required")?;
    let thread = load_record(root, "user_threads", &thread_id)?
        .with_context(|| format!("thread {thread_id} not found"))?;
    ensure_thread_participant_or_admin(session, &thread)?;
    let source = source_context(command);
    ensure_source_context_read_policy(root, session, &source)?;
    let module = source_string(&source, "module").unwrap_or_default();
    let record_id = source_string(&source, "record_id").unwrap_or_default();
    anyhow::ensure!(
        !module.is_empty() || !record_id.is_empty(),
        "link source module or record_id is required"
    );
    let link_id = first_string_field(&command.payload, &["link_id", "id"]).unwrap_or_else(|| {
        format!(
            "link_{}_{}_{}",
            slug_part(&thread_id),
            slug_part(&module),
            slug_part(&record_id)
        )
    });
    let record = json!({
        "id": link_id,
        "thread_id": thread_id,
        "source_module": module,
        "source_record_type": source_string(&source, "record_type").unwrap_or_default(),
        "source_record_id": record_id,
        "source_label": source_string(&source, "label").unwrap_or_default(),
        "link_role": first_string_field(&command.payload, &["link_role", "link_type"]).unwrap_or_else(|| "related".to_owned()),
        "command_id": first_string_field(&command.payload, &["command_id"]).unwrap_or_default(),
        "task_id": first_string_field(&command.payload, &["task_id"]).unwrap_or_default(),
        "case_id": first_string_field(&command.payload, &["case_id"]).unwrap_or_default(),
        "deep_link": first_string_field(&command.payload, &["deep_link"]).or_else(|| source_string(&source, "deep_link")).unwrap_or_default(),
        "context": source,
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    let conn = store::open_store(root)?;
    store::upsert_business_record(&conn, "user_thread_links", &link_id, now, record)?;
    Ok(json!({
        "ok": true,
        "thread_id": thread_id,
        "link_id": link_id,
        "projections": [{ "collection": "user_thread_links", "record_id": link_id }],
    }))
}

fn remove_link(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let link_id = first_string_field(&command.payload, &["link_id", "id"])
        .or_else(|| command.record_id.clone())
        .context("link_id is required")?;
    let mut link = load_record(root, "user_thread_links", &link_id)?
        .with_context(|| format!("thread link {link_id} not found"))?;
    let thread_id = value_string(&link, "thread_id");
    let thread = load_record(root, "user_threads", &thread_id)?
        .with_context(|| format!("thread {thread_id} not found"))?;
    ensure_thread_participant_or_admin(session, &thread)?;
    soft_delete_payload(&mut link, now);
    let conn = store::open_store(root)?;
    store::upsert_business_record(&conn, "user_thread_links", &link_id, now, link)?;
    Ok(json!({
        "ok": true,
        "thread_id": thread_id,
        "link_id": link_id,
        "projections": [{ "collection": "user_thread_links", "record_id": link_id }],
    }))
}

fn update_notification_status(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    status: &str,
) -> anyhow::Result<Value> {
    let now = now_ms();
    let notification_id = first_string_field(&command.payload, &["notification_id", "id"])
        .or_else(|| command.record_id.clone())
        .context("notification_id is required")?;
    let notification = load_record(root, "user_notifications", &notification_id)?
        .with_context(|| format!("notification {notification_id} not found"))?;
    let actor = actor_id(session);
    let notification_user = value_string(&notification, "user_id");
    anyhow::ensure!(
        is_admin_session(session) || notification_user == actor,
        "only notification owner or admin can update notification status"
    );
    let conn = store::open_store(root)?;
    let mut next = notification.clone();
    set_object_string(&mut next, "status", status);
    store::upsert_business_record(&conn, "user_notifications", &notification_id, now, next)?;
    Ok(json!({
        "ok": true,
        "notification_id": notification_id,
        "status": status,
        "projections": [{ "collection": "user_notifications", "record_id": notification_id }],
    }))
}

fn project_ctox_command_document(
    root: &Path,
    conn: &Connection,
    command: &Value,
    task: Option<&Value>,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let command_id = first_non_empty_owned([
        value_string(command, "command_id"),
        value_string(command, "id"),
    ]);
    if command_id.is_empty() || is_threads_internal_command(command) {
        return Ok(());
    }
    let Some(actor) = actor_from_ctox_documents(command, task) else {
        return Ok(());
    };
    let session = session_from_actor(actor.clone());
    let now = document_updated_at_ms(command).max(task.map(document_updated_at_ms).unwrap_or(0));
    let source = ctox_source_context(command, task, &command_id);
    let thread_id = ctox_thread_id(root, command, task, &source, &command_id);
    let mut participants = participant_set(
        root,
        &thread_id,
        [actor.id.as_str()],
        std::iter::empty::<&String>(),
    );
    if let Some(reviewer) = nested_string(command, &["payload", "approval", "reviewer_user_id"]) {
        participants.insert(reviewer);
    }
    let status = ctox_thread_status(command, task);
    let title = ctox_thread_title(command, task, &source, &command_id);
    let message_id = format!(
        "msg_ctox_{}_{}",
        slug_part(&command_id),
        slug_part(status.as_str())
    );
    upsert_thread(
        root,
        conn,
        &thread_id,
        &title,
        "ctox_task",
        &status,
        &participants,
        &source,
        &session,
        actor.id.as_str(),
        Some(&message_id),
        now,
        0,
        projections,
    )?;
    upsert_ctox_command_link(
        conn,
        &thread_id,
        &command_id,
        task,
        &source,
        now,
        projections,
    )?;
    if ctox_status_deserves_event(&status) {
        let body = ctox_status_body(command, task, &status);
        upsert_message(
            conn,
            &thread_id,
            &message_id,
            "ctox_status",
            &session,
            std::slice::from_ref(&actor.id),
            &body,
            &source,
            "",
            &command_id,
            now,
            projections,
        )?;
        upsert_status_notification(
            root,
            conn,
            &thread_id,
            &message_id,
            &command_id,
            &status,
            &actor.id,
            &title,
            &body,
            &source,
            now,
            projections,
        )?;
    }
    Ok(())
}

fn project_ctox_task_document(
    root: &Path,
    conn: &Connection,
    task: &Value,
    command: Option<&Value>,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let task_id = value_string(task, "id");
    if task_id.is_empty() {
        return Ok(());
    }
    if let Some(command) = command {
        return project_ctox_command_document(root, conn, command, Some(task), projections);
    }
    let empty_command = json!({});
    let Some(actor) = actor_from_ctox_documents(&empty_command, Some(task)) else {
        return Ok(());
    };
    let session = session_from_actor(actor.clone());
    let now = document_updated_at_ms(task);
    let source = ctox_source_context(&empty_command, Some(task), &task_id);
    let thread_id = ctox_thread_id(root, &empty_command, Some(task), &source, &task_id);
    let participants = participant_set(
        root,
        &thread_id,
        [actor.id.as_str()],
        std::iter::empty::<&String>(),
    );
    let status = ctox_thread_status(&empty_command, Some(task));
    let title = ctox_thread_title(&empty_command, Some(task), &source, &task_id);
    let message_id = format!("msg_ctox_{}_{}", slug_part(&task_id), slug_part(&status));
    upsert_thread(
        root,
        conn,
        &thread_id,
        &title,
        "ctox_task",
        &status,
        &participants,
        &source,
        &session,
        actor.id.as_str(),
        Some(&message_id),
        now,
        0,
        projections,
    )?;
    upsert_ctox_task_link(conn, &thread_id, task, &source, now, projections)?;
    if ctox_status_deserves_event(&status) {
        let body = ctox_status_body(&empty_command, Some(task), &status);
        upsert_message(
            conn,
            &thread_id,
            &message_id,
            "ctox_status",
            &session,
            std::slice::from_ref(&actor.id),
            &body,
            &source,
            "",
            "",
            now,
            projections,
        )?;
        upsert_status_notification(
            root,
            conn,
            &thread_id,
            &message_id,
            &task_id,
            &status,
            &actor.id,
            &title,
            &body,
            &source,
            now,
            projections,
        )?;
    }
    Ok(())
}

fn project_app_record_document(
    root: &Path,
    conn: &Connection,
    collection: &str,
    document: &Value,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let Some(spec) = app_relevance_spec(collection) else {
        return Ok(());
    };
    let record_id = value_string(document, "id");
    if record_id.is_empty() {
        return Ok(());
    }
    let user_ids = app_record_user_ids(document);
    if user_ids.is_empty() {
        return Ok(());
    }
    let actor = actor_from_app_record(document, &user_ids);
    let session = session_from_actor(actor.clone());
    let source = app_source_context(spec, document, &record_id);
    let thread_id = app_thread_id(spec, &source, &record_id);
    let participants = participant_set(root, &thread_id, [actor.id.as_str()], user_ids.iter());
    let assigned_user_id =
        app_assigned_user_id(document, &user_ids).unwrap_or_else(|| actor.id.clone());
    let status = app_thread_status(collection, document);
    let title = app_thread_title(spec, document, &source, &record_id);
    let event_message_id = app_status_deserves_event(&status).then(|| {
        format!(
            "msg_app_{}_{}_{}",
            slug_part(collection),
            slug_part(&record_id),
            slug_part(&status)
        )
    });
    let now = document_updated_at_ms(document);

    upsert_thread(
        root,
        conn,
        &thread_id,
        &title,
        spec.kind,
        &status,
        &participants,
        &source,
        &session,
        &assigned_user_id,
        event_message_id.as_deref(),
        now,
        0,
        projections,
    )?;
    upsert_app_record_link(
        conn,
        spec,
        &thread_id,
        document,
        &record_id,
        now,
        projections,
    )?;
    if let Some(message_id) = event_message_id {
        let body = app_status_body(collection, document, &status);
        let targets = app_notification_targets(document, &user_ids, &assigned_user_id, &status);
        upsert_message(
            conn,
            &thread_id,
            &message_id,
            "app_event",
            &session,
            &targets,
            &body,
            &source,
            "",
            &value_string(document, "command_id"),
            now,
            projections,
        )?;
        upsert_app_status_notifications(
            root,
            conn,
            &thread_id,
            &message_id,
            collection,
            &record_id,
            &status,
            &targets,
            &title,
            &body,
            &source,
            now,
            projections,
        )?;
    }
    Ok(())
}

fn app_relevance_spec(collection: &str) -> Option<AppRelevanceSpec> {
    APP_RELEVANCE_SPECS
        .iter()
        .copied()
        .find(|spec| spec.collection == collection)
}

fn app_source_context(spec: AppRelevanceSpec, document: &Value, record_id: &str) -> Value {
    let context_record_id = app_context_record_id(spec.collection, document, record_id);
    let context_record_type = app_context_record_type(spec.collection, spec.record_type);
    let label = app_thread_label(document, record_id);
    let mut source = document
        .get("source_context")
        .or_else(|| document.get("context"))
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    ensure_object_field(&mut source, "module", || spec.module.to_owned());
    ensure_object_field(&mut source, "record_type", || {
        context_record_type.to_owned()
    });
    ensure_object_field(&mut source, "record_id", || context_record_id.clone());
    ensure_object_field(&mut source, "label", || label.clone());
    ensure_object_field(&mut source, "deep_link", || {
        format!(
            "#{}?record={}&record_type={}",
            spec.module,
            slug_part(&context_record_id),
            slug_part(context_record_type)
        )
    });
    set_object_string(&mut source, "collection", spec.collection);
    source
}

fn app_context_record_type(collection: &str, fallback: &'static str) -> &'static str {
    match collection {
        "ctox_ticket_approvals"
        | "ctox_ticket_clarification_requests"
        | "ctox_ticket_self_work_items"
        | "ctox_ticket_self_work_notes" => "ticket_case",
        "support_conversations" | "support_agent_requests" | "support_notes" => "conversation",
        "outbound_approvals" => "engagement",
        "research_runs" => "research_task",
        _ => fallback,
    }
}

fn app_context_record_id(collection: &str, document: &Value, fallback_id: &str) -> String {
    let keys: &[&str] = match collection {
        "ctox_ticket_approvals"
        | "ctox_ticket_clarification_requests"
        | "ctox_ticket_self_work_items"
        | "ctox_ticket_self_work_notes" => &["case_id", "ticket_case_id", "ticket_id", "case_key"],
        "support_agent_requests" | "support_notes" => &["conversation_id"],
        "outbound_approvals" => &["engagement_id", "message_id"],
        "research_runs" => &["task_id"],
        _ => &[],
    };
    first_string_field(document, keys).unwrap_or_else(|| fallback_id.to_owned())
}

fn app_thread_id(spec: AppRelevanceSpec, source: &Value, fallback_id: &str) -> String {
    let module = source_string(source, "module").unwrap_or_else(|| spec.module.to_owned());
    let record_type =
        source_string(source, "record_type").unwrap_or_else(|| spec.record_type.to_owned());
    let record_id = source_string(source, "record_id").unwrap_or_else(|| fallback_id.to_owned());
    truncate(
        &format!(
            "thread_{}_{}_{}",
            slug_part(&module),
            slug_part(&record_type),
            slug_part(&record_id)
        ),
        220,
    )
}

fn app_thread_title(
    spec: AppRelevanceSpec,
    document: &Value,
    source: &Value,
    fallback_id: &str,
) -> String {
    let label = source_string(source, "label")
        .or_else(|| first_string_field(document, &["title", "subject", "name", "filename"]))
        .or_else(|| first_string_field(document, &["summary", "search_text", "body", "content"]))
        .map(|value| truncate(&value, 120))
        .unwrap_or_else(|| fallback_id.to_owned());
    match spec.module {
        "tickets" => format!("Ticket: {label}"),
        "support" => format!("Support: {label}"),
        "outbound" => format!("Outbound: {label}"),
        "research" => format!("Research: {label}"),
        "documents" => format!("Dokument: {label}"),
        "notes" => format!("Notiz: {label}"),
        _ => label,
    }
}

fn app_thread_label(document: &Value, fallback_id: &str) -> String {
    first_string_field(document, &["title", "subject", "name", "filename"])
        .or_else(|| first_string_field(document, &["summary", "search_text", "body", "content"]))
        .map(|value| truncate(&value, 120))
        .unwrap_or_else(|| fallback_id.to_owned())
}

fn app_thread_status(collection: &str, document: &Value) -> String {
    let status = first_string_field(
        document,
        &[
            "status",
            "approval_status",
            "decision",
            "task_status",
            "route_status",
            "draft_status",
            "send_status",
        ],
    )
    .unwrap_or_else(|| {
        if collection.contains("approval") {
            "pending".to_owned()
        } else {
            "open".to_owned()
        }
    })
    .to_ascii_lowercase();
    match status.as_str() {
        "pending" | "pending_review" | "review" | "needs_review" | "requested" => {
            "needs_review".to_owned()
        }
        "waiting" | "waiting_on_user" | "snoozed" => "waiting".to_owned(),
        "queued" | "accepted" | "running" | "in_progress" | "collecting" | "processing" => {
            "running".to_owned()
        }
        "blocked" | "failed" | "error" => "blocked".to_owned(),
        "completed" | "done" | "resolved" | "approved" | "sent" | "final" => "completed".to_owned(),
        "rejected" | "cancelled" | "canceled" | "closed" => "archived".to_owned(),
        "draft" | "imported" | "ready" | "open" | "new" => "open".to_owned(),
        other if !other.is_empty() => other.to_owned(),
        _ => "open".to_owned(),
    }
}

fn app_status_deserves_event(status: &str) -> bool {
    matches!(
        status,
        "needs_review" | "waiting" | "blocked" | "completed" | "archived"
    )
}

fn app_status_body(collection: &str, document: &Value, status: &str) -> String {
    let prefix = match status {
        "needs_review" => "App-Record wartet auf Freigabe.",
        "waiting" => "App-Record wartet auf Rueckmeldung.",
        "blocked" => "App-Record ist blockiert oder fehlgeschlagen.",
        "completed" => "App-Record wurde abgeschlossen.",
        "archived" => "App-Record wurde beendet.",
        _ => "App-Record wurde aktualisiert.",
    };
    let detail = first_string_field(
        document,
        &[
            "summary",
            "comment",
            "body",
            "content",
            "prompt",
            "error",
            "paused_reason",
            "closed_reason",
            "search_text",
        ],
    )
    .unwrap_or_default();
    let record_id = value_string(document, "id");
    [
        prefix.to_owned(),
        format!("{collection}/{record_id}"),
        detail,
    ]
    .into_iter()
    .map(|part| part.trim().to_owned())
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n")
}

fn app_record_user_ids(document: &Value) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for key in [
        "user_id",
        "actor_user_id",
        "actor_id",
        "author_id",
        "owner_user_id",
        "owner_id",
        "account_owner_id",
        "contact_owner_id",
        "assignee_id",
        "assigned_user_id",
        "assignee_user_id",
        "assigned_by_id",
        "requester_user_id",
        "reviewer_user_id",
        "decision_by_id",
        "created_by_id",
        "created_by_user_id",
        "updated_by_id",
        "recipient_user_id",
    ] {
        if let Some(id) = non_empty_string(document, key) {
            ids.insert(id);
        }
    }
    for path in [
        &["actor", "id"][..],
        &["author", "id"][..],
        &["owner", "id"][..],
        &["assignee", "id"][..],
        &["requester", "id"][..],
        &["reviewer", "id"][..],
        &["payload", "actor", "id"][..],
        &["payload", "user_id"][..],
        &["payload", "owner_user_id"][..],
        &["payload", "assignee_id"][..],
        &["payload", "reviewer_user_id"][..],
    ] {
        if let Some(id) = nested_string(document, path) {
            ids.insert(id);
        }
    }
    for key in [
        "participant_ids",
        "participant_user_ids",
        "target_user_ids",
        "mention_user_ids",
        "mentions_user_ids",
        "watcher_user_ids",
        "assignee_user_ids",
        "assigned_user_ids",
        "reviewer_user_ids",
    ] {
        ids.extend(array_strings(document.get(key)));
    }
    ids.into_iter().collect()
}

fn actor_from_app_record(document: &Value, user_ids: &[String]) -> ProjectionActor {
    let id = app_assigned_user_id(document, user_ids)
        .or_else(|| user_ids.first().cloned())
        .unwrap_or_else(|| "business-os".to_owned());
    let display_name = first_string_field(
        document,
        &[
            "actor_display_name",
            "author_display_name",
            "owner_display_name",
            "requester_display_name",
            "reviewer_display_name",
        ],
    )
    .or_else(|| nested_string(document, &["actor", "display_name"]))
    .or_else(|| nested_string(document, &["actor", "name"]))
    .unwrap_or_else(|| id.clone());
    ProjectionActor {
        id,
        display_name,
        role: "user".to_owned(),
    }
}

fn app_assigned_user_id(document: &Value, user_ids: &[String]) -> Option<String> {
    let preferred = first_string_field(
        document,
        &[
            "reviewer_user_id",
            "assignee_id",
            "assigned_user_id",
            "assignee_user_id",
            "owner_user_id",
            "owner_id",
            "actor_user_id",
            "author_id",
            "requester_user_id",
            "user_id",
        ],
    )?;
    if user_ids.iter().any(|id| id == &preferred) {
        Some(preferred)
    } else {
        None
    }
}

fn app_notification_targets(
    document: &Value,
    user_ids: &[String],
    assigned_user_id: &str,
    status: &str,
) -> Vec<String> {
    let mut targets = BTreeSet::new();
    if matches!(status, "needs_review" | "waiting") {
        if let Some(reviewer) = first_string_field(
            document,
            &[
                "reviewer_user_id",
                "assignee_id",
                "assigned_user_id",
                "assignee_user_id",
                "recipient_user_id",
            ],
        ) {
            targets.insert(reviewer);
        } else if !assigned_user_id.trim().is_empty() {
            targets.insert(assigned_user_id.to_owned());
        }
    } else {
        targets.extend(user_ids.iter().cloned());
    }
    targets.into_iter().collect()
}

fn upsert_app_record_link(
    conn: &Connection,
    spec: AppRelevanceSpec,
    thread_id: &str,
    document: &Value,
    record_id: &str,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let link_id = format!(
        "link_{}_{}_{}",
        slug_part(thread_id),
        slug_part(spec.collection),
        slug_part(record_id)
    );
    let record = json!({
        "id": link_id,
        "thread_id": thread_id,
        "source_module": spec.module,
        "source_record_type": spec.record_type,
        "source_record_id": record_id,
        "source_label": app_thread_label(document, record_id),
        "link_role": "app_record",
        "link_type": spec.link_type,
        "app_collection": spec.collection,
        "command_id": value_string(document, "command_id"),
        "task_id": first_non_empty_owned([
            value_string(document, "task_id"),
            value_string(document, "task_queue_id"),
        ]),
        "deep_link": format!(
            "#{}?record={}&record_type={}",
            spec.module,
            slug_part(record_id),
            slug_part(spec.record_type)
        ),
        "context": {
            "collection": spec.collection,
            "record_id": record_id,
            "status": first_string_field(document, &["status", "approval_status", "decision"]).unwrap_or_default(),
        },
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    store::upsert_business_record(conn, "user_thread_links", &link_id, now, record)?;
    projections.push(ProjectionRef {
        collection: "user_thread_links",
        record_id: link_id,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn upsert_app_status_notifications(
    root: &Path,
    conn: &Connection,
    thread_id: &str,
    message_id: &str,
    collection: &str,
    record_id: &str,
    status: &str,
    user_ids: &[String],
    title: &str,
    body: &str,
    source: &Value,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let notification_type = match status {
        "needs_review" => "approval_requested",
        "waiting" => "waiting_on_user",
        "blocked" => "app_blocked",
        "completed" => "app_completed",
        "archived" => "watch_update",
        _ => "watch_update",
    };
    for user_id in user_ids
        .iter()
        .map(String::as_str)
        .filter(|id| !id.trim().is_empty())
    {
        let notification_id = format!(
            "notif_app_{}_{}_{}_{}",
            slug_part(status),
            slug_part(collection),
            slug_part(record_id),
            slug_part(user_id)
        );
        let existing_status = load_record(root, "user_notifications", &notification_id)?
            .map(|record| value_string(&record, "status"))
            .filter(|status| !status.is_empty())
            .unwrap_or_else(|| "unread".to_owned());
        let record = json!({
            "id": notification_id,
            "notification_id": notification_id,
            "user_id": user_id,
            "thread_id": thread_id,
            "message_id": message_id,
            "approval_request_id": "",
            "notification_type": notification_type,
            "status": existing_status,
            "title": title,
            "body_preview": truncate(body, 180),
            "source_module": source_string(source, "module").unwrap_or_default(),
            "source_record_id": source_string(source, "record_id").unwrap_or_default(),
            "created_at_ms": now,
            "updated_at_ms": now,
        });
        store::upsert_business_record(conn, "user_notifications", &notification_id, now, record)?;
        projections.push(ProjectionRef {
            collection: "user_notifications",
            record_id: notification_id,
        });
    }
    Ok(())
}

fn enqueue_approved_ctox_command(
    root: &Path,
    session: &BusinessOsSession,
    approval: &Value,
    decision_command: &BusinessCommand,
) -> anyhow::Result<store::CommandAccepted> {
    let source = approval
        .get("source_context")
        .cloned()
        .unwrap_or_else(|| thread_source_context(approval).unwrap_or_else(|| json!({})));
    let prompt = value_string(approval, "prompt");
    let instruction = value_string(approval, "instruction");
    let target_module = value_string(approval, "target_module");
    let source_module = source_string(&source, "module").unwrap_or_else(|| "ctox".to_owned());
    let module = if target_module.is_empty() {
        source_module
    } else {
        target_module
    };
    let record_id = value_string(approval, "target_record_id");
    let command_type = non_empty_string(approval, "target_command_type")
        .unwrap_or_else(|| "business_os.chat.task".to_owned());
    let approval_id = value_string(approval, "approval_request_id");
    let thread_id = value_string(approval, "thread_id");
    let approved_command_id = format!("cmd_{}", Uuid::new_v4());
    let mut payload = approval
        .get("target_payload")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    payload
        .entry("prompt".to_owned())
        .or_insert_with(|| Value::String(prompt.clone()));
    payload.entry("instruction".to_owned()).or_insert_with(|| {
        Value::String(if instruction.is_empty() {
            prompt.clone()
        } else {
            instruction
        })
    });
    payload
        .entry("user_message".to_owned())
        .or_insert_with(|| Value::String(prompt.clone()));
    payload
        .entry("title".to_owned())
        .or_insert_with(|| Value::String(approval_title(approval, &prompt)));
    payload.entry("thread_key".to_owned()).or_insert_with(|| {
        Value::String(if thread_id.is_empty() {
            format!("business-os/{module}")
        } else {
            format!("business-os/threads/{thread_id}")
        })
    });
    payload
        .entry("context".to_owned())
        .or_insert_with(|| source.clone());
    payload.insert(
        "approval".to_owned(),
        json!({
            "approval_request_id": approval_id.clone(),
            "approved_by": actor_payload(session),
            "decision_command_id": decision_command.id.clone(),
            "requester_user_id": approval.get("requester_user_id").cloned().unwrap_or(Value::Null),
            "reviewer_user_id": approval.get("reviewer_user_id").cloned().unwrap_or(Value::Null),
        }),
    );

    let mut client_context = approval
        .get("client_context")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    client_context.insert("actor".to_owned(), actor_payload(session));
    client_context.insert(
        "action".to_owned(),
        Value::String("thread-approval-approved".to_owned()),
    );
    client_context.insert("approval_request_id".to_owned(), Value::String(approval_id));
    client_context.insert("source_module".to_owned(), Value::String(module.clone()));
    client_context.insert("module".to_owned(), Value::String(module.clone()));
    client_context.insert("module_id".to_owned(), Value::String(module.clone()));
    client_context.insert("app_id".to_owned(), Value::String(module.clone()));

    let accepted = store::record_command(
        root,
        BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some(approved_command_id),
            module,
            command_type,
            record_id: if record_id.is_empty() {
                None
            } else {
                Some(record_id)
            },
            payload: Value::Object(payload),
            client_context: Value::Object(client_context),
        },
    )?;
    anyhow::ensure!(
        accepted.ok,
        "approved CTOX command was rejected before queue creation"
    );
    Ok(accepted)
}

fn approval_record(
    approval_id: &str,
    thread_id: &str,
    status: &str,
    session: &BusinessOsSession,
    reviewer_user_id: &str,
    reviewer_display_name: &str,
    decision_note: &str,
    command: &BusinessCommand,
    source: &Value,
    prompt: &str,
    now: i64,
    decided_at_ms: Option<i64>,
) -> Value {
    let target_module = first_string_field(&command.payload, &["target_module"])
        .or_else(|| source_string(source, "module"))
        .unwrap_or_else(|| "ctox".to_owned());
    let target_record_id = first_string_field(&command.payload, &["target_record_id"])
        .or_else(|| source_string(source, "record_id"))
        .or_else(|| command.record_id.clone())
        .unwrap_or_default();
    json!({
        "id": approval_id,
        "approval_request_id": approval_id,
        "thread_id": thread_id,
        "status": status,
        "requester_user_id": actor_id(session),
        "requester_display_name": actor_display_name(session),
        "reviewer_user_id": reviewer_user_id,
        "reviewer_display_name": reviewer_display_name,
        "prompt": prompt,
        "instruction": first_string_field(&command.payload, &["instruction"]).unwrap_or_else(|| prompt.to_owned()),
        "target_command_type": first_string_field(&command.payload, &["target_command_type", "command_type"]).unwrap_or_else(|| "business_os.chat.task".to_owned()),
        "target_module": target_module,
        "target_record_id": target_record_id,
        "source_module": source_string(source, "module").unwrap_or_default(),
        "source_record_type": source_string(source, "record_type").unwrap_or_default(),
        "source_record_id": source_string(source, "record_id").unwrap_or_default(),
        "source_label": source_string(source, "label").unwrap_or_default(),
        "source_deep_link": source_string(source, "deep_link").unwrap_or_default(),
        "requested_at_ms": now,
        "decided_at_ms": decided_at_ms.unwrap_or(0),
        "decision_by_id": "",
        "decision_note": decision_note,
        "approved_command_id": "",
        "approved_task_id": "",
        "target_payload": command.payload.get("target_payload").cloned().unwrap_or_else(|| json!({})),
        "source_context": source,
        "client_context": command.client_context.clone(),
        "created_at_ms": now,
        "updated_at_ms": now,
    })
}

fn record_approval_decision_event(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    approval: &Value,
    decision: &str,
    approved_command_id: &str,
    approved_task_id: &str,
    decision_note: &str,
    observed_at_ms: i64,
) -> anyhow::Result<()> {
    let approval_id = value_string(approval, "approval_request_id");
    anyhow::ensure!(!approval_id.is_empty(), "approval_request_id is required");
    let source_context = approval
        .get("source_context")
        .cloned()
        .unwrap_or_else(|| thread_source_context(approval).unwrap_or_else(|| json!({})));
    store::insert_business_event(
        conn,
        "ctox_task_approval_requests",
        &approval_id,
        "business_os.external_approval.decided",
        json!({
            "event_type": "business_os.external_approval.decided",
            "approval_request_id": approval_id,
            "thread_id": value_string(approval, "thread_id"),
            "decision": decision,
            "decision_note": decision_note,
            "decision_command_id": command.id.as_deref(),
            "decision_command_type": command.command_type.as_str(),
            "requester_user_id": value_string(approval, "requester_user_id"),
            "reviewer_user_id": value_string(approval, "reviewer_user_id"),
            "decision_by": actor_payload(session),
            "source_module": value_string(approval, "source_module"),
            "source_record_type": value_string(approval, "source_record_type"),
            "source_record_id": value_string(approval, "source_record_id"),
            "source_label": value_string(approval, "source_label"),
            "source_deep_link": value_string(approval, "source_deep_link"),
            "source_context": source_context,
            "target_command_type": value_string(approval, "target_command_type"),
            "target_module": value_string(approval, "target_module"),
            "target_record_id": value_string(approval, "target_record_id"),
            "prompt": value_string(approval, "prompt"),
            "approved_command_id": approved_command_id,
            "approved_task_id": approved_task_id,
            "observed_at_ms": observed_at_ms,
        }),
        observed_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
fn upsert_thread(
    root: &Path,
    conn: &Connection,
    thread_id: &str,
    title: &str,
    kind: &str,
    status: &str,
    participants: &BTreeSet<String>,
    source: &Value,
    session: &BusinessOsSession,
    assigned_user_id: &str,
    last_message_id: Option<&str>,
    now: i64,
    pending_delta: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let existing = load_record(root, "user_threads", thread_id)?.unwrap_or_else(|| json!({}));
    let created_at_ms = existing
        .get("created_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(now);
    let pending = existing
        .get("pending_approval_count")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .saturating_add(pending_delta)
        .max(0);
    let actor_id = actor_id(session);
    let owner = existing
        .get("owner_user_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(actor_id.as_str())
        .to_owned();
    let created_by = existing
        .get("created_by_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(actor_id.as_str())
        .to_owned();
    let mut merged = array_strings(existing.get("participant_ids"))
        .into_iter()
        .collect::<BTreeSet<_>>();
    merged.extend(participants.iter().cloned());
    let record = json!({
        "id": thread_id,
        "thread_id": thread_id,
        "title": title,
        "kind": kind,
        "status": status,
        "participant_ids": merged.into_iter().collect::<Vec<_>>(),
        "watcher_user_ids": array_strings(existing.get("watcher_user_ids")),
        "owner_user_id": owner,
        "created_by_id": created_by,
        "assigned_user_id": assigned_user_id,
        "source_module": source_string(source, "module").unwrap_or_default(),
        "source_record_type": source_string(source, "record_type").unwrap_or_default(),
        "source_record_id": source_string(source, "record_id").unwrap_or_default(),
        "source_label": source_string(source, "label").unwrap_or_default(),
        "source_deep_link": source_string(source, "deep_link").unwrap_or_default(),
        "last_message_id": last_message_id.unwrap_or_else(|| existing.get("last_message_id").and_then(Value::as_str).unwrap_or_default()),
        "last_message_at_ms": now,
        "pending_approval_count": pending,
        "snoozed_until_ms": existing.get("snoozed_until_ms").and_then(Value::as_i64).unwrap_or(0),
        "archived_at_ms": existing.get("archived_at_ms").and_then(Value::as_i64).unwrap_or(0),
        "last_seen_by_user": existing.get("last_seen_by_user").cloned().unwrap_or_else(|| json!({})),
        "metadata": {
            "source_context": source,
        },
        "created_at_ms": created_at_ms,
        "updated_at_ms": now,
    });
    store::upsert_business_record(conn, "user_threads", thread_id, now, record)?;
    projections.push(ProjectionRef {
        collection: "user_threads",
        record_id: thread_id.to_owned(),
    });
    Ok(())
}

fn upsert_thread_status_delta(
    root: &Path,
    conn: &Connection,
    thread_id: &str,
    status: &str,
    last_message_id: Option<&str>,
    now: i64,
    pending_delta: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let mut existing = load_record(root, "user_threads", thread_id)?
        .with_context(|| format!("thread {thread_id} not found"))?;
    set_object_string(&mut existing, "status", status);
    if let Some(message_id) = last_message_id {
        set_object_string(&mut existing, "last_message_id", message_id);
        set_object_i64(&mut existing, "last_message_at_ms", now);
    }
    let pending = existing
        .get("pending_approval_count")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .saturating_add(pending_delta)
        .max(0);
    set_object_i64(&mut existing, "pending_approval_count", pending);
    store::upsert_business_record(conn, "user_threads", thread_id, now, existing)?;
    projections.push(ProjectionRef {
        collection: "user_threads",
        record_id: thread_id.to_owned(),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn upsert_message(
    conn: &Connection,
    thread_id: &str,
    message_id: &str,
    kind: &str,
    session: &BusinessOsSession,
    target_user_ids: &[String],
    body: &str,
    source: &Value,
    approval_request_id: &str,
    command_id: &str,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let record = json!({
        "id": message_id,
        "message_id": message_id,
        "thread_id": thread_id,
        "kind": kind,
        "author_user_id": actor_id(session),
        "author_display_name": actor_display_name(session),
        "target_user_ids": target_user_ids,
        "body": body,
        "source_module": source_string(source, "module").unwrap_or_default(),
        "source_record_type": source_string(source, "record_type").unwrap_or_default(),
        "source_record_id": source_string(source, "record_id").unwrap_or_default(),
        "source_label": source_string(source, "label").unwrap_or_default(),
        "source_deep_link": source_string(source, "deep_link").unwrap_or_default(),
        "approval_request_id": approval_request_id,
        "command_id": command_id,
        "metadata": {
            "source_context": source,
        },
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    store::upsert_business_record(conn, "user_thread_messages", message_id, now, record)?;
    projections.push(ProjectionRef {
        collection: "user_thread_messages",
        record_id: message_id.to_owned(),
    });
    Ok(())
}

fn upsert_source_link(
    conn: &Connection,
    thread_id: &str,
    source: &Value,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let module = source_string(source, "module").unwrap_or_default();
    let record_id = source_string(source, "record_id").unwrap_or_default();
    if module.is_empty() && record_id.is_empty() {
        return Ok(());
    }
    let link_id = format!(
        "link_{}_{}_{}",
        slug_part(thread_id),
        slug_part(&module),
        slug_part(&record_id)
    );
    let record = json!({
        "id": link_id,
        "thread_id": thread_id,
        "source_module": module,
        "source_record_type": source_string(source, "record_type").unwrap_or_default(),
        "source_record_id": record_id,
        "source_label": source_string(source, "label").unwrap_or_default(),
        "link_role": "source",
        "deep_link": source_string(source, "deep_link").unwrap_or_default(),
        "context": source,
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    store::upsert_business_record(conn, "user_thread_links", &link_id, now, record)?;
    projections.push(ProjectionRef {
        collection: "user_thread_links",
        record_id: link_id,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn upsert_notifications(
    conn: &Connection,
    thread_id: &str,
    message_id: &str,
    approval_request_id: &str,
    notification_type: &str,
    user_ids: &[String],
    title: &str,
    body: &str,
    source: &Value,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    for user_id in user_ids
        .iter()
        .map(String::as_str)
        .filter(|id| !id.is_empty())
    {
        let notification_id = format!("notif_{}_{}", slug_part(message_id), slug_part(user_id));
        let record = json!({
            "id": notification_id,
            "notification_id": notification_id,
            "user_id": user_id,
            "thread_id": thread_id,
            "message_id": message_id,
            "approval_request_id": approval_request_id,
            "notification_type": notification_type,
            "status": "unread",
            "title": title,
            "body_preview": truncate(body, 180),
            "source_module": source_string(source, "module").unwrap_or_default(),
            "source_record_id": source_string(source, "record_id").unwrap_or_default(),
            "created_at_ms": now,
            "updated_at_ms": now,
        });
        store::upsert_business_record(conn, "user_notifications", &notification_id, now, record)?;
        projections.push(ProjectionRef {
            collection: "user_notifications",
            record_id: notification_id,
        });
    }
    Ok(())
}

fn load_record(root: &Path, collection: &str, record_id: &str) -> anyhow::Result<Option<Value>> {
    if record_id.trim().is_empty() {
        return Ok(None);
    }
    store::pull_collection_record(root, collection, record_id)
}

fn pull_projection_documents(
    root: &Path,
    collection: &str,
    since_ms: i64,
    limit: usize,
) -> anyhow::Result<Vec<Value>> {
    let pulled = store::pull_collection_records_for_projection(
        root,
        collection,
        Some(since_ms),
        Some(limit),
    )?;
    Ok(pulled
        .get("documents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn project_existing_approval_thread(root: &Path, command: &Value) -> Option<String> {
    nested_string(command, &["payload", "approval", "approval_request_id"])
        .and_then(|approval_id| {
            load_record(root, "ctox_task_approval_requests", &approval_id)
                .ok()
                .flatten()
        })
        .map(|approval| value_string(&approval, "thread_id"))
        .filter(|thread_id| !thread_id.is_empty())
}

fn ctox_thread_id(
    root: &Path,
    command: &Value,
    task: Option<&Value>,
    source: &Value,
    fallback_id: &str,
) -> String {
    if let Some(thread_id) = project_existing_approval_thread(root, command) {
        return thread_id;
    }
    for key in [
        nested_string(command, &["payload", "thread_key"]),
        nested_string(command, &["client_context", "thread_key"]),
        task.and_then(|task| non_empty_string(task, "thread_key")),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(thread_id) = key.strip_prefix("business-os/threads/") {
            if !thread_id.trim().is_empty() {
                return thread_id.trim().to_owned();
            }
        }
    }
    let module = source_string(source, "module").unwrap_or_else(|| "ctox".to_owned());
    let record_type =
        source_string(source, "record_type").unwrap_or_else(|| "ctox_task".to_owned());
    let record_id = source_string(source, "record_id").unwrap_or_else(|| fallback_id.to_owned());
    truncate(
        &format!(
            "thread_{}_{}_{}",
            slug_part(&module),
            slug_part(&record_type),
            slug_part(&record_id)
        ),
        220,
    )
}

fn ctox_source_context(command: &Value, task: Option<&Value>, fallback_id: &str) -> Value {
    let payload_context = command
        .get("payload")
        .and_then(|payload| {
            payload
                .get("source_context")
                .or_else(|| payload.get("context"))
        })
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let client_context = command
        .get("client_context")
        .filter(|value| value.is_object());
    let selection = client_context
        .and_then(|context| context.get("scope"))
        .and_then(|scope| scope.get("selection"));
    let module = source_string(&payload_context, "module")
        .or_else(|| nested_string(command, &["payload", "source_module"]))
        .or_else(|| {
            client_context.and_then(|context| {
                first_string_field(context, &["source_module", "module", "module_id", "app_id"])
            })
        })
        .or_else(|| non_empty_string(command, "module"))
        .or_else(|| task.and_then(|task| non_empty_string(task, "source_module")))
        .unwrap_or_else(|| "ctox".to_owned());
    let command_has_identity = !first_non_empty_owned([
        value_string(command, "command_id"),
        value_string(command, "id"),
    ])
    .is_empty();
    let record_type = source_string(&payload_context, "record_type")
        .or_else(|| selection.and_then(|selection| non_empty_string(selection, "record_type")))
        .or_else(|| client_context.and_then(|context| non_empty_string(context, "record_type")))
        .unwrap_or_else(|| {
            if command_has_identity {
                "command"
            } else if task.is_some() {
                "queue_task"
            } else {
                "command"
            }
            .to_owned()
        });
    let record_id = source_string(&payload_context, "record_id")
        .or_else(|| selection.and_then(|selection| non_empty_string(selection, "record_id")))
        .or_else(|| client_context.and_then(|context| non_empty_string(context, "record_id")))
        .or_else(|| non_empty_string(command, "record_id"))
        .or_else(|| task.and_then(|task| non_empty_string(task, "id")))
        .unwrap_or_else(|| fallback_id.to_owned());
    let label = source_string(&payload_context, "label")
        .or_else(|| selection.and_then(|selection| non_empty_string(selection, "label")))
        .or_else(|| client_context.and_then(|context| non_empty_string(context, "label")))
        .or_else(|| nested_string(command, &["payload", "title"]))
        .or_else(|| task.and_then(|task| non_empty_string(task, "title")))
        .or_else(|| {
            nested_string(command, &["payload", "prompt"]).map(|prompt| truncate(&prompt, 120))
        })
        .unwrap_or_else(|| fallback_id.to_owned());
    let deep_link = source_string(&payload_context, "deep_link")
        .or_else(|| client_context.and_then(|context| non_empty_string(context, "deep_link")))
        .unwrap_or_else(|| {
            format!(
                "#{module}?record={}&record_type={}",
                slug_part(&record_id),
                slug_part(&record_type)
            )
        });
    json!({
        "module": module,
        "record_type": record_type,
        "record_id": record_id,
        "label": label,
        "deep_link": deep_link,
    })
}

fn ctox_thread_title(
    command: &Value,
    task: Option<&Value>,
    source: &Value,
    fallback_id: &str,
) -> String {
    task.and_then(|task| non_empty_string(task, "title"))
        .or_else(|| nested_string(command, &["payload", "title"]))
        .or_else(|| source_string(source, "label"))
        .or_else(|| {
            nested_string(command, &["payload", "prompt"]).map(|prompt| truncate(&prompt, 120))
        })
        .unwrap_or_else(|| format!("CTOX Arbeit {fallback_id}"))
}

fn ctox_thread_status(command: &Value, task: Option<&Value>) -> String {
    let status = task
        .and_then(|task| first_string_field(task, &["status", "task_status", "route_status"]))
        .or_else(|| first_string_field(command, &["status", "task_status"]))
        .unwrap_or_else(|| "open".to_owned())
        .to_ascii_lowercase();
    match status.as_str() {
        "queued" | "pending" | "pending_sync" | "accepted" | "leased" | "running"
        | "in_progress" => "running".to_owned(),
        "failed" | "blocked" | "error" => "blocked".to_owned(),
        "completed" | "handled" | "done" | "success" => "completed".to_owned(),
        "cancelled" | "canceled" => "archived".to_owned(),
        other if !other.is_empty() => other.to_owned(),
        _ => "open".to_owned(),
    }
}

fn ctox_status_deserves_event(status: &str) -> bool {
    matches!(status, "completed" | "blocked" | "failed" | "archived")
}

fn ctox_status_body(command: &Value, task: Option<&Value>, status: &str) -> String {
    let note = task
        .and_then(|task| first_string_field(task, &["status_note", "error"]))
        .or_else(|| first_string_field(command, &["status_note", "error"]))
        .unwrap_or_default();
    let prompt = task
        .and_then(|task| non_empty_string(task, "prompt"))
        .or_else(|| nested_string(command, &["payload", "prompt"]))
        .unwrap_or_default();
    let prefix = match status {
        "completed" => "CTOX Arbeit abgeschlossen.",
        "blocked" | "failed" => "CTOX Arbeit blockiert oder fehlgeschlagen.",
        "archived" => "CTOX Arbeit wurde beendet.",
        _ => "CTOX Status aktualisiert.",
    };
    [prefix.to_owned(), note, prompt]
        .into_iter()
        .map(|part| part.trim().to_owned())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn is_threads_internal_command(command: &Value) -> bool {
    value_string(command, "module") == "threads"
        || value_string(command, "command_type").starts_with("threads.")
}

fn actor_from_ctox_documents(command: &Value, task: Option<&Value>) -> Option<ProjectionActor> {
    let actor = command
        .get("client_context")
        .and_then(|context| context.get("actor").or_else(|| context.get("user")));
    let id = actor
        .and_then(|actor| non_empty_string(actor, "id"))
        .or_else(|| nested_string(command, &["client_context", "user_id"]))
        .or_else(|| nested_string(command, &["payload", "approval", "requester_user_id"]))
        .or_else(|| task.and_then(|task| non_empty_string(task, "owner_user_id")))
        .or_else(|| task.and_then(|task| non_empty_string(task, "lease_owner")))?;
    let display_name = actor
        .and_then(|actor| first_string_field(actor, &["display_name", "name"]))
        .unwrap_or_else(|| id.clone());
    let role = actor
        .and_then(|actor| non_empty_string(actor, "role"))
        .or_else(|| nested_string(command, &["client_context", "role"]))
        .unwrap_or_else(|| "user".to_owned());
    Some(ProjectionActor {
        id,
        display_name,
        role,
    })
}

fn session_from_actor(actor: ProjectionActor) -> BusinessOsSession {
    let role = actor.role.trim().to_owned();
    let is_admin = matches!(role.as_str(), "chef" | "admin");
    BusinessOsSession {
        ok: true,
        authenticated: true,
        auth_required: false,
        user: Some(BusinessOsSessionUser {
            id: actor.id,
            display_name: actor.display_name,
            role,
            is_admin,
        }),
        login_url: None,
        reason: None,
    }
}

fn upsert_ctox_command_link(
    conn: &Connection,
    thread_id: &str,
    command_id: &str,
    task: Option<&Value>,
    source: &Value,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let link_id = format!(
        "link_{}_business_commands_{}",
        slug_part(thread_id),
        slug_part(command_id)
    );
    let task_id = task
        .map(|task| value_string(task, "id"))
        .unwrap_or_default();
    let record = json!({
        "id": link_id,
        "thread_id": thread_id,
        "source_module": source_string(source, "module").unwrap_or_else(|| "ctox".to_owned()),
        "source_record_type": source_string(source, "record_type").unwrap_or_else(|| "command".to_owned()),
        "source_record_id": source_string(source, "record_id").unwrap_or_else(|| command_id.to_owned()),
        "source_label": source_string(source, "label").unwrap_or_else(|| command_id.to_owned()),
        "link_role": "ctox_command",
        "link_type": "business_command",
        "command_id": command_id,
        "task_id": task_id,
        "deep_link": source_string(source, "deep_link").unwrap_or_default(),
        "context": source,
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    store::upsert_business_record(conn, "user_thread_links", &link_id, now, record)?;
    projections.push(ProjectionRef {
        collection: "user_thread_links",
        record_id: link_id,
    });
    if let Some(task) = task {
        upsert_ctox_task_link(conn, thread_id, task, source, now, projections)?;
    }
    Ok(())
}

fn upsert_ctox_task_link(
    conn: &Connection,
    thread_id: &str,
    task: &Value,
    source: &Value,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let task_id = value_string(task, "id");
    if task_id.is_empty() {
        return Ok(());
    }
    let link_id = format!(
        "link_{}_ctox_queue_tasks_{}",
        slug_part(thread_id),
        slug_part(&task_id)
    );
    let record = json!({
        "id": link_id,
        "thread_id": thread_id,
        "source_module": "ctox",
        "source_record_type": "queue_task",
        "source_record_id": task_id,
        "source_label": non_empty_string(task, "title").or_else(|| source_string(source, "label")).unwrap_or_default(),
        "link_role": "ctox_task",
        "link_type": "queue_task",
        "command_id": value_string(task, "command_id"),
        "task_id": value_string(task, "id"),
        "deep_link": source_string(source, "deep_link").unwrap_or_default(),
        "context": source,
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    store::upsert_business_record(conn, "user_thread_links", &link_id, now, record)?;
    projections.push(ProjectionRef {
        collection: "user_thread_links",
        record_id: link_id,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn upsert_status_notification(
    root: &Path,
    conn: &Connection,
    thread_id: &str,
    message_id: &str,
    status_source_id: &str,
    status: &str,
    user_id: &str,
    title: &str,
    body: &str,
    source: &Value,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    if user_id.trim().is_empty() {
        return Ok(());
    }
    let notification_type = match status {
        "completed" => "ctox_completed",
        "blocked" | "failed" => "ctox_failed",
        "archived" => "ctox_finished",
        _ => "ctox_status",
    };
    let notification_id = format!(
        "notif_ctox_{}_{}_{}",
        slug_part(status),
        slug_part(status_source_id),
        slug_part(user_id)
    );
    let existing_status = load_record(root, "user_notifications", &notification_id)?
        .map(|record| value_string(&record, "status"))
        .filter(|status| !status.is_empty())
        .unwrap_or_else(|| "unread".to_owned());
    let record = json!({
        "id": notification_id,
        "notification_id": notification_id,
        "user_id": user_id,
        "thread_id": thread_id,
        "message_id": message_id,
        "approval_request_id": "",
        "notification_type": notification_type,
        "status": existing_status,
        "title": title,
        "body_preview": truncate(body, 180),
        "source_module": source_string(source, "module").unwrap_or_default(),
        "source_record_id": source_string(source, "record_id").unwrap_or_default(),
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    store::upsert_business_record(conn, "user_notifications", &notification_id, now, record)?;
    projections.push(ProjectionRef {
        collection: "user_notifications",
        record_id: notification_id,
    });
    Ok(())
}

fn approval_id_from_command(command: &BusinessCommand) -> anyhow::Result<String> {
    first_string_field(&command.payload, &["approval_request_id", "id"])
        .or_else(|| command.record_id.clone())
        .context("approval_request_id is required")
}

fn ensure_pending_approval(approval: &Value) -> anyhow::Result<()> {
    let status = value_string(approval, "status");
    anyhow::ensure!(status == "pending", "approval request is not pending");
    Ok(())
}

fn ensure_reviewer_or_admin(session: &BusinessOsSession, approval: &Value) -> anyhow::Result<()> {
    let reviewer = value_string(approval, "reviewer_user_id");
    let actor = actor_id(session);
    anyhow::ensure!(
        is_admin_session(session) || (!reviewer.is_empty() && reviewer == actor),
        "only the assigned reviewer or admin can decide this approval request"
    );
    Ok(())
}

fn ensure_approval_editor(session: &BusinessOsSession, approval: &Value) -> anyhow::Result<()> {
    let requester = value_string(approval, "requester_user_id");
    let reviewer = value_string(approval, "reviewer_user_id");
    let actor = actor_id(session);
    anyhow::ensure!(
        is_admin_session(session)
            || (!requester.is_empty() && requester == actor)
            || (!reviewer.is_empty() && reviewer == actor),
        "only requester, reviewer, or admin can edit this approval request"
    );
    Ok(())
}

fn ensure_approval_target_policy(
    root: &Path,
    session: &BusinessOsSession,
    approval: &Value,
) -> anyhow::Result<()> {
    let module = non_empty_string(approval, "target_module")
        .or_else(|| non_empty_string(approval, "source_module"))
        .unwrap_or_else(|| "ctox".to_owned());
    let command_type = value_string(approval, "target_command_type");
    let target_payload = approval
        .get("target_payload")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let target = target_payload
        .get("target")
        .or_else(|| target_payload.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let permission = if command_type == "ctox.business_os.app.modify" || target == "app" {
        BusinessOsPermission::AppsModify
    } else if target == "read" || target == "ask" || target == "question" {
        BusinessOsPermission::DataRead
    } else {
        BusinessOsPermission::DataWrite
    };
    let decision = store::module_policy_decision(root, session, permission, &module)?;
    anyhow::ensure!(
        decision.allowed,
        "reviewer cannot approve target `{}` for module `{}`: {}",
        command_type,
        module,
        decision.display_reason
    );
    Ok(())
}

fn ensure_source_context_read_policy(
    root: &Path,
    session: &BusinessOsSession,
    source: &Value,
) -> anyhow::Result<()> {
    let module = source_string(source, "module").unwrap_or_default();
    if module.is_empty() || module == "threads" {
        return Ok(());
    }
    let decision =
        store::module_policy_decision(root, session, BusinessOsPermission::DataRead, &module)?;
    anyhow::ensure!(
        decision.allowed,
        "source context for module `{}` is not readable by actor: {}",
        module,
        decision.display_reason
    );
    Ok(())
}

fn ctox_command_document_visible_to_user(document: &Value, user_id: &str) -> bool {
    document_mentions_user(document, user_id)
        || nested_string(document, &["client_context", "actor", "id"]).as_deref() == Some(user_id)
        || nested_string(document, &["client_context", "user", "id"]).as_deref() == Some(user_id)
        || nested_string(document, &["client_context", "user_id"]).as_deref() == Some(user_id)
        || nested_string(document, &["payload", "approval", "requester_user_id"]).as_deref()
            == Some(user_id)
        || nested_string(document, &["payload", "approval", "reviewer_user_id"]).as_deref()
            == Some(user_id)
}

fn ctox_task_document_visible_to_user(root: &Path, document: &Value, user_id: &str) -> bool {
    if document_mentions_user(document, user_id)
        || nested_string(document, &["actor", "id"]).as_deref() == Some(user_id)
    {
        return true;
    }
    let command_id = value_string(document, "command_id");
    if command_id.is_empty() {
        return false;
    }
    load_record(root, "business_commands", &command_id)
        .ok()
        .flatten()
        .map(|command| ctox_command_document_visible_to_user(&command, user_id))
        .unwrap_or(false)
}

fn document_mentions_user(document: &Value, user_id: &str) -> bool {
    if user_id.trim().is_empty() {
        return false;
    }
    for key in [
        "user_id",
        "actor_id",
        "created_by_id",
        "owner_user_id",
        "assigned_user_id",
        "requester_user_id",
        "reviewer_user_id",
        "decision_by_id",
        "lease_owner",
    ] {
        if value_string(document, key) == user_id {
            return true;
        }
    }
    for key in [
        "participant_ids",
        "target_user_ids",
        "watcher_user_ids",
        "assignee_user_ids",
    ] {
        if array_strings(document.get(key)).contains(&user_id.to_owned()) {
            return true;
        }
    }
    false
}

fn thread_document_visible_to_user(root: &Path, document: &Value, user_id: &str) -> bool {
    let thread_id = value_string(document, "thread_id");
    if thread_id.is_empty() {
        return false;
    }
    load_record(root, "user_threads", &thread_id)
        .ok()
        .flatten()
        .map(|thread| thread_record_visible_to_user(&thread, user_id))
        .unwrap_or(false)
}

fn thread_record_visible_to_user(thread: &Value, user_id: &str) -> bool {
    if user_id.trim().is_empty() {
        return false;
    }
    value_string(thread, "owner_user_id") == user_id
        || value_string(thread, "created_by_id") == user_id
        || value_string(thread, "assigned_user_id") == user_id
        || array_strings(thread.get("participant_ids")).contains(&user_id.to_owned())
        || array_strings(thread.get("watcher_user_ids")).contains(&user_id.to_owned())
}

fn ensure_thread_participant_or_admin(
    session: &BusinessOsSession,
    thread: &Value,
) -> anyhow::Result<()> {
    let actor = actor_id(session);
    let participants = array_strings(thread.get("participant_ids"));
    anyhow::ensure!(
        is_admin_session(session) || participants.iter().any(|id| id == &actor),
        "only participants or admins can update this thread"
    );
    Ok(())
}

fn ensure_message_author_or_admin(
    session: &BusinessOsSession,
    message: &Value,
) -> anyhow::Result<()> {
    let actor = actor_id(session);
    let author = value_string(message, "author_user_id");
    anyhow::ensure!(
        is_admin_session(session) || (!author.is_empty() && author == actor),
        "only message author or admin can edit this message"
    );
    Ok(())
}

fn participant_set<'a, I, J>(
    root: &Path,
    thread_id: &str,
    mandatory: I,
    extra: J,
) -> BTreeSet<String>
where
    I: IntoIterator<Item = &'a str>,
    J: IntoIterator<Item = &'a String>,
{
    let mut participants = load_record(root, "user_threads", thread_id)
        .ok()
        .flatten()
        .and_then(|record| record.get("participant_ids").cloned())
        .map(|value| array_strings(Some(&value)))
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeSet<_>>();
    for id in mandatory {
        let id = id.trim();
        if !id.is_empty() {
            participants.insert(id.to_owned());
        }
    }
    for id in extra {
        let id = id.trim();
        if !id.is_empty() {
            participants.insert(id.to_owned());
        }
    }
    participants
}

fn target_user_ids(payload: &Value) -> Vec<String> {
    let mut ids = array_strings(payload.get("target_user_ids"));
    if let Some(id) = first_string_field(payload, &["target_user_id", "assignee_user_id"]) {
        ids.push(id);
    }
    ids.sort();
    ids.dedup();
    ids
}

fn source_context(command: &BusinessCommand) -> Value {
    let mut source = command
        .payload
        .get("source_context")
        .or_else(|| command.payload.get("context"))
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    ensure_object_field(&mut source, "module", || {
        first_string_field(&command.payload, &["source_module", "module"])
            .or_else(|| first_string_field(&command.client_context, &["source_module", "module"]))
            .unwrap_or_else(|| command.module.clone())
    });
    ensure_object_field(&mut source, "record_type", || {
        first_string_field(&command.payload, &["record_type"])
            .or_else(|| first_string_field(&command.client_context, &["record_type"]))
            .unwrap_or_default()
    });
    ensure_object_field(&mut source, "record_id", || {
        first_string_field(&command.payload, &["record_id"])
            .or_else(|| first_string_field(&command.client_context, &["record_id"]))
            .or_else(|| command.record_id.clone())
            .unwrap_or_default()
    });
    ensure_object_field(&mut source, "label", || {
        first_string_field(&command.payload, &["label", "source_label", "title"])
            .or_else(|| first_string_field(&command.client_context, &["label"]))
            .unwrap_or_default()
    });
    source
}

fn thread_source_context(record: &Value) -> Option<Value> {
    if let Some(source) = record
        .get("metadata")
        .and_then(|metadata| metadata.get("source_context"))
        .filter(|source| source.is_object())
    {
        return Some(source.clone());
    }
    Some(json!({
        "module": value_string(record, "source_module"),
        "record_type": value_string(record, "source_record_type"),
        "record_id": value_string(record, "source_record_id"),
        "label": value_string(record, "source_label"),
        "deep_link": value_string(record, "source_deep_link"),
    }))
}

fn thread_id_for_command(command: &BusinessCommand, source: &Value) -> String {
    if let Some(thread_id) = first_string_field(&command.payload, &["thread_id"]) {
        return thread_id;
    }
    let module = source_string(source, "module").unwrap_or_default();
    let record_type = source_string(source, "record_type").unwrap_or_default();
    let record_id = source_string(source, "record_id")
        .or_else(|| command.record_id.clone())
        .unwrap_or_default();
    if !module.is_empty() && !record_id.is_empty() {
        return truncate(
            &format!(
                "thread_{}_{}_{}",
                slug_part(&module),
                slug_part(&record_type),
                slug_part(&record_id)
            ),
            220,
        );
    }
    format!("thread_{}", Uuid::new_v4())
}

fn projection_values(projections: Vec<ProjectionRef>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut values = Vec::new();
    for projection in projections {
        let key = format!("{}/{}", projection.collection, projection.record_id);
        if seen.insert(key) {
            values.push(json!({
                "collection": projection.collection,
                "record_id": projection.record_id,
            }));
        }
    }
    values
}

fn projection_pairs(projections: Vec<ProjectionRef>) -> Vec<(&'static str, String)> {
    let mut seen = BTreeSet::new();
    let mut values = Vec::new();
    for projection in projections {
        let key = format!("{}/{}", projection.collection, projection.record_id);
        if seen.insert(key) {
            values.push((projection.collection, projection.record_id));
        }
    }
    values
}

fn document_is_deleted(value: &Value) -> bool {
    value
        .get("_deleted")
        .or_else(|| value.get("is_deleted"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn document_updated_at_ms(value: &Value) -> i64 {
    value
        .get("updated_at_ms")
        .or_else(|| value.get("observed_at_ms"))
        .or_else(|| value.get("created_at_ms"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn first_non_empty_owned<const N: usize>(values: [String; N]) -> String {
    values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

fn required_string(value: &Value, keys: &[&str]) -> anyhow::Result<String> {
    first_string_field(value, keys).context("required text field is missing")
}

fn first_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    })
}

fn source_string(source: &Value, key: &str) -> Option<String> {
    source
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn non_empty_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn nested_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn value_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn ensure_object_field(value: &mut Value, key: &str, fallback: impl FnOnce() -> String) {
    let Some(object) = value.as_object_mut() else {
        *value = Value::Object(Map::new());
        ensure_object_field(value, key, fallback);
        return;
    };
    let existing = object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if existing.is_empty() {
        object.insert(key.to_owned(), Value::String(fallback()));
    }
}

fn set_object_string(value: &mut Value, key: &str, next: &str) {
    if !value.is_object() {
        *value = json!({});
    }
    if let Some(object) = value.as_object_mut() {
        object.insert(key.to_owned(), Value::String(next.to_owned()));
    }
}

fn set_object_i64(value: &mut Value, key: &str, next: i64) {
    if !value.is_object() {
        *value = json!({});
    }
    if let Some(object) = value.as_object_mut() {
        object.insert(key.to_owned(), Value::from(next));
    }
}

fn set_object_value(value: &mut Value, key: &str, next: Value) {
    if !value.is_object() {
        *value = json!({});
    }
    if let Some(object) = value.as_object_mut() {
        object.insert(key.to_owned(), next);
    }
}

fn set_object_array_strings(value: &mut Value, key: &str, next: &[String]) {
    set_object_value(
        value,
        key,
        Value::Array(
            next.iter()
                .map(|item| Value::String(item.clone()))
                .collect(),
        ),
    );
}

fn soft_delete_payload(value: &mut Value, now: i64) {
    if !value.is_object() {
        *value = json!({});
    }
    if let Some(object) = value.as_object_mut() {
        object.insert("is_deleted".to_owned(), Value::Bool(true));
        object.insert("deleted_at_ms".to_owned(), Value::from(now));
        object.insert("updated_at_ms".to_owned(), Value::from(now));
    }
}

fn array_strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn actor_id(session: &BusinessOsSession) -> String {
    session
        .user
        .as_ref()
        .map(|user| user.id.trim())
        .filter(|id| !id.is_empty())
        .unwrap_or("rxdb-command")
        .to_owned()
}

fn actor_display_name(session: &BusinessOsSession) -> String {
    let fallback = actor_id(session);
    session
        .user
        .as_ref()
        .map(|user| user.display_name.trim())
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback.as_str())
        .to_owned()
}

fn actor_role(session: &BusinessOsSession) -> String {
    session
        .user
        .as_ref()
        .map(|user| user.role.trim())
        .filter(|role| !role.is_empty())
        .unwrap_or("user")
        .to_owned()
}

fn actor_payload(session: &BusinessOsSession) -> Value {
    json!({
        "id": actor_id(session),
        "display_name": actor_display_name(session),
        "role": actor_role(session),
        "is_admin": is_admin_session(session),
    })
}

fn is_admin_session(session: &BusinessOsSession) -> bool {
    session
        .user
        .as_ref()
        .map(|user| user.is_admin)
        .unwrap_or(false)
        || matches!(actor_role(session).as_str(), "chef" | "admin")
}

fn approval_title(approval: &Value, prompt: &str) -> String {
    let source_label = value_string(approval, "source_label");
    if !source_label.is_empty() {
        return format!("Freigabe: {source_label}");
    }
    truncate(prompt, 80)
}

fn slug_part(value: &str) -> String {
    let mut slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    while slug.contains("__") {
        slug = slug.replace("__", "_");
    }
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "item".to_owned()
    } else {
        truncate(slug, 80)
    }
}

fn truncate(value: &str, max_len: usize) -> String {
    let mut text = value.trim().to_owned();
    if text.chars().count() <= max_len {
        return text;
    }
    text = text.chars().take(max_len).collect();
    text
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn threads_command_allowlist_is_explicit() {
        assert!(is_threads_command("threads.note.create"));
        assert!(is_threads_command("threads.note.update"));
        assert!(is_threads_command("threads.note.delete"));
        assert!(is_threads_command("threads.thread.watch"));
        assert!(is_threads_command("threads.thread.unwatch"));
        assert!(is_threads_command("threads.thread.snooze"));
        assert!(is_threads_command("threads.ctox_approval.edit"));
        assert!(is_threads_command("threads.ctox_approval.approve"));
        assert!(is_threads_command("threads.link.create"));
        assert!(is_threads_command("threads.link.remove"));
        assert!(!is_threads_command("threads.unknown"));
        assert!(requires_external_approval("threads.ctox_approval.approve"));
        assert!(!requires_external_approval("threads.ctox_approval.request"));
    }

    #[test]
    fn peer_write_gate_allows_command_admission_but_blocks_native_owned_records()
    -> anyhow::Result<()> {
        let temp = tempdir()?;
        assert!(may_accept_peer_write(temp.path(), "", "business_commands"));
        assert!(!may_accept_peer_write(temp.path(), "", "user_threads"));
        assert!(!may_accept_peer_write(temp.path(), "", "ctox_queue_tasks"));
        assert!(may_accept_peer_write(temp.path(), "", "support_notes"));
        Ok(())
    }

    #[test]
    fn threads_control_commands_do_not_project_queue_task_id() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let outcome = store::accept_rxdb_business_command(
            temp.path(),
            json!({
                "id": "cmd-thread-note",
                "module": "threads",
                "command_type": "threads.note.create",
                "record_id": "case-1",
                "payload": {
                    "body": "Bitte pruefen.",
                    "source_context": {
                        "module": "threads",
                        "record_type": "thread",
                        "record_id": "case-1"
                    }
                },
                "client_context": {
                    "actor": {
                        "id": "alice",
                        "display_name": "Alice",
                        "role": "user"
                    }
                }
            }),
        )?;
        assert_eq!(value_string(&outcome, "status"), "completed");
        assert_eq!(value_string(&outcome, "task_id"), "");

        let command = load_record(temp.path(), "business_commands", "cmd-thread-note")?
            .context("projected threads command")?;
        assert_eq!(value_string(&command, "task_id"), "");
        assert_eq!(value_string(&command, "record_id"), "case-1");
        Ok(())
    }

    #[test]
    fn source_thread_id_is_deterministic_for_app_record() {
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_test".to_owned()),
            module: "threads".to_owned(),
            command_type: "threads.note.create".to_owned(),
            record_id: Some("A/1".to_owned()),
            payload: json!({
                "source_context": {
                    "module": "support",
                    "record_type": "conversation",
                    "record_id": "A/1"
                }
            }),
            client_context: json!({}),
        };
        let source = source_context(&command);
        assert_eq!(
            thread_id_for_command(&command, &source),
            "thread_support_conversation_A_1"
        );
    }

    #[test]
    fn source_context_keeps_deep_link() {
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd_test".to_owned()),
            module: "threads".to_owned(),
            command_type: "threads.note.create".to_owned(),
            record_id: Some("case-1".to_owned()),
            payload: json!({
                "source_context": {
                    "module": "tickets",
                    "record_type": "ticket",
                    "record_id": "case-1",
                    "deep_link": "#tickets?record=case-1&record_type=ticket"
                }
            }),
            client_context: json!({}),
        };
        let source = source_context(&command);
        assert_eq!(
            source.get("deep_link").and_then(Value::as_str),
            Some("#tickets?record=case-1&record_type=ticket")
        );
    }

    #[test]
    fn approval_request_is_not_a_queue_task_until_reviewed() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let outcome = store::accept_rxdb_business_command(
            temp.path(),
            json!({
                "id": "cmd-approval-request",
                "module": "threads",
                "command_type": "threads.ctox_approval.request",
                "record_id": "case-1",
                "payload": {
                    "approval_request_id": "approval-request-1",
                    "prompt": "CTOX soll den Fall zusammenfassen.",
                    "reviewer_user_id": "lead",
                    "target_module": "ctox",
                    "target_record_id": "case-1",
                    "target_command_type": "business_os.chat.task",
                    "source_context": {
                        "module": "threads",
                        "record_type": "case",
                        "record_id": "case-1",
                        "label": "Case 1",
                        "deep_link": "#tickets?record=case-1"
                    }
                },
                "client_context": {
                    "actor": {
                        "id": "junior",
                        "display_name": "Junior",
                        "role": "user"
                    }
                }
            }),
        )?;

        assert_eq!(value_string(&outcome, "status"), "completed");
        assert_eq!(value_string(&outcome, "task_id"), "");
        assert_eq!(
            collection_document_count(temp.path(), "ctox_queue_tasks")?,
            0
        );

        let approval = load_record(
            temp.path(),
            "ctox_task_approval_requests",
            "approval-request-1",
        )?
        .context("approval request")?;
        assert_eq!(value_string(&approval, "status"), "pending");
        assert_eq!(value_string(&approval, "requester_user_id"), "junior");
        assert_eq!(value_string(&approval, "reviewer_user_id"), "lead");
        assert_eq!(
            approval
                .get("source_context")
                .and_then(|source| source.get("deep_link"))
                .and_then(Value::as_str),
            Some("#tickets?record=case-1")
        );

        Ok(())
    }

    #[test]
    fn rejected_approval_creates_no_queue_task() -> anyhow::Result<()> {
        let temp = tempdir()?;
        store::accept_rxdb_business_command(
            temp.path(),
            json!({
                "id": "cmd-reject-request",
                "module": "threads",
                "command_type": "threads.ctox_approval.request",
                "record_id": "case-2",
                "payload": {
                    "approval_request_id": "approval-reject-1",
                    "prompt": "CTOX soll unklaren Kontext veraendern.",
                    "reviewer_user_id": "lead",
                    "source_context": {
                        "module": "threads",
                        "record_type": "case",
                        "record_id": "case-2"
                    }
                },
                "client_context": {
                    "actor": {
                        "id": "junior",
                        "display_name": "Junior",
                        "role": "user"
                    }
                }
            }),
        )?;

        let rejected = store::accept_rxdb_business_command(
            temp.path(),
            json!({
                "id": "cmd-reject-decision",
                "module": "threads",
                "command_type": "threads.ctox_approval.reject",
                "record_id": "approval-reject-1",
                "payload": {
                    "approval_request_id": "approval-reject-1",
                    "decision_note": "Kontext reicht nicht."
                },
                "client_context": {
                    "actor": {
                        "id": "lead",
                        "display_name": "Lead",
                        "role": "user"
                    }
                }
            }),
        )?;

        assert_eq!(value_string(&rejected, "status"), "completed");
        assert_eq!(value_string(&rejected, "task_id"), "");
        assert_eq!(
            collection_document_count(temp.path(), "ctox_queue_tasks")?,
            0
        );
        let approval = load_record(
            temp.path(),
            "ctox_task_approval_requests",
            "approval-reject-1",
        )?
        .context("rejected approval")?;
        assert_eq!(value_string(&approval, "status"), "rejected");
        assert_eq!(value_string(&approval, "decision_by_id"), "lead");

        Ok(())
    }

    #[test]
    fn approved_request_creates_command_task_and_audit_linkage() -> anyhow::Result<()> {
        let temp = tempdir()?;
        store::accept_rxdb_business_command(
            temp.path(),
            json!({
                "id": "cmd-approve-request",
                "module": "threads",
                "command_type": "threads.ctox_approval.request",
                "record_id": "case-3",
                "payload": {
                    "approval_request_id": "approval-approve-1",
                    "prompt": "CTOX soll den Fall zusammenfassen.",
                    "reviewer_user_id": "lead",
                    "target_module": "ctox",
                    "target_record_id": "case-3",
                    "target_command_type": "business_os.chat.task",
                    "source_context": {
                        "module": "threads",
                        "record_type": "case",
                        "record_id": "case-3",
                        "label": "Case 3"
                    },
                    "target_payload": {
                        "mode": "data",
                        "target": "data"
                    }
                },
                "client_context": {
                    "actor": {
                        "id": "junior",
                        "display_name": "Junior",
                        "role": "user"
                    }
                }
            }),
        )?;

        let approved = store::accept_rxdb_business_command(
            temp.path(),
            json!({
                "id": "cmd-approve-decision",
                "module": "threads",
                "command_type": "threads.ctox_approval.approve",
                "record_id": "approval-approve-1",
                "payload": {
                    "approval_request_id": "approval-approve-1",
                    "decision_note": "Passt."
                },
                "client_context": {
                    "actor": {
                        "id": "admin",
                        "display_name": "Admin",
                        "role": "admin"
                    }
                }
            }),
        )?;

        assert_eq!(value_string(&approved, "status"), "completed");
        assert_eq!(
            collection_document_count(temp.path(), "ctox_queue_tasks")?,
            1
        );

        let approved_command_id = approved
            .pointer("/result/approved_command_id")
            .and_then(Value::as_str)
            .context("approved command id")?;
        let approved_task_id = approved
            .pointer("/result/approved_task_id")
            .and_then(Value::as_str)
            .context("approved task id")?;
        assert!(!approved_command_id.is_empty());
        assert!(!approved_task_id.is_empty());

        let approval = load_record(
            temp.path(),
            "ctox_task_approval_requests",
            "approval-approve-1",
        )?
        .context("approved approval")?;
        assert_eq!(value_string(&approval, "status"), "approved");
        assert_eq!(value_string(&approval, "decision_by_id"), "admin");
        assert_eq!(
            value_string(&approval, "approved_command_id"),
            approved_command_id
        );
        assert_eq!(
            value_string(&approval, "approved_task_id"),
            approved_task_id
        );

        let approved_command = load_record(temp.path(), "business_commands", approved_command_id)?
            .context("approved command projection")?;
        assert_eq!(value_string(&approved_command, "module"), "ctox");
        assert_eq!(
            value_string(&approved_command, "command_type"),
            "business_os.chat.task"
        );
        assert_eq!(value_string(&approved_command, "record_id"), "case-3");
        assert_eq!(
            approved_command
                .pointer("/payload/approval/approval_request_id")
                .and_then(Value::as_str),
            Some("approval-approve-1")
        );
        assert_eq!(
            approved_command
                .pointer("/payload/approval/requester_user_id")
                .and_then(Value::as_str),
            Some("junior")
        );
        assert_eq!(
            approved_command
                .pointer("/payload/approval/reviewer_user_id")
                .and_then(Value::as_str),
            Some("lead")
        );

        let task = load_record(temp.path(), "ctox_queue_tasks", approved_task_id)?
            .context("approved queue task projection")?;
        assert_eq!(value_string(&task, "command_id"), approved_command_id);

        let audit = business_event_payloads(
            temp.path(),
            "ctox_task_approval_requests",
            "approval-approve-1",
        )?;
        assert_eq!(audit.len(), 1, "expected one approval decision event");
        assert_eq!(
            audit[0].get("event_type").and_then(Value::as_str),
            Some("business_os.external_approval.decided")
        );
        assert_eq!(
            audit[0].get("decision").and_then(Value::as_str),
            Some("approved")
        );
        assert_eq!(
            audit[0].get("requester_user_id").and_then(Value::as_str),
            Some("junior")
        );
        assert_eq!(
            audit[0].get("reviewer_user_id").and_then(Value::as_str),
            Some("lead")
        );
        assert_eq!(
            audit[0].get("approved_command_id").and_then(Value::as_str),
            Some(approved_command_id)
        );
        assert_eq!(
            audit[0].get("approved_task_id").and_then(Value::as_str),
            Some(approved_task_id)
        );

        Ok(())
    }

    #[test]
    fn thread_document_replication_is_user_scoped() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let now = now_ms();
        let (alice_token, _) = store::issue_business_os_capability_token_for_managed_user(
            temp.path(),
            "alice",
            "Alice",
            "user",
            now,
        )?;
        let (admin_token, _) = store::issue_business_os_capability_token_for_managed_user(
            temp.path(),
            "admin",
            "Admin",
            "admin",
            now,
        )?;

        assert!(may_replicate_document(
            temp.path(),
            &alice_token,
            "user_notifications",
            &json!({ "id": "n1", "user_id": "alice" }),
        ));
        assert!(!may_replicate_document(
            temp.path(),
            &alice_token,
            "user_notifications",
            &json!({ "id": "n2", "user_id": "bob" }),
        ));
        assert!(may_replicate_document(
            temp.path(),
            &admin_token,
            "user_notifications",
            &json!({ "id": "n3", "user_id": "bob" }),
        ));
        assert!(may_replicate_document(
            temp.path(),
            &alice_token,
            "business_commands",
            &json!({
                "id": "cmd-alice",
                "client_context": { "actor": { "id": "alice" } }
            }),
        ));
        assert!(!may_replicate_document(
            temp.path(),
            &alice_token,
            "business_commands",
            &json!({
                "id": "cmd-bob",
                "client_context": { "actor": { "id": "bob" } }
            }),
        ));
        assert!(may_replicate_document(
            temp.path(),
            &alice_token,
            "ctox_queue_tasks",
            &json!({
                "id": "task-alice",
                "owner_user_id": "alice"
            }),
        ));
        assert!(!may_accept_peer_write(
            temp.path(),
            &alice_token,
            "user_threads"
        ));
        assert!(may_accept_peer_write(
            temp.path(),
            &alice_token,
            "business_commands"
        ));
        assert!(may_accept_peer_write(temp.path(), "", "business_commands"));
        assert!(!may_accept_peer_write(
            temp.path(),
            &alice_token,
            "ctox_queue_tasks"
        ));

        Ok(())
    }

    #[test]
    fn ctox_relevance_projection_joins_command_and_task_into_one_thread() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let now = now_ms();
        let conn = store::open_store(temp.path())?;
        store::upsert_business_record(
            &conn,
            "business_commands",
            "cmd-relevance",
            now,
            json!({
                "id": "cmd-relevance",
                "command_id": "cmd-relevance",
                "module": "support",
                "command_type": "business_os.chat.task",
                "record_id": "case-1",
                "status": "completed",
                "task_id": "task-relevance",
                "payload": {
                    "title": "Pruefe Antwortentwurf",
                    "prompt": "Bitte pruefe den Antwortentwurf fuer case-1."
                },
                "client_context": {
                    "actor": {
                        "id": "alice",
                        "display_name": "Alice",
                        "role": "user"
                    }
                }
            }),
        )?;
        store::upsert_business_record(
            &conn,
            "ctox_queue_tasks",
            "task-relevance",
            now + 1,
            json!({
                "id": "task-relevance",
                "command_id": "cmd-relevance",
                "source_module": "support",
                "status": "completed",
                "title": "Pruefe Antwortentwurf",
                "actor": {
                    "id": "alice",
                    "display_name": "Alice",
                    "role": "user"
                }
            }),
        )?;
        drop(conn);

        let outcome = project_ctox_relevance(temp.path(), 0, 0, 50)?;

        assert!(outcome.changed_count > 0);
        assert!(outcome.projections.iter().any(|(collection, record_id)| {
            *collection == "user_threads" && record_id == "thread_support_command_case-1"
        }));
        assert!(
            load_record(
                temp.path(),
                "user_threads",
                "thread_support_queue_task_case-1"
            )?
            .is_none()
        );

        let thread = load_record(temp.path(), "user_threads", "thread_support_command_case-1")?
            .context("projected relevance thread")?;
        assert_eq!(value_string(&thread, "source_record_type"), "command");
        assert_eq!(value_string(&thread, "status"), "completed");

        let command_link = load_record(
            temp.path(),
            "user_thread_links",
            "link_thread_support_command_case-1_business_commands_cmd-relevance",
        )?
        .context("projected command link")?;
        assert_eq!(value_string(&command_link, "link_role"), "ctox_command");
        assert_eq!(value_string(&command_link, "task_id"), "task-relevance");

        let task_link = load_record(
            temp.path(),
            "user_thread_links",
            "link_thread_support_command_case-1_ctox_queue_tasks_task-relevance",
        )?
        .context("projected task link")?;
        assert_eq!(value_string(&task_link, "link_role"), "ctox_task");

        let notification = load_record(
            temp.path(),
            "user_notifications",
            "notif_ctox_completed_cmd-relevance_alice",
        )?
        .context("projected completion notification")?;
        assert_eq!(
            value_string(&notification, "thread_id"),
            "thread_support_command_case-1"
        );
        assert_eq!(value_string(&notification, "user_id"), "alice");

        Ok(())
    }

    #[test]
    fn app_relevance_projection_surfaces_ticket_approval_for_reviewer() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let now = now_ms();
        let conn = store::open_store(temp.path())?;
        store::upsert_business_record(
            &conn,
            "ctox_ticket_approvals",
            "approval-1",
            now,
            json!({
                "id": "approval-1",
                "case_id": "case-7",
                "title": "Antwortentwurf freigeben",
                "status": "pending",
                "requester_user_id": "bob",
                "reviewer_user_id": "alice",
                "comment": "Bitte vor Versand pruefen.",
                "updated_at_ms": now
            }),
        )?;
        drop(conn);

        let outcome = project_app_relevance(temp.path(), &[("ctox_ticket_approvals", 0)], 50)?;

        assert!(outcome.changed_count > 0);
        assert!(outcome.projections.iter().any(|(collection, record_id)| {
            *collection == "user_threads" && record_id == "thread_tickets_ticket_case_case-7"
        }));

        let thread = load_record(
            temp.path(),
            "user_threads",
            "thread_tickets_ticket_case_case-7",
        )?
        .context("projected app relevance thread")?;
        assert_eq!(value_string(&thread, "kind"), "approval");
        assert_eq!(value_string(&thread, "status"), "needs_review");
        assert_eq!(value_string(&thread, "assigned_user_id"), "alice");
        assert!(array_strings(thread.get("participant_ids")).contains(&"alice".to_owned()));
        assert!(array_strings(thread.get("participant_ids")).contains(&"bob".to_owned()));

        let link = load_record(
            temp.path(),
            "user_thread_links",
            "link_thread_tickets_ticket_case_case-7_ctox_ticket_approvals_approval-1",
        )?
        .context("projected app record link")?;
        assert_eq!(value_string(&link, "link_role"), "app_record");
        assert_eq!(value_string(&link, "link_type"), "ticket_case");
        assert_eq!(value_string(&link, "source_record_id"), "approval-1");

        let notification = load_record(
            temp.path(),
            "user_notifications",
            "notif_app_needs_review_ctox_ticket_approvals_approval-1_alice",
        )?
        .context("projected reviewer notification")?;
        assert_eq!(
            value_string(&notification, "notification_type"),
            "approval_requested"
        );
        assert_eq!(value_string(&notification, "user_id"), "alice");
        assert!(
            load_record(
                temp.path(),
                "user_notifications",
                "notif_app_needs_review_ctox_ticket_approvals_approval-1_bob",
            )?
            .is_none()
        );

        Ok(())
    }

    #[test]
    fn app_relevance_projection_ignores_records_without_user_relevance() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let now = now_ms();
        let conn = store::open_store(temp.path())?;
        store::upsert_business_record(
            &conn,
            "support_conversations",
            "conv-public",
            now,
            json!({
                "id": "conv-public",
                "status": "open",
                "priority": "normal",
                "search_text": "Unassigned support conversation",
                "updated_at_ms": now
            }),
        )?;
        drop(conn);

        let outcome = project_app_relevance(temp.path(), &[("support_conversations", 0)], 50)?;

        assert_eq!(outcome.changed_count, 0);
        assert!(
            load_record(
                temp.path(),
                "user_threads",
                "thread_support_conversation_conv-public",
            )?
            .is_none()
        );
        Ok(())
    }

    fn collection_document_count(root: &Path, collection: &str) -> anyhow::Result<usize> {
        let pulled =
            store::pull_collection_records_for_projection(root, collection, Some(0), Some(1_000))?;
        Ok(pulled
            .get("documents")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0))
    }

    fn business_event_payloads(
        root: &Path,
        collection: &str,
        record_id: &str,
    ) -> anyhow::Result<Vec<Value>> {
        let conn = store::open_store(root)?;
        let mut stmt = conn.prepare(
            "SELECT payload_json FROM business_events
             WHERE collection = ?1 AND record_id = ?2
             ORDER BY observed_at_ms ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![collection, record_id], |row| {
            row.get::<_, String>(0)
        })?;
        let mut payloads = Vec::new();
        for row in rows {
            payloads.push(serde_json::from_str::<Value>(&row?)?);
        }
        Ok(payloads)
    }
}
