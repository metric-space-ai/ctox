//! Types for the SQLite storage backend.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::Connection;

use crate::rx_error::{new_rx_error, RxResult};

/// Matches upstream's `:memory:` SQLite database marker.
pub const SQLITE_IN_MEMORY_DB_NAME: &str = ":memory:";

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
}

impl RxStorageSqlite {
    pub fn new(settings: RxStorageSqliteSettings) -> Arc<Self> {
        Arc::new(Self {
            name: "sqlite".to_string(),
            settings,
            connection: Mutex::new(None),
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
            .execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                PRAGMA foreign_keys = ON;
                "#,
            )
            .map_err(sqlite_error)?;

        // Register the update hook for immediate same-process reactivity
        connection.update_hook(Some(|_action: rusqlite::hooks::Action, _db: &str, tbl: &str, _row_id: i64| {
            crate::storage::sqlite::instance::notify_table_change(tbl);
        }));

        let shared = Arc::new(Mutex::new(connection));
        *self.connection.lock() = Some(Arc::clone(&shared));
        Ok(shared)
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
