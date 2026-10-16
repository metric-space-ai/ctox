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
/// Declared as manifest right `own|role:machines.manage`; the v4 rights layer
/// (P6 scope/_rights) is its consumer, the v3 replication filter has no
/// per-record hook for it yet.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn may_read(viewer_id: &str, viewer_manages_machines: bool, device: &Value) -> bool {
    viewer_manages_machines
        || (!viewer_id.is_empty()
            && device.get("user_id").and_then(Value::as_str) == Some(viewer_id))
}

/// Resolve through the retained BOS owner, never from app payload or a guessed
/// same-creator association. Keep the public device document schema unchanged.
pub(crate) fn active_machine_enrollment(
    conn: &Connection,
    device_id: &str,
) -> anyhow::Result<Option<super::machine_enrollment::ActiveDeviceEnrollment>> {
    Ok(super::machine_enrollment::active_device_enrollments(conn)?.remove(device_id))
}

/// All active device documents of this instance.
pub(crate) fn list(conn: &Connection) -> anyhow::Result<Vec<Value>> {
    if !table_exists(conn, "business_mobile_invites")? {
        return Ok(Vec::new());
    }
    // Same migration as the binding owner: guarantees the §16.43 columns.
    super::mobile_invites::ensure_table(conn)?;
    super::machine_enrollment::ensure_table(conn)?;
    ensure_presence_table(conn)?;
    let mut statement = conn.prepare(
        "SELECT i.device_pairing_id, i.display_name,
                COALESCE(m.created_by, i.created_by_user_id, i.user_id),
                COALESCE(i.redeemed_at_ms, i.created_at_ms), m.enrollment_id, m.label,
                p.connected, p.last_seen_at_ms, i.device_kind, i.device_platform
         FROM business_mobile_invites i
         LEFT JOIN business_machine_enrollments m ON m.bound_user_id = i.user_id
         LEFT JOIN business_device_presence p ON p.id = i.device_pairing_id
         WHERE i.revoked_at_ms IS NULL AND i.device_pairing_id IS NOT NULL
         ORDER BY 4 DESC",
    )?;
    let rows = statement
        .query_map([], |r| {
            let machine: Option<String> = r.get(4)?;
            let label: String = r
                .get::<_, Option<String>>(5)?
                .unwrap_or(r.get::<_, String>(1)?);
            let declared: Option<String> = r.get(8)?;
            // Enrollment beats declaration; undeclared legacy edges stay mobile.
            let (kind, source) = match (&machine, &declared) {
                (Some(_), _) => ("machine".to_owned(), "enrollment"),
                (None, Some(kind)) => (kind.clone(), "client"),
                (None, None) => ("mobile".to_owned(), "legacy"),
            };
            let mut doc = json!({
                "id": r.get::<_, String>(0)?,
                "kind": kind,
                "kind_source": source,
                "label": label,
                "user_id": r.get::<_, String>(2)?,
                "created_at_ms": r.get::<_, i64>(3)?,
                "connected": r.get::<_, Option<i64>>(6)?.unwrap_or(0) != 0,
                "last_seen_at_ms": r.get::<_, Option<i64>>(7)?.unwrap_or(0),
            });
            if let Some(platform) = r.get::<_, Option<String>>(9)? {
                doc["platform"] = json!(platform);
            }
            Ok(doc)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// In-memory presence of admitted peers (§16.43). Decides without SQL when a
/// projection write is due: connect/disconnect immediately, `last_seen_at_ms`
/// at most every 30 s while connected. No client heartbeat traffic.
#[derive(Default)]
pub(crate) struct PresenceTracker {
    written_at: std::collections::BTreeMap<String, i64>,
}
impl PresenceTracker {
    /// Devices persisted as connected by a previous run start as connected so
    /// the first tick flips the ones that did not come back.
    pub(crate) fn resume(conn: &Connection) -> anyhow::Result<Self> {
        ensure_presence_table(conn)?;
        let mut statement = conn
            .prepare("SELECT id, written_at_ms FROM business_device_presence WHERE connected=1")?;
        let written_at = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Self { written_at })
    }

    /// `current` = device_pairing_ids of currently admitted peers. Returns the
    /// `(id, connected)` writes that are due now.
    pub(crate) fn tick(
        &mut self,
        current: &std::collections::BTreeSet<String>,
        now: i64,
    ) -> Vec<(String, bool)> {
        let mut due = Vec::new();
        for id in current {
            match self.written_at.get(id) {
                Some(written) if now - written < LAST_SEEN_THROTTLE_MS => {}
                _ => {
                    self.written_at.insert(id.clone(), now);
                    due.push((id.clone(), true));
                }
            }
        }
        let gone: Vec<String> = self
            .written_at
            .keys()
            .filter(|id| !current.contains(*id))
            .cloned()
            .collect();
        for id in gone {
            self.written_at.remove(&id);
            due.push((id, false));
        }
        due
    }
}

/// device_pairing_id behind a captured peer capability token.
fn device_for_capability(root: &std::path::Path, token: &str) -> Option<String> {
    let claims = super::store::verified_webrtc_capability_claims(root, token)?;
    if let Some(binding) = claims.device_binding {
        return Some(binding.device_pairing_id);
    }
    let conn = super::store::open_store(root).ok()?;
    device_id_for_user(&conn, &claims.user_id).ok().flatten()
}

type PeerPool = rxdb::plugins::replication_webrtc::index_mod::RxWebRTCReplicationPool<
    rxdb::plugins::replication_webrtc::WebRTCRsConnectionHandler,
>;

/// Presence feed from the existing replication: every admitted peer whose
/// capability resolves to a device-proof edge counts as that device being
/// connected. Polls in-memory transport state each second; writes only when
/// the tracker says so. Holds the pools weakly and ends with them.
pub(super) async fn presence_loop(root: std::path::PathBuf, pools: Vec<std::sync::Weak<PeerPool>>) {
    use rxdb::plugins::replication_webrtc::webrtc_types::WebRTCConnectionHandler;
    let Ok(mut tracker) =
        super::store::open_store(&root).and_then(|conn| PresenceTracker::resume(&conn))
    else {
        return;
    };
    let mut resolved: std::collections::HashMap<String, Option<String>> = Default::default();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        interval.tick().await;
        let mut tokens = Vec::new();
        let mut alive = false;
        for pool in &pools {
            let Some(pool) = pool.upgrade() else { continue };
            alive = true;
            for peer in pool.connection_handler.current_connections() {
                if let Some(token) = pool.connection_handler.peer_capability_token(&peer) {
                    tokens.push(token);
                }
            }
        }
        if !alive {
            return;
        }
        resolved.retain(|token, _| tokens.contains(token));
        let mut current = std::collections::BTreeSet::new();
        for token in tokens {
            let root = root.clone();
            let device = match resolved.get(&token) {
                Some(device) => device.clone(),
                None => {
                    let lookup = token.clone();
                    let device =
                        tokio::task::spawn_blocking(move || device_for_capability(&root, &lookup))
                            .await
                            .ok()
                            .flatten();
                    resolved.insert(token, device.clone());
                    device
                }
            };
            current.extend(device);
        }
        let now = chrono::Utc::now().timestamp_millis();
        for (id, connected) in tracker.tick(&current, now) {
            let root = root.clone();
            let _ = tokio::task::spawn_blocking(move || presence(&root, &id, connected, now)).await;
        }
    }
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
    anyhow::ensure!(command.command_type == UNBIND, "unsupported device command");
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
