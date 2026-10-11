// Origin: CTOX
// License: AGPL-3.0-only

//! Owner-issued native provenance for an actual Workjet Persistent Worker UUID.
//! This uses the existing DomainEffect/Threads producer, not a second projection.
use super::*;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

pub(in crate::business_os) const CONTRACT: &str = "ctox.workjet.persistent-worker-binding.v1";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::business_os) struct PersistentWorkerBinding {
    pub contract: String,
    pub owner_user_id: String,
    pub project_id: String,
    pub supervisor_thread_id: String,
    pub thread_id: String,
    pub role: String,
    pub revision: u64,
    pub state: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    project_id: String,
    supervisor_thread_id: String,
    thread_id: String,
    expected_revision: u64,
    #[serde(default)]
    title: Option<String>,
}

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_persistent_worker_bindings (
    thread_id TEXT PRIMARY KEY NOT NULL,
    owner_user_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    record_json TEXT NOT NULL
);";

fn canonical_uuid(value: &str) -> anyhow::Result<String> {
    let uuid = uuid::Uuid::parse_str(value).context("thread_id must be a CodeThread UUID")?;
    ensure!(
        !uuid.is_nil() && uuid.to_string() == value,
        "thread_id must be a canonical non-nil UUID"
    );
    Ok(value.to_owned())
}

/// Private provenance only; reading authority never creates a registry/table.
pub(in crate::business_os) fn for_thread(
    conn: &Connection,
    owner: &str,
    thread_id: &str,
) -> anyhow::Result<Option<PersistentWorkerBinding>> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_persistent_worker_bindings')",
        [], |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let record: Option<(String, String, String)> = conn.query_row(
        "SELECT owner_user_id, project_id, record_json FROM workjet_persistent_worker_bindings WHERE thread_id=?1",
        [thread_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;
    let Some((stored_owner, project, raw)) = record else {
        return Ok(None);
    };
    let binding: PersistentWorkerBinding = serde_json::from_str(&raw)?;
    ensure!(
        binding.contract == CONTRACT
            && binding.role == "persistent_worker"
            && binding.revision > 0
            && matches!(binding.state.as_str(), "active" | "revoked")
            && binding.owner_user_id == stored_owner
            && binding.project_id == project
            && binding.thread_id == thread_id,
        "persistent worker provenance is invalid"
    );
    ensure!(
        stored_owner == owner,
        "persistent worker belongs to another owner"
    );
    Ok(Some(binding))
}

fn history(conn: &Connection, binding: &PersistentWorkerBinding) -> anyhow::Result<Value> {
    let thread = outbound_load_record(conn, THREADS, &binding.thread_id)?
        .context("persistent worker history is unavailable")?;
    ensure!(
        thread["id"] == binding.thread_id
            && thread["thread_id"] == binding.thread_id
            && thread["is_deleted"] != true
            && thread["_deleted"] != true
            && thread["owner_user_id"] == binding.owner_user_id
            && thread["source_module"] == "ctox"
            && thread["source_record_type"] == "workjet_project"
            && thread["source_record_id"] == binding.project_id
            && thread["metadata"]["workjet_team_binding"] == serde_json::to_value(binding)?
            && thread["metadata"]["workjet_team_role"] == binding.role,
        "persistent worker history conflicts with its native role or provenance"
    );
    Ok(thread)
}

/// Consumers must carry the entire Owner-issued reference and compare it on
/// EVERY source admission/revalidation; a revoke/rebind invalidates old permits.
pub(in crate::business_os) fn require_active(
    conn: &Connection,
    owner: &str,
    project_id: &str,
    thread_id: &str,
    expected: &PersistentWorkerBinding,
) -> anyhow::Result<PersistentWorkerBinding> {
    let current = for_thread(conn, owner, thread_id)?
        .context("persistent worker is not registered natively")?;
    ensure!(
        current == *expected && current.project_id == project_id && current.state == "active",
        "persistent worker binding is stale, revoked or belongs to another project"
    );
    let supervisor = supervisor_binding::for_thread(conn, owner, &current.supervisor_thread_id)?
        .context("persistent worker supervisor is not registered natively")?;
    ensure!(
        supervisor.project_id == project_id,
        "persistent worker supervisor project differs"
    );
    owned_project(conn, project_id, owner, true)?;
    let thread = history(conn, &current)?;
    ensure!(
        thread["status"] == "open" && thread["archived_at_ms"].as_i64().unwrap_or(0) == 0,
        "persistent worker is archived or closed"
    );
    Ok(current)
}

pub(super) fn apply(
    conn: &Connection,
    command: &BusinessCommand,
    owner: &str,
    projections: &mut Vec<Projection>,
) -> anyhow::Result<Value> {
    let payload: Payload = serde_json::from_value(command.payload.clone())?;
    let binding_action = command.command_type.ends_with(".bind");
    let project = owned_project(conn, &payload.project_id, owner, binding_action)?;
    let thread_id = canonical_uuid(&payload.thread_id)?;
    let supervisor_id = canonical_uuid(&payload.supervisor_thread_id)?;
    ensure!(
        thread_id != supervisor_id,
        "a project supervisor cannot be enrolled as a persistent worker"
    );
    let supervisor = supervisor_binding::for_thread(conn, owner, &supervisor_id)?
        .context("project supervisor must be enrolled first")?;
    ensure!(
        supervisor.project_id == payload.project_id,
        "supervisor belongs to another project"
    );
    conn.execute_batch(SCHEMA)?;
    let previous = for_thread(conn, owner, &thread_id)?;
    let (mut binding, mut thread) = if let Some(previous) = previous {
        ensure!(
            previous.project_id == payload.project_id
                && previous.supervisor_thread_id == supervisor_id,
            "persistent worker affiliation is immutable"
        );
        ensure!(
            previous.revision == payload.expected_revision,
            "persistent worker revision conflict"
        );
        let thread = history(conn, &previous)?;
        (previous, thread)
    } else {
        ensure!(
            binding_action && payload.expected_revision == 0,
            "persistent worker registration does not exist"
        );
        ensure!(
            outbound_load_record(conn, THREADS, &thread_id)?.is_none(),
            "refusing to adopt unrelated existing chat history"
        );
        let title = required(
            payload.title.as_deref().unwrap_or("Persistent Worker"),
            "title",
            200,
        )?;
        let now = super::super::store::now_ms() as i64;
        let binding = PersistentWorkerBinding {
            contract: CONTRACT.into(),
            owner_user_id: owner.into(),
            project_id: payload.project_id.clone(),
            supervisor_thread_id: supervisor_id,
            thread_id: thread_id.clone(),
            role: "persistent_worker".into(),
            revision: 1,
            state: "active".into(),
        };
        let thread = json!({
            "id": thread_id, "thread_id": thread_id, "title": title, "kind": "chat", "status": "open",
            "owner_user_id": owner, "created_by_id": owner, "participant_ids": [owner],
            "watcher_user_ids": [], "assigned_user_id": "",
            "source_module": "ctox", "source_record_type": "workjet_project",
            "source_record_id": payload.project_id, "source_label": project["name"],
            "metadata": {}, "created_at_ms": now, "updated_at_ms": now, "is_deleted": false,
            "last_message_id": "", "last_message_at_ms": 0, "pending_approval_count": 0,
            "last_seen_by_user": {}, "snoozed_until_ms": 0, "archived_at_ms": 0
        });
        (binding, thread)
    };
    let desired_state = if binding_action { "active" } else { "revoked" };
    let unchanged = binding.state == desired_state && payload.expected_revision != 0;
    if !unchanged {
        if payload.expected_revision != 0 {
            binding.revision = binding
                .revision
                .checked_add(1)
                .context("persistent worker revision exhausted")?;
        }
        binding.state = desired_state.into();
        thread["metadata"]["workjet_team_binding"] = serde_json::to_value(&binding)?;
        thread["metadata"]["workjet_team_role"] = json!("persistent_worker");
        conn.execute(
            "INSERT INTO workjet_persistent_worker_bindings(thread_id,owner_user_id,project_id,record_json)
             VALUES (?1,?2,?3,?4) ON CONFLICT(thread_id) DO UPDATE SET record_json=excluded.record_json",
            params![thread_id, owner, binding.project_id, serde_json::to_string(&binding)?],
        )?;
        persist(conn, THREADS, &thread_id, thread, projections)?;
    } else {
        // Repeated confirmation preserves the whole existing history and timestamps.
        projections.push(Projection {
            collection: THREADS,
            id: thread_id,
        });
    }
    Ok(json!({"ok":true,"contract":CONTRACT,"binding":binding}))
}
