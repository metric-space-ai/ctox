// Origin: CTOX
// License: Apache-2.0

use super::store::{
    apply_queue_projection_status_fields, browser_context_artifact_for_command, clip_text,
    command_inbound_channel, command_status_for_queue_route_status,
    count_legacy_http_fallback_records, find_queue_task_for_command, first_string_field, now_ms,
    open_store, projection_route_status_for_command_status, projection_status_is_active,
    push_repair_action, queue_status_is_terminal_failure, queue_status_is_terminal_success,
    redact_document_client_context_secrets, repair_inline_payload_artifacts,
    repair_placeholder_business_chat_owners, upsert_command_projection_from_queue_status,
    upsert_rxdb_collection_record, upsert_rxdb_collection_record_cached, BusinessCommand,
    QueueProjectionRepairOptions, RxdbProjectionWriterCache,
    BUSINESS_OS_QUEUE_ORPHAN_REPAIR_AGE_MS,
};
use crate::mission::channels;
use anyhow::Context;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use uuid::Uuid;

pub(super) fn persist_terminal_business_chat_command_projection(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    task_id: &str,
    accepted: &Value,
) -> anyhow::Result<()> {
    let completed_at_ms = now_ms() as i64;
    conn.execute(
        "UPDATE business_commands SET status='completed', observed_at_ms=?2 WHERE command_id=?1",
        params![command_id, completed_at_ms],
    )?;
    let result = accepted.get("result").cloned().unwrap_or(Value::Null);
    let reply_text = accepted
        .get("outbound_text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let payload = serde_json::json!({
        "id": command_id,
        "command_id": command_id,
        "module": command.module.clone(),
        "command_type": command.command_type.clone(),
        "record_id": command.record_id.clone().unwrap_or_default(),
        "status": "completed",
        "execution_mode": "queue",
        "execution_phase": "terminal",
        "terminal_status": "completed",
        "route_status": "handled",
        "inbound_channel": command_inbound_channel(command),
        "task_id": task_id,
        "task_status": "completed",
        "payload": command.payload.clone(),
        "client_context": command.client_context.clone(),
        "result": result,
        "outbound_text": reply_text,
        "response": reply_text,
        "answer": reply_text,
        "updated_at_ms": completed_at_ms
    });
    upsert_business_record(
        conn,
        "business_commands",
        command_id,
        completed_at_ms,
        payload.clone(),
    )?;
    let mut rxdb_writers = RxdbProjectionWriterCache::new(root);
    upsert_rxdb_collection_record_cached(
        root,
        Some(&mut rxdb_writers),
        "business_commands",
        command_id,
        completed_at_ms,
        payload,
    )
}

pub(super) fn is_business_chat_command(command: &BusinessCommand) -> bool {
    matches!(
        command.command_type.as_str(),
        "business_os.chat.task" | "business_os.context.ask" | "business_os.data.modify"
    ) || first_string_field(
        &command.payload,
        &["response_channel", "outbound_channel", "inbound_channel"],
    )
    .or_else(|| {
        first_string_field(
            &command.client_context,
            &["response_channel", "outbound_channel", "inbound_channel"],
        )
    })
    .map(|value| {
        matches!(
            value.as_str(),
            "business_os_chat" | "business_os.llm.chat" | "business-os-chat"
        )
    })
    .unwrap_or(false)
}

pub(super) fn business_chat_id(command: &BusinessCommand, command_id: &str) -> String {
    first_string_field(&command.payload, &["reply_to", "chat_id"])
        .or_else(|| first_string_field(&command.client_context, &["chat_id", "reply_to"]))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("chat_{command_id}"))
}

pub(super) fn business_chat_title(command: &BusinessCommand) -> String {
    first_string_field(&command.payload, &["title"])
        .or_else(|| first_string_field(&command.client_context, &["title", "source_title"]))
        .map(|value| clip_text(&value, 42))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "CTOX".to_string())
}

fn client_context_string(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

pub(super) fn is_placeholder_business_chat_owner(owner: &str) -> bool {
    let trimmed = owner.trim();
    trimmed.is_empty() || trimmed == "local-dev"
}

fn non_placeholder_owner(value: Option<String>) -> Option<String> {
    value.filter(|item| !is_placeholder_business_chat_owner(item))
}

fn owner_user_id_from_context(value: &Value) -> Option<String> {
    for key in ["owner_user_id", "user_id", "owner"] {
        if let Some(owner) = non_placeholder_owner(first_string_field(value, &[key])) {
            return Some(owner);
        }
    }
    non_placeholder_owner(client_context_string(value, "/actor/id"))
        .or_else(|| non_placeholder_owner(client_context_string(value, "/actor/user_id")))
        .or_else(|| {
            non_placeholder_owner(
                value
                    .get("actor")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string),
            )
        })
}

fn resolve_existing_business_chat_owner(existing: &str, projected: String) -> String {
    let _ = projected;
    existing.trim().to_string()
}

pub(super) fn existing_business_chat_owner(
    conn: &Connection,
    chat_id: &str,
) -> anyhow::Result<Option<String>> {
    let payload = conn
        .query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'business_chats' AND record_id = ?1 AND deleted = 0",
            params![chat_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(payload) = payload else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(&payload)?;
    Ok(Some(
        value
            .get("owner_user_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string(),
    ))
}

pub(super) fn business_chat_owner_user_id(command: &BusinessCommand) -> String {
    owner_user_id_from_context(&command.client_context).unwrap_or_else(|| "local-dev".to_string())
}

pub(super) fn initial_pending_business_chat_payload(
    command: &BusinessCommand,
    command_id: &str,
    owner_user_id: &str,
    updated_at_ms: i64,
    task_id: &str,
) -> Value {
    let chat_id = business_chat_id(command, command_id);
    let title = business_chat_title(command);
    let auto_focus = command
        .client_context
        .get("business_chat_auto_focus")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    serde_json::json!({
        "id": chat_id,
        "title": title,
        "open": true,
        "minimized": !auto_focus,
        "owner_user_id": owner_user_id,
        "contextMeta": {
            "module": command.module,
            "source_module": command.module,
            "client_context": command.client_context
        },
        "lastTrackingId": task_id,
        "messages": [],
        "draft": "",
        "createdAt": updated_at_ms,
        "updated_at_ms": updated_at_ms
    })
}

pub(super) fn materialize_pending_business_chat(
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
    updated_at_ms: i64,
) -> anyhow::Result<String> {
    let chat_id = business_chat_id(command, command_id);
    let title = business_chat_title(command);
    let owner_user_id = business_chat_owner_user_id(command);
    let auto_focus = command
        .client_context
        .get("business_chat_auto_focus")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let task_id = queue_task
        .map(|task| task.message_key.clone())
        .unwrap_or_default();
    let status = queue_task
        .map(|task| normalize_queue_status(&task.route_status).to_string())
        .unwrap_or_else(|| "accepted".to_string());
    let user_message_id = first_string_field(&command.payload, &["message_id"])
        .or_else(|| first_string_field(&command.client_context, &["message_id"]))
        .unwrap_or_else(|| format!("chatmsg_{command_id}"));
    let user_text = first_string_field(
        &command.payload,
        &["user_message", "instruction", "prompt", "message"],
    )
    .or_else(|| first_string_field(&command.client_context, &["user_message", "message"]))
    .unwrap_or_else(|| {
        queue_task
            .map(|task| task.prompt.clone())
            .unwrap_or_default()
    });
    let mut chat = conn
        .query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'business_chats' AND record_id = ?1",
            params![chat_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|payload| serde_json::from_str::<Value>(&payload).ok())
        .unwrap_or_else(|| {
            initial_pending_business_chat_payload(
                command,
                command_id,
                &owner_user_id,
                updated_at_ms,
                &task_id,
            )
        });
    let obj = chat
        .as_object_mut()
        .context("pending business chat payload is not an object")?;
    obj.insert("id".to_string(), Value::String(chat_id.clone()));
    obj.entry("title".to_string())
        .or_insert_with(|| Value::String(title));
    obj.insert("open".to_string(), Value::Bool(true));
    obj.entry("minimized".to_string())
        .or_insert_with(|| Value::Bool(!auto_focus));
    obj.entry("contextMeta".to_string()).or_insert_with(|| {
        serde_json::json!({
            "module": command.module,
            "source_module": command.module,
            "client_context": command.client_context
        })
    });
    let existing_owner = obj
        .get("owner_user_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    obj.insert(
        "owner_user_id".to_string(),
        Value::String(resolve_existing_business_chat_owner(
            &existing_owner,
            owner_user_id,
        )),
    );
    obj.insert(
        "lastTrackingId".to_string(),
        Value::String(if task_id.is_empty() {
            command_id.to_string()
        } else {
            task_id.clone()
        }),
    );
    obj.entry("draft".to_string())
        .or_insert_with(|| Value::String(String::new()));
    obj.entry("createdAt".to_string())
        .or_insert_with(|| Value::from(updated_at_ms));
    obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    if !obj.get("messages").is_some_and(Value::is_array) {
        obj.insert("messages".to_string(), Value::Array(Vec::new()));
    }
    let messages = obj
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .context("pending business chat messages is not an array")?;
    if !user_text.trim().is_empty()
        && !messages
            .iter()
            .any(|item| item.get("id").and_then(Value::as_str) == Some(user_message_id.as_str()))
    {
        messages.push(serde_json::json!({
            "id": user_message_id,
            "role": "user",
            "text": user_text,
            "createdAt": updated_at_ms.saturating_sub(1)
        }));
    }
    let status_message_id = format!("status_{command_id}");
    if let Some(existing) = messages
        .iter_mut()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(status_message_id.as_str()))
    {
        existing["commandId"] = Value::String(command_id.to_string());
        existing["taskId"] = Value::String(task_id.clone());
        existing["kind"] = Value::String("status".into());
        existing["command_id"] = Value::String(command_id.to_string());
        existing["task_id"] = Value::String(task_id.clone());
        existing
            .as_object_mut()
            .unwrap()
            .entry("run_id")
            .or_insert(Value::Null);
        existing["status"] = Value::String(status.clone());
    } else {
        messages.push(serde_json::json!({
            "id": status_message_id,
            "role": "ctox",
            "kind": "status", "task_id": task_id, "command_id": command_id, "run_id": null,
            "text": super::harness_cockpit::queued_chat_text(command),
            "commandId": command_id,
            "taskId": task_id,
            "status": status,
            "createdAt": updated_at_ms
        }));
    }
    if messages.len() > 40 {
        super::harness_cockpit::trim_messages(messages);
    }
    update_business_chat_tracking_fields(obj);
    upsert_business_record(conn, "business_chats", &chat_id, updated_at_ms, chat)?;
    Ok(chat_id)
}

pub(super) fn materialize_control_business_chat_state(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    status: &str,
    result: &Value,
    terminal: bool,
    updated_at_ms: i64,
) -> anyhow::Result<String> {
    let chat_id =
        materialize_pending_business_chat(conn, command_id, command, None, updated_at_ms)?;
    let mut chat = conn
        .query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'business_chats' AND record_id = ?1",
            params![chat_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|payload| serde_json::from_str::<Value>(&payload).ok())
        .context("native control chat was not materialized")?;
    let obj = chat
        .as_object_mut()
        .context("native control chat payload is not an object")?;
    obj.insert("open".to_string(), Value::Bool(true));
    obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    obj.insert(
        "lastTrackingId".to_string(),
        Value::String(command_id.to_string()),
    );
    let messages = obj
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .context("native control chat messages is not an array")?;
    let message_id = format!("status_{command_id}");
    let normalized_status = normalize_business_chat_tracking_status(status);
    let functional_status = first_string_field(result, &["status", "state", "outcome_status"])
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    let functional_failure = functional_status
        .as_deref()
        .is_some_and(business_chat_result_status_is_failure);
    let message_status = if functional_failure {
        "failed".to_string()
    } else {
        normalized_status.clone()
    };
    let text = first_string_field(result, &["summary", "message", "error"])
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if terminal {
                if functional_failure {
                    format!(
                        "Ausführung abgeschlossen. Fachliches Ergebnis: {}.",
                        functional_status.as_deref().unwrap_or("fehlgeschlagen")
                    )
                } else if normalized_status == "completed" {
                    "Aufgabe abgeschlossen.".to_string()
                } else {
                    "Aufgabe konnte nicht abgeschlossen werden.".to_string()
                }
            } else if normalized_status == "running" {
                "Aufgabe wird ausgeführt.".to_string()
            } else {
                "Aufgabe wurde angenommen.".to_string()
            }
        });
    if let Some(message) = messages
        .iter_mut()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(message_id.as_str()))
    {
        message["text"] = Value::String(text);
        message["commandId"] = Value::String(command_id.to_string());
        message["taskId"] = Value::String(String::new());
        message["status"] = Value::String(message_status);
        message["createdAt"] = Value::from(updated_at_ms);
    } else {
        messages.push(serde_json::json!({
            "id": message_id,
            "role": "ctox",
            "text": text,
            "commandId": command_id,
            "taskId": "",
            "status": message_status,
            "createdAt": updated_at_ms
        }));
    }
    if messages.len() > 40 {
        super::harness_cockpit::trim_messages(messages);
    }
    update_business_chat_tracking_fields(obj);
    upsert_business_record(
        conn,
        "business_chats",
        &chat_id,
        updated_at_ms,
        chat.clone(),
    )?;
    upsert_rxdb_collection_record(root, "business_chats", &chat_id, updated_at_ms, chat)?;
    Ok(chat_id)
}

fn business_chat_result_status_is_failure(status: &str) -> bool {
    matches!(
        status.trim().to_lowercase().as_str(),
        "failed"
            | "failure"
            | "error"
            | "blocked"
            | "rejected"
            | "unreachable"
            | "temporary_unreachable"
            | "unavailable"
            | "temporary_unavailable"
    )
}

pub(super) fn business_chat_payload(
    conn: &Connection,
    chat_id: &str,
    title: &str,
    owner_user_id: &str,
    user_message_id: &str,
    user_text: &str,
    command_id: &str,
    task_id: &str,
    reply_text: &str,
    updated_at_ms: i64,
) -> anyhow::Result<Value> {
    let mut chat = conn
        .query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'business_chats' AND record_id = ?1",
            params![chat_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|payload| serde_json::from_str::<Value>(&payload).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "id": chat_id,
                "title": title,
                "open": true,
                "minimized": false,
                "owner_user_id": owner_user_id,
                "lastTrackingId": task_id,
                "messages": [],
                "draft": "",
                "createdAt": updated_at_ms,
                "updated_at_ms": updated_at_ms
            })
        });

    let obj = chat
        .as_object_mut()
        .context("business chat payload is not an object")?;
    obj.insert("id".to_string(), Value::String(chat_id.to_string()));
    obj.entry("title".to_string())
        .or_insert_with(|| Value::String(title.to_string()));
    obj.insert("open".to_string(), Value::Bool(true));
    obj.entry("minimized".to_string())
        .or_insert_with(|| Value::Bool(false));
    let existing_owner = obj
        .get("owner_user_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    obj.insert(
        "owner_user_id".to_string(),
        Value::String(resolve_existing_business_chat_owner(
            &existing_owner,
            owner_user_id.to_string(),
        )),
    );
    obj.insert(
        "lastTrackingId".to_string(),
        Value::String(task_id.to_string()),
    );
    obj.entry("draft".to_string())
        .or_insert_with(|| Value::String(String::new()));
    obj.entry("createdAt".to_string())
        .or_insert_with(|| Value::from(updated_at_ms));
    obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));

    if !obj.get("messages").is_some_and(Value::is_array) {
        obj.insert("messages".to_string(), Value::Array(Vec::new()));
    }
    let messages = obj
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .context("business chat messages is not an array")?;

    if !user_text.trim().is_empty()
        && !messages
            .iter()
            .any(|item| item.get("id").and_then(Value::as_str) == Some(user_message_id))
    {
        messages.push(serde_json::json!({
            "id": user_message_id,
            "role": "user",
            "text": user_text,
            "createdAt": updated_at_ms.saturating_sub(1)
        }));
    }

    let reply_for = if task_id.is_empty() {
        command_id
    } else {
        task_id
    };
    if !messages
        .iter()
        .any(|item| item.get("replyFor").and_then(Value::as_str) == Some(reply_for))
    {
        messages.push(serde_json::json!({
            "id": format!("reply_{command_id}"),
            "role": "ctox",
            "kind": "reply", "task_id": task_id, "command_id": command_id, "run_id": null,
            "text": reply_text,
            "replyFor": reply_for,
            "commandId": command_id,
            "taskId": task_id,
            "status": "completed",
            "createdAt": updated_at_ms
        }));
    }

    if messages.len() > 40 {
        super::harness_cockpit::trim_messages(messages);
    }

    update_business_chat_tracking_fields(obj);
    Ok(chat)
}

fn update_business_chat_tracking_fields(obj: &mut serde_json::Map<String, Value>) {
    let summary = business_chat_tracking_summary(
        obj.get("messages")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );
    obj.insert("tracking_active".to_string(), Value::Bool(summary.active));
    obj.insert("tracking_status".to_string(), Value::String(summary.status));
    obj.insert(
        "tracking_id".to_string(),
        Value::String(summary.tracking_id),
    );
    obj.insert(
        "tracking_command_id".to_string(),
        Value::String(summary.command_id),
    );
    obj.insert(
        "tracking_task_id".to_string(),
        Value::String(summary.task_id),
    );
    obj.insert(
        "tracking_message_id".to_string(),
        Value::String(summary.message_id),
    );
}

struct BusinessChatTrackingSummary {
    active: bool,
    status: String,
    tracking_id: String,
    command_id: String,
    task_id: String,
    message_id: String,
}

fn business_chat_tracking_summary(messages: &[Value]) -> BusinessChatTrackingSummary {
    for message in messages.iter().rev() {
        let Some(object) = message.as_object() else {
            continue;
        };
        let trackable = object
            .get("trackable")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let command_id = object
            .get("commandId")
            .or_else(|| object.get("command_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string();
        let task_id = object
            .get("taskId")
            .or_else(|| object.get("task_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string();
        if command_id.is_empty() && task_id.is_empty() {
            continue;
        }
        let status = object
            .get("status")
            .and_then(Value::as_str)
            .map(normalize_business_chat_tracking_status)
            .unwrap_or_else(|| "queued".to_string());
        let message_id = object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string();
        return BusinessChatTrackingSummary {
            active: trackable && business_chat_tracking_status_is_active(&status),
            status,
            tracking_id: if task_id.is_empty() {
                command_id.clone()
            } else {
                task_id.clone()
            },
            command_id,
            task_id,
            message_id,
        };
    }
    BusinessChatTrackingSummary {
        active: false,
        status: String::new(),
        tracking_id: String::new(),
        command_id: String::new(),
        task_id: String::new(),
        message_id: String::new(),
    }
}

fn normalize_business_chat_tracking_status(status: &str) -> String {
    match status.trim().to_lowercase().as_str() {
        "accepted" | "pending" | "pending_sync" | "waiting" => "queued".to_string(),
        "processing" | "executing" | "active" | "working" | "leased" => "running".to_string(),
        "success" | "done" | "erledigt" => "completed".to_string(),
        "error" => "failed".to_string(),
        value if value.is_empty() => "queued".to_string(),
        value => value.to_string(),
    }
}

fn business_chat_tracking_status_is_active(status: &str) -> bool {
    matches!(status, "queued" | "running")
}

pub(super) fn refresh_queue_task_projection(
    root: &Path,
    conn: &Connection,
    rxdb_writers: Option<&mut RxdbProjectionWriterCache>,
    command_id: &str,
    command: &BusinessCommand,
    original_task: Option<&channels::QueueTaskView>,
    updated_at_ms: i64,
) -> anyhow::Result<()> {
    let Some(task_id) = original_task
        .map(|task| task.message_key.clone())
        .or_else(|| find_queue_task_for_command(root, command_id))
    else {
        return Ok(());
    };
    let Some(task) = channels::load_queue_task(root, &task_id)? else {
        return Ok(());
    };
    let inbound_channel = command_inbound_channel(command);
    let structured_status =
        channels::inspect_business_command_for_task(root, &task_id)?.and_then(|context| {
            context
                .pointer("/command/terminal_status")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let mut payload = business_command_queue_task_payload(
        command_id,
        command,
        &task,
        &inbound_channel,
        structured_status.as_deref(),
        updated_at_ms,
    );
    if let Some(progress) = crate::lcm::run_task_execution_progress_for_task(
        &crate::paths::core_db(root),
        &task.message_key,
    )? {
        if let Some(object) = payload.as_object_mut() {
            object.insert("execution_progress".to_string(), progress);
        }
    }
    upsert_business_record(
        conn,
        "ctox_queue_tasks",
        &task.message_key,
        updated_at_ms,
        payload.clone(),
    )?;
    upsert_rxdb_collection_record_cached(
        root,
        rxdb_writers,
        "ctox_queue_tasks",
        &task.message_key,
        updated_at_ms,
        payload,
    )
}

pub(super) fn business_command_queue_task_payload(
    command_id: &str,
    command: &BusinessCommand,
    task: &channels::QueueTaskView,
    inbound_channel: &str,
    structured_status: Option<&str>,
    updated_at_ms: i64,
) -> Value {
    let route_status = effective_queue_projection_route_status(task, structured_status);
    let mut payload = serde_json::json!({
        "id": task.message_key,
        "command_id": command_id,
        "title": task.title,
        "status": normalize_queue_status(&route_status),
        "route_status": route_status,
        "module": "ctox",
        "source_module": command.module.clone(),
        "inbound_channel": inbound_channel,
        "command_type": command.command_type.clone(),
        "priority": task.priority,
        "thread_key": task.thread_key,
        "prompt": task.prompt,
        "workspace_root": task.workspace_root,
        "updated_at_ms": updated_at_ms
    });
    enrich_queue_projection_payload(&mut payload, task, &route_status);
    if let Some(artifact) = browser_context_artifact_for_command(command) {
        if let Some(object) = payload.as_object_mut() {
            object.insert("browser_context_artifact".to_string(), artifact);
        }
    }
    payload
}

pub(super) fn write_queue_task_projection(
    conn: &Connection,
    command_id: Option<&str>,
    task: &channels::QueueTaskView,
    updated_at_ms: i64,
) -> anyhow::Result<()> {
    let structured_status =
        queue_projection_structured_status(conn, command_id, &task.message_key)?;
    upsert_business_record(
        conn,
        "ctox_queue_tasks",
        &task.message_key,
        updated_at_ms,
        queue_task_payload(
            command_id,
            task,
            structured_status.as_deref(),
            updated_at_ms,
        ),
    )
}

pub(super) fn queue_task_payload(
    command_id: Option<&str>,
    task: &channels::QueueTaskView,
    structured_status: Option<&str>,
    updated_at_ms: i64,
) -> Value {
    let route_status = effective_queue_projection_route_status(task, structured_status);
    let mut payload = serde_json::json!({
        "id": task.message_key,
        "command_id": command_id.unwrap_or_default(),
        "title": task.title,
        "status": normalize_queue_status(&route_status),
        "route_status": route_status,
        "module": "ctox",
        "source_module": "ctox",
        "inbound_channel": "business_os.llm.chat",
        "command_type": "business_os.chat.task",
        "priority": task.priority,
        "thread_key": task.thread_key,
        "prompt": task.prompt,
        "workspace_root": task.workspace_root,
        "updated_at_ms": updated_at_ms
    });
    enrich_queue_projection_payload(&mut payload, task, &route_status);
    payload
}

pub(super) fn queue_projection_structured_status(
    conn: &Connection,
    command_id: Option<&str>,
    task_id: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(command_id) = command_id {
        let command_status = conn
            .query_row(
                "SELECT status FROM business_commands WHERE command_id = ?1",
                params![command_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if command_status.as_deref().is_some_and(|status| {
            queue_status_is_terminal_success(Some(status))
                || queue_status_is_terminal_failure(Some(status))
        }) {
            return Ok(command_status);
        }
    }
    let projection_status = conn
        .query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'ctox_queue_tasks' AND record_id = ?1 AND deleted = 0",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|payload| serde_json::from_str::<Value>(&payload).ok())
        .and_then(|payload| {
            structured_terminal_status_from_projection(&payload).map(str::to_string)
        });
    Ok(projection_status)
}

fn effective_queue_projection_route_status(
    task: &channels::QueueTaskView,
    structured_status: Option<&str>,
) -> String {
    if task.route_status == "leased"
        && (queue_status_is_terminal_success(structured_status)
            || queue_status_is_terminal_success(Some(&task.route_status)))
    {
        return "handled".to_string();
    }
    if task.route_status == "leased"
        && (queue_status_is_terminal_failure(structured_status)
            || queue_status_is_terminal_failure(Some(&task.route_status)))
    {
        return "failed".to_string();
    }
    // F-002 status coherence: a `leased` route without a durable owner or
    // lease timestamp is an orphaned/incomplete lease — no live worker can
    // own it. Surface it as failed/stalled, never as healthy progress.
    if task.route_status == "leased"
        && (task
            .lease_owner
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
            || task
                .leased_at
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none())
    {
        return "failed".to_string();
    }
    task.route_status.clone()
}

pub(super) fn enrich_queue_projection_payload(
    payload: &mut Value,
    task: &channels::QueueTaskView,
    route_status: &str,
) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    object.insert(
        "message_key".to_string(),
        Value::String(task.message_key.clone()),
    );
    object.insert(
        "status".to_string(),
        Value::String(normalize_queue_status(route_status).to_string()),
    );
    object.insert(
        "route_status".to_string(),
        Value::String(route_status.to_string()),
    );
    object.insert(
        "task_status".to_string(),
        Value::String(normalize_queue_status(route_status).to_string()),
    );
    // Explicit nulls clear prior values in the merging RxDB writer.
    for (key, value) in [
        ("lease_expires_at", &task.lease_expires_at),
        ("lease_worker_id", &task.lease_worker_id),
        ("first_pending_at", &task.first_pending_at),
        ("failure_class", &task.failure_class),
        ("retry_not_before", &task.retry_not_before),
        ("hold_reason", &task.hold_reason),
        ("wait_entity_type", &task.wait_entity_type),
        ("wait_entity_id", &task.wait_entity_id),
        ("crew_member_id", &task.crew_member_id),
        ("crew_assigned_member_id", &task.crew_assigned_member_id),
    ] {
        object.insert(key.to_string(), serde_json::json!(value));
    }
    // Ticket-born work carries its ticket key so the Tickets app can show
    // which member holds it and why it waits.
    object.insert(
        "ticket_key".to_string(),
        serde_json::json!(task
            .metadata
            .get("ticket_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())),
    );
    object.insert(
        "failure_attempt_count".into(),
        Value::from(task.failure_attempt_count),
    );
    object.insert(
        "priority_time_credit_hours".into(),
        Value::from(task.priority_time_credit_hours),
    );
    object.insert("attempt".into(), Value::from(task.attempt));
    if let Some(note) = task
        .status_note
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        object.insert("status_note".to_string(), Value::String(note.to_string()));
        if route_status == "failed" {
            object.insert("error".to_string(), Value::String(note.to_string()));
        }
    } else {
        object.remove("status_note");
        if route_status != "failed" {
            object.remove("error");
        }
    }
    if let Some(owner) = task
        .lease_owner
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        object.insert("lease_owner".to_string(), Value::String(owner.to_string()));
    } else {
        // Native projection writes merge into an existing document. Omitting
        // cleared lease fields would retain the previous worker indefinitely.
        object.insert("lease_owner".to_string(), Value::Null);
    }
    if let Some(leased_at) = task
        .leased_at
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        object.insert(
            "leased_at".to_string(),
            Value::String(leased_at.to_string()),
        );
    } else {
        object.insert("leased_at".to_string(), Value::Null);
    }
    if let Some(acked_at) = task
        .acked_at
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        object.insert("acked_at".to_string(), Value::String(acked_at.to_string()));
    } else {
        object.insert("acked_at".to_string(), Value::Null);
    }
}

pub(super) fn queue_projection_command_id(
    conn: &Connection,
    task_id: &str,
) -> anyhow::Result<Option<String>> {
    let value = conn
        .query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'ctox_queue_tasks' AND record_id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(value
        .and_then(|payload| serde_json::from_str::<Value>(&payload).ok())
        .and_then(|payload| {
            payload
                .get("command_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        }))
}

fn structured_terminal_status_from_projection(payload: &Value) -> Option<&str> {
    ["terminal_status", "route_status", "status", "task_status"]
        .into_iter()
        .filter_map(|field| payload.get(field).and_then(Value::as_str))
        .find(|status| {
            queue_status_is_terminal_success(Some(status))
                || queue_status_is_terminal_failure(Some(status))
        })
}

pub(super) fn normalize_queue_status(route_status: &str) -> &str {
    match route_status {
        "pending" => "queued",
        "leased" => "running",
        "handled" => "completed",
        "cancelled" => "cancelled",
        "blocked" => "blocked",
        "failed" => "failed",
        other => other,
    }
}

pub(super) fn queue_projection_execution_phase(
    route_status: &str,
    canonical_phase: Option<String>,
) -> String {
    match route_status {
        "handled" | "failed" | "cancelled" => "terminal".to_string(),
        "leased" => "leased".to_string(),
        "running" => "running".to_string(),
        "blocked" => "blocked".to_string(),
        "pending" => match canonical_phase.as_deref() {
            Some("accepted" | "queued" | "retry_wait" | "waiting_dependencies") => {
                canonical_phase.unwrap_or_else(|| "queued".to_string())
            }
            _ => "queued".to_string(),
        },
        _ => canonical_phase.unwrap_or_else(|| "queued".to_string()),
    }
}

pub(super) fn queue_projection_terminal_status(route_status: &str) -> &str {
    match route_status {
        "handled" => "completed",
        "failed" => "failed",
        "cancelled" => "cancelled",
        _ => "none",
    }
}

pub(super) fn upsert_business_record(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    updated_at_ms: i64,
    mut payload: Value,
) -> anyhow::Result<()> {
    let rev = format!("rev_{}", Uuid::new_v4());
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("id".to_string(), Value::String(record_id.to_string()));
        obj.insert("_rev".to_string(), Value::String(rev.clone()));
        obj.insert("_deleted".to_string(), Value::Bool(false));
        obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    }
    // SECURITY: strip bearer credentials (capability_token, …) from client_context
    // before this record replicates to peers. The verified token is retained only
    // in the native business_commands.client_context_json column, never here.
    redact_document_client_context_secrets(&mut payload);
    conn.execute(
        "INSERT INTO business_records
            (collection, record_id, rev, deleted, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, 0, ?4, ?5)
         ON CONFLICT(collection, record_id) DO UPDATE SET
            rev = excluded.rev,
            deleted = excluded.deleted,
            updated_at_ms = excluded.updated_at_ms,
            payload_json = excluded.payload_json",
        params![
            collection,
            record_id,
            rev,
            updated_at_ms,
            serde_json::to_string(&payload)?
        ],
    )?;
    Ok(())
}

pub fn repair_queue_projections(
    root: &Path,
    options: QueueProjectionRepairOptions,
) -> anyhow::Result<Value> {
    let apply = options.apply;
    let conn = open_store(root)?;
    let now = now_ms() as i64;
    let retention = super::harness_cockpit::queue_retention(root)?;
    let core_path = crate::paths::core_db(root);
    let has_routing = if core_path.is_file() {
        let source = rusqlite::Connection::open_with_flags(
            &core_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        source.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='communication_routing_state')",[],|row|row.get::<_,bool>(0))?
    } else {
        false
    };
    // This fresh connection is local to the repair call. Dropping it detaches
    // cockpit_repair_core on success and on every error path; it is never pooled.
    if has_routing {
        conn.execute(
            "ATTACH DATABASE ?1 AS cockpit_repair_core",
            [core_path.to_string_lossy().as_ref()],
        )?;
    } else {
        conn.execute_batch("ATTACH DATABASE ':memory:' AS cockpit_repair_core; CREATE TABLE cockpit_repair_core.communication_routing_state(message_key TEXT PRIMARY KEY,route_status TEXT,updated_at TEXT);")?;
    }
    let projection_rows = {
        let mut statement = conn.prepare(
            "WITH eligible AS (
                SELECT message_key FROM cockpit_repair_core.communication_routing_state WHERE route_status NOT IN ('handled','failed','cancelled')
                UNION ALL
                SELECT message_key FROM (SELECT message_key FROM cockpit_repair_core.communication_routing_state WHERE route_status IN ('handled','failed','cancelled') ORDER BY updated_at DESC,message_key DESC LIMIT ?1)
                UNION ALL
                SELECT record_id FROM (SELECT record_id FROM business_records p WHERE collection='ctox_queue_tasks' AND deleted=0 AND NOT EXISTS(SELECT 1 FROM cockpit_repair_core.communication_routing_state r WHERE r.message_key=p.record_id) ORDER BY updated_at_ms DESC,record_id DESC LIMIT ?1)
             )
             SELECT record_id, payload_json, updated_at_ms
             FROM business_records
             WHERE collection = 'ctox_queue_tasks' AND deleted = 0 AND record_id IN (SELECT message_key FROM eligible)
             ORDER BY updated_at_ms ASC, record_id ASC
             LIMIT (SELECT COUNT(*) FROM eligible)",
        )?;
        let rows = statement.query_map([retention], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut counters: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut actions: Vec<Value> = Vec::new();
    let mut touched_commands = HashSet::new();
    let mut rxdb_writers = RxdbProjectionWriterCache::new(root);

    let scanned_task_projections = projection_rows.len();
    for (task_id, payload_json, projection_updated_at_ms) in projection_rows {
        let mut payload = serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| {
            serde_json::json!({
                "id": task_id,
                "status": "queued",
                "route_status": "pending"
            })
        });
        let command_id = payload
            .get("command_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| queue_projection_command_id(&conn, &task_id).ok().flatten());
        let projection_route_status = payload
            .get("route_status")
            .and_then(Value::as_str)
            .or_else(|| payload.get("status").and_then(Value::as_str))
            .unwrap_or_default()
            .to_string();
        let projection_status = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let projection_task_status = payload
            .get("task_status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        match channels::load_queue_task(root, &task_id)? {
            Some(task) => {
                let desired_route_status = task.route_status.clone();
                let fallback_error_note =
                    if desired_route_status == "failed" && task.status_note.is_none() {
                        channels::load_queue_task_last_error(root, &task_id)?
                    } else {
                        None
                    };
                let canonical_note = task
                    .status_note
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .or_else(|| fallback_error_note.as_deref());

                let desired_status = normalize_queue_status(&desired_route_status).to_string();
                let needs_projection_repair = projection_route_status != desired_route_status
                    || projection_status != desired_status
                    || projection_task_status != desired_status;
                if needs_projection_repair && desired_route_status == "pending" {
                    let class = "queued_from_canonical";
                    *counters.entry(class).or_insert(0) += 1;
                    push_repair_action(
                        &mut actions,
                        class,
                        &task_id,
                        command_id.as_deref(),
                        &projection_route_status,
                        &desired_route_status,
                        canonical_note,
                    );
                    if apply {
                        payload = apply_queue_projection_status_fields(
                            payload,
                            &task,
                            &desired_route_status,
                            now,
                        );
                        if let Some(object) = payload.as_object_mut() {
                            object.insert(
                                "repair_note".to_string(),
                                Value::String(
                                    "queue projection repaired from canonical route_status=pending"
                                        .to_string(),
                                ),
                            );
                        }
                        upsert_business_record(
                            &conn,
                            "ctox_queue_tasks",
                            &task_id,
                            now,
                            payload.clone(),
                        )?;
                        rxdb_writers.upsert("ctox_queue_tasks", &task_id, now, payload)?;
                    }
                }
            }
            None => {
                let command_status = command_id.as_deref().and_then(|command_id| {
                    conn.query_row(
                        "SELECT status FROM business_commands WHERE command_id = ?1",
                        params![command_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .ok()
                    .flatten()
                });
                if let Some(route_status) = command_status
                    .as_deref()
                    .and_then(projection_route_status_for_command_status)
                {
                    let desired_status = normalize_queue_status(route_status).to_string();
                    if projection_route_status != route_status
                        || projection_status != desired_status
                        || projection_task_status != desired_status
                    {
                        *counters
                            .entry("projection_repaired_from_command")
                            .or_insert(0) += 1;
                        push_repair_action(
                            &mut actions,
                            "projection_repaired_from_command",
                            &task_id,
                            command_id.as_deref(),
                            &projection_route_status,
                            route_status,
                            None,
                        );
                        if apply {
                            if let Some(object) = payload.as_object_mut() {
                                object.insert("status".to_string(), Value::String(desired_status));
                                object.insert(
                                    "route_status".to_string(),
                                    Value::String(route_status.to_string()),
                                );
                                object.insert(
                                    "task_status".to_string(),
                                    Value::String(normalize_queue_status(route_status).to_string()),
                                );
                                object.insert("updated_at_ms".to_string(), Value::from(now));
                                object.insert(
                                    "repair_note".to_string(),
                                    Value::String(
                                        "queue projection repaired from terminal command status"
                                            .to_string(),
                                    ),
                                );
                            }
                            upsert_business_record(
                                &conn,
                                "ctox_queue_tasks",
                                &task_id,
                                now,
                                payload.clone(),
                            )?;
                            rxdb_writers.upsert("ctox_queue_tasks", &task_id, now, payload)?;
                        }
                    }
                } else if projection_status_is_active(&projection_status)
                    && now.saturating_sub(projection_updated_at_ms)
                        > BUSINESS_OS_QUEUE_ORPHAN_REPAIR_AGE_MS
                {
                    let error = "Queue task is no longer present in the CTOX durable queue; marking stale Business OS projection as failed.";
                    *counters.entry("orphaned_active_projection").or_insert(0) += 1;
                    push_repair_action(
                        &mut actions,
                        "orphaned_active_projection",
                        &task_id,
                        command_id.as_deref(),
                        &projection_route_status,
                        "failed",
                        Some(error),
                    );
                    if apply {
                        if let Some(object) = payload.as_object_mut() {
                            object
                                .insert("status".to_string(), Value::String("failed".to_string()));
                            object.insert(
                                "route_status".to_string(),
                                Value::String("failed".to_string()),
                            );
                            object.insert(
                                "task_status".to_string(),
                                Value::String("failed".to_string()),
                            );
                            object.insert("error".to_string(), Value::String(error.to_string()));
                            object.insert("updated_at_ms".to_string(), Value::from(now));
                            object.insert(
                                "repair_note".to_string(),
                                Value::String(
                                    "orphaned active queue projection failed".to_string(),
                                ),
                            );
                        }
                        upsert_business_record(
                            &conn,
                            "ctox_queue_tasks",
                            &task_id,
                            now,
                            payload.clone(),
                        )?;
                        rxdb_writers.upsert("ctox_queue_tasks", &task_id, now, payload)?;
                        if let Some(command_id) = command_id.as_deref() {
                            touched_commands.insert(command_id.to_string());
                            upsert_command_projection_from_queue_status(
                                root,
                                &conn,
                                Some(&mut rxdb_writers),
                                command_id,
                                None,
                                "failed",
                                now,
                                Some(error),
                            )?;
                        }
                    }
                }
            }
        }
    }

    repair_placeholder_business_chat_owners(
        root,
        &conn,
        &mut rxdb_writers,
        apply,
        now,
        &mut counters,
        &mut actions,
    )?;

    let redacted = repair_inline_payload_artifacts(root, &conn, apply, now)?;
    if redacted > 0 {
        counters.insert("oversized_inline_artifacts_redacted", redacted);
    }
    let legacy_records = count_legacy_http_fallback_records(&conn)?;
    if legacy_records > 0 {
        counters.insert("legacy_http_fallback_records", legacy_records);
    }

    Ok(serde_json::json!({
        "ok": true,
        "apply": apply,
        "queue_tasks_retention": retention,
        "scanned_task_projections": scanned_task_projections,
        "counts": counters,
        "actions": actions,
        "touched_commands": touched_commands.into_iter().collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::store::{
        accept_rxdb_business_command, load_rxdb_collection_record, now_ms, open_store,
        queue_status_is_terminal_success, rxdb_store_path, set_rxdb_chat_owner_repair_interleave,
        BusinessCommand, CommandOrigin,
    };
    use super::{
        business_chat_owner_user_id, business_chat_result_status_is_failure,
        effective_queue_projection_route_status, repair_queue_projections,
        resolve_existing_business_chat_owner, upsert_business_record, QueueProjectionRepairOptions,
    };
    use crate::mission::channels;
    use anyhow::Context;
    use rusqlite::{params, Connection};
    use serde_json::{json, Value};
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    pub(crate) fn create_repair_rxdb_tables(root: &Path) -> anyhow::Result<Connection> {
        fs::create_dir_all(root.join("runtime"))?;
        let conn = Connection::open(rxdb_store_path(root))?;
        conn.execute(
            "CREATE TABLE ctox_business_os__business_chats__v0 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE ctox_business_os__ctox_queue_tasks__v0 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE ctox_business_os__business_commands__v1 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE ctox_business_os__research_tasks__v0 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE ctox_business_os__research_runs__v0 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE ctox_business_os__knowledge_tables__v0 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            )",
            [],
        )?;
        Ok(conn)
    }

    pub(crate) fn insert_rxdb_test_record(
        conn: &Connection,
        table: &str,
        id: &str,
        payload: Value,
    ) -> anyhow::Result<()> {
        insert_rxdb_test_record_with_deleted(conn, table, id, payload, false)
    }

    pub(crate) fn insert_rxdb_test_record_with_deleted(
        conn: &Connection,
        table: &str,
        id: &str,
        payload: Value,
        deleted: bool,
    ) -> anyhow::Result<()> {
        conn.execute(
            &format!(
                "INSERT INTO {table} (id, revision, deleted, lastWriteTime, data)
                 VALUES (?1, 'rev_stale', ?2, 1.0, ?3)"
            ),
            params![
                id,
                if deleted { 1 } else { 0 },
                serde_json::to_string(&payload)?
            ],
        )?;
        Ok(())
    }

    fn chat_owner_command(client_context: Value) -> BusinessCommand {
        BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some("cmd-chat-owner".into()),
            module: "ctox".into(),
            command_type: "business_os.chat.task".into(),
            record_id: Some("chat-owner".into()),
            payload: json!({}),
            client_context,
        }
    }

    #[test]
    fn business_chat_owner_user_id_reads_owner_and_actor_user_id() {
        assert_eq!(
            business_chat_owner_user_id(&chat_owner_command(json!({"owner": "user-1"}))),
            "user-1"
        );
        assert_eq!(
            business_chat_owner_user_id(&chat_owner_command(
                json!({"actor": {"user_id": "user-2"}})
            )),
            "user-2"
        );
        assert_eq!(
            business_chat_owner_user_id(&chat_owner_command(json!({"actor": {"id": "user-3"}}))),
            "user-3"
        );
        assert_eq!(
            business_chat_owner_user_id(&chat_owner_command(json!({"actor": "user-4"}))),
            "user-4"
        );
        assert_eq!(
            business_chat_owner_user_id(&chat_owner_command(json!({}))),
            "local-dev"
        );
    }

    #[test]
    fn business_chat_owner_skips_placeholder_candidates_before_actor_id() {
        assert_eq!(
            business_chat_owner_user_id(&chat_owner_command(json!({
                "owner_user_id": "local-dev",
                "actor": { "id": "user-1" }
            }))),
            "user-1"
        );
        assert_eq!(
            business_chat_owner_user_id(&chat_owner_command(json!({
                "owner_user_id": "local-dev",
                "actor": { "user_id": "user-2" }
            }))),
            "user-2"
        );
    }

    #[test]
    fn business_chat_owner_does_not_clobber_or_transfer_a_real_owner() {
        assert_eq!(
            resolve_existing_business_chat_owner("user-1", "local-dev".to_string()),
            "user-1"
        );
        assert_eq!(
            resolve_existing_business_chat_owner("local-dev", "user-1".to_string()),
            "local-dev",
            "projection must not adopt an existing placeholder chat"
        );
        assert_eq!(
            resolve_existing_business_chat_owner("user-1", "user-2".to_string()),
            "user-1",
            "a real owner must not transfer to another real actor via projection"
        );
        assert_eq!(
            resolve_existing_business_chat_owner("", "local-dev".to_string()),
            "",
            "an existing empty owner stays unattributed"
        );
        assert_eq!(
            business_chat_owner_user_id(&chat_owner_command(json!({
                "owner_user_id": "local-dev",
                "contextMeta": {
                    "client_context": { "owner": "user-9", "actor": { "user_id": "user-9" } }
                }
            }))),
            "local-dev",
            "stored chat client_context is not a trusted owner receipt"
        );
    }

    #[test]
    fn business_chat_marks_functional_adapter_failures_as_failures() {
        assert!(business_chat_result_status_is_failure(
            "temporary_unreachable"
        ));
        assert!(business_chat_result_status_is_failure("FAILED"));
        assert!(!business_chat_result_status_is_failure("completed"));
        assert!(!business_chat_result_status_is_failure("ready"));
    }

    #[test]
    fn queue_status_detection_ignores_status_note_wording() {
        let mut task = channels::QueueTaskView {
            message_key: "queue:system::structured-status-wording".to_string(),
            thread_key: "queue/structured-status-wording".to_string(),
            title: "Structured status wording test".to_string(),
            prompt: "Use the structured terminal state.".to_string(),
            workspace_root: None,
            ticket_self_work_id: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            metadata: Value::Null,
            route_status: "leased".to_string(),
            status_note: Some(
                "Business-OS documents bug report completed. Changed editor rendering. Verified in browser."
                    .to_string(),
            ),
            lease_owner: Some("ctox-service".to_string()),
            leased_at: Some("2026-08-01T00:00:00Z".to_string()),
            acked_at: None,
            created_at: "2026-08-01T00:00:00Z".to_string(),
            sort_at: "2026-08-01T00:00:00Z".to_string(),
            updated_at: "2026-08-01T00:00:00Z".to_string(),
            lease_expires_at: None,
            lease_worker_id: None,
            first_pending_at: None,
            failure_class: None,
            failure_attempt_count: 0,
            retry_not_before: None,
            hold_reason: None,
            wait_entity_type: None,
            wait_entity_id: None,
            priority_time_credit_hours: 0,
            attempt: 0,
            crew_member_id: None,
            crew_assigned_member_id: None,
        };

        let success_with_old_wording =
            effective_queue_projection_route_status(&task, Some("completed"));
        task.status_note =
            Some("Erledigt; Nachweis liegt im strukturierten Ergebnisfeld.".to_string());
        let success_with_new_wording =
            effective_queue_projection_route_status(&task, Some("completed"));
        assert_eq!(success_with_old_wording, "handled");
        assert_eq!(success_with_new_wording, success_with_old_wording);

        task.status_note = Some("turn/start failed".to_string());
        let failure_with_old_wording =
            effective_queue_projection_route_status(&task, Some("failed"));
        task.status_note = Some("Ausführung beendet; Details stehen im Fehlerobjekt.".to_string());
        let failure_with_new_wording =
            effective_queue_projection_route_status(&task, Some("failed"));
        assert_eq!(failure_with_old_wording, "failed");
        assert_eq!(failure_with_new_wording, failure_with_old_wording);

        task.status_note = Some("terminal-success completed. Changed and verified.".to_string());
        assert_eq!(
            effective_queue_projection_route_status(&task, Some("running")),
            "leased",
            "terminal-looking prose must not override a non-terminal structured status"
        );
    }

    #[test]
    fn repair_dry_run_bounds_legacy_orphan_candidates_without_mutating_views() -> anyhow::Result<()>
    {
        let root = tempdir()?;
        crate::inference::runtime_env::set_runtime_env_value(
            root.path(),
            super::super::harness_cockpit::QUEUE_RETENTION_KEY,
            "3",
        )?;
        let conn = open_store(root.path())?;
        for index in 0..20 {
            let id = format!("legacy-{index:03}");
            upsert_business_record(
                &conn,
                "ctox_queue_tasks",
                &id,
                index,
                serde_json::json!({"id":id,"status":"failed","route_status":"failed","task_status":"failed","updated_at_ms":index}),
            )?;
        }
        let report =
            repair_queue_projections(root.path(), QueueProjectionRepairOptions { apply: false })?;
        assert_eq!(report["scanned_task_projections"], 3);
        assert_eq!(report["queue_tasks_retention"], 3);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM business_records WHERE collection='ctox_queue_tasks' AND deleted=0",[],|r|r.get::<_,i64>(0))?,20);
        Ok(())
    }

    #[test]
    fn repair_dry_run_accepts_initialized_core_without_queue_schema() -> anyhow::Result<()> {
        let root = tempdir()?;
        let core_path = crate::paths::core_db(root.path());
        std::fs::create_dir_all(core_path.parent().unwrap())?;
        let core = rusqlite::Connection::open(&core_path)?;
        core.execute_batch("CREATE TABLE startup_marker(id INTEGER PRIMARY KEY);")?;
        let report =
            repair_queue_projections(root.path(), QueueProjectionRepairOptions { apply: false })?;
        assert_eq!(report["ok"], true);
        assert_eq!(report["scanned_task_projections"], 0);
        assert_eq!(
            core.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='communication_routing_state'",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            0,
            "dry-run must not initialize canonical queue state"
        );
        Ok(())
    }

    #[test]
    fn canonical_queue_ack_refreshes_queue_and_command_without_repair() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let accepted = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_canonical_ack_refresh",
                "command_id": "cmd_canonical_ack_refresh",
                "module": "research",
                "command_type": "business_os.chat.task",
                "record_id": "research",
                "status": "pending_sync",
                "payload": {
                    "title": "Kontext-Aufgabe · Web Research",
                    "instruction": "teste kanonischen refresh",
                    "prompt": "teste kanonischen refresh"
                },
                "client_context": {
                    "source": "business-os-chat",
                    "module": "research"
                }
            }),
        )?;
        let task_id = accepted
            .get("task_id")
            .and_then(Value::as_str)
            .context("expected queue task id")?
            .to_string();
        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__ctox_queue_tasks__v0",
            &task_id,
            serde_json::json!({
                "id": task_id,
                "command_id": "cmd_canonical_ack_refresh",
                "status": "queued",
                "route_status": "pending",
                "task_status": "queued",
                "updated_at_ms": 1
            }),
        )?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__business_commands__v1",
            "cmd_canonical_ack_refresh",
            serde_json::json!({
                "id": "cmd_canonical_ack_refresh",
                "command_id": "cmd_canonical_ack_refresh",
                "status": "accepted",
                "route_status": "pending",
                "task_status": "queued",
                "updated_at_ms": 1
            }),
        )?;
        drop(rxdb_conn);

        channels::lease_queue_task(root, &task_id, "ctox-service")?;
        let conn = open_store(root)?;
        let leased_payload: Value = serde_json::from_str(&conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'ctox_queue_tasks' AND record_id = ?1",
            params![task_id.as_str()],
            |row| row.get::<_, String>(0),
        )?)?;
        assert_eq!(
            leased_payload.get("route_status").and_then(Value::as_str),
            Some("leased")
        );
        assert_eq!(
            leased_payload.get("status").and_then(Value::as_str),
            Some("running")
        );
        drop(conn);

        channels::ack_leased_messages_with_failure_reason(
            root,
            std::slice::from_ref(&task_id),
            "failed",
            "Input exceeds the maximum length of 1048576 characters.",
        )?;

        let conn = open_store(root)?;
        let queue_projection: Value = serde_json::from_str(&conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'ctox_queue_tasks' AND record_id = ?1",
            params![task_id.as_str()],
            |row| row.get::<_, String>(0),
        )?)?;
        assert_eq!(
            queue_projection.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            queue_projection.get("route_status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            queue_projection.get("task_status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            queue_projection.get("error").and_then(Value::as_str),
            Some("Input exceeds the maximum length of 1048576 characters.")
        );

        let command_projection: Value = serde_json::from_str(&conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'business_commands' AND record_id = 'cmd_canonical_ack_refresh'",
            [],
            |row| row.get::<_, String>(0),
        )?)?;
        assert_eq!(
            command_projection.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            command_projection
                .get("task_status")
                .and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            command_projection.get("error").and_then(Value::as_str),
            Some("Input exceeds the maximum length of 1048576 characters.")
        );
        drop(conn);

        let rxdb_queue = load_rxdb_collection_record(root, "ctox_queue_tasks", &task_id)?
            .context("expected refreshed RxDB queue projection")?;
        assert_eq!(
            rxdb_queue.get("route_status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            rxdb_queue.get("task_status").and_then(Value::as_str),
            Some("failed")
        );
        let rxdb_command =
            load_rxdb_collection_record(root, "business_commands", "cmd_canonical_ack_refresh")?
                .context("expected refreshed RxDB command projection")?;
        assert_eq!(
            rxdb_command.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            rxdb_command.get("task_status").and_then(Value::as_str),
            Some("failed")
        );
        Ok(())
    }

    #[test]
    fn repair_queue_projections_no_longer_owns_running_canonical_refresh() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let accepted = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_no_running_repair",
                "command_id": "cmd_no_running_repair",
                "module": "research",
                "command_type": "business_os.chat.task",
                "record_id": "research",
                "status": "pending_sync",
                "payload": {
                    "title": "No running repair",
                    "instruction": "running is refreshed by the mutation",
                    "prompt": "running is refreshed by the mutation"
                },
                "client_context": {
                    "source": "business-os-chat",
                    "module": "research"
                }
            }),
        )?;
        let task_id = accepted
            .get("task_id")
            .and_then(Value::as_str)
            .context("expected queue task id")?
            .to_string();
        channels::lease_queue_task(root, &task_id, "ctox-service")?;

        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "ctox_queue_tasks",
            &task_id,
            now_ms() as i64,
            serde_json::json!({
                "id": task_id,
                "command_id": "cmd_no_running_repair",
                "status": "completed",
                "route_status": "handled",
                "task_status": "completed"
            }),
        )?;
        drop(conn);

        let dry_run =
            repair_queue_projections(root, QueueProjectionRepairOptions { apply: false })?;
        assert!(dry_run.pointer("/counts/running_from_canonical").is_none());
        repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;

        let conn = open_store(root)?;
        let payload: Value = serde_json::from_str(&conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'ctox_queue_tasks' AND record_id = ?1",
            params![task_id.as_str()],
            |row| row.get::<_, String>(0),
        )?)?;
        assert_eq!(
            payload.get("route_status").and_then(Value::as_str),
            Some("handled"),
            "historical non-pending mismatches require an explicit migration, not repair_queue_projections"
        );
        Ok(())
    }

    fn seed_completed_chat_with_receipt(
        conn: &Connection,
        chat_id: &str,
        command_id: &str,
        event_id: &str,
        owner: &str,
        deleted: bool,
        payload_chat_id: &str,
        messages: Value,
    ) -> anyhow::Result<()> {
        let now = now_ms() as i64;
        conn.execute(
            "INSERT INTO business_records
                (collection, record_id, rev, deleted, updated_at_ms, payload_json)
             VALUES ('business_chats', ?1, 'rev_chat', ?2, ?3, ?4)",
            params![
                chat_id,
                if deleted { 1 } else { 0 },
                now,
                serde_json::to_string(&serde_json::json!({
                    "id": chat_id,
                    "owner_user_id": "local-dev",
                    "title": "Public answer",
                    "minimized": false,
                    "messages": messages,
                    "tracking_status": "completed"
                }))?
            ],
        )?;
        conn.execute(
            "INSERT INTO business_commands
                (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
             VALUES (?1, 'ctox', 'business_os.chat.task', ?2, 'completed', ?3, ?4, ?5)",
            params![
                command_id,
                payload_chat_id,
                serde_json::to_string(&serde_json::json!({
                    "reply_to": payload_chat_id,
                    "chat_id": payload_chat_id,
                    "title": "Public answer"
                }))?,
                serde_json::to_string(&serde_json::json!({
                    "source": "business-os-chat"
                }))?,
                now
            ],
        )?;
        conn.execute(
            "INSERT INTO business_events
                (event_id, collection, record_id, command_type, payload_json, observed_at_ms)
             VALUES (?1, 'business_commands', ?2, 'business_os.policy.allowed', ?3, ?4)",
            params![
                event_id,
                command_id,
                serde_json::to_string(&serde_json::json!({
                    "event_type": "business_os.policy.allowed",
                    "command_id": command_id,
                    "actor": { "id": owner, "trusted": true }
                }))?,
                now
            ],
        )?;
        Ok(())
    }

    #[test]
    fn repair_queue_projections_repairs_placeholder_chat_owner_from_trusted_receipt(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let now = now_ms() as i64;
        let conn = open_store(root)?;
        let messages = serde_json::json!([
            {"id": "chatmsg-public", "role": "user", "text": "What is the answer?"},
            {"id": "reply_cmd-public", "role": "ctox", "kind": "reply", "text": "42", "status": "completed"}
        ]);
        seed_completed_chat_with_receipt(
            &conn,
            "chat-public",
            "cmd-public",
            "evt-public",
            "user-a@example.test",
            false,
            "chat-public",
            messages.clone(),
        )?;
        seed_completed_chat_with_receipt(
            &conn,
            "chat-unproven",
            "cmd-unproven",
            "evt-unproven",
            "local-dev",
            false,
            "chat-unproven",
            serde_json::json!([{"id": "m1", "role": "user", "text": "orphan"}]),
        )?;
        seed_completed_chat_with_receipt(
            &conn,
            "chat-conflict",
            "cmd-conflict-a",
            "evt-conflict-a",
            "user-a@example.test",
            false,
            "chat-conflict",
            serde_json::json!([{"id": "m2", "role": "user", "text": "conflict"}]),
        )?;
        conn.execute(
            "INSERT INTO business_commands
                (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
             VALUES (?1, 'ctox', 'business_os.chat.task', ?2, 'completed', ?3, ?4, ?5)",
            params![
                "cmd-conflict-b",
                "chat-conflict",
                serde_json::to_string(&serde_json::json!({
                    "reply_to": "chat-conflict",
                    "chat_id": "chat-conflict"
                }))?,
                serde_json::to_string(&serde_json::json!({"source": "business-os-chat"}))?,
                now
            ],
        )?;
        conn.execute(
            "INSERT INTO business_events
                (event_id, collection, record_id, command_type, payload_json, observed_at_ms)
             VALUES (?1, 'business_commands', ?2, 'business_os.policy.allowed', ?3, ?4)",
            params![
                "evt-conflict-b",
                "cmd-conflict-b",
                serde_json::to_string(&serde_json::json!({
                    "event_type": "business_os.policy.allowed",
                    "command_id": "cmd-conflict-b",
                    "actor": { "id": "user-b@example.test", "trusted": true }
                }))?,
                now
            ],
        )?;
        seed_completed_chat_with_receipt(
            &conn,
            "chat-deleted",
            "cmd-deleted",
            "evt-deleted",
            "user-a@example.test",
            true,
            "chat-deleted",
            serde_json::json!([{"id": "m3", "role": "user", "text": "deleted"}]),
        )?;
        let queue_id = "queue:system::public-chat";
        let queue_payload = serde_json::json!({
            "id": queue_id,
            "command_id": "cmd-public",
            "status": "completed",
            "route_status": "handled",
            "task_status": "completed",
            "updated_at_ms": now
        });
        upsert_business_record(
            &conn,
            "ctox_queue_tasks",
            queue_id,
            now,
            queue_payload.clone(),
        )?;
        let queue_count_before: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection='ctox_queue_tasks' AND deleted=0",
            [],
            |row| row.get(0),
        )?;
        drop(conn);

        let dry_run =
            repair_queue_projections(root, QueueProjectionRepairOptions { apply: false })?;
        assert_eq!(
            dry_run
                .pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64),
            Some(1)
        );
        let conn = open_store(root)?;
        let dry_chat: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id='chat-public' AND deleted=0",
            [],
            |row| row.get(0),
        )?;
        let dry_chat: Value = serde_json::from_str(&dry_chat)?;
        assert_eq!(
            dry_chat.get("owner_user_id").and_then(Value::as_str),
            Some("local-dev")
        );
        assert_eq!(dry_chat.get("messages"), Some(&messages));
        let dry_queue: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection='ctox_queue_tasks' AND record_id=?1",
            params![queue_id],
            |row| row.get(0),
        )?;
        let dry_queue: Value = serde_json::from_str(&dry_queue)?;
        assert_eq!(
            dry_queue.get("route_status").and_then(Value::as_str),
            Some("handled")
        );
        let queue_count_dry: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection='ctox_queue_tasks' AND deleted=0",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(queue_count_dry, queue_count_before);
        drop(conn);
        assert!(
            load_rxdb_collection_record(root, "business_chats", "chat-public")?.is_none(),
            "dry-run must not create an RxDB chat projection"
        );

        let first_apply =
            repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            first_apply
                .pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64),
            Some(1)
        );
        let conn = open_store(root)?;
        let applied: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id='chat-public' AND deleted=0",
            [],
            |row| row.get(0),
        )?;
        let applied: Value = serde_json::from_str(&applied)?;
        assert_eq!(
            applied.get("owner_user_id").and_then(Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(applied.get("messages"), Some(&messages));
        assert_eq!(
            applied.get("title").and_then(Value::as_str),
            Some("Public answer")
        );
        assert_eq!(
            applied.get("tracking_status").and_then(Value::as_str),
            Some("completed")
        );
        let unproven: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id='chat-unproven' AND deleted=0",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            serde_json::from_str::<Value>(&unproven)?
                .get("owner_user_id")
                .and_then(Value::as_str),
            Some("local-dev")
        );
        let conflict: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id='chat-conflict' AND deleted=0",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            serde_json::from_str::<Value>(&conflict)?
                .get("owner_user_id")
                .and_then(Value::as_str),
            Some("local-dev")
        );
        let deleted: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id='chat-deleted' AND deleted=1",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            serde_json::from_str::<Value>(&deleted)?
                .get("owner_user_id")
                .and_then(Value::as_str),
            Some("local-dev")
        );
        let queue_count_after: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection='ctox_queue_tasks' AND deleted=0",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(queue_count_after, queue_count_before);
        let applied_queue: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection='ctox_queue_tasks' AND record_id=?1",
            params![queue_id],
            |row| row.get(0),
        )?;
        let applied_queue: Value = serde_json::from_str(&applied_queue)?;
        assert_eq!(
            applied_queue.get("route_status").and_then(Value::as_str),
            Some("handled")
        );
        drop(conn);
        assert!(
            load_rxdb_collection_record(root, "business_chats", "chat-public")?.is_none(),
            "first apply must leave RxDB unrepaired when the projection writer is unavailable"
        );

        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__business_chats__v0",
            "chat-public",
            serde_json::json!({
                "id": "chat-public",
                "owner_user_id": "local-dev",
                "title": "Public answer",
                "messages": messages,
                "tracking_status": "completed"
            }),
        )?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__ctox_queue_tasks__v0",
            queue_id,
            queue_payload.clone(),
        )?;
        drop(rxdb_conn);

        let retry = repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            retry
                .pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64),
            Some(1)
        );
        let conn = open_store(root)?;
        let retried: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection='business_chats' AND record_id='chat-public' AND deleted=0",
            [],
            |row| row.get(0),
        )?;
        let retried: Value = serde_json::from_str(&retried)?;
        assert_eq!(
            retried.get("owner_user_id").and_then(Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(retried.get("messages"), Some(&messages));
        let queue_count_retry: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection='ctox_queue_tasks' AND deleted=0",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(queue_count_retry, queue_count_before);
        drop(conn);
        let rxdb_chat = load_rxdb_collection_record(root, "business_chats", "chat-public")?
            .context("expected repaired rxdb chat")?;
        assert_eq!(
            rxdb_chat.get("owner_user_id").and_then(Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(rxdb_chat.get("messages"), Some(&messages));

        let noop = repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            noop.pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            0
        );
        Ok(())
    }

    fn rxdb_chat_row(root: &Path, chat_id: &str) -> anyhow::Result<(i64, f64, Value)> {
        let conn = Connection::open(rxdb_store_path(root))?;
        let (deleted, last_write_time, data): (i64, f64, String) = conn.query_row(
            "SELECT deleted, lastWriteTime, data
             FROM ctox_business_os__business_chats__v0
             WHERE id = ?1",
            [chat_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        Ok((deleted, last_write_time, serde_json::from_str(&data)?))
    }

    fn native_chat_payload(conn: &Connection, chat_id: &str) -> anyhow::Result<Value> {
        let raw: String = conn.query_row(
            "SELECT payload_json FROM business_records
             WHERE collection='business_chats' AND record_id=?1",
            [chat_id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&raw)?)
    }

    #[test]
    fn repair_queue_projections_does_not_resurrect_rxdb_chat_tombstone() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let native_messages = serde_json::json!([
            {"id": "native-live", "role": "user", "text": "keep native"}
        ]);
        let tombstone_messages = serde_json::json!([
            {"id": "rxdb-deleted", "role": "user", "text": "must stay deleted"}
        ]);
        seed_completed_chat_with_receipt(
            &conn,
            "chat-tombstone-json",
            "cmd-tombstone-json",
            "evt-tombstone-json",
            "user-a@example.test",
            false,
            "chat-tombstone-json",
            native_messages.clone(),
        )?;
        seed_completed_chat_with_receipt(
            &conn,
            "chat-tombstone-column",
            "cmd-tombstone-column",
            "evt-tombstone-column",
            "user-a@example.test",
            false,
            "chat-tombstone-column",
            native_messages.clone(),
        )?;
        seed_completed_chat_with_receipt(
            &conn,
            "chat-tombstone-flag",
            "cmd-tombstone-flag",
            "evt-tombstone-flag",
            "user-a@example.test",
            false,
            "chat-tombstone-flag",
            native_messages.clone(),
        )?;
        drop(conn);

        let rxdb_conn = create_repair_rxdb_tables(root)?;
        let json_tombstone = serde_json::json!({
            "id": "chat-tombstone-json",
            "owner_user_id": "local-dev",
            "title": "Deleted in browser",
            "minimized": true,
            "messages": tombstone_messages,
            "_deleted": true,
            "is_deleted": true
        });
        let column_tombstone = serde_json::json!({
            "id": "chat-tombstone-column",
            "owner_user_id": "local-dev",
            "title": "Column deleted",
            "minimized": true,
            "messages": tombstone_messages
        });
        insert_rxdb_test_record_with_deleted(
            &rxdb_conn,
            "ctox_business_os__business_chats__v0",
            "chat-tombstone-json",
            json_tombstone,
            true,
        )?;
        insert_rxdb_test_record_with_deleted(
            &rxdb_conn,
            "ctox_business_os__business_chats__v0",
            "chat-tombstone-column",
            column_tombstone,
            true,
        )?;
        insert_rxdb_test_record_with_deleted(
            &rxdb_conn,
            "ctox_business_os__business_chats__v0",
            "chat-tombstone-flag",
            serde_json::json!({
                "id": "chat-tombstone-flag",
                "owner_user_id": "local-dev",
                "title": "JSON deleted",
                "minimized": true,
                "messages": tombstone_messages,
                "_deleted": true
            }),
            false,
        )?;
        drop(rxdb_conn);

        let applied = repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            applied
                .pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64),
            Some(3),
            "live native placeholders still repair; RxDB tombstones must not block native"
        );

        let conn = open_store(root)?;
        assert_eq!(
            native_chat_payload(&conn, "chat-tombstone-json")?
                .get("owner_user_id")
                .and_then(Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(
            native_chat_payload(&conn, "chat-tombstone-column")?
                .get("owner_user_id")
                .and_then(Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(
            native_chat_payload(&conn, "chat-tombstone-flag")?
                .get("owner_user_id")
                .and_then(Value::as_str),
            Some("user-a@example.test")
        );
        drop(conn);

        let (json_deleted, json_lwt, json_data) = rxdb_chat_row(root, "chat-tombstone-json")?;
        assert_eq!(json_deleted, 1);
        assert_eq!(json_lwt, 1.0);
        assert_eq!(
            json_data.get("_deleted").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            json_data.get("is_deleted").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            json_data.get("owner_user_id").and_then(Value::as_str),
            Some("local-dev")
        );
        assert_eq!(
            json_data.get("title").and_then(Value::as_str),
            Some("Deleted in browser")
        );
        assert_eq!(json_data.get("messages"), Some(&tombstone_messages));

        let (column_deleted, column_lwt, column_data) =
            rxdb_chat_row(root, "chat-tombstone-column")?;
        assert_eq!(column_deleted, 1);
        assert_eq!(column_lwt, 1.0);
        assert_eq!(
            column_data.get("owner_user_id").and_then(Value::as_str),
            Some("local-dev")
        );
        assert_eq!(
            column_data.get("title").and_then(Value::as_str),
            Some("Column deleted")
        );
        assert_eq!(column_data.get("messages"), Some(&tombstone_messages));
        assert_ne!(
            column_data.get("_deleted").and_then(Value::as_bool),
            Some(false),
            "upsert must not reactivate a table-deleted tombstone"
        );

        let (flag_deleted, flag_lwt, flag_data) = rxdb_chat_row(root, "chat-tombstone-flag")?;
        assert_eq!(flag_deleted, 0);
        assert_eq!(flag_lwt, 1.0);
        assert_eq!(
            flag_data.get("_deleted").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            flag_data.get("owner_user_id").and_then(Value::as_str),
            Some("local-dev")
        );
        assert_eq!(
            flag_data.get("title").and_then(Value::as_str),
            Some("JSON deleted")
        );

        let noop = repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            noop.pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            0
        );
        let (json_deleted_again, json_lwt_again, _) = rxdb_chat_row(root, "chat-tombstone-json")?;
        assert_eq!(json_deleted_again, 1);
        assert_eq!(json_lwt_again, 1.0);
        Ok(())
    }

    #[test]
    fn repair_queue_projections_patches_only_live_rxdb_owner_and_inserts_when_absent(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let native_messages = serde_json::json!([
            {"id": "native-msg", "role": "user", "text": "native transcript"}
        ]);
        let browser_messages = serde_json::json!([
            {"id": "browser-msg", "role": "user", "text": "fresher browser transcript"}
        ]);
        let draft = serde_json::json!({"text": "unsent draft", "updated_at_ms": 99});
        seed_completed_chat_with_receipt(
            &conn,
            "chat-live-rxdb",
            "cmd-live-rxdb",
            "evt-live-rxdb",
            "user-a@example.test",
            false,
            "chat-live-rxdb",
            native_messages.clone(),
        )?;
        seed_completed_chat_with_receipt(
            &conn,
            "chat-absent-rxdb",
            "cmd-absent-rxdb",
            "evt-absent-rxdb",
            "user-a@example.test",
            false,
            "chat-absent-rxdb",
            native_messages.clone(),
        )?;
        drop(conn);

        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__business_chats__v0",
            "chat-live-rxdb",
            serde_json::json!({
                "id": "chat-live-rxdb",
                "owner_user_id": "local-dev",
                "title": "Browser title",
                "minimized": true,
                "messages": browser_messages,
                "draft": draft,
                "tracking_status": "completed"
            }),
        )?;
        drop(rxdb_conn);

        let applied = repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            applied
                .pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64),
            Some(2)
        );

        let conn = open_store(root)?;
        let native_live = native_chat_payload(&conn, "chat-live-rxdb")?;
        assert_eq!(
            native_live.get("owner_user_id").and_then(Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(native_live.get("messages"), Some(&native_messages));
        assert_eq!(
            native_live.get("title").and_then(Value::as_str),
            Some("Public answer")
        );
        let native_absent = native_chat_payload(&conn, "chat-absent-rxdb")?;
        assert_eq!(
            native_absent.get("owner_user_id").and_then(Value::as_str),
            Some("user-a@example.test")
        );
        drop(conn);

        let live_rxdb = load_rxdb_collection_record(root, "business_chats", "chat-live-rxdb")?
            .context("expected owner-patched live rxdb chat")?;
        assert_eq!(
            live_rxdb.get("owner_user_id").and_then(Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(live_rxdb.get("messages"), Some(&browser_messages));
        assert_eq!(
            live_rxdb.get("title").and_then(Value::as_str),
            Some("Browser title")
        );
        assert_eq!(
            live_rxdb.get("minimized").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(live_rxdb.get("draft"), Some(&draft));
        assert_eq!(
            live_rxdb.get("tracking_status").and_then(Value::as_str),
            Some("completed")
        );

        let inserted = load_rxdb_collection_record(root, "business_chats", "chat-absent-rxdb")?
            .context("expected full native insert for absent rxdb chat")?;
        assert_eq!(
            inserted.get("owner_user_id").and_then(Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(inserted.get("messages"), Some(&native_messages));
        assert_eq!(
            inserted.get("title").and_then(Value::as_str),
            Some("Public answer")
        );
        assert_eq!(
            inserted.get("minimized").and_then(Value::as_bool),
            Some(false)
        );
        assert!(inserted.get("draft").is_none());

        let noop = repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            noop.pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            0
        );
        Ok(())
    }

    fn overwrite_rxdb_chat(
        root: &Path,
        chat_id: &str,
        payload: Value,
        deleted: bool,
    ) -> anyhow::Result<()> {
        let conn = Connection::open(rxdb_store_path(root))?;
        let changed = conn.execute(
            "UPDATE ctox_business_os__business_chats__v0
             SET deleted = ?1, lastWriteTime = 9.0, data = ?2
             WHERE id = ?3",
            params![
                if deleted { 1 } else { 0 },
                serde_json::to_string(&payload)?,
                chat_id
            ],
        )?;
        anyhow::ensure!(changed == 1, "expected one RxDB chat row to update");
        Ok(())
    }

    #[test]
    fn repair_queue_projections_owner_patch_keeps_interleaved_rxdb_content_update(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let stale_messages = serde_json::json!([
            {"id": "stale-msg", "role": "user", "text": "selected before repair"}
        ]);
        let interleaved_messages = serde_json::json!([
            {"id": "fresh-msg", "role": "user", "text": "browser edited after selection"}
        ]);
        let interleaved_draft = serde_json::json!({"text": "typed after selection"});
        seed_completed_chat_with_receipt(
            &conn,
            "chat-race-update",
            "cmd-race-update",
            "evt-race-update",
            "user-a@example.test",
            false,
            "chat-race-update",
            stale_messages.clone(),
        )?;
        drop(conn);

        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__business_chats__v0",
            "chat-race-update",
            serde_json::json!({
                "id": "chat-race-update",
                "owner_user_id": "local-dev",
                "title": "Stale title",
                "minimized": false,
                "messages": stale_messages,
                "draft": {"text": "old draft"}
            }),
        )?;
        drop(rxdb_conn);

        let root_buf = root.to_path_buf();
        let interleaved_messages_hook = interleaved_messages.clone();
        let interleaved_draft_hook = interleaved_draft.clone();
        let _interleave = set_rxdb_chat_owner_repair_interleave(move |_, chat_id| {
            if chat_id != "chat-race-update" {
                return;
            }
            overwrite_rxdb_chat(
                &root_buf,
                chat_id,
                serde_json::json!({
                    "id": "chat-race-update",
                    "owner_user_id": "local-dev",
                    "title": "Browser title after selection",
                    "minimized": true,
                    "messages": interleaved_messages_hook,
                    "draft": interleaved_draft_hook
                }),
                false,
            )
            .expect("interleave content update");
        });

        let applied = repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            applied
                .pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64),
            Some(1)
        );
        let rxdb = load_rxdb_collection_record(root, "business_chats", "chat-race-update")?
            .context("expected live rxdb chat after interleaved update")?;
        assert_eq!(
            rxdb.get("owner_user_id").and_then(Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(rxdb.get("messages"), Some(&interleaved_messages));
        assert_eq!(
            rxdb.get("title").and_then(Value::as_str),
            Some("Browser title after selection")
        );
        assert_eq!(rxdb.get("minimized").and_then(Value::as_bool), Some(true));
        assert_eq!(rxdb.get("draft"), Some(&interleaved_draft));
        Ok(())
    }

    #[test]
    fn repair_queue_projections_does_not_resurrect_interleaved_rxdb_deletion() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let messages = serde_json::json!([
            {"id": "live-then-deleted", "role": "user", "text": "will be deleted after selection"}
        ]);
        seed_completed_chat_with_receipt(
            &conn,
            "chat-race-delete",
            "cmd-race-delete",
            "evt-race-delete",
            "user-a@example.test",
            false,
            "chat-race-delete",
            messages.clone(),
        )?;
        drop(conn);

        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__business_chats__v0",
            "chat-race-delete",
            serde_json::json!({
                "id": "chat-race-delete",
                "owner_user_id": "local-dev",
                "title": "Live at selection",
                "messages": messages
            }),
        )?;
        drop(rxdb_conn);

        let root_buf = root.to_path_buf();
        let tombstone_messages = messages.clone();
        let _interleave = set_rxdb_chat_owner_repair_interleave(move |_, chat_id| {
            if chat_id != "chat-race-delete" {
                return;
            }
            overwrite_rxdb_chat(
                &root_buf,
                chat_id,
                serde_json::json!({
                    "id": "chat-race-delete",
                    "owner_user_id": "local-dev",
                    "title": "Deleted after selection",
                    "messages": tombstone_messages,
                    "_deleted": true,
                    "is_deleted": true
                }),
                true,
            )
            .expect("interleave deletion");
        });

        let applied = repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            applied
                .pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64),
            Some(1),
            "native owner may still repair; interleaved RxDB tombstone must not be resurrected"
        );
        let conn = open_store(root)?;
        assert_eq!(
            native_chat_payload(&conn, "chat-race-delete")?
                .get("owner_user_id")
                .and_then(Value::as_str),
            Some("user-a@example.test")
        );
        drop(conn);
        let (deleted, last_write_time, data) = rxdb_chat_row(root, "chat-race-delete")?;
        assert_eq!(deleted, 1);
        assert_eq!(last_write_time, 9.0);
        assert_eq!(data.get("_deleted").and_then(Value::as_bool), Some(true));
        assert_eq!(data.get("is_deleted").and_then(Value::as_bool), Some(true));
        assert_eq!(
            data.get("owner_user_id").and_then(Value::as_str),
            Some("local-dev")
        );
        assert_eq!(
            data.get("title").and_then(Value::as_str),
            Some("Deleted after selection")
        );
        Ok(())
    }

    #[test]
    fn repair_queue_projections_does_not_overwrite_interleaved_rxdb_insert() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let native_messages = serde_json::json!([
            {"id": "native-msg", "role": "user", "text": "native snapshot at selection"}
        ]);
        let browser_messages = serde_json::json!([
            {"id": "browser-msg", "role": "user", "text": "created after selection"}
        ]);
        let browser_draft = serde_json::json!({"text": "unsent after selection"});
        seed_completed_chat_with_receipt(
            &conn,
            "chat-race-insert",
            "cmd-race-insert",
            "evt-race-insert",
            "user-a@example.test",
            false,
            "chat-race-insert",
            native_messages.clone(),
        )?;
        drop(conn);

        create_repair_rxdb_tables(root)?;
        assert!(
            load_rxdb_collection_record(root, "business_chats", "chat-race-insert")?.is_none(),
            "candidate selection starts from an absent RxDB chat"
        );

        let root_buf = root.to_path_buf();
        let browser_messages_hook = browser_messages.clone();
        let browser_draft_hook = browser_draft.clone();
        let _interleave = set_rxdb_chat_owner_repair_interleave(move |_, chat_id| {
            if chat_id != "chat-race-insert" {
                return;
            }
            let conn = Connection::open(rxdb_store_path(&root_buf)).expect("open rxdb");
            insert_rxdb_test_record(
                &conn,
                "ctox_business_os__business_chats__v0",
                chat_id,
                serde_json::json!({
                    "id": "chat-race-insert",
                    "owner_user_id": "local-dev",
                    "title": "Browser created after selection",
                    "minimized": true,
                    "messages": browser_messages_hook,
                    "draft": browser_draft_hook
                }),
            )
            .expect("interleave insert");
        });

        let applied = repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            applied
                .pointer("/counts/chat_owner_repaired")
                .and_then(Value::as_u64),
            Some(1)
        );
        let rxdb = load_rxdb_collection_record(root, "business_chats", "chat-race-insert")?
            .context("expected concurrent rxdb insert to remain")?;
        assert_eq!(
            rxdb.get("owner_user_id").and_then(Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(rxdb.get("messages"), Some(&browser_messages));
        assert_eq!(
            rxdb.get("title").and_then(Value::as_str),
            Some("Browser created after selection")
        );
        assert_eq!(rxdb.get("minimized").and_then(Value::as_bool), Some(true));
        assert_eq!(rxdb.get("draft"), Some(&browser_draft));
        assert_ne!(rxdb.get("messages"), Some(&native_messages));
        Ok(())
    }

    #[test]
    fn repair_queue_projections_updates_task_status_from_terminal_command() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let now = now_ms() as i64;
        let conn = open_store(root)?;
        conn.execute(
            "INSERT INTO business_commands
                (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
             VALUES (?1, 'documents', 'business_os.chat.task', 'documents', 'completed', ?2, ?3, ?4)",
            params![
                "cmd_repair_from_terminal_command",
                serde_json::to_string(&serde_json::json!({"prompt": "done"}))?,
                serde_json::to_string(&serde_json::json!({"source": "test"}))?,
                now,
            ],
        )?;
        upsert_business_record(
            &conn,
            "ctox_queue_tasks",
            "queue:system::repair_from_terminal_command",
            now,
            serde_json::json!({
                "id": "queue:system::repair_from_terminal_command",
                "command_id": "cmd_repair_from_terminal_command",
                "status": "completed",
                "route_status": "handled",
                "task_status": "handled",
                "updated_at_ms": now
            }),
        )?;
        drop(conn);
        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__ctox_queue_tasks__v0",
            "queue:system::repair_from_terminal_command",
            serde_json::json!({
                "id": "queue:system::repair_from_terminal_command",
                "command_id": "cmd_repair_from_terminal_command",
                "status": "completed",
                "route_status": "handled",
                "task_status": "handled",
                "updated_at_ms": now
            }),
        )?;
        drop(rxdb_conn);

        let dry_run =
            repair_queue_projections(root, QueueProjectionRepairOptions { apply: false })?;
        assert_eq!(
            dry_run
                .pointer("/counts/projection_repaired_from_command")
                .and_then(Value::as_u64),
            Some(1)
        );

        repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        let rxdb_queue = load_rxdb_collection_record(
            root,
            "ctox_queue_tasks",
            "queue:system::repair_from_terminal_command",
        )?
        .context("expected repaired rxdb queue row")?;
        assert_eq!(
            rxdb_queue.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            rxdb_queue.get("route_status").and_then(Value::as_str),
            Some("handled")
        );
        assert_eq!(
            rxdb_queue.get("task_status").and_then(Value::as_str),
            Some("completed")
        );
        Ok(())
    }

    #[test]
    fn repair_queue_projections_redacts_inline_report_artifacts_and_counts_legacy_records(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let inline_image = format!("data:image/png;base64,{}", "A".repeat(12_000));
        let inline_payload = serde_json::json!({
            "id": "cmd_inline_report_payload",
            "command_id": "cmd_inline_report_payload",
            "module": "documents",
            "command_type": "business_os.bug_report",
            "status": "accepted",
            "payload": {
                "title": "Gleichungen in word editor",
                "attachment": {
                    "data_url": inline_image
                },
                "strokes": [
                    [{"x": 1, "y": 2}],
                    [{"x": 3, "y": 4}]
                ]
            },
            "client_context": {
                "transport": "business-os-http-command-fallback"
            }
        });
        upsert_business_record(
            &conn,
            "business_commands",
            "cmd_inline_report_payload",
            now_ms() as i64,
            inline_payload.clone(),
        )?;
        drop(conn);
        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__business_commands__v1",
            "cmd_inline_report_payload",
            inline_payload,
        )?;
        drop(rxdb_conn);

        let dry_run =
            repair_queue_projections(root, QueueProjectionRepairOptions { apply: false })?;
        assert_eq!(
            dry_run
                .pointer("/counts/oversized_inline_artifacts_redacted")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            dry_run
                .pointer("/counts/legacy_http_fallback_records")
                .and_then(Value::as_u64),
            Some(1)
        );

        repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        let conn = open_store(root)?;
        let payload_json: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'business_commands' AND record_id = 'cmd_inline_report_payload'",
            [],
            |row| row.get(0),
        )?;
        assert!(
            !payload_json.contains("data:image/png;base64"),
            "inline image payload must be redacted"
        );
        let payload: Value = serde_json::from_str(&payload_json)?;
        assert_eq!(
            payload
                .pointer("/payload/attachment/data_url/redacted")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .pointer("/payload/strokes/redacted")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .pointer("/payload/strokes/stroke_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            payload
                .pointer("/client_context/transport")
                .and_then(Value::as_str),
            Some("business-os-http-command-fallback"),
            "legacy transport context is counted and quarantined, not rewritten or replayed"
        );
        let rxdb_payload =
            load_rxdb_collection_record(root, "business_commands", "cmd_inline_report_payload")?
                .context("expected redacted rxdb reporter command row")?;
        assert_eq!(
            rxdb_payload
                .pointer("/payload/attachment/data_url/redacted")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            rxdb_payload
                .pointer("/payload/strokes/redacted")
                .and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    /// ST3: the terminal decision must come from the status field, never from
    /// the wording of the human-readable note.
    ///
    /// Before this, `queue_status_note_is_terminal_success` searched the note
    /// for substrings — among them `" completed."` together with `"changed "`.
    /// A note rephrased by a translator or a log tweak silently changed whether
    /// a task counted as finished.
    ///
    /// The prose below is deliberately the exact shape the old matcher accepted.
    /// If someone reintroduces substring matching, this test goes red on the
    /// first two assertions.
    #[test]
    fn terminal_success_reads_the_status_field_and_not_the_note_wording() {
        for prose in [
            "business-os:terminal-success: all good",
            "Run completed. Changed 3 records and verified them.",
            "completed. verified everything",
        ] {
            assert!(
                !queue_status_is_terminal_success(Some(prose)),
                "note wording must not decide terminal success: {prose:?}"
            );
        }

        // Same state, three different notes, one answer — because none of them
        // is consulted.
        for status in ["completed", "handled", "done"] {
            assert!(
                queue_status_is_terminal_success(Some(status)),
                "structured status {status:?} must count as terminal success"
            );
        }
        assert!(!queue_status_is_terminal_success(Some("leased")));
        assert!(!queue_status_is_terminal_success(None));
    }
}
