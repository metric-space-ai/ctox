// Origin: CTOX
// License: AGPL-3.0-only
//
// IoT alarm store + lifecycle. Ported domain semantics from OpenRemote
// (AGPL-3.0, archive/openremote, HEAD 22a42a7); persistence reimplemented on
// CTOX-native SQLite (runtime/ctox.sqlite3 via crate::paths::core_db).
//
// The Alarm domain type mirrors org.openremote.model.alarm.Alarm and the
// persisted SentAlarm row; the lifecycle (create / get / list / status
// transitions / asset links) is ported from AlarmService.
//
// ref: Alarm.java:25-174
// ref: AlarmService.java:172-482
//
// Time model (see mod.rs): created/last_modified are i64 epoch-ms (domain
// time, §2A.13) via now_ms(); created_at/updated_at audit columns are RFC-3339
// via now_iso().

use crate::iot::model::*;
use crate::iot::{now_iso, now_ms, Result};
use anyhow::{bail, Context};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

// ---------------------------------------------------------------------------
// Domain enums (names match upstream org.openremote.model.alarm.Alarm)
// ---------------------------------------------------------------------------

/// ref: Alarm.java:43-47 (enum Severity { LOW, MEDIUM, HIGH })
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Severity {
    Low,
    Medium,
    High,
}

/// ref: Alarm.java:35-41 (enum Status { OPEN, ACKNOWLEDGED, IN_PROGRESS, RESOLVED, CLOSED })
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Status {
    Open,
    Acknowledged,
    InProgress,
    Resolved,
    Closed,
}

impl Status {
    /// Legal status transitions.
    ///
    /// Upstream models the lifecycle as the ordered enum
    /// OPEN → ACKNOWLEDGED → IN_PROGRESS → RESOLVED → CLOSED (Alarm.java:35-41).
    /// We enforce forward progression along that order, plus the operationally
    /// required escapes that the UI exposes:
    ///   * any non-closed state may be CLOSED directly (abandon / dismiss),
    ///   * RESOLVED may be re-OPENED (regression / reopen),
    ///   * idempotent self-transition (status set to its current value) is legal,
    ///   * CLOSED is terminal (no outgoing transition except to itself).
    /// ref: Alarm.java:35-41 (Status enum ordering is the lifecycle)
    /// ref: AlarmService.java:308-323 (updateAlarm sets status with no extra guard)
    pub(crate) fn can_transition_to(self, next: Status) -> bool {
        use Status::*;
        if self == next {
            // Idempotent self-transition is always legal (no-op update).
            return true;
        }
        match self {
            Open => matches!(next, Acknowledged | InProgress | Resolved | Closed),
            Acknowledged => matches!(next, InProgress | Resolved | Closed),
            InProgress => matches!(next, Resolved | Closed),
            Resolved => matches!(next, Open | Closed),
            // CLOSED is terminal.
            Closed => false,
        }
    }
}

/// ref: Alarm.java:26-33 (enum Source); CTOX adds AGENT semantics for harness-raised alarms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Source {
    Manual,
    Client,
    GlobalRuleset,
    RealmRuleset,
    AssetRuleset,
    Agent,
}

// ---------------------------------------------------------------------------
// Alarm (persisted SentAlarm row)
// ---------------------------------------------------------------------------

/// A persisted alarm. Mirrors the upstream SentAlarm row built from an Alarm
/// plus the server-assigned id / timestamps.
/// ref: Alarm.java:52-72 (fields)
/// ref: AlarmService.java:181-191 (SentAlarm construction)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Alarm {
    /// Server-assigned id (22-char Base62, reusing the asset id generator).
    pub id: String,
    pub realm: String,
    pub title: String,
    #[serde(default)]
    pub content: Option<String>,
    pub severity: Severity,
    pub status: Status,
    /// Assignee user id (null == unassigned). ref: Alarm.java:63 (assigneeId)
    #[serde(default)]
    pub assignee_id: Option<String>,
    pub source: Source,
    pub source_id: String,
    /// Domain time, epoch-ms. ref: AlarmService.java:190 (setCreatedOn)
    pub created: i64,
    /// Domain time, epoch-ms. ref: AlarmService.java:191 (setLastModified)
    pub last_modified: i64,
    /// Linked asset ids. ref: AlarmAssetLink (sentalarm_id ↔ asset_id)
    #[serde(default)]
    pub asset_ids: Vec<String>,
}

impl Alarm {
    /// Generate a server-side alarm id (22-char Base62, shared generator).
    fn generate_id() -> String {
        Asset::generate_id()
    }
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Low => "LOW",
        Severity::Medium => "MEDIUM",
        Severity::High => "HIGH",
    }
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Open => "OPEN",
        Status::Acknowledged => "ACKNOWLEDGED",
        Status::InProgress => "IN_PROGRESS",
        Status::Resolved => "RESOLVED",
        Status::Closed => "CLOSED",
    }
}

// ---------------------------------------------------------------------------
// Store: open + schema
// ---------------------------------------------------------------------------

/// Open the shared CTOX runtime store and ensure the alarm schema exists.
/// Mirrors business_os::store::open_store (WAL + busy_timeout house idiom).
pub(crate) fn open(root: &Path) -> Result<Connection> {
    let path = crate::paths::core_db(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime dir {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open core db at {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure IoT alarm SQLite busy_timeout")?;
    let ms = crate::persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL; PRAGMA busy_timeout={ms};"
    ))
    .context("failed to set IoT alarm SQLite pragmas")?;
    init_schema(&conn)?;
    Ok(conn)
}

/// Canonical JSON in `data`; light index columns alongside for querying.
fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS iot_alarms (
            id          TEXT PRIMARY KEY,
            realm       TEXT NOT NULL,
            status      TEXT NOT NULL,
            severity    TEXT NOT NULL,
            created_ms  INTEGER NOT NULL,
            data        TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_iot_alarms_realm ON iot_alarms(realm);
        CREATE INDEX IF NOT EXISTS idx_iot_alarms_status ON iot_alarms(realm, status);
        CREATE INDEX IF NOT EXISTS idx_iot_alarms_created ON iot_alarms(realm, created_ms);",
    )
    .context("failed to create iot_alarms schema")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

/// Parameters for raising a new alarm (the inbound Alarm before persistence).
/// ref: Alarm.java:74-82 (constructor: status defaults to OPEN)
pub(crate) struct NewAlarm {
    pub realm: String,
    pub title: String,
    pub content: Option<String>,
    pub severity: Severity,
    pub assignee_id: Option<String>,
    pub source: Source,
    pub source_id: String,
}

/// Persist a new alarm. Required fields are non-null-checked exactly as
/// upstream sendAlarm; new alarms always start OPEN.
/// ref: AlarmService.java:172-204 (sendAlarm)
/// ref: Alarm.java:75-82 (status = Status.OPEN at construction)
pub(crate) fn create(conn: &Connection, new: NewAlarm, asset_ids: Vec<String>) -> Result<Alarm> {
    // Objects.requireNonNull(...) — ref: AlarmService.java:173-178
    if new.realm.trim().is_empty() {
        bail!("Alarm realm cannot be null");
    }
    if new.title.trim().is_empty() {
        bail!("Alarm title cannot be null");
    }
    if new.source_id.trim().is_empty() {
        bail!("Source ID cannot be null");
    }

    // Instant timestamp = current time millis — ref: AlarmService.java:180
    let ts = now_ms();
    let alarm = Alarm {
        id: Alarm::generate_id(),
        realm: new.realm,
        title: new.title,
        content: new.content,
        severity: new.severity,
        status: Status::Open,
        assignee_id: new.assignee_id,
        source: new.source,
        source_id: new.source_id,
        created: ts,
        last_modified: ts,
        // linkAssets(...) when assetIds present — ref: AlarmService.java:193-195
        asset_ids,
    };
    insert(conn, &alarm)?;
    Ok(alarm)
}

fn insert(conn: &Connection, alarm: &Alarm) -> Result<()> {
    let data = serde_json::to_string(alarm).context("failed to serialize alarm")?;
    let now = now_iso();
    conn.execute(
        "INSERT INTO iot_alarms (id, realm, status, severity, created_ms, data, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            alarm.id,
            alarm.realm,
            status_str(alarm.status),
            severity_str(alarm.severity),
            alarm.created,
            data,
            now,
        ],
    )
    .context("failed to insert alarm")?;
    Ok(())
}

/// Persist a mutated alarm (status/assignee/links). Bumps last_modified to
/// the current epoch-ms and refreshes the updated_at audit column.
/// ref: AlarmService.java:308-323 (updateAlarm: lastModified = currentTimeMillis)
fn persist_update(conn: &Connection, alarm: &mut Alarm) -> Result<()> {
    alarm.last_modified = now_ms();
    let data = serde_json::to_string(alarm).context("failed to serialize alarm")?;
    let now = now_iso();
    let changed = conn
        .execute(
            "UPDATE iot_alarms
             SET realm = ?2, status = ?3, severity = ?4, data = ?5, updated_at = ?6
             WHERE id = ?1",
            params![
                alarm.id,
                alarm.realm,
                status_str(alarm.status),
                severity_str(alarm.severity),
                data,
                now,
            ],
        )
        .context("failed to update alarm")?;
    if changed == 0 {
        bail!("Alarm does not exist: {}", alarm.id);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Read
// ---------------------------------------------------------------------------

/// Retrieve a single alarm by id, or Err if it does not exist.
/// ref: AlarmService.java:385-401 (getAlarm; throws EntityNotFoundException)
pub(crate) fn get(conn: &Connection, id: &str) -> Result<Alarm> {
    let row: Option<String> = conn
        .query_row(
            "SELECT data FROM iot_alarms WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .context("failed to query alarm")?;
    match row {
        Some(data) => serde_json::from_str(&data).context("failed to deserialize alarm"),
        None => bail!("Alarm does not exist: {id}"),
    }
}

/// Realm-scoped alarm fetch (multi-realm isolation, Phase 2). Returns the alarm
/// ONLY when it exists AND belongs to `realm`; a cross-realm id errors with the
/// same "does not exist" message, so a caller cannot read or mutate another
/// realm's alarm by id. The realm is enforced at the SQL layer.
pub(crate) fn get_in_realm(conn: &Connection, id: &str, realm: &str) -> Result<Alarm> {
    let row: Option<String> = conn
        .query_row(
            "SELECT data FROM iot_alarms WHERE id = ?1 AND realm = ?2",
            params![id, realm],
            |r| r.get(0),
        )
        .optional()
        .context("failed to query alarm")?;
    match row {
        Some(data) => serde_json::from_str(&data).context("failed to deserialize alarm"),
        None => bail!("Alarm does not exist: {id}"),
    }
}

/// List alarms in a realm, optionally filtered by status, newest first.
/// ref: AlarmService.java:430-457 (getAlarms; order by createdOn desc)
pub(crate) fn list(conn: &Connection, realm: &str, status: Option<Status>) -> Result<Vec<Alarm>> {
    // ref: AlarmService.java:432-450 (StringBuilder where realm [+ status] order desc)
    let mut sql = String::from("SELECT data FROM iot_alarms WHERE realm = ?1");
    if status.is_some() {
        sql.push_str(" AND status = ?2");
    }
    sql.push_str(" ORDER BY created_ms DESC");

    let mut stmt = conn.prepare(&sql).context("failed to prepare alarm list")?;
    let map_row = |r: &rusqlite::Row| -> rusqlite::Result<String> { r.get(0) };
    let rows: Vec<String> = match status {
        Some(s) => stmt
            .query_map(params![realm, status_str(s)], map_row)?
            .collect::<rusqlite::Result<_>>()?,
        None => stmt
            .query_map(params![realm], map_row)?
            .collect::<rusqlite::Result<_>>()?,
    };
    rows.iter()
        .map(|d| serde_json::from_str::<Alarm>(d).context("failed to deserialize alarm"))
        .collect()
}

// ---------------------------------------------------------------------------
// Lifecycle transitions
// ---------------------------------------------------------------------------

/// Apply a status transition, enforcing the legal lifecycle. Rejects an
/// illegal transition before touching the store.
/// ref: Status enum ordering (Alarm.java:35-41) + AlarmService.java:308-323
pub(crate) fn update_status(conn: &Connection, id: &str, next: Status) -> Result<Alarm> {
    let mut alarm = get(conn, id)?;
    if !alarm.status.can_transition_to(next) {
        bail!(
            "illegal alarm status transition {} -> {}",
            status_str(alarm.status),
            status_str(next)
        );
    }
    alarm.status = next;
    persist_update(conn, &mut alarm)?;
    Ok(alarm)
}

/// Realm-scoped status transition (multi-realm isolation). Identical to
/// `update_status` but loads the alarm through `get_in_realm`, so a cross-realm
/// alarm id is rejected as "does not exist" before any mutation.
pub(crate) fn update_status_in_realm(
    conn: &Connection,
    id: &str,
    realm: &str,
    next: Status,
) -> Result<Alarm> {
    let mut alarm = get_in_realm(conn, id, realm)?;
    if !alarm.status.can_transition_to(next) {
        bail!(
            "illegal alarm status transition {} -> {}",
            status_str(alarm.status),
            status_str(next)
        );
    }
    alarm.status = next;
    persist_update(conn, &mut alarm)?;
    Ok(alarm)
}

/// Realm-scoped assignee update (multi-realm isolation).
pub(crate) fn assign_in_realm(
    conn: &Connection,
    id: &str,
    realm: &str,
    assignee_id: Option<String>,
) -> Result<Alarm> {
    let mut alarm = get_in_realm(conn, id, realm)?;
    alarm.assignee_id = assignee_id;
    persist_update(conn, &mut alarm)?;
    Ok(alarm)
}

/// Acknowledge an open alarm. ref: Status.ACKNOWLEDGED (Alarm.java:37)
pub(crate) fn acknowledge(conn: &Connection, id: &str) -> Result<Alarm> {
    update_status(conn, id, Status::Acknowledged)
}

/// Mark an alarm in-progress. ref: Status.IN_PROGRESS (Alarm.java:38)
pub(crate) fn start_progress(conn: &Connection, id: &str) -> Result<Alarm> {
    update_status(conn, id, Status::InProgress)
}

/// Resolve an alarm. ref: Status.RESOLVED (Alarm.java:39)
pub(crate) fn resolve(conn: &Connection, id: &str) -> Result<Alarm> {
    update_status(conn, id, Status::Resolved)
}

/// Close an alarm (terminal). ref: Status.CLOSED (Alarm.java:40)
pub(crate) fn close(conn: &Connection, id: &str) -> Result<Alarm> {
    update_status(conn, id, Status::Closed)
}

/// Assign (or clear, with None) the alarm assignee; bumps last_modified.
/// ref: AlarmService.java:309-323 (assigneeId updated in updateAlarm)
pub(crate) fn assign(conn: &Connection, id: &str, assignee_id: Option<String>) -> Result<Alarm> {
    let mut alarm = get(conn, id)?;
    alarm.assignee_id = assignee_id;
    persist_update(conn, &mut alarm)?;
    Ok(alarm)
}

// ---------------------------------------------------------------------------
// Asset links
// ---------------------------------------------------------------------------

/// Link assets to an alarm (set semantics, deduplicated — mirrors the
/// `on conflict ... do nothing` upstream insert).
/// ref: AlarmService.java:336-364 (linkAssets)
pub(crate) fn link_assets(conn: &Connection, id: &str, asset_ids: &[String]) -> Result<Alarm> {
    // validateAssetIds — ref: AlarmService.java:158-167
    if asset_ids.iter().any(|a| a.trim().is_empty()) {
        bail!("Missing asset ID");
    }
    let mut alarm = get(conn, id)?;
    for asset_id in asset_ids {
        if !alarm.asset_ids.iter().any(|a| a == asset_id) {
            alarm.asset_ids.push(asset_id.clone());
        }
    }
    persist_update(conn, &mut alarm)?;
    Ok(alarm)
}

/// Remove asset links from an alarm.
pub(crate) fn unlink_assets(conn: &Connection, id: &str, asset_ids: &[String]) -> Result<Alarm> {
    let mut alarm = get(conn, id)?;
    alarm
        .asset_ids
        .retain(|a| !asset_ids.iter().any(|r| r == a));
    persist_update(conn, &mut alarm)?;
    Ok(alarm)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn sample(realm: &str) -> NewAlarm {
        NewAlarm {
            realm: realm.to_string(),
            title: "High CPU".to_string(),
            content: Some("cpu > 95%".to_string()),
            severity: Severity::High,
            assignee_id: None,
            source: Source::Agent,
            source_id: "agent-1".to_string(),
        }
    }

    #[test]
    fn create_ack_assign_resolve_lifecycle() {
        let root = temp_root();
        let conn = open(root.path()).unwrap();

        let alarm = create(&conn, sample("master"), vec![]).unwrap();
        assert_eq!(alarm.status, Status::Open);
        assert_eq!(alarm.created, alarm.last_modified);

        let acked = acknowledge(&conn, &alarm.id).unwrap();
        assert_eq!(acked.status, Status::Acknowledged);

        let assigned = assign(&conn, &alarm.id, Some("user-42".into())).unwrap();
        assert_eq!(assigned.assignee_id.as_deref(), Some("user-42"));
        assert_eq!(assigned.status, Status::Acknowledged, "assign keeps status");

        let progressing = start_progress(&conn, &alarm.id).unwrap();
        assert_eq!(progressing.status, Status::InProgress);

        let resolved = resolve(&conn, &alarm.id).unwrap();
        assert_eq!(resolved.status, Status::Resolved);
        assert!(resolved.last_modified >= alarm.created);
    }

    #[test]
    fn illegal_transition_rejected() {
        let root = temp_root();
        let conn = open(root.path()).unwrap();
        let alarm = create(&conn, sample("master"), vec![]).unwrap();

        // OPEN -> CLOSED then CLOSED is terminal: any further transition fails.
        let closed = close(&conn, &alarm.id).unwrap();
        assert_eq!(closed.status, Status::Closed);
        let err = acknowledge(&conn, &alarm.id).unwrap_err();
        assert!(
            err.to_string().contains("illegal alarm status transition"),
            "got: {err}"
        );

        // RESOLVED cannot go back to ACKNOWLEDGED (only Open or Closed).
        let a2 = create(&conn, sample("master"), vec![]).unwrap();
        resolve(&conn, &a2.id).unwrap();
        let err2 = update_status(&conn, &a2.id, Status::Acknowledged).unwrap_err();
        assert!(err2.to_string().contains("illegal"), "got: {err2}");
        // ...but RESOLVED -> OPEN (reopen) is legal.
        let reopened = update_status(&conn, &a2.id, Status::Open).unwrap();
        assert_eq!(reopened.status, Status::Open);
    }

    #[test]
    fn list_by_status() {
        let root = temp_root();
        let conn = open(root.path()).unwrap();

        let a = create(&conn, sample("master"), vec![]).unwrap();
        let b = create(&conn, sample("master"), vec![]).unwrap();
        let _other_realm = create(&conn, sample("tenant-2"), vec![]).unwrap();
        acknowledge(&conn, &b.id).unwrap();

        let all_master = list(&conn, "master", None).unwrap();
        assert_eq!(all_master.len(), 2);
        // Newest first.
        assert!(all_master[0].created >= all_master[1].created);

        let open_only = list(&conn, "master", Some(Status::Open)).unwrap();
        assert_eq!(open_only.len(), 1);
        assert_eq!(open_only[0].id, a.id);

        let acked = list(&conn, "master", Some(Status::Acknowledged)).unwrap();
        assert_eq!(acked.len(), 1);
        assert_eq!(acked[0].id, b.id);

        let tenant = list(&conn, "tenant-2", None).unwrap();
        assert_eq!(tenant.len(), 1);
    }

    #[test]
    fn sqlite_round_trip_and_links() {
        let root = temp_root();
        // Persist with one connection...
        let id = {
            let conn = open(root.path()).unwrap();
            let alarm = create(&conn, sample("master"), vec!["asset-a".into()]).unwrap();
            link_assets(&conn, &alarm.id, &["asset-b".into(), "asset-a".into()]).unwrap();
            alarm.id
        };
        // ...and re-read with a fresh connection on the same core db.
        let conn2 = open(root.path()).unwrap();
        let loaded = get(&conn2, &id).unwrap();
        assert_eq!(loaded.realm, "master");
        assert_eq!(loaded.title, "High CPU");
        assert_eq!(loaded.content.as_deref(), Some("cpu > 95%"));
        assert_eq!(loaded.severity, Severity::High);
        assert_eq!(loaded.source, Source::Agent);
        // Dedup: asset-a was linked at create and again via link_assets.
        assert_eq!(loaded.asset_ids, vec!["asset-a", "asset-b"]);

        let unlinked = unlink_assets(&conn2, &id, &["asset-a".into()]).unwrap();
        assert_eq!(unlinked.asset_ids, vec!["asset-b"]);

        let missing = get(&conn2, "does-not-exist");
        assert!(missing.is_err());
    }
}
