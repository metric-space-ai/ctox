//! Types for the SQLite storage backend.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::rx_error::{new_rx_error, RxResult};

/// Matches upstream's `:memory:` SQLite database marker.
pub const SQLITE_IN_MEMORY_DB_NAME: &str = ":memory:";
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(10);
const SQLITE_EXTERNAL_DATABASE_POLL_ACTIVE_INTERVAL: Duration = Duration::from_secs(1);
// One persistent PRAGMA data_version read is the cross-process wake source for
// every collection in this database. Keeping that cheap database-wide watcher
// responsive lets latency-sensitive consumers sleep instead of reopening
// SQLite and querying their collection every second.
const SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL: Duration = Duration::from_millis(1_500);
const SQLITE_EXTERNAL_DATABASE_POLL_BACKOFF_AFTER_IDLE_READS: u32 = 3;
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

/// Storage factory holding a shared SQLite connection.
pub struct RxStorageSqlite {
    pub name: String,
    pub settings: RxStorageSqliteSettings,
    pub connection: Mutex<Option<SharedSqliteConnection>>,
    external_poll_key: Mutex<Option<String>>,
}

impl RxStorageSqlite {
    pub fn new(settings: RxStorageSqliteSettings) -> Arc<Self> {
        Arc::new(Self {
            name: "sqlite".to_string(),
            settings,
            connection: Mutex::new(None),
            external_poll_key: Mutex::new(None),
        })
    }

    pub fn connection(&self) -> RxResult<SharedSqliteConnection> {
        if let Some(existing) = self.connection.lock().clone() {
            return Ok(existing);
        }

        let path = &self.settings.database_path;
        if path != SQLITE_IN_MEMORY_DB_NAME {
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
            let _statement_timer = crate::storage::sqlite::instance::timed_sqlite_statement();
            connection
                .execute_batch(
                    r#"
                PRAGMA journal_mode = WAL;
                PRAGMA busy_timeout = 10000;
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
        *self.connection.lock() = Some(Arc::clone(&shared));
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
    if path.as_os_str() == SQLITE_IN_MEMORY_DB_NAME {
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
    if path.as_os_str() == SQLITE_IN_MEMORY_DB_NAME {
        return;
    }
    let _ = thread::Builder::new()
        .name("rxdb-sqlite-external-poll".to_string())
        .spawn(move || {
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
                            sleep_external_poll(&stop, poll_interval);
                            if stop.load(Ordering::SeqCst) {
                                break;
                            }
                            crate::storage::sqlite::instance::record_sqlite_external_poll_wakeup(
                                poll_interval >= SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL,
                                &database_key,
                            );
                            let Ok(version) = read_data_version(&conn) else {
                                break;
                            };
                            let previous_version = last_version.replace(version);
                            if previous_version != Some(version) {
                                if previous_version.is_some() {
                                    crate::storage::sqlite::instance::record_sqlite_external_poll_data_version_change();
                                }
                                let mut keep_active = true;
                                if let Ok(next_changed_tables) = read_changed_table_versions(&conn) {
                                    keep_active = false;
                                    crate::storage::sqlite::instance::record_sqlite_external_poll_changed_table_rows(
                                        next_changed_tables.len(),
                                    );
                                    for (table_name, changed_at) in next_changed_tables.iter() {
                                        if changed_tables.get(table_name) != Some(changed_at) {
                                            keep_active |= notify_external_table_change_unless_local_hook_ran(
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
            crate::storage::sqlite::instance::record_sqlite_external_poll_active_reset();
        }
    } else {
        *idle_reads = idle_reads.saturating_add(1);
        *poll_interval = external_database_poll_interval_for_idle_reads(*idle_reads);
        if previous_interval != *poll_interval
            && *poll_interval == SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL
        {
            crate::storage::sqlite::instance::record_sqlite_external_poll_standby_entry();
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
            crate::storage::sqlite::instance::record_sqlite_external_poll_changed_table_notification(
                table_name,
            );
            local_hook_generations.insert(table_name.to_string(), current_local_hook_generation);
            return true;
        }
        local_hook_generations.insert(table_name.to_string(), current_local_hook_generation);
        false
    } else {
        crate::storage::sqlite::instance::record_sqlite_external_poll_local_hook_suppression(
            table_name,
        );
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
            crate::storage::sqlite::instance::record_sqlite_external_poll_connection_open_failure();
            return Err(err);
        }
    };
    if let Err(err) = conn.busy_timeout(SQLITE_BUSY_TIMEOUT) {
        crate::storage::sqlite::instance::record_sqlite_external_poll_connection_open_failure();
        return Err(err);
    }
    crate::storage::sqlite::instance::record_sqlite_external_poll_connection_open();
    Ok(conn)
}

fn read_data_version(conn: &Connection) -> rusqlite::Result<i64> {
    crate::storage::sqlite::instance::record_sqlite_external_poll_data_version_read();
    let _statement_timer = crate::storage::sqlite::instance::timed_sqlite_statement();
    let result = conn.query_row("PRAGMA data_version", [], |row| row.get(0));
    if result.is_err() {
        crate::storage::sqlite::instance::record_sqlite_external_poll_data_version_read_failure();
    }
    result
}

fn read_changed_table_versions(conn: &Connection) -> rusqlite::Result<HashMap<String, i64>> {
    crate::storage::sqlite::instance::record_sqlite_external_poll_changed_table_read();
    let result = (|| {
        let exists = {
            let _statement_timer = crate::storage::sqlite::instance::timed_sqlite_statement();
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
        let _statement_timer = crate::storage::sqlite::instance::timed_sqlite_statement();
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
        crate::storage::sqlite::instance::record_sqlite_external_poll_changed_table_read_failure();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_counter(name: &str) -> u64 {
        crate::storage::sqlite::instance::sqlite_runtime_counters_snapshot()
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
            SQLITE_EXTERNAL_DATABASE_POLL_STANDBY_INTERVAL <= Duration::from_secs(2),
            "cross-process collection writes must wake consumers within two seconds"
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
