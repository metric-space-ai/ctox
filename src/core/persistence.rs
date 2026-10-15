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
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_SQLITE_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";

/// First Core owner adapter (P3-17). The daemon supplies its existing writer;
/// CLI uses P3-08 IPC. Synchronous callers below remain migration inventory.
#[derive(Clone)]
pub(crate) struct CorePersistence {
    writer: ctox_store::writer::WriterHandle,
    readers: ctox_store::read_pool::ReadPool,
    runtime: tokio::runtime::Handle,
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
        let reader_file: String = readers
            .read_job(ctox_store::ReadJob::new(Default::default(), |conn| {
                Ok(conn.query_row(
                    "SELECT file FROM pragma_database_list WHERE name='main'",
                    [],
                    |row| row.get(0),
                )?)
            }))
            .await?;
        anyhow::ensure!(
            std::path::Path::new(&reader_file) == writer.db_id().path(),
            "Core reader pool belongs to another database"
        );
        write_transaction::owner_write(&writer, "core.schema", Default::default(), |tx| {
            tx.execute_batch("CREATE TABLE IF NOT EXISTS ctox_payload_store (payload_key TEXT PRIMARY KEY, payload_json TEXT NOT NULL, updated_at INTEGER NOT NULL);
                CREATE TABLE IF NOT EXISTS ctox_kv_store (kv_key TEXT PRIMARY KEY, kv_value TEXT NOT NULL, updated_at INTEGER NOT NULL);
                CREATE TABLE IF NOT EXISTS ctox_lease_store (lease_key TEXT PRIMARY KEY, lease_value TEXT NOT NULL, updated_at INTEGER NOT NULL);")?;
            Ok(())
        }).await?;
        Ok(Self {
            writer,
            readers,
            runtime: tokio::runtime::Handle::try_current()?,
        })
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

    pub(crate) async fn load_text_value(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_owned();
        Ok(self
            .readers
            .read_job(ctox_store::ReadJob::new(Default::default(), move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT kv_value FROM ctox_kv_store WHERE kv_key=?1",
                        [key],
                        |r| r.get(0),
                    )
                    .optional()?)
            }))
            .await?)
    }

    pub(crate) async fn store_text_value(&self, key: &str, value: Option<&str>) -> Result<()> {
        let key = key.to_owned();
        let value = value.map(str::to_owned);
        write_transaction::owner_write(&self.writer,"kv.store",Default::default(),move |tx| {
            match value {
                Some(value)=>{tx.execute("INSERT INTO ctox_kv_store VALUES(?1,?2,?3) ON CONFLICT(kv_key) DO UPDATE SET kv_value=excluded.kv_value,updated_at=excluded.updated_at",params![key,value,now_epoch_secs()])?;}
                None=>{tx.execute("DELETE FROM ctox_kv_store WHERE kv_key=?1",[key])?;}
            }
            Ok(())
        }).await?;
        Ok(())
    }

    /// The same Core writer owns transfer metadata; no new actor or reader pool.
    /// FULL must be selected by composition before this writer starts any tx.
    pub(crate) async fn transfer_store(&self) -> Result<ctox_transfers::OwnedTransferStore> {
        write_transaction::owner_write(&self.writer, "transfer.schema", Default::default(), |tx| {
            let sync: i64 = tx.pragma_query_value(None, "synchronous", |r| r.get(0))?;
            if sync != 2 {
                return Err(ctox_store::StoreError::Invalid(
                    "Core transfer owner requires FULL before BEGIN".into(),
                ));
            }
            tx.execute_batch(ctox_transfers::TRANSFER_SCHEMA)?;
            Ok(())
        })
        .await?;
        Ok(ctox_transfers::OwnedTransferStore::from_owner(
            std::sync::Arc::new(CoreTransferOwner(self.writer.clone())),
        ))
    }

    fn read_sync<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Connection) -> std::result::Result<T, ctox_store::StoreError>
            + Send
            + 'static,
    ) -> Result<T> {
        let _admission = sync_admission()?;
        let readers = self.readers.clone();
        let (reply, result) = tokio::sync::oneshot::channel();
        self.runtime.spawn(async move {
            let value = readers
                .read_job(ctox_store::ReadJob::new(Default::default(), operation))
                .await;
            let _ = reply.send(value);
        });
        Ok(result
            .blocking_recv()
            .context("Core owner runtime closed")??)
    }

    fn write_sync<T: Send + 'static>(
        &self,
        name: &'static str,
        operation: impl FnOnce(&rusqlite::Transaction<'_>) -> std::result::Result<T, ctox_store::StoreError>
            + Send
            + 'static,
    ) -> Result<T> {
        let _admission = sync_admission()?;
        Ok(write_transaction::owner_write_blocking(
            &self.writer,
            name,
            Default::default(),
            operation,
        )?)
    }

    pub(crate) async fn shutdown(&self) -> Result<()> {
        self.writer.stop_admission();
        self.readers.shutdown().await?;
        self.writer.shutdown().await?;
        Ok(())
    }
}

struct CoreTransferOwner(ctox_store::writer::WriterHandle);
impl ctox_transfers::TransferOwner for CoreTransferOwner {
    fn submit(&self, job: ctox_transfers::TransferOwnerJob) -> ctox_transfers::TransferOwnerReply {
        let writer = self.0.clone();
        Box::pin(async move {
            Ok(write_transaction::owner_write(
                &writer,
                "transfer.command",
                Default::default(),
                move |tx| {
                    job.apply(tx)
                        .map_err(|error| ctox_store::StoreError::Invalid(error.to_string()))
                },
            )
            .await?)
        })
    }
    fn worker(
        &self,
        job: ctox_transfers::TransferWorkerJob,
    ) -> ctox_transfers::TransferWorkerReply {
        let writer = self.0.clone();
        Box::pin(async move {
            Ok(write_transaction::owner_write(
                &writer,
                "transfer.worker",
                Default::default(),
                move |tx| {
                    job.apply(tx)
                        .map_err(|error| ctox_store::StoreError::Invalid(error.to_string()))
                },
            )
            .await?)
        })
    }
}

thread_local! { static CORE_SYNC_CONTEXT:std::cell::Cell<bool>=const {std::cell::Cell::new(false)}; }
fn sync_slots() -> std::sync::Arc<tokio::sync::Semaphore> {
    static SLOTS: OnceLock<std::sync::Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SLOTS
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(4)))
        .clone()
}
fn sync_admission() -> Result<()> {
    // A private admission token is set only on the bounded compatibility
    // executor. Owner callbacks and async workers can never wait recursively.
    anyhow::ensure!(
        CORE_SYNC_CONTEXT.with(std::cell::Cell::get),
        "synchronous Core caller must use run_core_sync or an async owner API"
    );
    Ok(())
}

/// Transitional sync callers await this bounded bridge. It reuses the existing
/// runtime, caps waiting compatibility work at four, and never executes SQLite
/// itself. Do not call a sync facade directly from an async worker.
pub(crate) async fn run_core_sync<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let admission = sync_slots()
        .try_acquire_owned()
        .map_err(|_| ctox_store::StoreError::Busy)?;
    tokio::task::spawn_blocking(move || {
        let _admission = admission;
        struct Reset(bool);
        impl Drop for Reset {
            fn drop(&mut self) {
                CORE_SYNC_CONTEXT.with(|value| value.set(self.0));
            }
        }
        let _reset = Reset(CORE_SYNC_CONTEXT.with(|value| value.replace(true)));
        operation()
    })
    .await
    .context("Core compatibility worker failed")?
}

type InstalledCoreOwners =
    std::collections::HashMap<ctox_store::DbId, std::sync::Weak<CorePersistence>>;
fn installed_core_owners() -> &'static Mutex<InstalledCoreOwners> {
    static OWNERS: OnceLock<Mutex<InstalledCoreOwners>> = OnceLock::new();
    OWNERS.get_or_init(Default::default)
}

/// Single composition interface for CALLERS-1. Keep the returned Arc for the
/// daemon lifetime. Lookup never opens a file, creates a pool, or invents an owner.
pub(crate) async fn install_core_owner(
    root: &Path,
    writer: ctox_store::writer::WriterHandle,
    readers: ctox_store::read_pool::ReadPool,
) -> Result<std::sync::Arc<CorePersistence>> {
    let expected = ctox_store::DbId::new(sqlite_path(root))?;
    anyhow::ensure!(
        writer.db_id() == &expected,
        "injected Core writer path differs"
    );
    let core = std::sync::Arc::new(CorePersistence::attach(writer, readers).await?);
    let mut owners = installed_core_owners()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    owners.retain(|_, owner| owner.strong_count() > 0);
    if let Some(existing) = owners.get(&expected).and_then(std::sync::Weak::upgrade) {
        anyhow::ensure!(
            existing.writer.generation() == core.writer.generation(),
            "Core owner generation differs"
        );
        return Ok(existing);
    }
    owners.insert(expected, std::sync::Arc::downgrade(&core));
    Ok(core)
}

fn injected_core(root: &Path) -> Result<std::sync::Arc<CorePersistence>> {
    let id = ctox_store::DbId::new(sqlite_path(root))?;
    installed_core_owners()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .and_then(std::sync::Weak::upgrade)
        .context("Core owner not installed; composition must inject its existing handles")
}

pub(crate) async fn load_json_payload_owned<T: DeserializeOwned>(
    root: &Path,
    key: &str,
) -> Result<Option<T>> {
    injected_core(root)?.load_json_payload(key).await
}
pub(crate) async fn store_json_payload_owned<T: Serialize>(
    root: &Path,
    key: &str,
    payload: Option<&T>,
) -> Result<()> {
    injected_core(root)?.store_json_payload(key, payload).await
}
pub(crate) async fn load_or_insert_json_payload_owned<T: Serialize + DeserializeOwned>(
    root: &Path,
    key: &str,
    initialize: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let core = injected_core(root)?;
    if let Some(value) = core.load_json_payload(key).await? {
        return Ok(value);
    }
    let candidate = initialize()?;
    core.load_or_insert_json_payload(key, &candidate).await
}
pub(crate) async fn load_text_value_owned(root: &Path, key: &str) -> Result<Option<String>> {
    injected_core(root)?.load_text_value(key).await
}
pub(crate) async fn store_text_value_owned(
    root: &Path,
    key: &str,
    value: Option<&str>,
) -> Result<()> {
    injected_core(root)?.store_text_value(key, value).await
}
pub(crate) async fn try_acquire_lease_owned(root: &Path, key: &str, value: &str) -> Result<bool> {
    injected_core(root)?.try_acquire_lease(key, value).await
}
pub(crate) async fn load_lease_value_owned(root: &Path, key: &str) -> Result<Option<String>> {
    injected_core(root)?.load_lease_value(key).await
}
pub(crate) async fn release_lease_owned(root: &Path, key: &str) -> Result<()> {
    injected_core(root)?.release_lease(key).await
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

/// Legacy signatures are retained only as injected-owner facades. They never
/// open a database. Async callers use the owned APIs or run_core_sync.
pub fn load_json_payload<T: DeserializeOwned>(root: &Path, key: &str) -> Result<Option<T>> {
    let key = key.to_owned();
    let label = key.clone();
    let raw: Option<String> = injected_core(root)?.read_sync(move |conn| {
        Ok(conn
            .query_row(
                "SELECT payload_json FROM ctox_payload_store WHERE payload_key=?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    })?;
    raw.map(|raw| {
        serde_json::from_str(&raw).with_context(|| format!("failed to decode payload {label}"))
    })
    .transpose()
}
pub fn store_json_payload<T: Serialize>(root: &Path, key: &str, payload: Option<&T>) -> Result<()> {
    let key = key.to_owned();
    let raw = payload.map(serde_json::to_string_pretty).transpose()?;
    injected_core(root)?.write_sync("payload.store",move|tx| {
        match raw {Some(raw)=>{tx.execute("INSERT INTO ctox_payload_store VALUES(?1,?2,?3) ON CONFLICT(payload_key) DO UPDATE SET payload_json=excluded.payload_json,updated_at=excluded.updated_at",params![key,raw,now_epoch_secs()])?;},None=>{tx.execute("DELETE FROM ctox_payload_store WHERE payload_key=?1",[key])?;}}
        Ok(())
    })
}
pub(crate) fn load_or_insert_json_payload<T: DeserializeOwned + Serialize>(
    root: &Path,
    key: &str,
    initialize: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if let Some(value) = load_json_payload(root, key)? {
        return Ok(value);
    }
    // Preparation may do file I/O and stays outside the owner's transaction.
    // Concurrent canonical rows always win, including an explicitly empty row.
    let candidate = serde_json::to_string_pretty(&initialize()?)?;
    let key = key.to_owned();
    let raw: String = injected_core(root)?.write_sync("payload.legacy_import", move |tx| {
        tx.execute(
            "INSERT OR IGNORE INTO ctox_payload_store VALUES(?1,?2,?3)",
            params![key, candidate, now_epoch_secs()],
        )?;
        Ok(tx.query_row(
            "SELECT payload_json FROM ctox_payload_store WHERE payload_key=?1",
            [key],
            |row| row.get(0),
        )?)
    })?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn load_text_value(root: &Path, key: &str) -> Result<Option<String>> {
    let key = key.to_owned();
    injected_core(root)?.read_sync(move |conn| {
        Ok(conn
            .query_row(
                "SELECT kv_value FROM ctox_kv_store WHERE kv_key=?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    })
}
pub fn store_text_value(root: &Path, key: &str, value: Option<&str>) -> Result<()> {
    let key = key.to_owned();
    let value = value.map(str::to_owned);
    injected_core(root)?.write_sync("kv.store",move|tx| {
        match value {Some(value)=>{tx.execute("INSERT INTO ctox_kv_store VALUES(?1,?2,?3) ON CONFLICT(kv_key) DO UPDATE SET kv_value=excluded.kv_value,updated_at=excluded.updated_at",params![key,value,now_epoch_secs()])?;},None=>{tx.execute("DELETE FROM ctox_kv_store WHERE kv_key=?1",[key])?;}}
        Ok(())
    })
}
pub fn try_acquire_lease(root: &Path, key: &str, value: &str) -> Result<bool> {
    let key = key.to_owned();
    let value = value.to_owned();
    injected_core(root)?.write_sync("lease.acquire", move |tx| {
        Ok(tx.execute(
            "INSERT OR IGNORE INTO ctox_lease_store VALUES(?1,?2,?3)",
            params![key, value, now_epoch_secs()],
        )? > 0)
    })
}
pub fn load_lease_value(root: &Path, key: &str) -> Result<Option<String>> {
    let key = key.to_owned();
    injected_core(root)?.read_sync(move |conn| {
        Ok(conn
            .query_row(
                "SELECT lease_value FROM ctox_lease_store WHERE lease_key=?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    })
}
/// Best-effort RAII release: admission is nonblocking and the owner drains an
/// admitted deletion. Drop never waits or claims a commit acknowledgement.
pub(crate) fn release_lease_on_drop(root: &Path, key: &str) -> Result<()> {
    let key = key.to_owned();
    let owner = injected_core(root)?;
    let handle = owner
        .writer
        .submit(ctox_store::WriteJob::new(Default::default(), move |tx| {
            tx.execute("DELETE FROM ctox_lease_store WHERE lease_key=?1", [key])?;
            Ok(())
        }))?;
    drop(handle);
    Ok(())
}

pub fn release_lease(root: &Path, key: &str) -> Result<()> {
    let key = key.to_owned();
    injected_core(root)?.write_sync("lease.release", move |tx| {
        tx.execute("DELETE FROM ctox_lease_store WHERE lease_key=?1", [key])?;
        Ok(())
    })
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
    use ctox_store::{
        registry::Registry, testkit::StoreFixture, writer::WriterOptions, DbId, JobOptions, StoreError,
    };

    async fn adapter(path: &Path) -> Result<CorePersistence> {
        let (writer, readers) = StoreFixture::open(DbId::new(path)?).await?.into_parts();
        CorePersistence::attach(writer, readers).await
    }

    #[tokio::test]
    async fn injected_core_payload_lease_and_transfer_worker_share_the_existing_writer(
    ) -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = sqlite_path(root.path());
        anyhow::ensure!(
            path.starts_with(root.path()),
            "isolated fixture state root required"
        );
        assert!(load_json_payload_owned::<i32>(root.path(), "missing")
            .await
            .is_err());
        assert!(
            !path.exists(),
            "missing injection must never lazily create a Core store"
        );
        std::fs::create_dir_all(path.parent().unwrap())?;
        let writer = Registry.open(
            DbId::new(&path)?,
            WriterOptions {
                configure: Some(std::sync::Arc::new(|conn| {
                    conn.pragma_update(None, "synchronous", "FULL")?;
                    conn.execute_batch("CREATE TEMP TABLE owner_audit(state TEXT)")?;
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
        let core = install_core_owner(root.path(), writer.clone(), readers).await?;
        store_json_payload_owned(root.path(), "payload", Some(&vec![1, 2])).await?;
        assert_eq!(
            load_json_payload_owned::<Vec<i32>>(root.path(), "payload").await?,
            Some(vec![1, 2])
        );
        assert_eq!(
            load_or_insert_json_payload_owned::<Vec<i32>>(root.path(), "payload", || panic!(
                "canonical row must bypass initializer"
            ))
            .await?,
            vec![1, 2]
        );
        store_text_value_owned(root.path(), "text", Some("before")).await?;
        assert_eq!(
            load_text_value_owned(root.path(), "text").await?.as_deref(),
            Some("before")
        );
        store_text_value_owned(root.path(), "text", None).await?;
        assert!(load_text_value_owned(root.path(), "text").await?.is_none());
        assert!(try_acquire_lease_owned(root.path(), "lease", "one").await?);
        assert!(!try_acquire_lease_owned(root.path(), "lease", "two").await?);
        assert_eq!(
            load_lease_value_owned(root.path(), "lease")
                .await?
                .as_deref(),
            Some("one")
        );
        release_lease_owned(root.path(), "lease").await?;
        assert!(load_lease_value_owned(root.path(), "lease")
            .await?
            .is_none());
        assert!(store_text_value(root.path(), "must-not-block", Some("bad")).is_err());
        let sync_root = root.path().to_path_buf();
        run_core_sync(move || {
            store_json_payload(&sync_root, "sync-payload", Some(&vec![3, 4]))?;
            assert_eq!(
                load_json_payload::<Vec<i32>>(&sync_root, "sync-payload")?,
                Some(vec![3, 4])
            );
            assert_eq!(
                load_or_insert_json_payload::<Vec<i32>>(&sync_root, "sync-payload", || panic!(
                    "canonical initializer"
                ))?,
                vec![3, 4]
            );
            store_text_value(&sync_root, "sync-text", Some("committed"))?;
            assert_eq!(
                load_text_value(&sync_root, "sync-text")?.as_deref(),
                Some("committed")
            );
            assert!(try_acquire_lease(&sync_root, "sync-lease", "one")?);
            assert!(!try_acquire_lease(&sync_root, "sync-lease", "two")?);
            assert_eq!(
                load_lease_value(&sync_root, "sync-lease")?.as_deref(),
                Some("one")
            );
            release_lease(&sync_root, "sync-lease")?;
            assert!(load_lease_value(&sync_root, "sync-lease")?.is_none());
            Ok(())
        })
        .await?;
        assert!(try_acquire_lease_owned(root.path(), "drop-lease", "holder").await?);
        release_lease_on_drop(root.path(), "drop-lease")?;
        write_transaction::owner_write(
            &writer,
            "fixture.release.barrier",
            Default::default(),
            |_| Ok(()),
        )
        .await?;
        assert!(load_lease_value_owned(root.path(), "drop-lease")
            .await?
            .is_none());
        let transfers = core.transfer_store().await?;
        write_transaction::owner_write(&writer,"fixture.audit",Default::default(),|tx| {
            tx.execute_batch("CREATE TEMP TRIGGER audit_owned_transfer AFTER UPDATE ON main.ctox_transfer_jobs BEGIN INSERT INTO owner_audit VALUES(new.state); END;")?; Ok(())
        }).await?;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let artifacts = root.path().join("artifacts");
        let worker = transfers
            .worker_with_sources(artifacts.clone(), None, None, stop.clone())
            .await?;
        let request = ctox_transfers::DownloadRequest {
            id: "owned-cache-recovery".into(),
            sources: vec!["http://127.0.0.1/unused".into()],
            peer_source: None,
            storage: None,
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
            size: 0,
        };
        std::fs::write(artifacts.join("objects").join(&request.sha256), [])?;
        transfers.enqueue(request.clone()).await?;
        assert!(worker.run_next(&stop).await?);
        let saved = transfers.get(request.id).await?;
        assert_eq!(saved.state, "completed");
        assert!(saved.receipt.is_some());
        let states: Vec<String> = write_transaction::owner_write(
            &writer,
            "fixture.audit.read",
            Default::default(),
            |tx| {
                let mut statement = tx.prepare("SELECT state FROM owner_audit")?;
                let rows = statement.query_map([], |row| row.get(0))?;
                let values = rows.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(values)
            },
        )
        .await?;
        assert!(
            states.iter().any(|state| state == "completed"),
            "receipt must pass the original owner's connection-local trigger"
        );
        assert!(!worker.run_next(&stop).await?);
        stop.store(true, std::sync::atomic::Ordering::Release);
        transfers
            .enqueue(ctox_transfers::DownloadRequest {
                id: "stopped-owner".into(),
                sources: vec!["http://127.0.0.1/unused".into()],
                peer_source: None,
                storage: None,
                sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
                size: 0,
            })
            .await?;
        let unrelated_stop = std::sync::atomic::AtomicBool::new(false);
        assert!(
            !worker.run_next(&unrelated_stop).await?,
            "the injected owner stop flag must govern the whole attempt"
        );
        assert_eq!(transfers.get("stopped-owner".into()).await?.state, "queued");
        assert!(matches!(
            Registry.open(DbId::new(&path)?, WriterOptions::default()),
            Err(StoreError::AlreadyOwned)
        ));
        drop(worker);
        drop(transfers);
        core.shutdown().await?;
        drop(core);
        assert!(load_json_payload_owned::<i32>(root.path(), "missing")
            .await
            .is_err());
        Ok(())
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
