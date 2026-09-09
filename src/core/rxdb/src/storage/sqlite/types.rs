//! Types for the SQLite storage backend.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use notify::event::{AccessKind, AccessMode, EventKind, MetadataKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::rx_error::{new_rx_error, RxError, RxResult};

use super::metrics;

/// Matches upstream's `:memory:` SQLite database marker.
pub const SQLITE_IN_MEMORY_DB_NAME: &str = ":memory:";

pub(crate) fn sqlite_path_is_in_memory(path: &Path) -> bool {
    path == Path::new(SQLITE_IN_MEMORY_DB_NAME)
}

pub(crate) fn sqlite_concurrent_reader_error() -> RxError {
    new_rx_error(
        "SQLITE_QUERY",
        Some(serde_json::json!({
            "message": "in-memory SQLite does not support concurrent readers; use file-backed storage in production"
        })),
    )
}

// 10s lost real user writes: a credential-save command raced a long
// replication checkpoint on the shared store and surfaced "database is
// locked" in the app. 30s matches the daemon-wide timeout in
// `crate::persistence::sqlite_busy_timeout_duration`.
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(30);
const SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL: Duration = Duration::from_secs(1);
const SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL: Duration = Duration::from_secs(30 * 60);
const SQLITE_EXTERNAL_DATABASE_POLL_BACKOFF_AFTER_IDLE_READS: u32 = 3;
const SQLITE_FILE_EVENT_COALESCE_QUIET_PERIOD: Duration = Duration::from_millis(25);
const SQLITE_FILE_EVENT_COALESCE_MAX_PERIOD: Duration = Duration::from_millis(250);
const SQLITE_CHANGED_TABLES_TABLE: &str = "__rxdb_changed_tables";

#[derive(Debug, Clone)]
pub struct RxStorageSqliteSettings {
    pub database_path: PathBuf,
}

impl Default for RxStorageSqliteSettings {
    fn default() -> Self {
        Self {
            database_path: PathBuf::from("runtime/ctox.sqlite3"),
        }
    }
}

pub type SharedSqliteConnection = Arc<Mutex<Connection>>;

pub(crate) type SqliteReaderCache = Arc<Mutex<Option<SharedSqliteConnection>>>;
pub(crate) const SQLITE_POINT_READER_COUNT: usize = 4;

/// Storage factory holding a shared SQLite connection.
pub struct RxStorageSqlite {
    pub name: String,
    pub settings: RxStorageSqliteSettings,
    pub connection: Mutex<Option<SharedSqliteConnection>>,
    external_poll_key: Mutex<Option<String>>,
    // A collection is a table, not a separate SQLite database. Share a bounded
    // set of schema caches across its short reads instead of opening one per
    // collection. Change-feed drains have their own reader; long query streams
    // continue to use dedicated connections and cannot occupy these slots.
    point_readers: [SqliteReaderCache; SQLITE_POINT_READER_COUNT],
    change_feed_reader: SqliteReaderCache,
    next_point_reader: AtomicUsize,
}

impl RxStorageSqlite {
    pub fn new(settings: RxStorageSqliteSettings) -> Arc<Self> {
        Arc::new(Self {
            name: "sqlite".to_string(),
            settings,
            connection: Mutex::new(None),
            external_poll_key: Mutex::new(None),
            point_readers: std::array::from_fn(|_| Arc::new(Mutex::new(None))),
            change_feed_reader: Arc::new(Mutex::new(None)),
            next_point_reader: AtomicUsize::new(0),
        })
    }

    pub(crate) fn collection_readers(&self) -> (SqliteReaderCache, SqliteReaderCache) {
        let index =
            self.next_point_reader.fetch_add(1, Ordering::Relaxed) % SQLITE_POINT_READER_COUNT;
        (
            Arc::clone(&self.point_readers[index]),
            Arc::clone(&self.change_feed_reader),
        )
    }

    pub fn connection(&self) -> RxResult<SharedSqliteConnection> {
        let mut connection_slot = self.connection.lock();
        if let Some(existing) = connection_slot.as_ref() {
            return Ok(Arc::clone(existing));
        }

        let path = &self.settings.database_path;
        if !sqlite_path_is_in_memory(path) {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(sqlite_io_error)?;
                }
            }
        }
        let connection = Connection::open(path).map_err(sqlite_error)?;
        connection
            .busy_timeout(SQLITE_BUSY_TIMEOUT)
            .map_err(sqlite_error)?;
        {
            let _statement_timer = metrics::timed_sqlite_statement();
            connection
                .execute_batch(
                    r#"
                PRAGMA journal_mode = WAL;
                PRAGMA busy_timeout = 30000;
                PRAGMA synchronous = NORMAL;
                PRAGMA foreign_keys = ON;
                "#,
                )
                .map_err(sqlite_error)?;
        }

        let database_key = crate::storage::sqlite::instance::database_key_for_path(path);
        if let Some(external_poll_key) =
            acquire_external_database_poll(path.clone(), database_key.clone())
        {
            *self.external_poll_key.lock() = Some(external_poll_key);
        }

        // Register the update hook for immediate same-process reactivity.
        let hook_database_key = database_key.clone();
        connection.update_hook(Some(
            move |_action: rusqlite::hooks::Action, _db: &str, tbl: &str, _row_id: i64| {
                crate::storage::sqlite::instance::notify_table_change(&hook_database_key, tbl);
            },
        ));

        let shared = Arc::new(Mutex::new(connection));
        *connection_slot = Some(Arc::clone(&shared));
        Ok(shared)
    }
}

impl Drop for RxStorageSqlite {
    fn drop(&mut self) {
        if let Some(database_key) = self.external_poll_key.lock().take() {
            release_external_database_poll(&database_key);
        }
    }
}

struct ExternalDatabasePollRegistration {
    stop: Arc<AtomicBool>,
    references: usize,
}

static EXTERNAL_DATABASE_POLLS: OnceLock<Mutex<HashMap<String, ExternalDatabasePollRegistration>>> =
    OnceLock::new();

fn acquire_external_database_poll(path: PathBuf, database_key: String) -> Option<String> {
    if sqlite_path_is_in_memory(&path) {
        return None;
    }
    let mut polls = EXTERNAL_DATABASE_POLLS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock();
    if let Some(existing) = polls.get_mut(&database_key) {
        existing.references = existing.references.saturating_add(1);
        return Some(database_key);
    }
    let stop = Arc::new(AtomicBool::new(false));
    start_external_database_poll(path, database_key.clone(), Arc::clone(&stop));
    polls.insert(
        database_key.clone(),
        ExternalDatabasePollRegistration {
            stop,
            references: 1,
        },
    );
    Some(database_key)
}

fn release_external_database_poll(database_key: &str) {
    let Some(registry) = EXTERNAL_DATABASE_POLLS.get() else {
        return;
    };
    let mut polls = registry.lock();
    let Some(existing) = polls.get_mut(database_key) else {
        return;
    };
    if existing.references > 1 {
        existing.references -= 1;
        return;
    }
    if let Some(existing) = polls.remove(database_key) {
        existing.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
fn external_database_poll_reference_count(database_key: &str) -> Option<usize> {
    EXTERNAL_DATABASE_POLLS.get().and_then(|registry| {
        registry
            .lock()
            .get(database_key)
            .map(|poll| poll.references)
    })
}

fn start_external_database_poll(path: PathBuf, database_key: String, stop: Arc<AtomicBool>) {
    if sqlite_path_is_in_memory(&path) {
        return;
    }
    let _ = thread::Builder::new()
        .name("rxdb-sqlite-external-poll".to_string())
        .spawn(move || {
            let file_watcher = sqlite_file_watcher(&path);
            let mut last_version: Option<i64> = None;
            let mut changed_tables: HashMap<String, i64>;
            let mut local_hook_generations: HashMap<String, u64>;
            let mut idle_reads = 0u32;
            let mut poll_interval = SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL;
            while !stop.load(Ordering::SeqCst) {
                match open_external_poll_connection(&path) {
                    Ok(conn) => {
                        last_version = read_data_version(&conn).ok().or(last_version);
                        changed_tables = read_changed_table_versions(&conn).unwrap_or_default();
                        local_hook_generations =
                            current_local_hook_generations(&database_key, changed_tables.keys());
                        while !stop.load(Ordering::SeqCst) {
                            if poll_interval == SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL {
                                if let Some((_, file_events)) = &file_watcher {
                                    wait_for_sqlite_file_change(&stop, poll_interval, file_events);
                                } else {
                                    sleep_external_poll(&stop, poll_interval);
                                }
                            } else {
                                sleep_external_poll(&stop, poll_interval);
                            }
                            if stop.load(Ordering::SeqCst) {
                                break;
                            }
                            metrics::record_sqlite_external_poll_wakeup(
                                poll_interval >= SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL,
                                &database_key,
                            );
                            let Ok(version) = read_data_version(&conn) else {
                                break;
                            };
                            let previous_version = last_version.replace(version);
                            if previous_version != Some(version) {
                                if previous_version.is_some() {
                                    metrics::record_sqlite_external_poll_data_version_change();
                                }
                                let mut keep_active = true;
                                if let Ok(next_changed_tables) = read_changed_table_versions(&conn)
                                {
                                    keep_active = false;
                                    metrics::record_sqlite_external_poll_changed_table_rows(
                                        next_changed_tables.len(),
                                    );
                                    for (table_name, changed_at) in next_changed_tables.iter() {
                                        if changed_tables.get(table_name) != Some(changed_at) {
                                            keep_active |=
                                                notify_external_table_change_unless_local_hook_ran(
                                                    &database_key,
                                                    table_name,
                                                    &mut local_hook_generations,
                                                );
                                        }
                                    }
                                    changed_tables = next_changed_tables;
                                    local_hook_generations.retain(|table_name, _| {
                                        changed_tables.contains_key(table_name)
                                    });
                                }
                                update_external_database_poll_backoff(
                                    keep_active,
                                    &mut idle_reads,
                                    &mut poll_interval,
                                );
                            } else {
                                update_external_database_poll_backoff(
                                    false,
                                    &mut idle_reads,
                                    &mut poll_interval,
                                );
                            }
                        }
                    }
                    Err(_) => {
                        sleep_external_poll(&stop, SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL);
                    }
                }
            }
        });
}

fn external_database_poll_interval_for_idle_reads(idle_reads: u32) -> Duration {
    if idle_reads >= SQLITE_EXTERNAL_DATABASE_POLL_BACKOFF_AFTER_IDLE_READS {
        // Same-process writes wake observers through SQLite update_hook. The
        // database-wide poll is only a rescue path for other processes touching
        // the file, so it must not become a daemon idle heartbeat.
        SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL
    } else {
        SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL
    }
}

fn update_external_database_poll_backoff(
    keep_active: bool,
    idle_reads: &mut u32,
    poll_interval: &mut Duration,
) {
    let previous_interval = *poll_interval;
    if keep_active {
        *idle_reads = 0;
        *poll_interval = SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL;
        if previous_interval != *poll_interval {
            metrics::record_sqlite_external_poll_active_reset();
        }
    } else {
        *idle_reads = idle_reads.saturating_add(1);
        *poll_interval = external_database_poll_interval_for_idle_reads(*idle_reads);
        if previous_interval != *poll_interval
            && *poll_interval == SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL
        {
            metrics::record_sqlite_external_poll_standby_entry();
        }
    }
}

fn sleep_external_poll(stop: &AtomicBool, duration: Duration) {
    let mut remaining = duration;
    let chunk = Duration::from_millis(250);
    while !stop.load(Ordering::SeqCst) && remaining > Duration::ZERO {
        let sleep_for = remaining.min(chunk);
        thread::sleep(sleep_for);
        remaining = remaining.saturating_sub(sleep_for);
    }
}

fn sqlite_file_watcher(
    database_path: &Path,
) -> Option<(RecommendedWatcher, Receiver<notify::Result<notify::Event>>)> {
    let (sender, receiver) = mpsc::channel();
    let watched_database = database_path.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let should_wake = event
            .as_ref()
            .map(|event| {
                is_sqlite_database_change_event(event)
                    && event
                        .paths
                        .iter()
                        .any(|path| is_sqlite_database_file(path, &watched_database))
            })
            .unwrap_or(true);
        if should_wake {
            let _ = sender.send(event);
        }
    })
    .ok()?;
    let parent = database_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    watcher.watch(parent, RecursiveMode::NonRecursive).ok()?;
    Some((watcher, receiver))
}

fn wait_for_sqlite_file_change(
    stop: &AtomicBool,
    duration: Duration,
    file_events: &Receiver<notify::Result<notify::Event>>,
) -> usize {
    let mut remaining = duration;
    let chunk = Duration::from_secs(1);
    while !stop.load(Ordering::SeqCst) && remaining > Duration::ZERO {
        let wait_for = remaining.min(chunk);
        match file_events.recv_timeout(wait_for) {
            Ok(_) => {
                // A single WAL commit commonly produces a burst of inotify
                // events. Reading PRAGMA data_version once per queued event
                // turns that burst into a hot loop even though every event
                // describes the same commit. Wait for a short quiet period and
                // consume the burst before performing the one rescue read.
                let started = std::time::Instant::now();
                let mut received = 1usize;
                while !stop.load(Ordering::SeqCst)
                    && started.elapsed() < SQLITE_FILE_EVENT_COALESCE_MAX_PERIOD
                {
                    let max_remaining =
                        SQLITE_FILE_EVENT_COALESCE_MAX_PERIOD.saturating_sub(started.elapsed());
                    let debounce = SQLITE_FILE_EVENT_COALESCE_QUIET_PERIOD.min(max_remaining);
                    if debounce.is_zero() {
                        break;
                    }
                    match file_events.recv_timeout(debounce) {
                        Ok(_) => received = received.saturating_add(1),
                        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
                    }
                }
                return received;
            }
            Err(RecvTimeoutError::Disconnected) => return 0,
            Err(RecvTimeoutError::Timeout) => {
                remaining = remaining.saturating_sub(wait_for);
            }
        }
    }
    0
}

fn is_sqlite_database_change_event(event: &notify::Event) -> bool {
    match event.kind {
        EventKind::Create(_) | EventKind::Remove(_) => true,
        EventKind::Modify(ModifyKind::Data(_))
        | EventKind::Modify(ModifyKind::Name(_))
        | EventKind::Modify(ModifyKind::Any)
        | EventKind::Modify(ModifyKind::Other) => true,
        EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime)) => false,
        EventKind::Modify(ModifyKind::Metadata(_)) => true,
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) => false,
        EventKind::Any | EventKind::Other => true,
    }
}

fn is_sqlite_database_file(path: &Path, database_path: &Path) -> bool {
    if path == database_path {
        return true;
    }
    let Some(database_name) = database_path.file_name() else {
        return false;
    };
    let Some(path_name) = path.file_name() else {
        return false;
    };
    let database_name = database_name.to_string_lossy();
    let path_name = path_name.to_string_lossy();
    // A committed WAL-mode write always changes the WAL (and a checkpoint
    // changes the database file).  The shared-memory file is different: SQLite
    // readers update its lock/index state as well. Watching `-shm` therefore
    // lets the poller's own PRAGMA data_version read enqueue the next wakeup,
    // turning the intended 30-minute standby into a permanent hot loop.
    path_name == database_name || path_name == format!("{database_name}-wal")
}

fn current_local_hook_generations<'a>(
    database_key: &str,
    table_names: impl Iterator<Item = &'a String>,
) -> HashMap<String, u64> {
    table_names
        .map(|table_name| {
            (
                table_name.clone(),
                crate::storage::sqlite::instance::table_local_hook_generation(
                    database_key,
                    table_name,
                )
                .unwrap_or(0),
            )
        })
        .collect()
}

fn notify_external_table_change_unless_local_hook_ran(
    database_key: &str,
    table_name: &str,
    local_hook_generations: &mut HashMap<String, u64>,
) -> bool {
    let current_local_hook_generation =
        crate::storage::sqlite::instance::table_local_hook_generation(database_key, table_name)
            .unwrap_or(0);
    let previous_local_hook_generation =
        local_hook_generations.get(table_name).copied().unwrap_or(0);
    if current_local_hook_generation == previous_local_hook_generation {
        if crate::storage::sqlite::instance::notify_external_table_change(database_key, table_name)
        {
            metrics::record_sqlite_external_poll_changed_table_notification(table_name);
            local_hook_generations.insert(table_name.to_string(), current_local_hook_generation);
            return true;
        }
        local_hook_generations.insert(table_name.to_string(), current_local_hook_generation);
        false
    } else {
        metrics::record_sqlite_external_poll_local_hook_suppression(table_name);
        local_hook_generations.insert(table_name.to_string(), current_local_hook_generation);
        false
    }
}

fn open_external_poll_connection(path: &PathBuf) -> rusqlite::Result<Connection> {
    let conn = match Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => conn,
        Err(err) => {
            metrics::record_sqlite_external_poll_connection_open_failure();
            return Err(err);
        }
    };
    if let Err(err) = conn.busy_timeout(SQLITE_BUSY_TIMEOUT) {
        metrics::record_sqlite_external_poll_connection_open_failure();
        return Err(err);
    }
    metrics::record_sqlite_external_poll_connection_open();
    Ok(conn)
}

fn read_data_version(conn: &Connection) -> rusqlite::Result<i64> {
    metrics::record_sqlite_external_poll_data_version_read();
    let _statement_timer = metrics::timed_sqlite_statement();
    let result = conn.query_row("PRAGMA data_version", [], |row| row.get(0));
    if result.is_err() {
        metrics::record_sqlite_external_poll_data_version_read_failure();
    }
    result
}

fn read_changed_table_versions(conn: &Connection) -> rusqlite::Result<HashMap<String, i64>> {
    metrics::record_sqlite_external_poll_changed_table_read();
    let result = (|| {
        let exists = {
            let _statement_timer = metrics::timed_sqlite_statement();
            conn.query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
                [SQLITE_CHANGED_TABLES_TABLE],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some()
        };
        if !exists {
            return Ok(HashMap::new());
        }
        let _statement_timer = metrics::timed_sqlite_statement();
        let mut stmt = conn.prepare(&format!(
            "SELECT table_name, changed_at FROM {}",
            crate::storage::sqlite::sql::quote_identifier(SQLITE_CHANGED_TABLES_TABLE)
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (table_name, changed_at) = row?;
            out.insert(table_name, changed_at);
        }
        Ok(out)
    })();
    if result.is_err() {
        metrics::record_sqlite_external_poll_changed_table_read_failure();
    }
    result
}

pub fn sqlite_error(err: rusqlite::Error) -> crate::rx_error::RxError {
    new_rx_error(
        "SQLITE",
        Some(serde_json::json!({ "message": err.to_string() })),
    )
}

pub fn sqlite_io_error(err: std::io::Error) -> crate::rx_error::RxError {
    new_rx_error(
        "SQLITE",
        Some(serde_json::json!({ "message": err.to_string() })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_counter(name: &str) -> u64 {
        metrics::sqlite_runtime_counters_snapshot()
            .get(name)
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
    }

    #[test]
    fn external_database_poll_registry_is_per_database_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ctox.sqlite3");
        let database_key = crate::storage::sqlite::instance::database_key_for_path(&path);
        assert_eq!(external_database_poll_reference_count(&database_key), None);

        let first = RxStorageSqlite::new(RxStorageSqliteSettings {
            database_path: path.clone(),
        });
        first.connection().unwrap();
        assert_eq!(
            external_database_poll_reference_count(&database_key),
            Some(1)
        );

        let second = RxStorageSqlite::new(RxStorageSqliteSettings {
            database_path: path,
        });
        second.connection().unwrap();
        assert_eq!(
            external_database_poll_reference_count(&database_key),
            Some(2),
            "one DB-wide external poller should be shared per SQLite path"
        );

        drop(first);
        assert_eq!(
            external_database_poll_reference_count(&database_key),
            Some(1),
            "dropping one storage factory must keep the shared poller alive"
        );
        drop(second);
        assert_eq!(
            external_database_poll_reference_count(&database_key),
            None,
            "dropping the last storage factory must stop and unregister the shared poller"
        );
    }

    #[test]
    fn concurrent_first_connections_initialize_once_and_release_poll_registration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("concurrent-first.sqlite3");
        let database_key = crate::storage::sqlite::instance::database_key_for_path(&path);
        let storage = RxStorageSqlite::new(RxStorageSqliteSettings {
            database_path: path,
        });
        let worker_count = 16;
        let start = Arc::new(std::sync::Barrier::new(worker_count));

        let workers = (0..worker_count)
            .map(|_| {
                let storage = Arc::clone(&storage);
                let start = Arc::clone(&start);
                thread::spawn(move || {
                    start.wait();
                    storage.connection().unwrap()
                })
            })
            .collect::<Vec<_>>();
        let connections = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();

        let first = connections.first().unwrap();
        assert!(
            connections
                .iter()
                .all(|connection| Arc::ptr_eq(first, connection)),
            "all concurrent first callers must receive the single initialized connection"
        );
        assert_eq!(
            external_database_poll_reference_count(&database_key),
            Some(1),
            "concurrent initialization must acquire exactly one poll registration"
        );

        drop(connections);
        drop(storage);
        assert_eq!(
            external_database_poll_reference_count(&database_key),
            None,
            "dropping the storage must fully release its poll registration"
        );
    }

    #[test]
    fn external_database_poll_enters_standby_after_idle_reads() {
        assert_eq!(
            external_database_poll_interval_for_idle_reads(0),
            SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL
        );
        assert_eq!(
            external_database_poll_interval_for_idle_reads(
                SQLITE_EXTERNAL_DATABASE_POLL_BACKOFF_AFTER_IDLE_READS - 1,
            ),
            SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL
        );
        assert_eq!(
            external_database_poll_interval_for_idle_reads(
                SQLITE_EXTERNAL_DATABASE_POLL_BACKOFF_AFTER_IDLE_READS,
            ),
            SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL
        );
        assert!(
            SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL >= Duration::from_secs(30 * 60),
            "standby poll must not be a frequent daemon idle heartbeat"
        );
    }

    #[test]
    fn external_database_poll_keeps_standby_for_local_only_changes() {
        let mut idle_reads = SQLITE_EXTERNAL_DATABASE_POLL_BACKOFF_AFTER_IDLE_READS;
        let mut poll_interval = SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL;

        update_external_database_poll_backoff(false, &mut idle_reads, &mut poll_interval);
        assert_eq!(
            poll_interval, SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL,
            "local-only data_version changes must not restart 1s active polling"
        );

        update_external_database_poll_backoff(true, &mut idle_reads, &mut poll_interval);
        assert_eq!(idle_reads, 0);
        assert_eq!(poll_interval, SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL);
    }

    #[test]
    fn external_database_poll_records_backoff_transitions() {
        let standby_entries_before = runtime_counter("external_poll_standby_entries");
        let active_resets_before = runtime_counter("external_poll_active_resets");

        let mut idle_reads = SQLITE_EXTERNAL_DATABASE_POLL_BACKOFF_AFTER_IDLE_READS - 1;
        let mut poll_interval = SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL;

        update_external_database_poll_backoff(false, &mut idle_reads, &mut poll_interval);
        assert_eq!(
            poll_interval,
            SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL
        );
        assert!(
            runtime_counter("external_poll_standby_entries") > standby_entries_before,
            "entering the DB-wide poll standby interval must be visible in runtime counters"
        );

        update_external_database_poll_backoff(true, &mut idle_reads, &mut poll_interval);
        assert_eq!(poll_interval, SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL);
        assert!(
            runtime_counter("external_poll_active_resets") > active_resets_before,
            "resetting the DB-wide poll to active mode must be visible in runtime counters"
        );
    }

    #[test]
    fn sqlite_database_file_matching_includes_wal_but_excludes_reader_shm() {
        let dir = tempfile::tempdir().unwrap();
        let database_path = dir.path().join("ctox.sqlite3");
        assert!(is_sqlite_database_file(&database_path, &database_path));
        assert!(is_sqlite_database_file(
            &dir.path().join("ctox.sqlite3-wal"),
            &database_path
        ));
        assert!(
            !is_sqlite_database_file(&dir.path().join("ctox.sqlite3-shm"), &database_path),
            "reader-side shared-memory activity must not wake the poller"
        );
        assert!(!is_sqlite_database_file(
            &dir.path().join("other.sqlite3"),
            &database_path
        ));
    }

    #[test]
    fn sqlite_database_change_event_ignores_reader_access() {
        let read_event = notify::Event::new(EventKind::Access(AccessKind::Read));
        let open_read_event =
            notify::Event::new(EventKind::Access(AccessKind::Open(AccessMode::Read)));
        let close_read_event =
            notify::Event::new(EventKind::Access(AccessKind::Close(AccessMode::Read)));
        let close_write_event =
            notify::Event::new(EventKind::Access(AccessKind::Close(AccessMode::Write)));
        let data_event = notify::Event::new(EventKind::Modify(ModifyKind::Data(
            notify::event::DataChange::Any,
        )));

        assert!(!is_sqlite_database_change_event(&read_event));
        assert!(!is_sqlite_database_change_event(&open_read_event));
        assert!(!is_sqlite_database_change_event(&close_read_event));
        assert!(is_sqlite_database_change_event(&close_write_event));
        assert!(is_sqlite_database_change_event(&data_event));
    }

    #[test]
    fn sqlite_file_change_wait_coalesces_a_queued_event_burst() {
        let (sender, receiver) = mpsc::channel();
        for _ in 0..4 {
            sender
                .send(Ok(notify::Event::new(EventKind::Modify(ModifyKind::Data(
                    notify::event::DataChange::Any,
                )))))
                .unwrap();
        }
        let stop = AtomicBool::new(false);
        assert_eq!(
            wait_for_sqlite_file_change(&stop, Duration::from_secs(1), &receiver),
            4,
            "one WAL commit burst must trigger one rescue read, not one read per event"
        );
    }
}
