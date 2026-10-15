#[path = "persistence_write_transaction.rs"]
mod write_transaction;
pub(crate) use write_transaction::SqliteWriteTransaction;

use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_SQLITE_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";
const PAYLOAD_TABLE: &str = "ctox_payload_store";
const KV_TABLE: &str = "ctox_kv_store";
const LEASE_TABLE: &str = "ctox_lease_store";

/// First Core owner adapter (P3-17). The daemon supplies its existing writer;
/// CLI uses P3-08 IPC. Synchronous callers below remain migration inventory.
#[derive(Clone)]
pub(crate) struct CorePersistence {
    writer: ctox_store::writer::WriterHandle,
    readers: ctox_store::read_pool::ReadPool,
}

impl CorePersistence {
    /// The composition root supplies both handles once per DbId. Attaching or
    /// cloning an adapter must never create another reader pool or file owner.
    pub(crate) async fn attach(
        writer: ctox_store::writer::WriterHandle,
        readers: ctox_store::read_pool::ReadPool,
    ) -> Result<Self> {
        writer.ready().await?;
        anyhow::ensure!(
            readers.generation() == writer.generation().0,
            "Core reader pool does not belong to this owner generation"
        );
        write_transaction::owner_write(&writer, "core.schema", Default::default(), |tx| {
            tx.execute_batch("CREATE TABLE IF NOT EXISTS ctox_payload_store (payload_key TEXT PRIMARY KEY, payload_json TEXT NOT NULL, updated_at INTEGER NOT NULL);
                CREATE TABLE IF NOT EXISTS ctox_kv_store (kv_key TEXT PRIMARY KEY, kv_value TEXT NOT NULL, updated_at INTEGER NOT NULL);
                CREATE TABLE IF NOT EXISTS ctox_lease_store (lease_key TEXT PRIMARY KEY, lease_value TEXT NOT NULL, updated_at INTEGER NOT NULL);")?;
            Ok(())
        }).await?;
        Ok(Self { writer, readers })
    }

    pub(crate) async fn load_json_payload<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>> {
        let key = key.to_owned();
        let raw: Option<String> = self
            .readers
            .read_job(ctox_store::ReadJob::new(Default::default(), move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT payload_json FROM ctox_payload_store WHERE payload_key=?1",
                        [key],
                        |row| row.get(0),
                    )
                    .optional()?)
            }))
            .await?;
        raw.map(|raw| serde_json::from_str(&raw))
            .transpose()
            .context("decode owner payload")
    }

    pub(crate) async fn store_json_payload<T: Serialize>(
        &self,
        key: &str,
        payload: Option<&T>,
    ) -> Result<()> {
        let key = key.to_owned();
        // Serialization stays outside the writer's SQLite lock window.
        let raw = payload.map(serde_json::to_string_pretty).transpose()?;
        write_transaction::owner_write(&self.writer, "payload.store", Default::default(), move |tx| {
            match raw {
                Some(raw) => { tx.execute("INSERT INTO ctox_payload_store VALUES(?1,?2,?3) ON CONFLICT(payload_key) DO UPDATE SET payload_json=excluded.payload_json,updated_at=excluded.updated_at", params![key, raw, now_epoch_secs()])?; }
                None => { tx.execute("DELETE FROM ctox_payload_store WHERE payload_key=?1", [key])?; }
            }
            Ok(())
        }).await?;
        Ok(())
    }

    /// The prepared candidate never overwrites a canonical row, including empty
    /// JSON. Preparation may do I/O, so it must happen before writer admission.
    pub(crate) async fn load_or_insert_json_payload<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
        candidate: &T,
    ) -> Result<T> {
        let key = key.to_owned();
        let candidate = serde_json::to_string_pretty(candidate)?;
        let raw: String = write_transaction::owner_write(
            &self.writer,
            "payload.legacy_import",
            Default::default(),
            move |tx| {
                tx.execute(
                    "INSERT OR IGNORE INTO ctox_payload_store VALUES(?1,?2,?3)",
                    params![key, candidate, now_epoch_secs()],
                )?;
                Ok(tx.query_row(
                    "SELECT payload_json FROM ctox_payload_store WHERE payload_key=?1",
                    [key],
                    |row| row.get(0),
                )?)
            },
        )
        .await?;
        serde_json::from_str(&raw).context("decode canonical owner payload")
    }

    pub(crate) async fn try_acquire_lease(&self, key: &str, value: &str) -> Result<bool> {
        let (key, value) = (key.to_owned(), value.to_owned());
        Ok(write_transaction::owner_write(
            &self.writer,
            "lease.acquire",
            Default::default(),
            move |tx| {
                Ok(tx.execute(
                    "INSERT OR IGNORE INTO ctox_lease_store VALUES(?1,?2,?3)",
                    params![key, value, now_epoch_secs()],
                )? > 0)
            },
        )
        .await?)
    }

    pub(crate) async fn load_lease_value(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_owned();
        Ok(self
            .readers
            .read_job(ctox_store::ReadJob::new(Default::default(), move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT lease_value FROM ctox_lease_store WHERE lease_key=?1",
                        [key],
                        |row| row.get(0),
                    )
                    .optional()?)
            }))
            .await?)
    }

    pub(crate) async fn release_lease(&self, key: &str) -> Result<()> {
        let key = key.to_owned();
        write_transaction::owner_write(
            &self.writer,
            "lease.release",
            Default::default(),
            move |tx| {
                tx.execute("DELETE FROM ctox_lease_store WHERE lease_key=?1", [key])?;
                Ok(())
            },
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn shutdown(&self) -> Result<()> {
        self.readers.shutdown().await?;
        self.writer.shutdown().await?;
        Ok(())
    }
}

pub fn sqlite_path(root: &Path) -> PathBuf {
    crate::paths::core_db(root)
}

pub fn sqlite_busy_timeout_duration() -> Duration {
    let millis = std::env::var("CTOX_SQLITE_BUSY_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1..=120_000).contains(value))
        .unwrap_or(30_000);
    Duration::from_millis(millis)
}

pub fn sqlite_busy_timeout_millis() -> u64 {
    sqlite_busy_timeout_duration().as_millis() as u64
}

pub fn load_json_payload<T>(root: &Path, key: &str) -> Result<Option<T>>
where
    T: DeserializeOwned,
{
    let conn = open_sqlite(root)?;
    let payload_json: Option<String> = conn
        .query_row(
            &format!("SELECT payload_json FROM {PAYLOAD_TABLE} WHERE payload_key = ?1"),
            params![key],
            |row| row.get(0),
        )
        .optional()
        .with_context(|| format!("failed to load payload {key}"))?;
    payload_json
        .map(|payload| {
            serde_json::from_str::<T>(&payload)
                .with_context(|| format!("failed to decode payload {key}"))
        })
        .transpose()
}

pub fn store_json_payload<T>(root: &Path, key: &str, payload: Option<&T>) -> Result<()>
where
    T: Serialize,
{
    let conn = open_sqlite(root)?;
    match payload {
        Some(payload) => {
            let encoded = serde_json::to_string_pretty(payload)
                .with_context(|| format!("failed to encode payload {key}"))?;
            conn.execute(
                &format!(
                    "INSERT INTO {PAYLOAD_TABLE} (payload_key, payload_json, updated_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(payload_key) DO UPDATE SET
                       payload_json = excluded.payload_json,
                       updated_at = excluded.updated_at"
                ),
                params![key, encoded, now_epoch_secs()],
            )
            .with_context(|| format!("failed to persist payload {key}"))?;
        }
        None => {
            conn.execute(
                &format!("DELETE FROM {PAYLOAD_TABLE} WHERE payload_key = ?1"),
                params![key],
            )
            .with_context(|| format!("failed to delete payload {key}"))?;
        }
    }
    Ok(())
}

/// Resolve a one-time legacy import under the canonical payload's writer fence.
/// The initializer is never used when a canonical row (even empty) exists.
pub(crate) fn load_or_insert_json_payload<T>(
    root: &Path,
    key: &str,
    initialize: impl FnOnce() -> Result<T>,
) -> Result<T>
where
    T: DeserializeOwned + Serialize,
{
    let conn = open_sqlite(root)?;
    let tx = SqliteWriteTransaction::begin(&conn, "payload.legacy_import")?;
    let existing: Option<String> = tx
        .query_row(
            &format!("SELECT payload_json FROM {PAYLOAD_TABLE} WHERE payload_key = ?1"),
            params![key],
            |row| row.get(0),
        )
        .optional()?;
    let value = match existing {
        Some(raw) => serde_json::from_str(&raw)?,
        None => {
            let value = initialize()?;
            tx.execute(
                &format!("INSERT INTO {PAYLOAD_TABLE} (payload_key, payload_json, updated_at) VALUES (?1, ?2, ?3)"),
                params![key, serde_json::to_string_pretty(&value)?, now_epoch_secs()],
            )?;
            value
        }
    };
    tx.commit()?;
    Ok(value)
}

// Keep only a connection, never a value or a transaction. In particular the
// legacy secret-key check must see insertions/removals made after an earlier
// read. One slot per calling thread bounds retention when roots change.
#[cfg(unix)]
struct CachedTextReader {
    identity: (PathBuf, u64, u64),
    connection: Connection,
}

#[cfg(unix)]
thread_local! {
    static TEXT_READER: std::cell::RefCell<Option<CachedTextReader>> =
        const { std::cell::RefCell::new(None) };
    #[cfg(test)]
    static TEXT_READER_OPENS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(unix)]
fn text_reader_identity(path: &Path) -> std::io::Result<(PathBuf, u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let canonical = std::fs::canonicalize(path)?;
    let metadata = std::fs::metadata(&canonical)?;
    Ok((canonical, metadata.dev(), metadata.ino()))
}

fn read_text_value(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        &format!("SELECT kv_value FROM {KV_TABLE} WHERE kv_key = ?1"),
        params![key],
        |row| row.get(0),
    )
    .optional()
    .with_context(|| format!("failed to load kv value {key}"))
}

pub fn load_text_value(root: &Path, key: &str) -> Result<Option<String>> {
    #[cfg(unix)]
    {
        let path = sqlite_path(root);
        TEXT_READER.with(|slot| {
            let mut cached = slot.borrow_mut();
            let identity = text_reader_identity(&path).ok();
            if identity.is_none()
                || cached.as_ref().map(|reader| &reader.identity) != identity.as_ref()
            {
                // Drop the old handle before replacing its only cache slot.
                *cached = None;
                let connection = open_sqlite(root)?;
                let identity = text_reader_identity(&path)?;
                *cached = Some(CachedTextReader {
                    identity,
                    connection,
                });
                #[cfg(test)]
                TEXT_READER_OPENS.with(|count| count.set(count.get() + 1));
            }
            let result = read_text_value(
                &cached.as_ref().expect("initialized text reader").connection,
                key,
            );
            if result.is_err() {
                *cached = None;
            }
            result
        })
    }
    #[cfg(not(unix))]
    {
        // Retain the existing behavior until a stable file-identity mechanism
        // for replacement detection is certified on this platform.
        read_text_value(&open_sqlite(root)?, key)
    }
}

pub fn store_text_value(root: &Path, key: &str, value: Option<&str>) -> Result<()> {
    let conn = open_sqlite(root)?;
    match value {
        Some(value) => {
            conn.execute(
                &format!(
                    "INSERT INTO {KV_TABLE} (kv_key, kv_value, updated_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(kv_key) DO UPDATE SET
                       kv_value = excluded.kv_value,
                       updated_at = excluded.updated_at"
                ),
                params![key, value, now_epoch_secs()],
            )
            .with_context(|| format!("failed to persist kv value {key}"))?;
        }
        None => {
            conn.execute(
                &format!("DELETE FROM {KV_TABLE} WHERE kv_key = ?1"),
                params![key],
            )
            .with_context(|| format!("failed to delete kv value {key}"))?;
        }
    }
    Ok(())
}

pub fn try_acquire_lease(root: &Path, key: &str, value: &str) -> Result<bool> {
    let conn = open_sqlite(root)?;
    let inserted = conn
        .execute(
            &format!(
                "INSERT OR IGNORE INTO {LEASE_TABLE} (lease_key, lease_value, updated_at)
                 VALUES (?1, ?2, ?3)"
            ),
            params![key, value, now_epoch_secs()],
        )
        .with_context(|| format!("failed to acquire lease {key}"))?;
    Ok(inserted > 0)
}

pub fn load_lease_value(root: &Path, key: &str) -> Result<Option<String>> {
    let conn = open_sqlite(root)?;
    conn.query_row(
        &format!("SELECT lease_value FROM {LEASE_TABLE} WHERE lease_key = ?1"),
        params![key],
        |row| row.get(0),
    )
    .optional()
    .with_context(|| format!("failed to load lease {key}"))
}

pub fn release_lease(root: &Path, key: &str) -> Result<()> {
    let conn = open_sqlite(root)?;
    conn.execute(
        &format!("DELETE FROM {LEASE_TABLE} WHERE lease_key = ?1"),
        params![key],
    )
    .with_context(|| format!("failed to release lease {key}"))?;
    Ok(())
}

/// Set of database paths whose schema has already been initialized in this
/// process. Subsequent opens skip the writer-locked `PRAGMA journal_mode=WAL`
/// and `CREATE TABLE IF NOT EXISTS` batch, which is the main source of
/// `busy_timeout`-bounded stalls (up to 30s) when another process — typically
/// the long-running `ctox` daemon — holds the writer.
fn initialized_paths() -> &'static Mutex<HashSet<PathBuf>> {
    static INITIALIZED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    INITIALIZED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn open_sqlite(root: &Path) -> Result<Connection> {
    let path = sqlite_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create sqlite dir {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open sqlite db {}", path.display()))?;
    conn.busy_timeout(sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout")?;

    let needs_init = {
        let guard = initialized_paths()
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        !guard.contains(&path)
    };
    if needs_init {
        let busy_timeout_ms = sqlite_busy_timeout_millis();
        conn.execute_batch(&format!(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout={busy_timeout_ms};
             CREATE TABLE IF NOT EXISTS {PAYLOAD_TABLE} (
                 payload_key TEXT PRIMARY KEY,
                 payload_json TEXT NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS {KV_TABLE} (
                 kv_key TEXT PRIMARY KEY,
                 kv_value TEXT NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS {LEASE_TABLE} (
                 lease_key TEXT PRIMARY KEY,
                 lease_value TEXT NOT NULL,
                 updated_at INTEGER NOT NULL
             );"
        ))
        .context("failed to initialize sqlite persistence schema")?;
        let mut guard = initialized_paths()
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        guard.insert(path);
    }
    Ok(conn)
}

#[cfg(all(test, unix))]
#[path = "persistence_tests.rs"]
mod tests;

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod sqlite_build_flags_tests {
    /// Guards `.cargo/config.toml` `LIBSQLITE3_FLAGS`: the bundled SQLite must
    /// not share one page cache and one allocation mutex across all daemon
    /// connections (see the comment there; customer on-prem, 06.10.2026).
    #[test]
    fn bundled_sqlite_has_no_global_page_cache_or_memstatus_mutex() {
        let conn = rusqlite::Connection::open_in_memory().expect("sqlite");
        let used = |option: &str| -> i64 {
            conn.query_row("SELECT sqlite_compileoption_used(?1)", [option], |row| {
                row.get(0)
            })
            .expect("compileoption")
        };
        assert_eq!(used("ENABLE_MEMORY_MANAGEMENT"), 0);
        assert_eq!(used("DEFAULT_MEMSTATUS=0"), 1);
        assert_eq!(used("THREADSAFE=1"), 1);
    }
}

#[cfg(test)]
mod owner_adapter_tests {
    use super::*;
    use ctox_store::{registry::Registry, writer::WriterOptions, DbId, JobOptions, StoreError};

    async fn adapter(path: &Path) -> Result<CorePersistence> {
        let writer = Registry.open(
            DbId::new(path)?,
            WriterOptions {
                configure: Some(std::sync::Arc::new(|conn| {
                    ctox_store::config::ConnectionConfig::default().apply_writer(conn)?;
                    Ok(())
                })),
                ..Default::default()
            },
        )?;
        writer.ready().await?;
        let readers = ctox_store::read_pool::ReadPool::for_database(
            writer.db_id(),
            writer.generation(),
            Default::default(),
            Default::default(),
        )
        .await?;
        CorePersistence::attach(writer, readers).await
    }

    #[tokio::test]
    async fn payload_commit_import_delete_and_reopen_use_owner() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("core.sqlite3");
        let core = adapter(&path).await?;
        core.store_json_payload("payload", Some(&vec![1, 2]))
            .await?;
        assert_eq!(
            core.load_json_payload::<Vec<i32>>("payload").await?,
            Some(vec![1, 2])
        );
        assert_eq!(
            core.load_or_insert_json_payload("payload", &vec![9])
                .await?,
            vec![1, 2]
        );
        assert_eq!(
            core.load_or_insert_json_payload("empty", &Vec::<i32>::new())
                .await?,
            Vec::<i32>::new()
        );
        assert_eq!(
            core.load_or_insert_json_payload("empty", &vec![9]).await?,
            Vec::<i32>::new()
        );
        core.store_json_payload::<Vec<i32>>("payload", None).await?;
        assert_eq!(core.load_json_payload::<Vec<i32>>("payload").await?, None);
        core.shutdown().await?;
        assert!(core.store_json_payload("closed", Some(&1)).await.is_err());
        let reopened = adapter(&path).await?;
        assert_eq!(
            reopened.load_json_payload::<Vec<i32>>("empty").await?,
            Some(vec![])
        );
        reopened.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_lease_admission_has_one_winner() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let core = adapter(&dir.path().join("core.sqlite3")).await?;
        let mut jobs = Vec::new();
        for index in 0..16 {
            let core = core.clone();
            jobs.push(tokio::spawn(async move {
                let value = index.to_string();
                Ok::<_, anyhow::Error>((core.try_acquire_lease("lease", &value).await?, value))
            }));
        }
        let mut winners = Vec::new();
        for job in jobs {
            let (won, value) = job.await??;
            if won {
                winners.push(value);
            }
        }
        assert_eq!(winners.len(), 1);
        assert_eq!(
            core.load_lease_value("lease").await?,
            Some(winners.remove(0))
        );
        core.release_lease("lease").await?;
        assert_eq!(core.load_lease_value("lease").await?, None);
        assert!(core.try_acquire_lease("lease", "next").await?);
        core.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn attaching_another_owners_readers_fails_without_cross_store_access() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let first = adapter(&dir.path().join("first.sqlite3")).await?;
        let second = adapter(&dir.path().join("second.sqlite3")).await?;
        second
            .store_json_payload("identity", Some(&"second"))
            .await?;
        let mixed = CorePersistence::attach(first.writer.clone(), second.readers.clone()).await;
        assert!(mixed.is_err());
        assert_eq!(first.load_json_payload::<String>("identity").await?, None);
        assert_eq!(
            second
                .load_json_payload::<String>("identity")
                .await?
                .as_deref(),
            Some("second")
        );
        first.shutdown().await?;
        second.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn owner_failure_and_cancelled_admission_leave_no_partial_payload() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let core = adapter(&dir.path().join("core.sqlite3")).await?;
        let result: std::result::Result<(), StoreError> = write_transaction::owner_write(
            &core.writer,
            "fixture.rollback",
            Default::default(),
            |tx| {
                tx.execute("INSERT INTO ctox_payload_store VALUES('partial','1',0)", [])?;
                Err(StoreError::Invalid("fixture rejection".into()))
            },
        )
        .await;
        assert!(matches!(result, Err(StoreError::Invalid(_))));
        assert_eq!(core.load_json_payload::<i32>("partial").await?, None);
        let options = JobOptions::default();
        options.cancellation.cancel();
        let cancelled =
            write_transaction::owner_write(&core.writer, "fixture.cancel", options, |_| Ok(()))
                .await;
        assert_eq!(cancelled, Err(StoreError::Cancelled));
        core.store_json_payload("after", Some(&3)).await?;
        assert_eq!(core.load_json_payload::<i32>("after").await?, Some(3));
        core.shutdown().await?;
        Ok(())
    }
}
