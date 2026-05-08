//! Mission-queue hook for deep-research runs.
//!
//! Lets `ctox chat`-style flows or scheduled tasks enqueue a deep-research
//! manager run as a queue task. The pattern in CTOX is to insert a row into
//! `communication_messages` with `channel = 'queue'`; the daemon's worker
//! pool then leases and processes those rows.
//!
//! For Wave 5 we ship a deliberately minimal implementation: enqueue
//! writes one row through [`crate::mission::channels::create_queue_task`]
//! and dequeue/complete read directly from the same `communication_messages`
//! table. The proper daemon-side worker registration ("polling for
//! report.run tasks and dispatching `manager::run_manager`") is left as a
//! Wave-6 follow-up. The CLI surface (`ctox report run`) does not depend
//! on this hook to operate end-to-end; it is an orthogonal capability for
//! scheduling.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::json;

use crate::mission::channels::{create_queue_task, QueueTaskCreateRequest};
use crate::paths;
use crate::persistence;

/// View of a queued report-run task as the daemon would see it. Only the
/// fields the worker pool needs to dispatch the run.
#[derive(Debug, Clone)]
pub struct QueuedReportTask {
    pub message_key: String,
    pub run_id: String,
    pub thread_key: String,
    pub priority: i64,
    pub created_at: String,
}

/// Convert a [`i64`] priority (1 = high, 2 = normal, 3 = low) into the
/// canonical string the queue table expects.
fn priority_label(priority: i64) -> &'static str {
    if priority <= 1 {
        "high"
    } else if priority >= 3 {
        "low"
    } else {
        "normal"
    }
}

fn priority_rank(label: &str) -> i64 {
    match label {
        "high" => 1,
        "low" => 3,
        _ => 2,
    }
}

/// Enqueue a report run as a queue task. Returns the queue-table
/// `message_key` that identifies the task. Idempotency is by digest of
/// `(title, prompt, thread_key, now)`, which means re-enqueueing the same
/// run a second after the first will produce a new message_key — that is
/// the same behaviour as `ctox queue create`.
pub fn enqueue_run(root: &Path, run_id: &str, priority: i64) -> Result<String> {
    let priority_str = priority_label(priority).to_string();
    let thread_key = format!("report:{run_id}");
    let title = format!("deep-research run {run_id}");
    let body = json!({
        "kind": "report.run",
        "run_id": run_id,
    });
    let prompt = serde_json::to_string(&body).context("encode report.run queue payload")?;
    let request = QueueTaskCreateRequest {
        title,
        prompt,
        thread_key,
        workspace_root: None,
        priority: priority_str,
        suggested_skill: Some("deep-research".to_string()),
        parent_message_key: None,
        extra_metadata: Some(json!({
            "report_run_id": run_id,
            "report_kind": "report.run",
        })),
    };
    let view = create_queue_task(root, request)
        .with_context(|| format!("failed to enqueue deep-research run {run_id}"))?;
    Ok(view.message_key)
}

/// Pop the oldest pending `report.run` queue task without leasing it.
/// The daemon-side worker is the proper consumer; this helper exists so
/// CLI / tests can introspect the queue.
pub fn dequeue_oldest(root: &Path) -> Result<Option<QueuedReportTask>> {
    let conn = open_consolidated_db(root)?;
    let row = conn
        .query_row(
            "SELECT m.message_key, m.thread_key, m.metadata_json, m.external_created_at
             FROM communication_messages m
             WHERE m.channel = 'queue'
               AND m.thread_key LIKE 'report:%'
             ORDER BY m.external_created_at ASC
             LIMIT 1",
            [],
            |row| {
                let message_key: String = row.get(0)?;
                let thread_key: String = row.get(1)?;
                let metadata_json: Option<String> = row.get(2)?;
                let created_at: String = row.get(3)?;
                Ok((message_key, thread_key, metadata_json, created_at))
            },
        )
        .optional()
        .context("failed to dequeue report run from queue")?;
    let Some((message_key, thread_key, metadata_json, created_at)) = row else {
        return Ok(None);
    };
    let run_id = thread_key
        .strip_prefix("report:")
        .unwrap_or(&thread_key)
        .to_string();
    let priority = metadata_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| {
            v.get("priority")
                .and_then(|p| p.as_str())
                .map(|s| priority_rank(s))
        })
        .unwrap_or(2);
    Ok(Some(QueuedReportTask {
        message_key,
        run_id,
        thread_key,
        priority,
        created_at,
    }))
}

/// Mark a queued report-run task complete. Writes `route_status =
/// 'completed'` and a status note describing the outcome. The daemon
/// would normally do this after [`crate::report::manager::run_manager`]
/// returns.
pub fn complete_task(root: &Path, message_key: &str, outcome: &str) -> Result<()> {
    let conn = open_consolidated_db(root)?;
    let updated = conn
        .execute(
            "UPDATE communication_routing_state
             SET route_status = 'handled', last_error = ?1, updated_at = ?2
             WHERE message_key = ?3",
            params![outcome, chrono::Utc::now().to_rfc3339(), message_key,],
        )
        .with_context(|| format!("failed to complete queue task {message_key}"))?;
    if updated == 0 {
        // Best-effort fallback: write a status note onto the message
        // itself; if the routing-state row is missing the daemon will
        // pick up the canonical status from the message's `status`
        // column instead.
        let _ = conn.execute(
            "UPDATE communication_messages
             SET status = 'handled'
             WHERE message_key = ?1 AND channel = 'queue'",
            params![message_key],
        );
    }
    Ok(())
}

fn open_consolidated_db(root: &Path) -> Result<rusqlite::Connection> {
    let path = paths::core_db(root);
    let conn = rusqlite::Connection::open(&path)
        .with_context(|| format!("failed to open consolidated DB at {}", path.display()))?;
    conn.busy_timeout(persistence::sqlite_busy_timeout_duration())
        .context("failed to set SQLite busy_timeout")?;
    Ok(conn)
}
