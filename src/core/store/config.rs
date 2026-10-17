//! The connection policy for every v4 store connection. No ambient tuning knobs.

use std::num::NonZeroUsize;
use std::time::Duration;

use rusqlite::Connection;

pub const READ_CONNECTIONS: usize = 4;
pub const MAX_BLOCKING_READS: usize = 16;
pub const MAX_ADMITTED_JOBS: usize = 1024;

/// Admission waits are distinct from SQLite execution: SQLite never sleeps on a lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmissionBudget {
    pub capacity: NonZeroUsize,
    pub wait: Duration,
    pub execution: Duration,
}

impl AdmissionBudget {
    pub fn new(capacity: NonZeroUsize, wait: Duration, execution: Duration) -> Option<Self> {
        if wait.is_zero()
            || execution.is_zero()
            || capacity.get() > MAX_ADMITTED_JOBS
            || std::time::Instant::now().checked_add(wait).is_none()
            || std::time::Instant::now().checked_add(execution).is_none()
        {
            return None;
        }
        Some(Self {
            capacity,
            wait,
            execution,
        })
    }
}

impl Default for AdmissionBudget {
    fn default() -> Self {
        Self {
            capacity: NonZeroUsize::new(1024).expect("nonzero constant"),
            wait: Duration::from_millis(50),
            execution: Duration::from_millis(20),
        }
    }
}

/// File-wide journal policy is initialized by the owning writer, never by a reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionConfig {
    pub journal_size_limit: u32,
    /// Zero disables SQLite's automatic checkpoint; the store writer schedules it.
    pub wal_autocheckpoint: u32,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            journal_size_limit: 64 * 1024 * 1024,
            wal_autocheckpoint: 0,
        }
    }
}

impl ConnectionConfig {
    pub fn apply_writer(self, connection: &Connection) -> rusqlite::Result<()> {
        connection.busy_timeout(Duration::ZERO)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        self.apply_common(connection)?;
        connection.pragma_update(None, "query_only", false)?;
        self.require_wal(connection)
    }

    /// A reader must not create a missing file or silently switch its journal mode.
    pub fn apply_reader(self, connection: &Connection) -> rusqlite::Result<()> {
        connection.busy_timeout(Duration::ZERO)?;
        self.require_wal(connection)?;
        self.apply_common(connection)?;
        connection.pragma_update(None, "query_only", true)
    }

    fn apply_common(self, connection: &Connection) -> rusqlite::Result<()> {
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_size_limit", self.journal_size_limit)?;
        connection.pragma_update(None, "wal_autocheckpoint", self.wal_autocheckpoint)
    }

    fn require_wal(self, connection: &Connection) -> rusqlite::Result<()> {
        let mode: String = connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        if mode.eq_ignore_ascii_case("wal") {
            Ok(())
        } else {
            Err(rusqlite::Error::InvalidParameterName(format!(
                "v4 store requires WAL, found {mode}"
            )))
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
