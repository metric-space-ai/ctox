//! Server-owned cockpit controls and bounded projections.
use super::session::{session_user_id, BusinessOsSession};
use super::store::BusinessCommand;
use crate::inference::runtime_env;
use crate::mission::channels;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

#[path = "harness_cockpit_chat.rs"]
mod chat;
#[path = "harness_cockpit_projections.rs"]
mod projections;
pub(crate) use chat::{queued_chat_text, trim_messages};
#[cfg(test)]
pub(crate) use projections::refresh_plan_projection_for_test;
pub(crate) use projections::{
    publish_service_stopped, publish_worker_snapshot, refresh_after_finalization,
    schedule_flow_refresh, schedule_refresh, schedule_runs_refresh, WorkerSnapshot,
};

pub const QUEUE_RETENTION_KEY: &str = "business_os.projection.queue_tasks_retention";
const PAUSE_KEY: &str = "queue.pause";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct QueuePause {
    pub paused: bool,
    pub reason: Option<String>,
}

pub fn queue_pause(root: &Path) -> Result<QueuePause> {
    runtime_env::get_runtime_env_value(root, PAUSE_KEY)
        .map(|raw| serde_json::from_str(&raw).context("invalid persisted queue.pause"))
        .transpose()
        .map(|value| value.unwrap_or_default())
}

pub fn queue_is_paused(root: &Path) -> bool {
    queue_pause_state(root).0.paused
}

/// Invalid configuration must be visible, but must not silently stop admission.
/// Report once per root until the configuration has been repaired.
pub(super) fn queue_pause_state(root: &Path) -> (QueuePause, Option<String>) {
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    static REPORTED: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();
    let reported = REPORTED.get_or_init(|| Mutex::new(BTreeSet::new()));
    match queue_pause(root) {
        Ok(pause) => {
            reported
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(root);
            (pause, None)
        }
        Err(error) => {
            let error = format!("Invalid queue.pause; admission remains enabled: {error:#}");
            let first = reported
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(root.to_path_buf());
            if first {
                eprintln!("[ctox cockpit] {}: {error}", root.display());
                schedule_refresh(root);
            }
            (QueuePause::default(), Some(error))
        }
    }
}

pub fn queue_retention(root: &Path) -> Result<i64> {
    let value = runtime_env::get_runtime_env_value(root, QUEUE_RETENTION_KEY)
        .map(|raw| raw.parse::<i64>())
        .transpose()?
        .unwrap_or(300);
    anyhow::ensure!(
        (1..=100_000).contains(&value),
        "queue projection retention must be 1..100000"
    );
    Ok(value)
}

pub(super) fn control(
    root: &Path,
    command: &BusinessCommand,
    session: &BusinessOsSession,
) -> Result<Value> {
    let task_id = command
        .payload
        .get("task_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    // Authorization already succeeded in the central dispatcher. Persist intent before
    // changing queue/config state so even a failed result-audit write remains explainable.
    crate::service::harness_flow::record_harness_flow_event(
        root,
        crate::service::harness_flow::RecordHarnessFlowEventRequest {
            event_kind: "cockpit.control",
            title: &command.command_type,
            body_text: "",
            message_key: (!task_id.is_empty()).then_some(task_id),
            work_id: None,
            ticket_key: None,
            attempt_index: None,
            metadata: json!({"actor":session_user_id(session),"command_type":command.command_type,"stage":"requested","payload":control_audit_payload(&command.payload)}),
        },
    )?;
    let result = (|| -> Result<Value> {
        if !matches!(
            command.command_type.as_str(),
            "ctox.queue.pause" | "ctox.queue.capacity" | "ctox.plan.step.retry"
        ) {
            anyhow::ensure!(!task_id.is_empty(), "task_id is required");
        }
        Ok(match command.command_type.as_str() {
            "ctox.plan.step.retry" => {
                let step_id = optional_text(&command.payload, "step_id", 256)?
                    .context("step_id is required")?;
                let actor = session_user_id(session).context("authenticated actor is required")?;
                let retry = crate::mission::plan::retry_failed_step(
                    root,
                    command.id.as_deref().context("command id is required")?,
                    &step_id,
                    actor,
                )?;
                json!({"ok": true, "status": "pending", "retry": retry})
            }
            "ctox.queue.pause" => {
                let paused = command
                    .payload
                    .get("paused")
                    .and_then(Value::as_bool)
                    .context("paused must be a boolean")?;
                let reason = optional_text(&command.payload, "reason", 1000)?;
                let value = QueuePause {
                    paused,
                    reason: if paused { reason } else { None },
                };
                runtime_env::set_runtime_env_value(
                    root,
                    PAUSE_KEY,
                    &serde_json::to_string(&value)?,
                )?;
                json!({"ok":true, "pause":value})
            }
            "ctox.queue.capacity" => {
                let workers = command
                    .payload
                    .get("workers")
                    .and_then(Value::as_u64)
                    .context("workers must be an integer from 1 to 8")?;
                anyhow::ensure!((1..=8).contains(&workers), "workers must be 1..8");
                let mut capacity =
                    crate::service::configure_queue_worker_capacity(root, Some(workers as usize))?;
                capacity["ok"] = json!(true);
                capacity
            }
            "ctox.queue.abort_turn" => json!({
                "ok":false, "status":"unsupported", "code":"unsupported",
                "reason":"The worker owns the active session. Safe abort requires a task-scoped interrupt acknowledgement and an atomic aborted-attempt/blocked-lease transition. Use queue.pause to stop new admission."
            }),
            "ctox.queue.release" | "ctox.queue.block" | "ctox.queue.retry" => {
                anyhow::ensure!(!task_id.is_empty(), "task_id is required");
                let current =
                    channels::load_queue_task(root, task_id)?.context("queue task not found")?;
                anyhow::ensure!(current.route_status != "leased", "active leases must finish before release, block or retry; abort_turn is unsupported");
                let block = command.command_type == "ctox.queue.block";
                if command.command_type == "ctox.queue.retry" {
                    anyhow::ensure!(
                        matches!(current.route_status.as_str(), "failed" | "blocked"),
                        "retry requires a failed or blocked task"
                    );
                }
                let note = optional_text(
                    &command.payload,
                    if block { "reason" } else { "note" },
                    1000,
                )?;
                if block {
                    anyhow::ensure!(note.is_some(), "reason is required");
                }
                let task = channels::control_queue_task(
                    root,
                    channels::QueueTaskUpdateRequest {
                        message_key: task_id.to_string(),
                        route_status: Some(if block { "blocked" } else { "pending" }.to_string()),
                        priority: if command.command_type == "ctox.queue.release" {
                            optional_text(&command.payload, "priority", 16)?
                        } else {
                            None
                        },
                        clear_note: command.command_type == "ctox.queue.retry",
                        status_note: note,
                        ..Default::default()
                    },
                    command.command_type == "ctox.queue.retry",
                )?;
                json!({"ok":true, "task":task})
            }
            _ => anyhow::bail!("unsupported cockpit command"),
        })
    })();
    let audit_result = match &result {
        Ok(value) => value.clone(),
        Err(error) => json!({"ok":false,"error":error.to_string()}),
    };
    crate::service::harness_flow::record_harness_flow_event_lossy(
        root,
        crate::service::harness_flow::RecordHarnessFlowEventRequest {
            event_kind: "cockpit.control",
            title: &command.command_type,
            body_text: "",
            message_key: (!task_id.is_empty()).then_some(task_id),
            work_id: None,
            ticket_key: None,
            attempt_index: None,
            metadata: json!({"actor":session_user_id(session), "command_type":command.command_type, "stage":"result", "result":audit_result}),
        },
    );
    schedule_refresh(root);
    result
}

/// Intent is recorded before validation, so never copy arbitrary payload values.
/// Bound even recognized text and only accept scalar values of the expected type.
fn control_audit_payload(payload: &Value) -> Value {
    let mut audit = serde_json::Map::new();
    for (field, max_chars) in [
        ("task_id", 1024),
        ("step_id", 256),
        ("priority", 16),
        ("reason", 1000),
        ("note", 1000),
    ] {
        if let Some(value) = payload.get(field).and_then(Value::as_str) {
            audit.insert(
                field.into(),
                Value::String(value.trim().chars().take(max_chars).collect()),
            );
        }
    }
    if let Some(workers) = payload.get("workers").and_then(Value::as_u64) {
        audit.insert("workers".into(), json!(workers));
    }
    if let Some(paused) = payload.get("paused").and_then(Value::as_bool) {
        audit.insert("paused".into(), json!(paused));
    }
    Value::Object(audit)
}

fn optional_text(payload: &Value, field: &str, max: usize) -> Result<Option<String>> {
    match payload.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            anyhow::ensure!(
                value.chars().count() <= max,
                "{field} exceeds {max} characters"
            );
            Ok((!value.trim().is_empty()).then(|| value.trim().to_string()))
        }
        _ => anyhow::bail!("{field} must be text"),
    }
}
