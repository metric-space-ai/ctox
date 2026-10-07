// Origin: CTOX
// License: AGPL-3.0-only

//! Register the existing Workjet CodeThread identity at the native Threads
//! producer. This is not a transfer session or a model/provider capability.
use super::*;
use rusqlite::OptionalExtension;
use serde::Serialize;

pub(super) const CONTRACT: &str = "ctox.workjet.supervisor_binding.v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindPayload {
    project_id: String,
    thread_id: String,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::business_os) struct SupervisorBinding {
    pub project_id: String,
    pub thread_id: String,
    pub thread_key: String,
}

// Native-only identity registry. The domain transaction commits the binding,
// Threads source record and effect receipt together. Browser metadata alone
// cannot claim this provenance or replace an established project supervisor.
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_supervisor_bindings (
    project_id TEXT PRIMARY KEY NOT NULL,
    owner_user_id TEXT NOT NULL,
    thread_id TEXT UNIQUE NOT NULL,
    created_at_ms INTEGER NOT NULL
);";

pub(super) fn apply(
    conn: &Connection,
    command: &BusinessCommand,
    owner: &str,
    projections: &mut Vec<Projection>,
) -> anyhow::Result<Value> {
    let payload: BindPayload = serde_json::from_value(command.payload.clone())?;
    let project = owned_project(conn, &payload.project_id, owner, true)?;
    let project_id = text(&project, "id")?;
    let thread_id = uuid::Uuid::parse_str(&payload.thread_id)
        .context("thread_id must be the existing CodeThread UUID")?
        .to_string();
    ensure!(thread_id == payload.thread_id, "thread_id must be a canonical UUID");
    let binding = SupervisorBinding {
        project_id: project_id.to_owned(),
        thread_key: format!("business-os/threads/{thread_id}"),
        thread_id,
    };
    conn.execute_batch(SCHEMA)?;
    let prior: Option<(String, String)> = conn.query_row(
        "SELECT owner_user_id, thread_id FROM workjet_supervisor_bindings WHERE project_id=?1",
        [project_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    let thread = outbound_load_record(conn, THREADS, &binding.thread_id)?;
    if let Some((prior_owner, prior_thread)) = prior {
        ensure!(prior_owner == owner && prior_thread == binding.thread_id,
            "project supervisor is already bound to another identity");
        let thread = thread.context("registered supervisor history is unavailable; repair is required")?;
        ensure!(thread["is_deleted"] != true
            && thread["owner_user_id"] == owner
            && thread["source_module"] == "ctox"
            && thread["source_record_type"] == "workjet_project"
            && thread["source_record_id"] == project_id
            && thread["metadata"]["workjet_supervisor_contract"] == CONTRACT
            && thread["metadata"]["workjet_supervisor"] == serde_json::to_value(&binding)?,
            "registered supervisor history conflicts with its native binding");
        // Do not rewrite title, messages, UI state or timestamps on a replay.
        projections.push(Projection { collection: THREADS, id: binding.thread_id.clone() });
    } else {
        ensure!(thread.is_none(), "refusing to adopt unrelated existing chat history");
        let already_bound: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM workjet_supervisor_bindings WHERE thread_id=?1)",
            [&binding.thread_id], |row| row.get(0),
        )?;
        ensure!(!already_bound, "supervisor UUID is already bound to another project");
        let now = super::super::store::now_ms() as i64;
        conn.execute(
            "INSERT INTO workjet_supervisor_bindings(project_id,owner_user_id,thread_id,created_at_ms) VALUES (?1,?2,?3,?4)",
            rusqlite::params![project_id, owner, binding.thread_id, now],
        )?;
        persist(conn, THREADS, &binding.thread_id, json!({
            "id": binding.thread_id, "thread_id": binding.thread_id,
            "title": project["name"], "kind": "chat", "status": "open",
            "owner_user_id": owner, "created_by_id": owner,
            "participant_ids": [owner], "watcher_user_ids": [], "assigned_user_id": "",
            "source_module": "ctox", "source_record_type": "workjet_project",
            "source_record_id": project_id, "source_label": project["name"],
            "metadata": {"workjet_supervisor_contract": CONTRACT, "workjet_supervisor": binding},
            "created_at_ms": now, "updated_at_ms": now, "is_deleted": false,
            "last_message_id": "", "last_message_at_ms": 0, "pending_approval_count": 0,
            "last_seen_by_user": {}, "snoozed_until_ms": 0, "archived_at_ms": 0,
        }), projections)?;
    }
    Ok(json!({"ok": true, "contract": CONTRACT, "binding": binding}))
}

/// A read of native provenance; no table migration or browser metadata grant.
pub(in crate::business_os) fn for_thread(
    conn: &Connection,
    owner: &str,
    thread_id: &str,
) -> anyhow::Result<Option<SupervisorBinding>> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_supervisor_bindings')",
        [], |row| row.get(0),
    )?;
    if !exists { return Ok(None); }
    let project: Option<String> = conn.query_row(
        "SELECT project_id FROM workjet_supervisor_bindings WHERE owner_user_id=?1 AND thread_id=?2",
        rusqlite::params![owner, thread_id], |row| row.get(0),
    ).optional()?;
    Ok(project.map(|project_id| SupervisorBinding {
        project_id, thread_id: thread_id.to_owned(),
        thread_key: format!("business-os/threads/{thread_id}"),
    }))
}
