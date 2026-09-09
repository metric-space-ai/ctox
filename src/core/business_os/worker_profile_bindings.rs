// Origin: CTOX
// License: AGPL-3.0-only

//! An owner-authorized reference to WorkjetWorkerProfile.id and an optional
//! existing Crew identity. Profile settings and Crew persona are not copied.
use super::project_chats::{persist, required, stable_id, text, Projection};
use super::store::{outbound_load_record, BusinessCommand};
use super::store_workjet_computers::require_assigned_workjet_computer;
use anyhow::{ensure, Context};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

pub(super) const COLLECTION: &str = "workjet_worker_profile_bindings";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindPayload {
    worker_profile_id: String,
    computer_id: String,
    #[serde(default)]
    crew_member_id: Option<String>,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnbindPayload {
    worker_profile_id: String,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

pub(super) fn binding_id(owner: &str, worker_id: &str) -> String {
    stable_id("workjet_profile", &[owner, worker_id])
}

pub(super) fn require_active(
    conn: &Connection,
    owner: &str,
    worker_id: &str,
) -> anyhow::Result<Value> {
    let binding = outbound_load_record(conn, COLLECTION, &binding_id(owner, worker_id))?
        .context("worker profile is not registered for this user and instance")?;
    ensure!(
        binding["owner_user_id"] == owner
            && binding["worker_profile_id"] == worker_id
            && binding["status"] == "active"
            && binding["is_deleted"] != true,
        "worker profile binding is unavailable"
    );
    require_assigned_workjet_computer(conn, text(&binding, "computer_id")?, owner)?;
    Ok(binding)
}

pub(super) fn apply_command(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    owner: &str,
    projections: &mut Vec<Projection>,
) -> anyhow::Result<Value> {
    let now = super::store::now_ms() as i64;
    if command.command_type == "ctox.workjet.worker_profile.unbind" {
        let payload: UnbindPayload = serde_json::from_value(command.payload.clone())?;
        let worker_id = required(&payload.worker_profile_id, "worker_profile_id", 256)?;
        let id = binding_id(owner, &worker_id);
        if let Some(mut binding) = outbound_load_record(conn, COLLECTION, &id)? {
            ensure!(
                binding["owner_user_id"] == owner,
                "worker binding owner mismatch"
            );
            binding["status"] = json!("inactive");
            persist(conn, COLLECTION, &id, binding, projections)?;
        }
        return Ok(json!({"ok": true, "binding_id": id, "active": false}));
    }

    let payload: BindPayload = serde_json::from_value(command.payload.clone())?;
    let worker_id = required(&payload.worker_profile_id, "worker_profile_id", 256)?;
    let computer_id = required(&payload.computer_id, "computer_id", 256)?;
    require_assigned_workjet_computer(conn, &computer_id, owner)?;
    let member_id = payload
        .crew_member_id
        .map(|id| required(&id, "crew_member_id", 256))
        .transpose()?;
    if let Some(member_id) = &member_id {
        // Read the existing Core authority, not a peer-supplied appearance
        // record. Binding an identity never creates a Crew member or a run.
        let core = Connection::open_with_flags(
            crate::paths::core_db(root),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let available = crate::crew::members(&core)?
            .iter()
            .any(|member| member.id == *member_id && !member.archived);
        ensure!(available, "Crew member is unavailable or archived");
    }
    let id = binding_id(owner, &worker_id);
    let existing = outbound_load_record(conn, COLLECTION, &id)?;
    let mut binding = json!({
        "id": id, "worker_profile_id": worker_id, "computer_id": computer_id,
        "owner_user_id": owner, "status": "active",
        "created_at_ms": existing.as_ref().and_then(|v| v["created_at_ms"].as_i64()).unwrap_or(now),
        "updated_at_ms": now, "is_deleted": false,
    });
    if let Some(member_id) = member_id {
        binding["crew_member_id"] = json!(member_id);
    }
    persist(conn, COLLECTION, &id, binding, projections)?;
    Ok(json!({"ok": true, "binding_id": id, "worker_profile_id": worker_id, "active": true}))
}
