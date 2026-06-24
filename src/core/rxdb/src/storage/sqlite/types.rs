//! Types for the SQLite storage backend.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::rx_error::{new_rx_error, RxResult};

/// Matches upstream's `:memory:` SQLite database marker.
pub const SQLITE_IN_MEMORY_DB_NAME: &str = ":memory:";
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(10);
const SQLITE_EXTERNAL_DATABASE_POLL_MIN_INTERVAL: Duration = Duration::from_secs(1);
const SQLITE_EXTERNAL_DATABASE_POLL_MAX_INTERVAL: Duration = Duration::from_secs(30);
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
    external_poll_stop: Arc<AtomicBool>,
}

impl RxStorageSqlite {
    pub fn new(settings: RxStorageSqliteSettings) -> Arc<Self> {
        Arc::new(Self {
            name: "sqlite".to_string(),
            settings,
            connection: Mutex::new(None),
            external_poll_stop: Arc::new(AtomicBool::new(false)),
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

        let database_key = crate::storage::sqlite::instance::database_key_for_path(path);
        start_external_database_poll(
            path.clone(),
            database_key.clone(),
            Arc::clone(&self.external_poll_stop),
        );

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
        self.external_poll_stop.store(true, Ordering::SeqCst);
    }
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
            let mut table_generations: HashMap<String, u64>;
            let mut idle_reads = 0u32;
            let mut poll_interval = SQLITE_EXTERNAL_DATABASE_POLL_MIN_INTERVAL;
            while !stop.load(Ordering::SeqCst) {
                match open_external_poll_connection(&path) {
                    Ok(conn) => {
                        last_version = read_data_version(&conn).ok().or(last_version);
                        changed_tables = read_changed_table_versions(&conn).unwrap_or_default();
                        table_generations =
                            current_table_generations(&database_key, changed_tables.keys());
                        while !stop.load(Ordering::SeqCst) {
                            sleep_external_poll(&stop, poll_interval);
                            if stop.load(Ordering::SeqCst) {
                                break;
                            }
                            let Ok(version) = read_data_version(&conn) else {
                                break;
                            };
                            if last_version.replace(version) != Some(version) {
                                idle_reads = 0;
                                poll_interval = SQLITE_EXTERNAL_DATABASE_POLL_MIN_INTERVAL;
                                if let Ok(next_changed_tables) = read_changed_table_versions(&conn)
                                {
                                    for (table_name, changed_at) in next_changed_tables.iter() {
                                        if changed_tables.get(table_name) != Some(changed_at) {
                                            notify_external_table_change_unless_local_hook_ran(
                                                &database_key,
                                                table_name,
                                                &mut table_generations,
                                            );
                                        }
                                    }
                                    changed_tables = next_changed_tables;
                                    table_generations.retain(|table_name, _| {
                                        changed_tables.contains_key(table_name)
                                    });
                                }
                            } else {
                                idle_reads = idle_reads.saturating_add(1);
                                if idle_reads
                                    >= SQLITE_EXTERNAL_DATABASE_POLL_BACKOFF_AFTER_IDLE_READS
                                {
                                    poll_interval = (poll_interval * 2)
                                        .min(SQLITE_EXTERNAL_DATABASE_POLL_MAX_INTERVAL);
                                }
                            }
                        }
                    }
                    Err(_) => {
                        sleep_external_poll(&stop, SQLITE_EXTERNAL_DATABASE_POLL_MIN_INTERVAL);
                    }
                }
            }
        });
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

fn current_table_generations<'a>(
    database_key: &str,
    table_names: impl Iterator<Item = &'a String>,
) -> HashMap<String, u64> {
    table_names
        .map(|table_name| {
            (
                table_name.clone(),
                crate::storage::sqlite::instance::table_change_generation(database_key, table_name)
                    .unwrap_or(0),
            )
        })
        .collect()
}

fn notify_external_table_change_unless_local_hook_ran(
    database_key: &str,
    table_name: &str,
    table_generations: &mut HashMap<String, u64>,
) {
    let current_generation =
        crate::storage::sqlite::instance::table_change_generation(database_key, table_name)
            .unwrap_or(0);
    let previous_generation = table_generations.get(table_name).copied().unwrap_or(0);
    if current_generation == previous_generation {
        crate::storage::sqlite::instance::notify_table_change(database_key, table_name);
        let next_generation =
            crate::storage::sqlite::instance::table_change_generation(database_key, table_name)
                .unwrap_or(current_generation);
        table_generations.insert(table_name.to_string(), next_generation);
    } else {
        table_generations.insert(table_name.to_string(), current_generation);
    }
}

fn open_external_poll_connection(path: &PathBuf) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
    Ok(conn)
}

fn read_data_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA data_version", [], |row| row.get(0))
}

fn read_changed_table_versions(conn: &Connection) -> rusqlite::Result<HashMap<String, i64>> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            [SQLITE_CHANGED_TABLES_TABLE],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(HashMap::new());
    }
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
