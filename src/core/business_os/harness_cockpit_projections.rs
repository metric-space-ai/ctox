//! Lossy delivery, durable sources: no cockpit write participates in admission or finalization.
#[path = "harness_cockpit_schedule.rs"]
mod schedule;
#[cfg(test)]
#[path = "harness_cockpit_projection_tests.rs"]
mod tests;
use super::{queue_pause_state, queue_retention};
use crate::business_os::store::{self, BusinessProjectionWriter as NativeProjectionWriter};
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub(crate) struct WorkerSnapshot {
    pub service_running: bool,
    pub busy: bool,
    pub worker_active_count: usize,
    pub worker_phase: Option<String>,
    pub active_task_ids: Vec<String>,
    pub last_error: Option<String>,
    pub boot_id: String,
}

/// Successful payloads only: failed RxDB delivery is retried, and restart replays
/// durable sources. Per-root cache entries are removed with their tombstones.
struct BusinessProjectionWriter {
    inner: NativeProjectionWriter,
    payloads: BTreeMap<(String, String), Value>,
    crew_sources: BTreeMap<String, (String, Option<String>, bool)>,
    crew_maintenance_warned: bool,
    // Append-only ledger position, advanced only after successful delivery.
    // Periodic replay repairs dropped notifications and in-place source repairs.
    event_cursor: Option<i64>,
}
impl BusinessProjectionWriter {
    fn open(root: &Path) -> Result<Self> {
        let inner = NativeProjectionWriter::open(root)?;
        inner.source_connection().execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_cockpit_live_records
                ON business_records(collection, record_id) WHERE deleted=0;
             CREATE INDEX IF NOT EXISTS idx_cockpit_event_task_time
                ON business_records(json_extract(payload_json,'$.task_id'),
                    json_extract(payload_json,'$.created_at_ms') DESC, record_id DESC)
                WHERE collection='ctox_harness_events' AND deleted=0;
             CREATE INDEX IF NOT EXISTS idx_cockpit_event_created
                ON business_records(collection, json_extract(payload_json,'$.created_at_ms'), record_id)
                WHERE collection='ctox_harness_events' AND deleted=0;
             CREATE INDEX IF NOT EXISTS idx_cockpit_run_finished
                ON business_records(json_extract(payload_json,'$.finished_at_ms') DESC, record_id DESC)
                WHERE collection='ctox_runs' AND deleted=0;
             CREATE INDEX IF NOT EXISTS idx_cockpit_run_task_finished
                ON business_records(json_extract(payload_json,'$.task_id'),
                    json_extract(payload_json,'$.finished_at_ms') DESC, record_id DESC)
                WHERE collection='ctox_runs' AND deleted=0;
             CREATE INDEX IF NOT EXISTS idx_cockpit_run_crew
                ON business_records(json_extract(payload_json,'$.crew_member_id'), record_id)
                WHERE collection='ctox_runs' AND deleted=0;
             CREATE INDEX IF NOT EXISTS idx_crew_projection_member_state
                ON business_records(json_extract(payload_json,'$.archived'),json_extract(payload_json,'$.state'),record_id)
                WHERE collection='ctox_crew_members' AND deleted=0;
             CREATE INDEX IF NOT EXISTS idx_crew_projection_learning_time
                ON business_records(json_extract(payload_json,'$.member_id'),json_extract(payload_json,'$.created_at_ms'),record_id)
                WHERE collection='ctox_crew_learnings' AND deleted=0;
             CREATE INDEX IF NOT EXISTS idx_crew_projection_learning_member_id
                ON business_records(json_extract(payload_json,'$.member_id'),record_id)
                WHERE collection='ctox_crew_learnings' AND deleted=0;
             CREATE INDEX IF NOT EXISTS idx_crew_projection_learning_confirmed
                ON business_records(json_extract(payload_json,'$.member_id'),json_extract(payload_json,'$.confirmed_by_owner'),record_id)
                WHERE collection='ctox_crew_learnings' AND deleted=0;
             CREATE INDEX IF NOT EXISTS idx_crew_projection_updated
                ON business_records(collection,json_extract(payload_json,'$.updated_at_ms'),record_id)
                WHERE deleted=0 AND collection IN ('ctox_crew_members','ctox_crew_learnings');",

        )?;
        Ok(Self {
            inner,
            payloads: BTreeMap::new(),
            crew_sources: BTreeMap::new(),
            crew_maintenance_warned: false,
            event_cursor: None,
        })
    }
    fn upsert_source_projection(
        &mut self,
        collection: &str,
        id: &str,
        source_ms: i64,
        mut payload: Value,
    ) -> Result<()> {
        let key = (collection.to_string(), id.to_string());
        let mut comparable = payload.clone();
        comparable
            .as_object_mut()
            .map(|object| object.remove("updated_at_ms"));
        if self.payloads.get(&key) == Some(&comparable) {
            return Ok(());
        }
        let observed = Utc::now().timestamp_millis().max(source_ms);
        payload["updated_at_ms"] = json!(observed);
        self.inner
            .upsert_source_projection(collection, id, observed, payload)?;
        if self.inner.delivered_to_rxdb(collection) {
            self.payloads.insert(key, comparable);
        }
        Ok(())
    }
    fn tombstone_source_projection(&mut self, collection: &str, id: &str, now: i64) -> Result<()> {
        self.inner
            .tombstone_source_projection(collection, id, now)?;
        self.payloads
            .remove(&(collection.to_string(), id.to_string()));
        Ok(())
    }
}

fn persisted_snapshot(root: &Path) -> Result<WorkerSnapshot> {
    let conn = store::open_store(root)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM business_records
         WHERE collection='ctox_harness_status' AND record_id='harness' AND deleted=0",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(raw
        .map(|raw| serde_json::from_str(&raw))
        .transpose()?
        .unwrap_or_default())
}

struct Pump {
    wake: mpsc::SyncSender<(PathBuf, u8)>,
    latest: Arc<Mutex<BTreeMap<PathBuf, PublishedSnapshot>>>,
}

struct PublishedSnapshot {
    snapshot: WorkerSnapshot,
    publications: u64,
    changes: u64,
}

fn record_snapshot(
    latest: &mut BTreeMap<PathBuf, PublishedSnapshot>,
    root: &Path,
    snapshot: WorkerSnapshot,
) -> bool {
    use std::collections::btree_map::Entry;
    match latest.entry(root.to_path_buf()) {
        Entry::Vacant(entry) => {
            entry.insert(PublishedSnapshot {
                snapshot,
                publications: 1,
                changes: 1,
            });
            true
        }
        Entry::Occupied(mut entry) => {
            let published = entry.get_mut();
            published.publications += 1;
            if published.snapshot == snapshot {
                return false;
            }
            published.snapshot = snapshot;
            published.changes += 1;
            true
        }
    }
}

#[derive(Default)]
struct PumpWork {
    wakes: u64,
    passes: u64,
    elapsed: Duration,
}

const STATUS: u8 = 1;
const EVENTS: u8 = 2;
const RUNS: u8 = 4;
const QUEUE: u8 = 8;
const CHAT: u8 = 16;
// Core-ledger retention belongs to the periodic pump sweep, not admission wakes.
const MAINTENANCE: u8 = 32;
const ALL: u8 = STATUS | EVENTS | RUNS | QUEUE | CHAT;

fn pump() -> Option<&'static Pump> {
    static PUMP: OnceLock<Option<Pump>> = OnceLock::new();
    PUMP.get_or_init(|| {
        let (wake, receive) = mpsc::sync_channel::<(PathBuf, u8)>(128);
        let latest = Arc::new(Mutex::new(BTreeMap::<PathBuf, PublishedSnapshot>::new()));
        let snapshots = latest.clone();
        let worker = std::thread::Builder::new()
            .name("cockpit-projections".into())
            .spawn(move || {
                let mut roots = BTreeSet::<PathBuf>::new();
                let mut writers = BTreeMap::<PathBuf, BusinessProjectionWriter>::new();
                let mut schedule = schedule::Schedule::default();
                let mut work = BTreeMap::<PathBuf, PumpWork>::new();
                let mut measured_since = Instant::now();
                let mut next_sweep = Instant::now() + Duration::from_secs(60);
                loop {
                    match receive.recv_timeout(schedule.wait(Instant::now(), next_sweep))
                    {
                        Ok((root, flags)) => {
                            work.entry(root.clone()).or_default().wakes += 1;
                            schedule.mark(root, flags);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    // A producer may keep refilling the queue. Bound the drain so
                    // ready roots and maintenance cannot starve under that load.
                    for (root, flags) in receive.try_iter().take(128) {
                        work.entry(root.clone()).or_default().wakes += 1;
                        schedule.mark(root, flags);
                    }
                    if Instant::now() >= next_sweep {
                        for root in &roots {
                            schedule.mark(root.clone(), ALL | MAINTENANCE);
                        }
                        // Never hold the publication lock while logging: worker
                        // snapshot publication must not wait on the log sink.
                        let mut publications = {
                            let mut snapshots = snapshots.lock().unwrap_or_else(|e| e.into_inner());
                            snapshots.iter_mut().map(|(root, published)| {
                                let counts = (root.clone(), (published.publications, published.changes));
                                published.publications = 0;
                                published.changes = 0;
                                counts
                            }).collect::<BTreeMap<_, _>>()
                        };
                        for root in roots.iter().chain(work.keys()) {
                            publications.entry(root.clone()).or_default();
                        }
                        for (root, (publication_count, change_count)) in publications {
                            schedule.mark(root.clone(), ALL | MAINTENANCE);
                            let stats = work.entry(root.clone()).or_default();
                            eprintln!("[ctox cockpit] root={} interval_ms={} snapshot_publications={} snapshot_changes={} wakes={} passes={} projection_ms={}",
                                root.display(), measured_since.elapsed().as_millis(),
                                publication_count, change_count, stats.wakes, stats.passes, stats.elapsed.as_millis());
                        }
                        work.clear();
                        measured_since = Instant::now();
                        next_sweep = Instant::now() + Duration::from_secs(60);
                    }
                    for (root, mut flags) in schedule.take_ready(Instant::now()) {
                        if !crate::paths::core_db(&root).is_file() {
                            schedule.forget(&root);
                            work.remove(&root);
                            continue;
                        }
                        if roots.insert(root.clone()) {
                            flags |= ALL | MAINTENANCE;
                        }
                        let snapshot = snapshots
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .get(&root)
                            .map(|published| published.snapshot.clone());
                        let started = Instant::now();
                        let mut timing = ProjectionTiming::new(&root, flags);
                        let outcome = (|| -> Result<()> {
                            let snapshot = timing.phase("snapshot", || snapshot
                                .map(Ok)
                                .unwrap_or_else(|| persisted_snapshot(&root)))?;
                            if !writers.contains_key(&root) {
                                writers
                                    .insert(root.clone(), timing.phase("writer_open", || BusinessProjectionWriter::open(&root))?);
                            }
                            refresh_measured(
                                &root,
                                &snapshot,
                                writers.get_mut(&root).expect("inserted writer"),
                                flags,
                                &mut timing,
                            )
                        })();
                        let stats = work.entry(root.clone()).or_default();
                        stats.passes += 1;
                        stats.elapsed += started.elapsed();
                        schedule.completed(root.clone(), Instant::now());
                        if let Err(error) = outcome {
                            eprintln!(
                                "[ctox cockpit] projection deferred for {}: {error:#}",
                                root.display()
                            );
                        }
                    }
                    let removed = roots
                        .iter()
                        .filter(|root| !crate::paths::core_db(root).is_file())
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    roots.retain(|root| !removed.contains(root));
                    writers.retain(|root, _| !removed.contains(root));
                    for root in &removed {
                        schedule.forget(root);
                        work.remove(root);
                    }
                    snapshots
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .retain(|root, _| !removed.contains(root));
                }
            });
        match worker {
            Ok(_) => Some(Pump { wake, latest }),
            Err(error) => {
                eprintln!("[ctox cockpit] projection worker unavailable: {error}");
                None
            }
        }
    })
    .as_ref()
}

fn wake(root: &Path, flags: u8) {
    if let Some(pump) = pump() {
        if pump.wake.try_send((root.to_path_buf(), flags)).is_err() {
            static DROPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let dropped = DROPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if dropped.is_power_of_two() {
                eprintln!("[ctox cockpit] coalesced wake dropped ({dropped}); durable sources replay on maintenance");
            }
        }
    }
}
pub(crate) fn schedule_flow_refresh(root: &Path, kind: &str) {
    let flags = flow_refresh_flags(kind);
    if flags != 0 {
        wake(root, flags);
    }
}

fn flow_refresh_flags(kind: &str) -> u8 {
    let chat = matches!(
        kind,
        "worker.plan_updated" | "worker.turn_started" | "cockpit.review"
    );
    let crew = matches!(
        kind,
        "crew_selected" | "crew.selected" | "crew_selection_unavailable"
    );
    if event_kind(kind).is_none() && !chat && !crew {
        return 0;
    }
    EVENTS | if chat { CHAT } else { 0 } | if crew { STATUS | QUEUE } else { 0 }
}
pub(crate) fn schedule_runs_refresh(root: &Path) {
    wake(root, RUNS | STATUS);
}

pub(crate) fn publish_service_stopped(root: &Path, boot_id: String) {
    let snapshot = WorkerSnapshot {
        boot_id,
        last_error: persisted_snapshot(root).ok().and_then(|s| s.last_error),
        ..Default::default()
    };
    publish_worker_snapshot(root, snapshot.clone());
    // Graceful process exit cannot rely on a detached worker being scheduled before termination.
    let outcome = (|| -> Result<()> {
        let conn = core(root)?;
        if has_table(&conn, "communication_routing_state")? {
            project_status(
                root,
                &conn,
                &mut BusinessProjectionWriter::open(root)?,
                &snapshot,
            )?;
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        eprintln!("[ctox cockpit] shutdown status write failed: {error:#}");
    }
}

pub(crate) fn schedule_refresh(root: &Path) {
    wake(root, STATUS | QUEUE | CHAT);
}

// Continue a bounded chat page through the same per-root pump throttle.
pub(super) fn schedule_chat_refresh(root: &Path) {
    wake(root, CHAT);
}

/// Only a short in-memory update and a nonblocking wake, including when called under SharedState.
pub(crate) fn publish_worker_snapshot(root: &Path, snapshot: WorkerSnapshot) {
    if let Some(pump) = pump() {
        let changed = record_snapshot(
            &mut pump.latest.lock().unwrap_or_else(|e| e.into_inner()),
            root,
            snapshot,
        );
        if changed {
            wake(root, STATUS);
        }
    }
}

pub(crate) fn refresh_after_finalization(db_path: &Path) {
    if let Some(root) = db_path.parent().and_then(Path::parent) {
        wake(root, STATUS | RUNS | QUEUE | CHAT);
    }
}

/// Required/index timestamps must remain numbers. Keep malformed documents in
/// the source ledger for replay after repair; log once per root and collection,
/// rather than retaining an unbounded set of individual bad record IDs.
fn required_projection_millis(root: &Path, collection: &'static str, value: &str) -> Option<i64> {
    let parsed = millis(value);
    if parsed.is_none() {
        static REPORTED: OnceLock<Mutex<BTreeSet<(PathBuf, &'static str)>>> = OnceLock::new();
        if REPORTED
            .get_or_init(|| Mutex::new(BTreeSet::new()))
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert((root.to_path_buf(), collection))
        {
            eprintln!("[ctox cockpit] {}: skipping {collection} documents with invalid required timestamps; source evidence remains replayable", root.display());
        }
    }
    parsed
}

fn millis(value: &str) -> Option<i64> {
    // LCM finalizations use epoch-millisecond strings; flow/routing use RFC3339.
    if let Ok(value) = value.parse::<i64>() {
        return Some(value);
    }
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.timestamp_millis())
        .ok()
}

fn has_table(conn: &Connection, table: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

fn core(root: &Path) -> Result<Connection> {
    let conn = Connection::open(crate::paths::core_db(root))?;
    conn.busy_timeout(Duration::from_millis(100))?;
    // Indexes are additive. The flow ledger remains the authoritative evidence.
    if has_table(&conn, "ctox_harness_flow_events")? {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_cockpit_flow_attempt
                ON ctox_harness_flow_events(json_extract(metadata_json,'$.attempt_id'), created_at);
             CREATE INDEX IF NOT EXISTS idx_cockpit_flow_task_time
                ON ctox_harness_flow_events(message_key, created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_crew_selection_diagnostic_time
                ON ctox_harness_flow_events(created_at)
                WHERE event_kind IN ('crew_selected','crew_selection_unavailable')
                  AND COALESCE(json_extract(metadata_json,'$.repaired'),0)=0;",
        )?;
    }
    if has_table(&conn, "worker_attempt_finalizations")? {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_cockpit_finalized_time
                ON worker_attempt_finalizations(COALESCE(terminal_at,updated_at) DESC, attempt_id DESC)
                WHERE status!='finalizing';",
        )?;
    }
    if has_table(&conn, "api_model_cost_events")? {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_cockpit_cost_turn ON api_model_cost_events(turn_id);",
        )?;
    }
    Ok(conn)
}

#[cfg(test)]
fn refresh_selected(
    root: &Path,
    snapshot: &WorkerSnapshot,
    writer: &mut BusinessProjectionWriter,
    flags: u8,
) -> Result<()> {
    let mut timing = ProjectionTiming::new(root, flags);
    refresh_measured(root, snapshot, writer, flags, &mut timing)
}

/// Pump-thread wall time, including failed phases. No payloads or task content
/// enter diagnostics. Coalesced passes report every phase, rather than blaming the
/// event cursor for work done by status, run joins or store initialization.
struct ProjectionTiming<'a> {
    root: &'a Path,
    flags: u8,
    started: Instant,
    phases: Vec<(&'static str, Duration)>,
}
impl<'a> ProjectionTiming<'a> {
    fn new(root: &'a Path, flags: u8) -> Self {
        Self {
            root,
            flags,
            started: Instant::now(),
            phases: Vec::new(),
        }
    }
    fn phase<T>(&mut self, name: &'static str, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let started = Instant::now();
        let result = work();
        self.phases.push((name, started.elapsed()));
        result
    }
}
impl Drop for ProjectionTiming<'_> {
    fn drop(&mut self) {
        // The per-root scheduler bounds this to one line per three seconds;
        // record fast passes too so field measurements have a denominator.
        let phases = self
            .phases
            .iter()
            .map(|(name, elapsed)| format!("{name}_us={}", elapsed.as_micros()))
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!(
            "[ctox cockpit phases] root={} flags={} total_us={} {phases}",
            self.root.display(),
            self.flags,
            self.started.elapsed().as_micros()
        );
    }
}

fn refresh_measured(
    root: &Path,
    snapshot: &WorkerSnapshot,
    writer: &mut BusinessProjectionWriter,
    flags: u8,
    timing: &mut ProjectionTiming<'_>,
) -> Result<()> {
    let conn = timing.phase("core_open", || core(root))?;
    if !has_table(&conn, "communication_routing_state")? {
        return Ok(());
    }
    if flags & MAINTENANCE != 0 {
        // Maintenance must never suppress unrelated cockpit projections. Older
        // databases have attempts but no tombstone outbox until their migration.
        let maintenance =
            timing.phase("retain_attempts", || -> Result<()> {
                if !has_table(&conn, "crew_attempts")?
                    || !has_table(&conn, "crew_projection_tombstones")?
                {
                    return Ok(());
                }
                crate::crew::retain_attempts(&conn, Utc::now().timestamp_millis())?;
                let ids = conn
            .prepare("SELECT event_id FROM crew_projection_tombstones ORDER BY event_id LIMIT 128")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
                for id in ids {
                    writer.tombstone_source_projection(
                        "ctox_harness_events",
                        &id,
                        Utc::now().timestamp_millis(),
                    )?;
                    if writer.inner.delivered_to_rxdb("ctox_harness_events") {
                        conn.execute(
                            "DELETE FROM crew_projection_tombstones WHERE event_id=?1",
                            [&id],
                        )?;
                    }
                }
                Ok(())
            });
        match maintenance {
            Ok(()) => writer.crew_maintenance_warned = false,
            Err(_) if !writer.crew_maintenance_warned => {
                eprintln!("[ctox cockpit] crew maintenance failed; other projections continue; next maintenance retries");
                writer.crew_maintenance_warned = true;
            }
            Err(_) => {}
        }
    }
    if flags & STATUS != 0 {
        timing.phase("project_status", || {
            project_status(root, &conn, writer, snapshot)
        })?;
    }
    if flags & EVENTS != 0 {
        // The outbox also marks the lifecycle migration: PR-59 attempts do not
        // yet have started_at. Their existing events can still be projected.
        if flags & MAINTENANCE != 0
            && has_table(&conn, "crew_attempts")?
            && has_table(&conn, "crew_projection_tombstones")?
            && has_table(&conn, "ctox_harness_flow_events")?
        {
            timing.phase("repair_selection_events", || {
                crate::crew::repair_selection_events(root, &conn)
            })?;
        }
        timing.phase("project_events", || {
            project_events_since(root, &conn, writer, flags & MAINTENANCE != 0)
        })?;
    }
    if flags & RUNS != 0 {
        timing.phase("project_runs", || project_runs(root, &conn, writer))?;
    }
    if flags & (STATUS | QUEUE) != 0 && has_table(&conn, "crew_members")? {
        timing.phase("project_crew", || project_crew(root, &conn, writer))?;
    }
    if flags & CHAT != 0 && has_table(&conn, "ctox_harness_flow_events")? {
        timing.phase("project_chat", || super::chat::project(root, &conn))?;
    }
    let mut collections = Vec::new();
    if flags & QUEUE != 0 {
        collections.push("ctox_queue_tasks");
    }
    if flags & EVENTS != 0 && flags & MAINTENANCE != 0 {
        collections.push("ctox_harness_events");
    }
    if flags & RUNS != 0 {
        collections.push("ctox_runs");
    }
    timing.phase("retain_selected", || {
        retain_selected(
            root,
            &conn,
            writer,
            Utc::now().timestamp_millis(),
            &collections,
        )
    })?;
    Ok(())
}

fn project_status(
    root: &Path,
    conn: &Connection,
    writer: &mut BusinessProjectionWriter,
    snapshot: &WorkerSnapshot,
) -> Result<()> {
    let mut counts = BTreeMap::new();
    let mut snapshot = snapshot.clone();
    if crate::service::cockpit_service_running(root) == Some(false) {
        snapshot.service_running = false;
        snapshot.busy = false;
        snapshot.worker_active_count = 0;
        snapshot.worker_phase = None;
        snapshot.active_task_ids.clear();
    }
    let mut statement = conn.prepare(
        "SELECT r.route_status, COUNT(*) FROM communication_routing_state r
         JOIN communication_messages m ON m.message_key=r.message_key
         WHERE m.channel='queue' AND m.direction='inbound' GROUP BY r.route_status",
    )?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (status, count) = row?;
        counts.insert(status, count);
    }
    let count = |status: &str| counts.get(status).copied().unwrap_or(0);
    let failed_recent: i64 = conn.query_row(
        "SELECT COUNT(*) FROM communication_routing_state r
         JOIN communication_messages m ON m.message_key=r.message_key
         WHERE m.channel='queue' AND m.direction='inbound' AND r.route_status='failed'
           AND julianday(r.updated_at)>=julianday('now','-1 day')",
        [],
        |row| row.get(0),
    )?;
    let review_count = if has_table(conn, "business_command_aggregates")?
        && has_table(conn, "business_command_task_links")?
    {
        conn.query_row(
            "SELECT COUNT(*) FROM communication_routing_state r
             JOIN communication_messages m ON m.message_key=r.message_key
             WHERE m.channel='queue' AND m.direction='inbound'
               AND (r.route_status='review_rework' OR EXISTS (
                   SELECT 1 FROM business_command_task_links l
                   JOIN business_command_aggregates a ON a.command_id=l.command_id
                   WHERE l.task_id=r.message_key AND a.execution_phase='awaiting_review'))",
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        count("review_rework")
    };
    // The persisted snapshot may contain our previous configuration diagnostic.
    // Recompute it, so repairing queue.pause also clears the visible error.
    snapshot.last_error = snapshot.last_error.and_then(|error| {
        if error.starts_with("Invalid queue.pause;") {
            None
        } else {
            Some(
                error
                    .split("; Invalid queue.pause;")
                    .next()
                    .unwrap_or(&error)
                    .to_string(),
            )
        }
    });
    let (pause, pause_error) = queue_pause_state(root);
    snapshot.last_error = snapshot.last_error.and_then(|error| {
        let previous = error
            .split("Crew selection unavailable:")
            .next()
            .unwrap_or("")
            .trim_end_matches([';', ' ']);
        (!previous.is_empty()).then(|| previous.to_string())
    });
    let crew_error = crate::crew::selection_last_error(root)
        .or(crate::crew::durable_selection_last_error(conn)?);
    if let Some(error) = crew_error {
        snapshot.last_error = Some(match snapshot.last_error {
            Some(other) => format!("{other}; {error}"),
            None => error,
        });
    }
    if let Some(error) = pause_error {
        snapshot.last_error = Some(match snapshot.last_error {
            Some(previous) => format!("{previous}; {error}"),
            None => error,
        });
    }
    let now = Utc::now().timestamp_millis();
    let capacity = crate::service::configure_queue_worker_capacity(root, None)?;
    let threshold = crate::service::queue_pressure_threshold();
    let active_crew: Option<String> = if has_table(conn, "crew_members")? {
        conn.query_row(
            "SELECT crew_member_id FROM communication_routing_state
         WHERE route_status='leased' AND crew_member_id IS NOT NULL
         ORDER BY leased_at,message_key LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?
    } else {
        None
    };
    writer.upsert_source_projection("ctox_harness_status","harness",now,json!({
        "id":"harness", "service_running":snapshot.service_running, "busy":snapshot.busy,
        "paused":pause.paused,"pause_reason":pause.reason,
        "worker_active_count":snapshot.worker_active_count,"worker_phase":snapshot.worker_phase,
        "worker_capacity":capacity["max_workers"],"pending_count":count("pending"),"leased_count":count("leased"),
        "blocked_count":count("blocked"),"review_count":review_count,"failed_recent_count":failed_recent,
        "pressure_active":count("pending") >= threshold as i64,"pressure_threshold":threshold,
        "work_hours":crate::service::working_hours::snapshot(root),"active_task_ids":snapshot.active_task_ids,
        "active_crew_member_id":active_crew,"last_error":snapshot.last_error,"boot_id":snapshot.boot_id,"updated_at_ms":now
    }))?;
    Ok(())
}

fn event_kind(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "worker.tool_started" => "tool_started",
        "worker.tool_completed" => "tool_completed",
        "worker.thinking_started" | "worker.thinking" => "thinking",
        "worker.plan_updated" => "plan_updated",
        "worker.token_usage" => "token_usage",
        "worker.turn_completed" => "turn_completed",
        "worker.phase" | "worker.turn_started" => "phase",
        "crew.selected" | "crew_selected" => "crew_selected",
        "crew_selection_unavailable" => "crew_selection_unavailable",
        "crew.memory_read" => "memory_read",
        "crew.learning" => "learning",
        _ => return None,
    })
}

// Each query materializes at most one page. Keyset iteration also preserves
// all active tasks/runs when their count exceeds the terminal retention limit.
const PROJECTION_PAGE_SIZE: i64 = 128;

// Drive from the rowid range, then probe routing by its primary key. Selecting
// the task/time index to satisfy message-key ordering would rescan history on
// empty deltas; NOT INDEXED still permits SQLite's integer-primary-key lookup.
const CHANGED_EVENT_TASKS_SQL: &str =
    "SELECT DISTINCT e.message_key FROM ctox_harness_flow_events e NOT INDEXED
     CROSS JOIN communication_routing_state r ON r.message_key=e.message_key
     WHERE e.rowid>?1 AND e.rowid<=?2 AND e.message_key>?3
       AND (r.route_status NOT IN ('handled','failed','cancelled')
            OR julianday(r.updated_at)>=julianday('now','-1 day'))
     ORDER BY e.message_key LIMIT ?4";

const EXCESS_TASK_EVENTS_SQL: &str =
    "SELECT record_id FROM business_records INDEXED BY idx_cockpit_event_task_time
     WHERE collection='ctox_harness_events' AND deleted=0
       AND json_extract(payload_json,'$.task_id')=?1
     ORDER BY json_extract(payload_json,'$.created_at_ms') DESC,record_id DESC
     LIMIT 128 OFFSET 200";

#[cfg(test)]
fn project_events(
    root: &Path,
    conn: &Connection,
    writer: &mut BusinessProjectionWriter,
) -> Result<()> {
    project_events_since(root, conn, writer, true)
}

fn project_events_since(
    root: &Path,
    conn: &Connection,
    writer: &mut BusinessProjectionWriter,
    replay: bool,
) -> Result<()> {
    if !has_table(conn, "ctox_harness_flow_events")? {
        return Ok(());
    }
    // Rowid tracks insertion order, not source timestamps: late/backdated
    // events are still seen. Capture before reading; concurrent inserts remain
    // eligible on the next pass. A regressed high-water mark forces replay;
    // periodic replay also covers same-size ledger replacement/source repairs.
    let high_water: i64 = conn.query_row(
        "SELECT COALESCE(MAX(rowid),0) FROM ctox_harness_flow_events",
        [],
        |row| row.get(0),
    )?;
    let since = writer
        .event_cursor
        .filter(|previous| !replay && *previous <= high_water);
    let mut delivered = true;
    let mut cursor = String::new();
    loop {
        // Short turns may finish before the pump wakes. Replay recent terminal
        // tasks as well; explicit false eligibility always excludes an event.
        let tasks = if let Some(since) = since {
            conn.prepare(CHANGED_EVENT_TASKS_SQL)?
                .query_map(
                    params![since, high_water, cursor, PROJECTION_PAGE_SIZE],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            conn.prepare(
                "SELECT message_key FROM communication_routing_state
             WHERE message_key > ?1
               AND (route_status NOT IN ('handled','failed','cancelled')
                    OR julianday(updated_at) >= julianday('now','-1 day'))
             ORDER BY message_key LIMIT ?2",
            )?
            .query_map(params![cursor, PROJECTION_PAGE_SIZE], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        if tasks.is_empty() {
            break;
        }
        for task in tasks {
            cursor = task.clone();
            let mut statement = conn.prepare(
                "SELECT event_id, event_kind, title, attempt_index, metadata_json, created_at
                 FROM ctox_harness_flow_events
                 WHERE message_key=?1
                   AND COALESCE(json_extract(metadata_json,'$.cockpit_eligible'),1)=1
                   AND event_kind IN (
                       'worker.turn_started','worker.tool_started','worker.tool_completed',
                       'worker.thinking_started','worker.thinking','worker.plan_updated',
                       'worker.token_usage','worker.turn_completed','worker.phase',
                       'crew.selected','crew_selected','crew_selection_unavailable',
                       'crew.memory_read','crew.learning')
                 ORDER BY created_at DESC, event_id DESC LIMIT 200",
            )?;
            let events = statement
                .query_map([&task], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (id, kind, title, attempt, metadata, created_at) in events {
                let metadata: Value = serde_json::from_str(&metadata)?;
                let Some(created_at_ms) =
                    required_projection_millis(root, "ctox_harness_events", &created_at)
                else {
                    continue;
                };
                let usage = metadata.get("usage").unwrap_or(&Value::Null);
                let attempt_id = metadata.get("attempt_id").and_then(Value::as_str);
                let attempt = attempt.or_else(|| metadata.get("attempt").and_then(Value::as_i64));
                let attempt = if attempt.is_some() || attempt_id.is_none() {
                    attempt
                } else {
                    conn.query_row(
                        "SELECT json_extract(metadata_json,'$.attempt')
                         FROM ctox_harness_flow_events
                         WHERE message_key=?1 AND json_extract(metadata_json,'$.attempt_id')=?2
                           AND json_extract(metadata_json,'$.attempt') IS NOT NULL
                         ORDER BY created_at, event_id LIMIT 1",
                        params![task, attempt_id],
                        |r| r.get::<_, Option<i64>>(0),
                    )
                    .optional()?
                    .flatten()
                };
                let step_position = event_step_position(conn, &task, &created_at, &metadata)?;
                // Tool payloads and raw reasoning stay in the authoritative ledger.
                writer.upsert_source_projection(
                    "ctox_harness_events", &id,
                    created_at_ms,
                    json!({
                        "id":id,"task_id":task,"command_id":metadata.get("command_id"),"attempt":attempt,
                        "kind":event_kind(&kind),"title":title,"tool_type":metadata.pointer("/tool/type"),
                        "tool_name":metadata.pointer("/tool/name"),"call_id":metadata.pointer("/tool/call_id"),
                        "success":metadata.pointer("/tool/success"),
                        "usage":{"input":usage.get("input_tokens"),"output":usage.get("output_tokens"),
                            "reasoning":usage.get("reasoning_output_tokens"),"total":usage.get("total_tokens")},
                        "runtime_seconds":metadata.pointer("/runtime/seconds"),"step_position":step_position,
                        "created_at_ms":created_at_ms,"updated_at_ms":created_at_ms
                    }),
                )?;
                delivered &= writer.inner.delivered_to_rxdb("ctox_harness_events");
            }
            // Enforce the per-task cap immediately with the task/time index.
            // The expensive cross-task age/window sweep stays on maintenance.
            delivered &= retain_task_events(writer, &task)?;
        }
    }
    if delivered {
        writer.event_cursor = Some(high_water);
    }
    Ok(())
}

fn retain_task_events(writer: &mut BusinessProjectionWriter, task: &str) -> Result<bool> {
    let mut delivered = true;
    loop {
        let ids = writer
            .inner
            .source_connection()
            .prepare(EXCESS_TASK_EVENTS_SQL)?
            .query_map([task], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.is_empty() {
            break;
        }
        // Keep the source rows retryable if the collection is not ready yet.
        if !writer.inner.delivered_to_rxdb("ctox_harness_events") {
            return Ok(false);
        }
        for id in ids {
            writer.tombstone_source_projection(
                "ctox_harness_events",
                &id,
                Utc::now().timestamp_millis(),
            )?;
            delivered &= writer.inner.delivered_to_rxdb("ctox_harness_events");
        }
    }
    Ok(delivered)
}

/// Resolve the plan at the event's time, not the task's newest revision. This
/// runs exclusively on the projection pump, never in the harness progress hook.
fn event_step_position(
    conn: &Connection,
    task: &str,
    created: &str,
    metadata: &Value,
) -> Result<Value> {
    if let Some(position) = metadata.get("step_position") {
        return Ok(position.clone());
    }
    let current_steps = metadata.pointer("/plan/plan").and_then(Value::as_array);
    let prior: Option<String> = if current_steps.is_none() {
        conn.query_row(
            "SELECT json_extract(metadata_json,'$.plan.plan')
             FROM ctox_harness_flow_events
             WHERE message_key=?1 AND event_kind='worker.plan_updated' AND created_at<=?2
               AND (?3 IS NULL OR json_extract(metadata_json,'$.attempt_id')=?3)
             ORDER BY created_at DESC, event_id DESC LIMIT 1",
            params![
                task,
                created,
                metadata.get("attempt_id").and_then(Value::as_str)
            ],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten()
    } else {
        None
    };
    let prior = prior
        .map(|raw| serde_json::from_str::<Value>(&raw))
        .transpose()?;
    let steps = current_steps.or_else(|| prior.as_ref().and_then(Value::as_array));
    Ok(json!(steps
        .and_then(|steps| steps
            .iter()
            .position(|step| { step.get("status").and_then(Value::as_str) == Some("in_progress") }))
        .map(|position| position + 1)))
}

fn finalized_runs_sql(crew: bool) -> String {
    let pending_crew = if crew {
        "EXISTS(SELECT 1 FROM crew_attempts c WHERE c.attempt_id=f.attempt_id AND c.finalized_at IS NULL) OR"
    } else {
        ""
    };
    // json_extract has no affinity. The unary + removes the outer TEXT
    // column's affinity (without changing its value), matching bound-string
    // attempt lookups and letting SQLite seek idx_cockpit_flow_attempt.
    // Without it each correlated subquery scans the entire flow ledger.
    format!(
        "SELECT f.attempt_id,
                    (SELECT e.work_id FROM ctox_harness_flow_events e
                     WHERE json_extract(e.metadata_json,'$.attempt_id')=+f.attempt_id
                       AND e.work_id IS NOT NULL ORDER BY e.created_at LIMIT 1),
                    f.status, f.agent_outcome, f.created_at,
                    COALESCE(f.terminal_at,f.updated_at), f.error_text, f.resumable,
                    (SELECT e.message_key FROM ctox_harness_flow_events e
                     WHERE json_extract(e.metadata_json,'$.attempt_id')=+f.attempt_id
                       AND e.message_key IS NOT NULL ORDER BY e.created_at LIMIT 1)
             FROM worker_attempt_finalizations f
             WHERE f.attempt_id>?1 AND f.status!='finalizing'
               AND ({pending_crew} f.attempt_id IN (
                        SELECT attempt_id FROM worker_attempt_finalizations
                        WHERE status!='finalizing'
                        ORDER BY COALESCE(terminal_at,updated_at) DESC, attempt_id DESC LIMIT 500)
                    OR EXISTS (
                        SELECT 1 FROM ctox_harness_flow_events e
                        JOIN communication_routing_state r ON r.message_key=e.message_key
                        WHERE json_extract(e.metadata_json,'$.attempt_id')=+f.attempt_id
                          AND r.route_status NOT IN ('handled','failed','cancelled')))
             ORDER BY f.attempt_id LIMIT ?2"
    )
}

fn project_runs(
    root: &Path,
    conn: &Connection,
    writer: &mut BusinessProjectionWriter,
) -> Result<()> {
    if !has_table(conn, "worker_attempt_finalizations")?
        || !has_table(conn, "ctox_harness_flow_events")?
    {
        return Ok(());
    }
    let mut cursor = String::new();
    loop {
        // A fixed page replaces the unbounded active-run UNION and COUNT limit.
        // The bounded recent set plus an indexed EXISTS retains every active run.
        // Pending identity accounting is independent of the 500 visible-run cap.
        // A long offline interval must not silently lose crew statistics.
        let sql = finalized_runs_sql(has_table(conn, "crew_attempts")?);
        let mut statement = conn.prepare(&sql)?;
        let rows = statement
            .query_map(params![cursor, PROJECTION_PAGE_SIZE], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, bool>(7)?,
                    r.get::<_, Option<String>>(8)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.is_empty() {
            break;
        }
        for (id, work, status, outcome, created, finished, error, resumable, task) in rows {
            cursor = id.clone();
            let (started, command_id): (Option<String>, Option<String>) = conn.query_row(
                "SELECT MIN(created_at), MAX(json_extract(metadata_json,'$.command_id'))
                 FROM ctox_harness_flow_events WHERE json_extract(metadata_json,'$.attempt_id')=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let started_ms = millis(started.as_deref().unwrap_or(&created));
            let Some(finished_ms) = required_projection_millis(root, "ctox_runs", &finished) else {
                continue;
            };
            let mut metrics = run_metrics(conn, &id)?;
            metrics["elapsed_ms"] =
                json!(started_ms.map(|started| finished_ms.saturating_sub(started).max(0)));
            let review: Option<String> = conn
                .query_row(
                    "SELECT json_extract(metadata_json,'$.review') FROM ctox_harness_flow_events
                 WHERE json_extract(metadata_json,'$.attempt_id')=?1
                   AND json_extract(metadata_json,'$.review') IS NOT NULL
                 ORDER BY created_at DESC LIMIT 1",
                    [&id],
                    |r| r.get(0),
                )
                .optional()?;
            let review = review
                .and_then(|r| serde_json::from_str::<Value>(&r).ok())
                .unwrap_or(json!({"disposition":null,"hold_reason":null}));
            let mut crew_member: Option<String> = None;
            let mut retrospective: Option<String> = None;
            if has_table(conn, "crew_attempts")? {
                let pending: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM crew_attempts WHERE attempt_id=?1 AND finalized_at IS NULL)",
                    [&id], |r|r.get(0),
                )?;
                if pending {
                    let reply: String = conn.query_row(
                    "SELECT substr(reply_text,1,1048577) FROM worker_attempt_finalizations WHERE attempt_id=?1",
                    [&id],
                    |r| r.get(0),
                )?;
                    let owner_feedback: Option<String> = if has_table(
                        conn,
                        "business_command_aggregates",
                    )? {
                        conn.query_row(
                        "SELECT substr(COALESCE(json_extract(a.intent_json,'$.payload.owner_feedback'),
                            json_extract(a.intent_json,'$.payload.prompt'),json_extract(a.intent_json,'$.payload.message')),1,16000)
                         FROM business_command_task_links l JOIN business_command_aggregates a ON a.command_id=l.command_id
                         WHERE l.task_id=?1 AND json_extract(a.intent_json,'$.native_authorization.allowed')=1
                           AND json_extract(a.intent_json,'$.native_authorization.actor.trusted')=1
                           AND json_extract(a.intent_json,'$.native_authorization.actor.role') IN ('chef','admin')
                         ORDER BY l.created_at_ms DESC,l.command_id LIMIT 1",
                        [task.as_deref()], |r|r.get(0),
                    ).optional()?.flatten()
                    } else {
                        None
                    };
                    crate::crew::finalize_attempt(
                        conn,
                        &id,
                        &status,
                        match review.get("disposition").and_then(Value::as_str) {
                            Some("approved") => Some(true),
                            None | Some("none") => None,
                            Some(_) => Some(false),
                        },
                        &finished,
                        metrics.get("elapsed_ms").and_then(Value::as_i64),
                        &reply,
                        owner_feedback.as_deref(),
                    )?;
                }
                if let Some((member, text)) = conn
                    .query_row(
                        "SELECT member_id,retrospective FROM crew_attempts WHERE attempt_id=?1",
                        [&id],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
                    )
                    .optional()?
                {
                    crew_member = Some(member);
                    retrospective = text;
                }
            }
            writer.upsert_source_projection(
                "ctox_runs", &id, finished_ms,
                json!({
                    "id":id,"task_id":task,"command_id":command_id,"work_id":work,"crew_member_id":crew_member,
                    "status":status,"agent_outcome":outcome,"started_at_ms":started_ms,"finished_at_ms":finished_ms,
                    "metrics":metrics,"review":review,"error_text":error,"resumable":resumable,
                    "retrospective":retrospective,"updated_at_ms":finished_ms
                }),
            )?;
        }
    }
    Ok(())
}

fn project_crew(
    root: &Path,
    conn: &Connection,
    writer: &mut BusinessProjectionWriter,
) -> Result<()> {
    let now = Utc::now().timestamp_millis();
    for mut member in crate::crew::members(conn)? {
        let active: Option<String> = conn
            .query_row(
                "SELECT message_key FROM communication_routing_state
             WHERE crew_member_id=?1 AND route_status='leased'
             ORDER BY leased_at,message_key LIMIT 1",
                [&member.id],
                |r| r.get(0),
            )
            .optional()?;
        let failed: bool = conn.query_row(
                "SELECT COALESCE((SELECT succeeded=0 AND julianday(finalized_at)>=julianday(?2,'unixepoch')
             FROM crew_attempts WHERE member_id=?1
             AND finalized_at IS NOT NULL ORDER BY finalized_at DESC,attempt_id DESC LIMIT 1),0)",
                params![member.id, now / 1000 - 86_400],
                |r| r.get(0),
            )?;
        let stamp = (member.updated_at.clone(), active.clone(), failed);
        if writer.crew_sources.get(&member.id) == Some(&stamp) {
            continue;
        }
        crate::crew::retain_learnings(conn, &member.id)?;
        // Retention's delete trigger touches the member. Cache and project the
        // post-retention timestamp so an unchanged next wake remains cheap.
        member.updated_at = conn.query_row(
            "SELECT updated_at FROM crew_members WHERE id=?1",
            [&member.id],
            |r| r.get(0),
        )?;
        let stamp = (member.updated_at.clone(), active.clone(), failed);
        let Some(updated) =
            required_projection_millis(root, "ctox_crew_members", &member.updated_at)
        else {
            continue;
        };
        let state = if active.is_some() {
            "on_duty"
        } else if failed {
            "resting_after_failure"
        } else {
            "home"
        };
        // Memory (LCM continuity of the member) and the derived field of work:
        // the modules it succeeded in most, from its finalized attempts.
        // Read through the pump's own connection: no LCM engine (and no
        // migration write lock) inside the projection pass.
        let memory = crate::crew::load_member_memory_from_conn(conn, &member.id);
        let domain = conn
            .prepare(
                "SELECT module FROM crew_attempts
                 WHERE member_id=?1 AND finalized_at IS NOT NULL AND succeeded=1 AND module IS NOT NULL AND module!=''
                 GROUP BY module ORDER BY COUNT(*) DESC, module LIMIT 3",
            )?
            .query_map([&member.id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // Expression stamps: read at attempt start, learned after the tick.
        let (last_memory_read_at, last_learning_at): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT last_memory_read_at,last_learning_at FROM crew_members WHERE id=?1",
                [&member.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((None, None));
        let stamp_ms = |value: Option<String>| value.as_deref().and_then(millis);
        writer.upsert_source_projection(
            "ctox_crew_members",
            &member.id,
            updated,
            json!({
                "id":member.id,"name":member.name,"shape":member.shape,"color":member.color,
                "archived":member.archived,"state":state,"active_task_id":active,
                "soul":member.soul,"specialties":member.specialties,"stats":member.stats,
                "memory":{"anchors":memory.anchors,"narrative":memory.narrative,
                    "anchor_count":crate::crew::anchor_lines(&memory.anchors).len(),
                    "experience_count":crate::crew::narrative_lines(&memory.narrative).len(),
                    "updated_at":memory.updated_at},
                "domain":domain,
                "last_memory_read_at_ms":stamp_ms(last_memory_read_at),
                "last_learning_at_ms":stamp_ms(last_learning_at),
                "updated_at_ms":updated
            }),
        )?;
        let mut learning_ids = BTreeSet::new();
        let rows = conn.prepare(
            "SELECT id,text,kind,scope_json,evidence_run_id,created_at,confirmed_by_owner,archived
             FROM crew_member_learnings WHERE member_id=?1
             ORDER BY confirmed_by_owner,created_at,id LIMIT 200",
        )?.query_map([&member.id], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,
            r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,
            r.get::<_,bool>(6)?,r.get::<_,bool>(7)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, text, kind, scope, run, created, confirmed, archived) in rows {
            // A malformed timestamp is not a source deletion. Keep the last
            // valid projection until the source can be repaired.
            learning_ids.insert(id.clone());
            let Some(created_ms) =
                required_projection_millis(root, "ctox_crew_learnings", &created)
            else {
                continue;
            };
            let scope: Value = serde_json::from_str(&scope)?;
            writer.upsert_source_projection(
                "ctox_crew_learnings",
                &id,
                created_ms,
                json!({
                    "id":id,"member_id":member.id,"text":text,"kind":kind,"scope":scope,
                    "evidence_run_id":run,"created_at_ms":created_ms,"confirmed_by_owner":confirmed,
                    "archived":archived,"updated_at_ms":created_ms
                }),
            )?;
        }
        // Native source deletions (retention/Owner delete) become durable tombstones.
        let mut cursor = String::new();
        loop {
            let ids = writer
                .inner
                .source_connection()
                .prepare(
                    "SELECT record_id FROM business_records WHERE collection='ctox_crew_learnings'
             AND deleted=0 AND json_extract(payload_json,'$.member_id')=?2
             AND record_id>?1 ORDER BY record_id LIMIT 128",
                )?
                .query_map(params![cursor, member.id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if ids.is_empty() {
                break;
            }
            for id in ids {
                cursor = id.clone();
                if !learning_ids.contains(&id) {
                    writer.tombstone_source_projection("ctox_crew_learnings", &id, now)?;
                }
            }
        }
        if writer.inner.delivered_to_rxdb("ctox_crew_members")
            && (learning_ids.is_empty() || writer.inner.delivered_to_rxdb("ctox_crew_learnings"))
        {
            writer.crew_sources.insert(member.id, stamp);
        }
    }
    Ok(())
}

fn run_metrics(conn: &Connection, attempt: &str) -> Result<Value> {
    let (tools, thinking): (i64, i64) = conn.query_row(
        "SELECT COUNT(DISTINCT CASE WHEN event_kind='worker.tool_started'
                    THEN COALESCE(json_extract(metadata_json,'$.tool.call_id'),event_id) END),
                COUNT(DISTINCT CASE WHEN event_kind='worker.thinking_started'
                    THEN COALESCE(json_extract(metadata_json,'$.activity.id'),event_id) END)
         FROM ctox_harness_flow_events WHERE json_extract(metadata_json,'$.attempt_id')=?1",
        [attempt],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut result = json!({"model":null,"provider":null,"input_tokens":null,"output_tokens":null,
        "reasoning_tokens":null,"cost_usd":null,"tool_calls":tools,"thinking_turns":thinking});
    if !has_table(conn, "api_model_cost_events")? {
        return Ok(result);
    }
    let (input, output, reasoning, provider, model): (
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<String>,
    ) = conn.query_row(
        "SELECT SUM(input_tokens), SUM(output_tokens), SUM(reasoning_output_tokens),
                GROUP_CONCAT(DISTINCT provider), GROUP_CONCAT(DISTINCT model)
         FROM api_model_cost_events WHERE turn_id IN (
             SELECT DISTINCT json_extract(metadata_json,'$.turn_id')
             FROM ctox_harness_flow_events WHERE json_extract(metadata_json,'$.attempt_id')=?1)",
        [attempt],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    result["input_tokens"] = json!(input);
    result["output_tokens"] = json!(output);
    result["reasoning_tokens"] = json!(reasoning);
    result["provider"] = json!(provider);
    result["model"] = json!(model);
    if has_table(conn, "api_model_price_rates")? {
        let (count, priced, cost): (i64, i64, Option<f64>) = conn.query_row(
            "SELECT COUNT(*), COUNT(p.model), SUM((
                 (e.input_tokens-e.cached_input_tokens)*p.input_usd_per_million
                 + e.cached_input_tokens*COALESCE(p.cached_input_usd_per_million,p.input_usd_per_million)
                 + e.output_tokens*p.output_usd_per_million)/1000000.0)
             FROM api_model_cost_events e
             LEFT JOIN api_model_price_rates p ON p.provider=e.provider AND p.model=e.model
                 AND p.effective_from_day=(
                     SELECT MAX(p2.effective_from_day) FROM api_model_price_rates p2
                     WHERE p2.provider=e.provider AND p2.model=e.model AND p2.effective_from_day<=e.day)
             WHERE e.turn_id IN (
                 SELECT DISTINCT json_extract(metadata_json,'$.turn_id')
                 FROM ctox_harness_flow_events WHERE json_extract(metadata_json,'$.attempt_id')=?1)",
            [attempt], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if count > 0 && count == priced {
            result["cost_usd"] = json!(cost);
        }
    }
    Ok(result)
}

#[cfg(test)]
fn retain(
    root: &Path,
    core: &Connection,
    writer: &mut BusinessProjectionWriter,
    now: i64,
) -> Result<()> {
    retain_selected(
        root,
        core,
        writer,
        now,
        &["ctox_queue_tasks", "ctox_harness_events", "ctox_runs"],
    )
}

fn retain_selected(
    root: &Path,
    core: &Connection,
    writer: &mut BusinessProjectionWriter,
    now: i64,
    collections: &[&str],
) -> Result<()> {
    // STATUS-only wakes have nothing to retain. In particular, do not open and
    // initialize another Business OS store merely to iterate an empty list.
    if collections.is_empty() {
        return Ok(());
    }
    let business = store::open_store(root)?;
    // Function-local, unpooled connection: Drop detaches cockpit_core on every
    // return path, including errors. No attachment survives into another sweep.
    business.execute(
        "ATTACH DATABASE ?1 AS cockpit_core",
        [crate::paths::core_db(root).to_string_lossy().as_ref()],
    )?;
    let retention = queue_retention(root)?;
    for &collection in collections {
        let predicate = match collection {
            "ctox_queue_tasks" => "
                record_id NOT IN (
                    SELECT message_key FROM cockpit_core.communication_routing_state
                    WHERE route_status IN ('handled','failed','cancelled')
                    ORDER BY updated_at DESC, message_key DESC LIMIT ?2)
                AND EXISTS (
                    SELECT 1 FROM cockpit_core.communication_routing_state r
                    WHERE r.message_key=record_id AND r.route_status IN ('handled','failed','cancelled'))",
            "ctox_harness_events" => "
                json_extract(payload_json,'$.task_id') IN (
                    SELECT message_key FROM cockpit_core.communication_routing_state
                    WHERE route_status IN ('handled','failed','cancelled')
                      AND julianday(updated_at)<julianday(?3/1000.0,'unixepoch','-1 day'))
                OR record_id IN (
                    SELECT record_id FROM (
                        SELECT record_id, ROW_NUMBER() OVER (
                            PARTITION BY json_extract(payload_json,'$.task_id')
                            ORDER BY json_extract(payload_json,'$.created_at_ms') DESC, record_id DESC
                        ) position
                        FROM business_records WHERE collection='ctox_harness_events' AND deleted=0)
                    WHERE position>200)",
            "ctox_runs" => "
                record_id NOT IN (
                    SELECT record_id FROM business_records WHERE collection='ctox_runs' AND deleted=0
                    ORDER BY json_extract(payload_json,'$.finished_at_ms') DESC, record_id DESC LIMIT 500)
                AND NOT EXISTS (
                    SELECT 1 FROM cockpit_core.communication_routing_state r
                    WHERE r.message_key=json_extract(payload_json,'$.task_id')
                      AND r.route_status NOT IN ('handled','failed','cancelled'))",
            _ => anyhow::bail!("unknown cockpit retention collection: {collection}"),
        };
        let sql = format!(
            "SELECT record_id FROM business_records
             WHERE collection=?1 AND deleted=0 AND ({predicate})
               AND ?2>=0 AND ?3>=0 AND record_id>?4
             ORDER BY record_id LIMIT 256"
        );
        let mut cursor = String::new();
        loop {
            let ids = business
                .prepare(&sql)?
                .query_map(params![collection, retention, now, cursor], |r| {
                    r.get::<_, String>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if ids.is_empty() {
                break;
            }
            for id in ids {
                cursor = id.clone();
                writer.tombstone_source_projection(collection, &id, now)?;
                // A release/retry can commit while this lossy sweep is writing the mirrors.
                // Re-read after the tombstone: an earlier transition is repaired here; a later
                // transition's normal source callback writes after our tombstone.
                if collection == "ctox_queue_tasks" {
                    let active: bool = core.query_row(
                        "SELECT EXISTS(SELECT 1 FROM communication_routing_state
                         WHERE message_key=?1 AND route_status NOT IN ('handled','failed','cancelled'))",
                        [&id], |r| r.get(0),
                    )?;
                    if active {
                        if let Some(task) = crate::mission::channels::load_queue_task(root, &id)? {
                            if store::refresh_business_command_queue_task_projection(root, &id)?
                                .is_none()
                            {
                                let payload =
                                    crate::business_os::store_projections::queue_task_payload(
                                        None, &task, None, now,
                                    );
                                writer.upsert_source_projection(collection, &id, now, payload)?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
