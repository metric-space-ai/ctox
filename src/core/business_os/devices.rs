// Origin: CTOX
// License: AGPL-3.0-only
//! Device projection (Sync v4 §16.43): platform collection `devices`, written
//! only by the core. Source of truth stays the existing device binding
//! (`business_mobile_invites` device-proof edges) plus §16.41 machine
//! enrollments; presence is throttled in `business_device_presence`.
use anyhow::Context;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub(crate) const COLLECTION: &str = "devices";
pub(crate) const UNBIND: &str = "ctox.device.unbind";
/// `last_seen_at_ms` is written at most this often while connected.
pub(crate) const LAST_SEEN_THROTTLE_MS: i64 = 30_000;

fn table_exists(conn: &Connection, name: &str) -> anyhow::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            params![name],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub(crate) fn ensure_presence_table(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS business_device_presence (
            id TEXT PRIMARY KEY,
            connected INTEGER NOT NULL,
            last_seen_at_ms INTEGER NOT NULL,
            written_at_ms INTEGER NOT NULL
        );",
    )?;
    Ok(())
}

/// Throttle decision for one presence observation. Returns `true` when the
/// projection must be written now: immediately on a `connected` change, and
/// while connected at most every 30 s. The observation is always recorded.
pub(crate) fn observe_presence(
    conn: &Connection,
    id: &str,
    connected: bool,
    now: i64,
) -> anyhow::Result<bool> {
    ensure_presence_table(conn)?;
    let previous: Option<(bool, i64)> = conn
        .query_row(
            "SELECT connected, written_at_ms FROM business_device_presence WHERE id=?1",
            params![id],
            |r| Ok((r.get::<_, i64>(0)? != 0, r.get(1)?)),
        )
        .optional()?;
    let due = match previous {
        None => true,
        Some((was, _)) if was != connected => true,
        Some((_, written)) => connected && now - written >= LAST_SEEN_THROTTLE_MS,
    };
    let written = if due {
        now
    } else {
        previous.map_or(now, |(_, written)| written)
    };
    conn.execute(
        "INSERT INTO business_device_presence (id, connected, last_seen_at_ms, written_at_ms)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET connected=excluded.connected,
            last_seen_at_ms=excluded.last_seen_at_ms, written_at_ms=excluded.written_at_ms",
        params![id, connected as i64, now, written],
    )?;
    Ok(due)
}

/// Read right (§16.43): own devices; with `machines.manage` all devices.
pub(crate) fn may_read(viewer_id: &str, viewer_manages_machines: bool, device: &Value) -> bool {
    viewer_manages_machines
        || (!viewer_id.is_empty()
            && device.get("user_id").and_then(Value::as_str) == Some(viewer_id))
}

/// All active device documents of this instance.
pub(crate) fn list(conn: &Connection) -> anyhow::Result<Vec<Value>> {
    if !table_exists(conn, "business_mobile_invites")? {
        return Ok(Vec::new());
    }
    super::machine_enrollment::ensure_table(conn)?;
    ensure_presence_table(conn)?;
    let owner_column = conn
        .prepare("SELECT 1 FROM pragma_table_info('business_mobile_invites') WHERE name='created_by_user_id'")?
        .exists([])?;
    let owner = if owner_column {
        "COALESCE(m.created_by, i.created_by_user_id, i.user_id)"
    } else {
        "COALESCE(m.created_by, i.user_id)"
    };
    let sql = format!(
        "SELECT i.device_pairing_id, i.display_name, {owner},
                COALESCE(i.redeemed_at_ms, i.created_at_ms), m.enrollment_id, m.label,
                p.connected, p.last_seen_at_ms
         FROM business_mobile_invites i
         LEFT JOIN business_machine_enrollments m ON m.bound_user_id = i.user_id
         LEFT JOIN business_device_presence p ON p.id = i.device_pairing_id
         WHERE i.revoked_at_ms IS NULL AND i.device_pairing_id IS NOT NULL
         ORDER BY 4 DESC"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map([], |r| {
            let machine: Option<String> = r.get(4)?;
            let label: String = r
                .get::<_, Option<String>>(5)?
                .unwrap_or(r.get::<_, String>(1)?);
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "kind": if machine.is_some() { "machine" } else { "mobile" },
                "label": label,
                "user_id": r.get::<_, String>(2)?,
                "created_at_ms": r.get::<_, i64>(3)?,
                "connected": r.get::<_, Option<i64>>(6)?.unwrap_or(0) != 0,
                "last_seen_at_ms": r.get::<_, Option<i64>>(7)?.unwrap_or(0),
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The device-proof edge (device_pairing_id) of an authenticated device user.
pub(crate) fn device_id_for_user(
    conn: &Connection,
    user_id: &str,
) -> anyhow::Result<Option<String>> {
    if !table_exists(conn, "business_mobile_invites")? {
        return Ok(None);
    }
    Ok(conn
        .query_row(
            "SELECT device_pairing_id FROM business_mobile_invites
             WHERE user_id=?1 AND revoked_at_ms IS NULL AND device_pairing_id IS NOT NULL",
            params![user_id],
            |r| r.get(0),
        )
        .optional()?)
}

fn project(root: &std::path::Path, id: &str, now: i64) -> anyhow::Result<()> {
    let conn = super::store::open_store(root)?;
    let doc = list(&conn)?
        .into_iter()
        .find(|doc| doc["id"] == id)
        .unwrap_or_else(|| json!({"id": id, "_deleted": true}));
    super::store::upsert_rxdb_collection_record(root, COLLECTION, id, now, doc)
}

/// Presence feed entry point (peer lease / machine heartbeat). Writes the
/// projection only when the throttle says so.
pub(crate) fn presence(
    root: &std::path::Path,
    id: &str,
    connected: bool,
    now: i64,
) -> anyhow::Result<()> {
    let due = observe_presence(&super::store::open_store(root)?, id, connected, now)?;
    if due {
        project(root, id, now)?;
    }
    Ok(())
}

/// `ctox.device.unbind {id}`: explicit admin action, never automatic.
pub(super) fn handle_command(
    root: &std::path::Path,
    command: &super::store::BusinessCommand,
) -> anyhow::Result<Value> {
    use super::policy::BusinessOsPermission;
    use super::store_policy::{enforce_command_policy, CommandPolicyRequirement};
    enforce_command_policy(
        root,
        command,
        |_| {
            Ok(CommandPolicyRequirement::workspace(
                BusinessOsPermission::MachinesManage,
            ))
        },
        |_| {
            let id = command
                .payload
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty() && id.len() <= 256)
                .context("device id is required")?
                .to_owned();
            super::mobile_invites::revoke_by_device_pairing_id(root, &id)?;
            let now = super::store::now_ms() as i64;
            super::store::open_store(root)?
                .execute(
                    "DELETE FROM business_device_presence WHERE id=?1",
                    params![id],
                )
                .ok();
            project(root, &id, now)?;
            super::command_plane::write_rxdb_control_command_outcome(
                root,
                command,
                "completed",
                None,
                Some("completed"),
                json!({"ok": true, "id": id, "unbound": true}),
            )
        },
    )?
    .into_outcome()
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod tests;
