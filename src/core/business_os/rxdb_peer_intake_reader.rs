// Origin: CTOX
// License: AGPL-3.0-only

//! A reader owned by one command-consumer task, not a process-wide pool.
//! Only the physical read-only connection is reused. Queries, table discovery
//! and domain-receipt selection remain fresh; no read transaction survives.

use anyhow::Context;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

tokio::task_local! {
    static INTAKE_READER: Arc<IntakeReader>;
}

#[derive(Default)]
struct IntakeReader {
    connection: Mutex<Option<ReaderConnection>>,
    #[cfg(unix)]
    blocked_wal: Mutex<Option<(std::path::PathBuf, u64, u64)>>,
}

struct ReaderConnection {
    conn: Connection,
    #[cfg(unix)]
    identity: (std::path::PathBuf, u64, u64),
    #[cfg(unix)]
    wal_identity: Option<(std::path::PathBuf, u64, u64)>,
    opened_at: Instant,
    remaining_reads: u8,
}

#[cfg(unix)]
fn identity(path: &Path) -> anyhow::Result<(std::path::PathBuf, u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let canonical = std::fs::canonicalize(path)?;
    let metadata = std::fs::metadata(&canonical)?;
    Ok((canonical, metadata.dev(), metadata.ino()))
}

#[cfg(unix)]
fn wal_identity(path: &Path) -> anyhow::Result<Option<(std::path::PathBuf, u64, u64)>> {
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    let wal = std::path::PathBuf::from(wal);
    match std::fs::metadata(&wal) {
        // A header without frames cannot replay the old database's rows.
        Ok(metadata) if metadata.len() > 32 => Ok(Some(identity(&wal)?)),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

impl IntakeReader {
    fn query<T>(
        &self,
        path: &Path,
        absent: T,
        query: impl FnOnce(&Connection) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut slot = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("command intake reader poisoned"))?;
        // Dropping the slot before opening or querying also makes errors
        // fail closed. Missing files never preserve a stale connection.
        let previous = slot.take();
        if !path.is_file() {
            return Ok(absent);
        }
        #[cfg(unix)]
        let current_identity = identity(path)?;
        #[cfg(unix)]
        let current_wal = wal_identity(path)?;
        #[cfg(unix)]
        {
            let mut blocked = self
                .blocked_wal
                .lock()
                .map_err(|_| anyhow::anyhow!("command intake WAL fence poisoned"))?;
            // A fresh SQLite connection can replay an old WAL onto a replaced
            // main file too. Never delete/checkpoint it from a read-only lane;
            // keep rejecting this family until its stale WAL is removed/replaced.
            if let Some(stale) = blocked.as_ref() {
                anyhow::ensure!(
                    current_wal.as_ref() != Some(stale),
                    "command intake database replacement retained the old WAL"
                );
                *blocked = None;
            }
            if let Some(old) = previous.as_ref() {
                if old.identity != current_identity
                    && old.wal_identity.is_some()
                    && old.wal_identity == current_wal
                {
                    *blocked = current_wal;
                    anyhow::bail!("command intake database replacement retained the old WAL");
                }
            }
        }
        let reusable = previous.filter(|reader| {
            let within_budget = reader.remaining_reads > 0
                && reader.opened_at.elapsed() < Duration::from_secs(30)
                && reader.conn.is_autocommit();
            #[cfg(unix)]
            {
                within_budget && reader.identity == current_identity
            }
            #[cfg(not(unix))]
            {
                let _ = within_budget;
                false
            }
        });
        let mut reader = match reusable {
            Some(reader) => reader,
            None => {
                let conn = Connection::open_with_flags(
                    path,
                    OpenFlags::SQLITE_OPEN_READ_ONLY
                        | OpenFlags::SQLITE_OPEN_NO_MUTEX
                        | OpenFlags::SQLITE_OPEN_URI,
                )?;
                conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
                ReaderConnection {
                    conn,
                    #[cfg(unix)]
                    identity: current_identity.clone(),
                    #[cfg(unix)]
                    wal_identity: current_wal.clone(),
                    opened_at: Instant::now(),
                    remaining_reads: 64,
                }
            }
        };
        let result = query(&reader.conn);
        // retry_predicate attaches the current receipt store when needed.
        // Detach on success AND failure, so a later query cannot see an old
        // attached database (or fail with an already-attached alias).
        let attached: bool = reader.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_database_list WHERE name = 'domain_receipt_source')", [], |row| row.get(0))?;
        if attached {
            reader
                .conn
                .execute_batch("DETACH DATABASE domain_receipt_source")?;
        }
        let value = result?;
        anyhow::ensure!(
            reader.conn.is_autocommit(),
            "command intake query retained a transaction"
        );
        reader.remaining_reads -= 1;
        #[cfg(unix)]
        {
            let after_wal = wal_identity(path)?;
            if identity(path)? != current_identity {
                if reader.wal_identity.is_some() && reader.wal_identity == after_wal {
                    *self
                        .blocked_wal
                        .lock()
                        .map_err(|_| anyhow::anyhow!("command intake WAL fence poisoned"))? =
                        after_wal;
                }
                anyhow::bail!("command intake database replaced during read");
            }
            reader.wal_identity = after_wal;
            *slot = Some(reader);
        }
        Ok(value)
    }
}

pub(super) async fn scope<T>(future: impl std::future::Future<Output = T>) -> T {
    INTAKE_READER
        .scope(Arc::new(IntakeReader::default()), future)
        .await
}

pub(super) async fn read<T: Send + 'static>(
    root: &Path,
    absent: T,
    query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    // Capture the task-owned reader before crossing the blocking-task boundary.
    // Calls outside a consumer scope get their own disposable reader.
    let reader = INTAKE_READER.try_with(Arc::clone).unwrap_or_default();
    let path = super::store::rxdb_store_path(root);
    tokio::task::spawn_blocking(move || reader.query(&path, absent, query))
        .await
        .context("join command intake SQLite reader")?
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn intake_reader_sees_new_schema_and_releases_an_unfinished_transaction() -> anyhow::Result<()>
    {
        let root = tempfile::tempdir()?;
        let path = root.path().join("commands.sqlite3");
        let writer = Connection::open(&path)?;
        writer.busy_timeout(Duration::ZERO)?;
        writer.execute_batch("CREATE TABLE sample(value INTEGER); INSERT INTO sample VALUES(1)")?;
        let reader = IntakeReader::default();
        assert_eq!(value(&reader, &path)?, 1);
        writer.execute_batch("ALTER TABLE sample ADD COLUMN fresh INTEGER DEFAULT 7")?;
        assert_eq!(
            reader.query(&path, -1, |conn| Ok(conn.query_row(
                "SELECT fresh FROM sample",
                [],
                |row| row.get::<_, i64>(0)
            )?))?,
            7
        );
        let retained: anyhow::Result<i64> = reader.query(&path, -1, |conn| {
            conn.execute_batch("BEGIN")?;
            Ok(conn.query_row("SELECT value FROM sample", [], |row| row.get(0))?)
        });
        assert!(retained.is_err());
        assert!(reader.connection.lock().unwrap().is_none());
        // A separate writer with no busy wait proves that the rejected read
        // transaction did not escape the query boundary.
        writer.execute("UPDATE sample SET value=2", [])?;
        assert_eq!(value(&reader, &path)?, 2);
        Ok(())
    }

    #[tokio::test]
    async fn intake_reader_scope_survives_blocking_tasks_and_keeps_peers_separate(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = super::super::store::rxdb_store_path(root.path());
        std::fs::create_dir_all(path.parent().unwrap())?;
        let writer = Connection::open(&path)?;
        writer.execute_batch("CREATE TABLE sample(value INTEGER); INSERT INTO sample VALUES(1)")?;
        let first = scope(async {
            let reader = INTAKE_READER.with(Arc::clone);
            let read_value = |conn: &Connection| {
                Ok(conn.query_row("SELECT value FROM sample", [], |row| row.get::<_, i64>(0))?)
            };
            assert_eq!(read(root.path(), -1, read_value).await?, 1);
            let opened_at = reader
                .connection
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .opened_at;
            writer.execute("UPDATE sample SET value=2", [])?;
            assert_eq!(read(root.path(), -1, read_value).await?, 2);
            assert_eq!(
                reader
                    .connection
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .opened_at,
                opened_at
            );
            Ok::<_, anyhow::Error>(reader)
        })
        .await?;
        scope(async {
            let second = INTAKE_READER.with(Arc::clone);
            assert!(!Arc::ptr_eq(&first, &second));
            assert!(second.connection.lock().unwrap().is_none());
        })
        .await;
        Ok(())
    }

    fn value(reader: &IntakeReader, path: &Path) -> anyhow::Result<i64> {
        reader.query(path, -1, |conn| {
            Ok(conn.query_row("SELECT value FROM sample", [], |row| row.get(0))?)
        })
    }

    #[test]
    fn intake_reader_reuses_connection_but_reads_external_changes_and_replacement(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("commands.sqlite3");
        let writer = Connection::open(&path)?;
        writer.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE sample(value INTEGER); INSERT INTO sample VALUES(1)")?;
        let reader = IntakeReader::default();
        assert_eq!(value(&reader, &path)?, 1);
        let opened_at = reader
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .opened_at;
        writer.execute("UPDATE sample SET value=2", [])?;
        assert_eq!(value(&reader, &path)?, 2);
        assert_eq!(
            reader
                .connection
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .opened_at,
            opened_at
        );
        assert!(reader
            .connection
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .conn
            .is_autocommit());
        drop(writer);
        let replacement = root.path().join("replacement.sqlite3");
        let writer = Connection::open(&replacement)?;
        writer.execute_batch("CREATE TABLE sample(value INTEGER); INSERT INTO sample VALUES(3)")?;
        drop(writer);
        // Replace the complete SQLite family. Keeping the old WAL at this
        // pathname is not a valid replacement: even an uncached fresh reader
        // replays its rows onto the replacement main file.
        std::fs::rename(&path, root.path().join("old.sqlite3"))?;
        for suffix in ["-wal", "-shm"] {
            let side = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
            if side.exists() {
                std::fs::rename(side, root.path().join(format!("old.sqlite3{suffix}")))?;
            }
        }
        std::fs::rename(&replacement, &path)?;
        assert_eq!(value(&reader, &path)?, 3);
        std::fs::remove_file(&path)?;
        assert_eq!(value(&reader, &path)?, -1);
        assert!(reader.connection.lock().unwrap().is_none());
        Ok(())
    }

    #[test]
    fn intake_reader_rejects_main_only_replacement_until_the_old_wal_is_removed(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("commands.sqlite3");
        let writer = Connection::open(&path)?;
        writer.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE sample(value INTEGER); INSERT INTO sample VALUES(1)")?;
        let reader = IntakeReader::default();
        assert_eq!(value(&reader, &path)?, 1);
        writer.execute("UPDATE sample SET value=2", [])?;
        assert_eq!(value(&reader, &path)?, 2);
        drop(writer);
        let replacement = root.path().join("replacement.sqlite3");
        let writer = Connection::open(&replacement)?;
        writer.execute_batch("CREATE TABLE sample(value INTEGER); INSERT INTO sample VALUES(3)")?;
        drop(writer);
        std::fs::rename(&replacement, &path)?;
        // Even a newly opened connection would replay value 2 from this WAL.
        // Reject both the first and subsequent reads instead of accepting it.
        for _ in 0..2 {
            let error = value(&reader, &path).unwrap_err();
            assert!(error.to_string().contains("retained the old WAL"));
            assert!(reader.connection.lock().unwrap().is_none());
        }
        // Only the fixture's replacement owner removes the old sidecars. The
        // production reader must never mutate or checkpoint this file family.
        for suffix in ["-wal", "-shm"] {
            let side = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
            if side.exists() {
                std::fs::remove_file(side)?;
            }
        }
        assert_eq!(value(&reader, &path)?, 3);
        assert!(reader.blocked_wal.lock().unwrap().is_none());
        Ok(())
    }

    #[test]
    fn intake_reader_detaches_receipts_and_discards_errors_and_expired_connections(
    ) -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("commands.sqlite3");
        let writer = Connection::open(&path)?;
        writer.execute_batch("CREATE TABLE sample(value INTEGER); INSERT INTO sample VALUES(1)")?;
        let reader = IntakeReader::default();
        for _ in 0..2 {
            reader.query(&path, (), |conn| {
                conn.execute_batch("ATTACH DATABASE ':memory:' AS domain_receipt_source")?;
                Ok(())
            })?;
        }
        assert_eq!(value(&reader, &path)?, 1);
        let failed: anyhow::Result<()> = reader.query(&path, (), |conn| {
            conn.execute_batch("ATTACH DATABASE ':memory:' AS domain_receipt_source")?;
            anyhow::bail!("query failed")
        });
        assert!(failed.is_err());
        assert!(reader.connection.lock().unwrap().is_none());
        value(&reader, &path)?;
        for expire_by_age in [false, true] {
            let opened_at;
            {
                let mut slot = reader.connection.lock().unwrap();
                let cached = slot.as_mut().unwrap();
                if expire_by_age {
                    cached.opened_at = Instant::now() - Duration::from_secs(31);
                } else {
                    cached.remaining_reads = 0;
                }
                opened_at = cached.opened_at;
            }
            value(&reader, &path)?;
            assert_ne!(
                reader
                    .connection
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .opened_at,
                opened_at
            );
        }
        Ok(())
    }
}
