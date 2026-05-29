// Origin: CTOX
// License: Apache-2.0

use super::browser_runtime::browser_runtime_manager;
use super::store;
use crate::mission::channels;
use crate::mission::tickets;
use anyhow::Context;
use base64::Engine;
use rxdb::plugins::replication_webrtc::{
    RTCIceServer, RxWebRTCReplicationPool, WebRTCRsConnectionHandler,
};
use rxdb::rx_collection::RxCollection;
use rxdb::rx_collection_helper::fill_object_data_before_insert;
use rxdb::rx_database::{create_rx_database, RxCollectionCreator, RxDatabase, RxDatabaseCreator};
use rxdb::storage::sqlite::{get_rx_storage_sqlite, RxStorageSqliteSettings};
use rxdb::types::{BulkWriteRow, HashOutput, MangoQuery, RxJsonSchema};
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::sync::Mutex as AsyncMutex;
use url::Url;
use uuid::Uuid;

static NATIVE_PEER_STARTED: AtomicBool = AtomicBool::new(false);
static NATIVE_PEER_RUNNING: AtomicBool = AtomicBool::new(false);
static NATIVE_PEER: Mutex<Option<Arc<NativePeer>>> = Mutex::new(None);
static TEMPORARY_RXDB_DATABASE_LOCK: Mutex<()> = Mutex::new(());
static NATIVE_RXDB_WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static BROWSER_RUNTIME_COMMAND_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const SIGNALING_TOKEN_TTL_SECONDS: u64 = 24 * 60 * 60;
/// Phase 3: single-session signaling/replication bring-up timeout. One room is
/// joined once for the whole sync room; if it cannot come up in this window we
/// log and continue (collections stay locally queryable).
const NATIVE_COLLECTION_BRINGUP_TIMEOUT_SECS: u64 = 20;
const CTOX_RXDB_PROTOCOL: &str = "ctox-rxdb-protocol-v1";
const CTOX_NATIVE_CAPABILITIES: &[&str] = &[
    "ctox-control-plane-v1",
    "ctox-rxdb-native-v1",
    "ctox-file-chunks-v1",
    "ctox-schema-hash-v1",
    "ctox-peer-session-v1",
    "ctox-checkpoint-epoch-v1",
];
const DESKTOP_FILE_CHUNK_SIZE: usize = 16 * 1024;
const DESKTOP_FILE_EAGER_LIMIT_BYTES: u64 = 1024 * 1024;
const DESKTOP_FILE_SCAN_INTERVAL_SECS: u64 = 15;
const DESKTOP_FILE_SCAN_MAX_DEPTH: usize = 6;
const DESKTOP_FILE_SCAN_MAX_FILES: usize = 200;
const DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS: usize = 2;
const DESKTOP_FILE_CHUNK_CLEANUP_SCAN_LIMIT: u64 = 100_000;
const DESKTOP_FILE_CONTENT_HASH_SCHEME: &str = "sha256-bytes-v1";
const DESKTOP_FILE_CHUNK_HASH_SCHEME: &str = "sha256-base64-chunk-v1";
const CTOX_DESKTOP_FOLDER_ID: &str = "fs_ctox";
const CTOX_DESKTOP_FOLDER_PATH: &str = "/CTOX";
const CHANNEL_STATE_SYNC_INTERVAL_SECS: u64 = 3;
const BUSINESS_USERS_SYNC_INTERVAL_SECS: u64 = 3;
const RUNTIME_SETTINGS_SYNC_INTERVAL_SECS: u64 = 3;
const MODULE_CATALOG_SYNC_INTERVAL_SECS: u64 = 3;
const TICKET_STATE_SYNC_INTERVAL_SECS: u64 = 3;
const BUSINESS_RECORD_PROJECTION_SYNC_INTERVAL_SECS: u64 = 3;
const BUSINESS_RECORD_PROJECTION_SYNC_LIMIT: usize = 2_000;
const TICKET_STATE_SYNC_LIMIT: usize = 500;
const BUSINESS_OS_CHANNEL_IDS: &[&str] = &["whatsapp", "jami", "teams", "email", "meeting"];
const NATIVE_PEER_STATUS_VERSION: &str = "ctox-native-rxdb-peer-status-v1";
const NATIVE_PEER_HEARTBEAT_INTERVAL_SECS: u64 = 5;
const NATIVE_PEER_HEARTBEAT_TTL_MS: u64 = 30_000;
/// FIX 2: the peer's runtime must not be single-threaded on a small VPS. With
/// 1-2 worker threads the per-collection pollers + blocking work starved the
/// heartbeat and replication. We floor the worker count at 4 (and scale up
/// with available cores) so timers, replication, and blocking offload have
/// room to make progress concurrently.
const NATIVE_PEER_MIN_WORKER_THREADS: usize = 4;
/// FIX 2: how often the lock/heartbeat watchdog wakes inside `run_native_peer`
/// to confirm the peer's own status heartbeat is still being written. If the
/// dedicated heartbeat thread has died/stalled, the watchdog shuts the peer
/// down cleanly so the OS process lock is released for a fresh start.
const NATIVE_PEER_WATCHDOG_INTERVAL_SECS: u64 = 15;
/// FIX 2: maximum tolerated heartbeat staleness before the watchdog considers
/// its own liveness machinery wedged. Generously above the write interval and
/// the published TTL so a healthy peer never trips it.
const NATIVE_PEER_WATCHDOG_MAX_HEARTBEAT_AGE_MS: u64 = 90_000;

/// FIX 2: worker-thread count for the peer's tokio runtime: `max(4, cores)`.
fn native_peer_worker_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(NATIVE_PEER_MIN_WORKER_THREADS)
        .max(NATIVE_PEER_MIN_WORKER_THREADS)
}

type WebRtcPool = Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>;

struct NativePeer {
    database: Arc<RxDatabase>,
    peer_session_id: String,
    shutdown_tx: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    _process_lock: File,
    _pools: Vec<WebRtcPool>,
    _command_consumer: tokio::task::JoinHandle<()>,
    _notes_sync: tokio::task::JoinHandle<()>,
    _file_index_sync: tokio::task::JoinHandle<()>,
    _channel_state_sync: tokio::task::JoinHandle<()>,
    _business_users_sync: tokio::task::JoinHandle<()>,
    _runtime_settings_sync: tokio::task::JoinHandle<()>,
    _module_catalog_sync: tokio::task::JoinHandle<()>,
    _ticket_state_sync: tokio::task::JoinHandle<()>,
    _business_record_projection_sync: tokio::task::JoinHandle<()>,
    _browser_runtime_maintenance: tokio::task::JoinHandle<()>,
    // FIX 2: the status heartbeat now runs on a dedicated OS thread (see
    // `StatusHeartbeatHandle`) so its liveness is independent of the tokio
    // runtime. `Mutex` lets `shutdown` take/stop it through `&self`.
    _status_heartbeat: Mutex<Option<StatusHeartbeatHandle>>,
}

impl NativePeer {
    async fn shutdown(&self) {
        for pool in &self._pools {
            pool.cancel().await;
        }
        self._command_consumer.abort();
        self._notes_sync.abort();
        self._file_index_sync.abort();
        self._channel_state_sync.abort();
        self._business_users_sync.abort();
        self._runtime_settings_sync.abort();
        self._module_catalog_sync.abort();
        self._ticket_state_sync.abort();
        self._business_record_projection_sync.abort();
        self._browser_runtime_maintenance.abort();
        // FIX 2: stop the dedicated heartbeat OS thread.
        if let Ok(mut heartbeat) = self._status_heartbeat.lock() {
            if let Some(handle) = heartbeat.as_mut() {
                handle.stop();
            }
            *heartbeat = None;
        }
        // Tear down any live browser processes so stop leaves no zombies.
        for session_id in browser_runtime_manager().active_session_ids() {
            browser_runtime_manager().stop(&session_id).await;
        }
        let _ = self.database.close().await;
    }
}

struct Sha256HashFunction;

impl rxdb::types::HashFunction for Sha256HashFunction {
    fn hash<'a>(&'a self, input: String) -> HashOutput<'a> {
        Box::pin(async move {
            let mut hasher = sha2::Sha256::new();
            hasher.update(input.as_bytes());
            format!("{:x}", hasher.finalize())
        })
    }
}

pub fn is_native_peer_running() -> bool {
    NATIVE_PEER_RUNNING.load(Ordering::SeqCst) || NATIVE_PEER_STARTED.load(Ordering::SeqCst)
}

pub fn is_native_peer_running_for_root(root: &Path) -> bool {
    is_native_peer_running() || native_peer_heartbeat_is_fresh(root)
}

pub fn native_peer_status(root: &Path) -> Value {
    let in_process_started = NATIVE_PEER_STARTED.load(Ordering::SeqCst);
    let in_process_running = NATIVE_PEER_RUNNING.load(Ordering::SeqCst);
    let process_lock_held = native_peer_process_lock_is_held(root);
    let heartbeat = read_native_peer_heartbeat(root);
    let heartbeat_updated_at_ms = heartbeat_updated_at_ms(heartbeat.as_ref());
    let heartbeat_age_ms = heartbeat_updated_at_ms.map(|updated_at_ms| {
        let now = now_ms() as u64;
        now.saturating_sub(updated_at_ms)
    });
    let heartbeat_fresh = heartbeat_age_ms
        .map(|age_ms| age_ms <= NATIVE_PEER_HEARTBEAT_TTL_MS)
        .unwrap_or(false);
    let running = in_process_running || in_process_started || heartbeat_fresh;
    let health_errors = if running {
        Vec::<Value>::new()
    } else if process_lock_held {
        vec![native_peer_health_error(
            "ctox_native_peer_lock_without_fresh_heartbeat",
            "CtoxNativePeerCollectionDegraded",
            "native-peer",
            "recoverable",
            true,
            "Business OS native RxDB peer lock is held, but no fresh native peer heartbeat is visible.",
        )]
    } else {
        vec![native_peer_health_error(
            "ctox_native_peer_not_running",
            "CtoxNativePeerCollectionDegraded",
            "native-peer",
            "recoverable",
            true,
            "Business OS native RxDB peer is not running.",
        )]
    };
    json!({
        "version": NATIVE_PEER_STATUS_VERSION,
        "running": running,
        "in_process_started": in_process_started,
        "in_process_running": in_process_running,
        "process_lock_held": process_lock_held,
        "heartbeat": {
            "fresh": heartbeat_fresh,
            "ttl_ms": NATIVE_PEER_HEARTBEAT_TTL_MS,
            "updated_at_ms": heartbeat_updated_at_ms,
            "age_ms": heartbeat_age_ms,
            "path": native_peer_heartbeat_path(root).display().to_string(),
        },
        "health": {
            "errorTotal": health_errors.len(),
            "errors": health_errors,
        },
        "nativePeerRecovery": {
            "code": "ctox_optional_schema_drift",
            "action": "repair-optional-drift",
            "status": "available",
        },
        "peer_session_id": current_peer()
            .map(|peer| peer.peer_session_id.clone())
            .or_else(|| heartbeat_peer_session_id(heartbeat.as_ref()))
            .unwrap_or_default(),
        "lock_path": native_peer_lock_path(root).display().to_string(),
        "database_path": store::rxdb_store_path(root).display().to_string(),
    })
}

fn native_peer_health_error(
    code: &str,
    name: &str,
    phase: &str,
    severity: &str,
    retryable: bool,
    message: &str,
) -> Value {
    json!({
        "name": name,
        "code": code,
        "phase": phase,
        "severity": severity,
        "retryable": retryable,
        "message": message,
    })
}

pub fn ensure_native_peer(root: &Path) -> anyhow::Result<()> {
    let config = store::sync_config(root)?;
    spawn_native_peer(
        root,
        config.sync_room.clone(),
        config.signaling_urls.clone(),
        config.signaling_room_password.clone(),
    );
    Ok(())
}

pub fn restart_native_peer(root: &Path) -> anyhow::Result<Value> {
    if let Some(peer) = current_peer() {
        if let Ok(mut sender) = peer.shutdown_tx.lock() {
            if let Some(sender) = sender.take() {
                let _ = sender.send(());
            }
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while NATIVE_PEER_STARTED.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        if NATIVE_PEER_STARTED.load(Ordering::SeqCst) {
            anyhow::bail!("native RxDB peer did not stop before restart deadline");
        }
    }
    ensure_native_peer(root)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while current_peer().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(native_peer_status(root))
}

pub fn run_native_peer_foreground(root: &Path) -> anyhow::Result<()> {
    let config = store::sync_config(root)?;
    let root = root.to_path_buf();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(native_peer_worker_threads())
        .thread_name("business-os-rxdb-peer")
        .build()
        .context("failed to create Business OS native RxDB peer runtime")?;
    runtime.block_on(run_native_peer(
        root,
        config.sync_room.clone(),
        config.signaling_urls.clone(),
        config.signaling_room_password.clone(),
    ))
}

pub fn spawn_native_peer(
    root: &Path,
    sync_room: String,
    signaling_urls: Vec<String>,
    signaling_room_password: String,
) {
    if NATIVE_PEER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let root = root.to_path_buf();
    if let Err(err) = std::thread::Builder::new()
        .name("business-os-rxdb-peer".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(native_peer_worker_threads())
                .thread_name("business-os-rxdb-peer")
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    NATIVE_PEER_STARTED.store(false, Ordering::SeqCst);
                    eprintln!("[business-os] native rxdb peer runtime failed: {err:#}");
                    return;
                }
            };
            if let Err(err) = runtime.block_on(run_native_peer(
                root,
                sync_room,
                signaling_urls,
                signaling_room_password,
            )) {
                NATIVE_PEER_RUNNING.store(false, Ordering::SeqCst);
                NATIVE_PEER_STARTED.store(false, Ordering::SeqCst);
                eprintln!("[business-os] native rxdb peer failed: {err:#}");
            }
            NATIVE_PEER_RUNNING.store(false, Ordering::SeqCst);
            NATIVE_PEER_STARTED.store(false, Ordering::SeqCst);
        })
    {
        NATIVE_PEER_STARTED.store(false, Ordering::SeqCst);
        eprintln!("[business-os] native rxdb peer thread failed: {err:#}");
    }
}

pub fn sync_desktop_file_from_path(root: &Path, path: &Path) -> anyhow::Result<()> {
    sync_desktop_file_from_path_with_policy(root, path, None)
}

pub fn materialize_desktop_file_from_path(root: &Path, path: &Path) -> anyhow::Result<()> {
    sync_desktop_file_from_path_with_policy(root, path, Some(DesktopFileContentPolicy::Eager))
}

fn sync_desktop_file_from_path_with_policy(
    root: &Path,
    path: &Path,
    forced_policy: Option<DesktopFileContentPolicy>,
) -> anyhow::Result<()> {
    let path = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize desktop file {}", path.display()))?;
    let metadata = fs::metadata(&path)
        .with_context(|| format!("failed to read desktop file metadata {}", path.display()))?;
    let policy = forced_policy.unwrap_or_else(|| {
        if should_eager_sync_file(&path, &metadata) {
            DesktopFileContentPolicy::Eager
        } else {
            DesktopFileContentPolicy::Lazy
        }
    });
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS desktop file sync runtime")?;
    if let Some(peer) = current_peer() {
        return runtime
            .block_on(async move { peer.upsert_desktop_file_from_path(path, policy).await });
    }
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        upsert_desktop_file_with_policy(&database, path, policy).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(())
    })
}

pub fn sync_desktop_files_from_workspace_root(
    root: &Path,
    workspace_root: &Path,
) -> anyhow::Result<usize> {
    let workspace_root = workspace_root.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize Business OS workspace file root {}",
            workspace_root.display()
        )
    })?;
    if !is_safe_desktop_file_scan_root(&workspace_root) {
        anyhow::bail!(
            "workspace file root is not a safe bounded scan root: {}",
            workspace_root.display()
        );
    }
    let label = desktop_file_scan_root_label(&workspace_root);
    let scan_roots = vec![DesktopFileScanRoot {
        path: workspace_root,
        label,
    }];
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS workspace file sync runtime")?;
    if let Some(peer) = current_peer() {
        return runtime
            .block_on(async move { peer.sync_desktop_files_from_scan_roots(scan_roots).await });
    }
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let indexed = sync_desktop_file_scan_roots_with_database(&database, scan_roots).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(indexed)
    })
}

#[cfg(test)]
fn sync_desktop_file_index(root: &Path) -> anyhow::Result<usize> {
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS desktop file index runtime")?;
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        if let Some(peer) = current_peer() {
            return sync_desktop_file_index_with_database(root, &peer.database).await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let indexed = sync_desktop_file_index_with_database(root, &database).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(indexed)
    })
}

#[cfg(test)]
fn sync_channel_state(root: &Path) -> anyhow::Result<usize> {
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS channel state sync runtime")?;
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        if let Some(peer) = current_peer() {
            return sync_channel_state_with_database(root, &peer.database).await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced = sync_channel_state_with_database(root, &database).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

#[cfg(test)]
fn sync_business_users(root: &Path) -> anyhow::Result<usize> {
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS users sync runtime")?;
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        if let Some(peer) = current_peer() {
            return sync_business_users_with_database(root, &peer.database).await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced = sync_business_users_with_database(root, &database).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

#[cfg(test)]
fn sync_runtime_settings(root: &Path) -> anyhow::Result<()> {
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS runtime settings sync runtime")?;
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        if let Some(peer) = current_peer() {
            return sync_runtime_settings_with_database(root, &peer.database).await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        sync_runtime_settings_with_database(root, &database).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(())
    })
}

#[cfg(test)]
fn sync_module_catalog(root: &Path) -> anyhow::Result<()> {
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS module catalog sync runtime")?;
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        if let Some(peer) = current_peer() {
            return sync_module_catalog_with_database(root, &peer.database).await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        sync_module_catalog_with_database(root, &database).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(())
    })
}

#[cfg(test)]
fn sync_ticket_state(root: &Path) -> anyhow::Result<usize> {
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS ticket state sync runtime")?;
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        if let Some(peer) = current_peer() {
            return sync_ticket_state_with_database(root, &peer.database).await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced = sync_ticket_state_with_database(root, &database).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

#[cfg(test)]
fn sync_business_record_projections(root: &Path) -> anyhow::Result<usize> {
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS business record projection sync runtime")?;
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        if let Some(peer) = current_peer() {
            let mut since_by_collection = HashMap::new();
            return sync_business_record_projections_with_database(
                root,
                &peer.database,
                &mut since_by_collection,
            )
            .await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let mut since_by_collection = HashMap::new();
        let synced = sync_business_record_projections_with_database(
            root,
            &database,
            &mut since_by_collection,
        )
        .await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

pub fn enqueue_business_command_document(root: &Path, document: Value) -> anyhow::Result<Value> {
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS command enqueue runtime")?;
    if let Some(peer) = current_peer() {
        return runtime.block_on(async move {
            enqueue_business_command_document_with_database(&peer.database, document).await
        });
    }
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let stored = enqueue_business_command_document_with_database(&database, document).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(stored)
    })
}

pub fn browser_session_status(root: &Path, session_id: &str) -> anyhow::Result<Value> {
    let session_id = session_id.trim().to_string();
    if session_id.is_empty() {
        anyhow::bail!("session_id is required");
    }
    let database_path = store::rxdb_store_path(root);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS browser session status runtime")?;
    if let Some(peer) = current_peer() {
        return runtime.block_on(async move {
            browser_session_status_with_database(&peer.database, &session_id).await
        });
    }
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let status = browser_session_status_with_database(&database, &session_id).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(status)
    })
}

#[derive(Debug, Clone)]
pub struct BrowserContextCaptureRequest {
    pub session_id: String,
    pub source_id: Option<String>,
    pub requesting_task_id: Option<String>,
    pub enqueue_handoff: bool,
}

pub fn browser_context_capture(
    root: &Path,
    request: BrowserContextCaptureRequest,
) -> anyhow::Result<Value> {
    let session_id = request.session_id.trim().to_string();
    if session_id.is_empty() {
        anyhow::bail!("session_id is required");
    }
    let database_path = store::rxdb_store_path(root);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS browser context capture runtime")?;
    let mut outcome = if let Some(peer) = current_peer() {
        runtime.block_on(async move {
            browser_context_snapshot_with_database(&peer.database, &session_id).await
        })?
    } else {
        let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime.block_on(async move {
            let database = open_database(database_path).await?;
            database
                .add_collections(collection_creators())
                .await
                .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
            let capture = browser_context_snapshot_with_database(&database, &session_id).await?;
            database.close().await.map_err(|err| {
                anyhow::anyhow!("close temporary Business OS RxDB database: {err}")
            })?;
            Ok::<Value, anyhow::Error>(capture)
        })?
    };
    if request.enqueue_handoff {
        let now = now_ms() as u64;
        let command_id = format!("browser_context_handoff_{now}");
        let browser_context = outcome
            .get("browser_context")
            .cloned()
            .unwrap_or(Value::Null);
        let stored = enqueue_business_command_document(
            root,
            json!({
                "id": command_id,
                "command_id": command_id,
                "command_type": "ctox.browser_context.handoff",
                "type": "ctox.browser_context.handoff",
                "status": "pending_sync",
                "payload": {
                    "browser_context": browser_context,
                    "source_id": request.source_id,
                    "requesting_task_id": request.requesting_task_id,
                    "secret_value_in_payload": false
                },
                "created_at_ms": now,
                "updated_at_ms": now
            }),
        )?;
        if let Some(object) = outcome.as_object_mut() {
            object.insert("handoff_enqueued".to_string(), Value::Bool(true));
            object.insert(
                "handoff_command_id".to_string(),
                stored
                    .get("command_id")
                    .or_else(|| stored.get("id"))
                    .cloned()
                    .unwrap_or(Value::Null),
            );
        }
    }
    Ok(outcome)
}

fn current_peer() -> Option<Arc<NativePeer>> {
    NATIVE_PEER.lock().ok().and_then(|guard| guard.clone())
}

async fn run_native_peer(
    root: PathBuf,
    sync_room: String,
    signaling_urls: Vec<String>,
    signaling_room_password: String,
) -> anyhow::Result<()> {
    let Some(process_lock) = acquire_native_peer_process_lock(&root)? else {
        eprintln!("[business-os] native rxdb peer already runs in another process");
        return Ok(());
    };
    let signaling_url = signaling_urls
        .into_iter()
        .find(|url| !url.trim().is_empty())
        .context("Business OS native RxDB peer requires a signaling URL")?;
    let signaling_url =
        signaling_url_with_native_metadata(&signaling_url, &sync_room, &signaling_room_password);
    let ice_servers = ice_servers_from_sync_config(&store::sync_config(&root)?.ice_servers);
    let peer_session_id = format!("rxdb-rs-{}", Uuid::new_v4().simple());
    let database_path = store::rxdb_store_path(&root);
    let database = open_database(database_path.clone()).await?;
    let database_write_lock = Arc::new(AsyncMutex::new(()));

    // FIX 2: start the status heartbeat on its dedicated OS thread NOW — right
    // after the process lock and DB are ready and BEFORE the collection
    // bring-up loop. If bring-up stalls, the heartbeat must still be written so
    // `business-os-rxdb-peer.status.json` stays fresh and the process is not
    // mistaken for dead-but-lock-held.
    let status_heartbeat = spawn_native_peer_status_heartbeat(
        root.clone(),
        peer_session_id.clone(),
        database_path.clone(),
    );

    // FIX 4: register collections fault tolerantly. A drifted/failing OPTIONAL
    // collection is logged and skipped; a failing REQUIRED collection still
    // aborts the peer (the daemon depends on those). The strict
    // all-or-nothing `add_collections` is no longer used here.
    let (collections, failed_collections) = database
        .add_collections_tolerant(collection_creators())
        .await
        .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
    for (collection_name, err) in &failed_collections {
        if is_required_native_collection(collection_name) {
            // Drop the heartbeat handle + process lock before returning so the
            // lock is released for a clean restart.
            return Err(anyhow::anyhow!(
                "required Business OS RxDB collection `{collection_name}` failed to register: {err}"
            ));
        }
        eprintln!(
            "[business-os] skipping optional Business OS RxDB collection `{collection_name}` \
             (registration failed: {err})"
        );
    }

    // Phase 3 (single multiplexed stream): bring up ONE replication session
    // for the WHOLE sync room. Instead of one signaling socket +
    // RTCPeerConnection + DataChannel per collection (~88 of each per browser),
    // we join the bare `sync_room` once with one WebRTCRsConnectionHandler and
    // register EVERY collection's master handler + fork state behind it. Frames
    // are demultiplexed by their `collection` field inside the rxdb crate. This
    // is what collapses the FD/socket explosion that the LimitNOFILE
    // workaround was papering over.
    //
    // Per-collection batch tuning that used to live on each
    // `SyncOptionsWebRTCRs` (e.g. the tighter `desktop_file_chunks` window) is
    // now subsumed by the Phase-1 native backpressure + chunking, which is
    // collection-agnostic; the multiplexed session uses uniform batch sizes.
    let collection_list: Vec<Arc<RxCollection>> =
        collections.into_iter().map(|(_, collection)| collection).collect();
    let collection_count = collection_list.len();
    let mut pools: Vec<WebRtcPool> = Vec::with_capacity(1);
    if collection_count == 0 {
        eprintln!("[business-os] no Business OS RxDB collections to replicate; skipping WebRTC bring-up");
    } else {
        let topic = sync_room.clone();
        let multi_signaling_url = signaling_url.clone();
        let multi_peer_session_id = peer_session_id.clone();
        let multi_ice_servers = ice_servers.clone();
        let bringup = tokio::spawn(async move {
            rxdb::plugins::replication_webrtc::replicate_web_rtc_rs_multi(
                collection_list,
                multi_signaling_url,
                topic,
                multi_peer_session_id,
                multi_ice_servers,
                None,
                20,
                20,
                5_000,
            )
            .await
        });
        match tokio::time::timeout(
            Duration::from_secs(NATIVE_COLLECTION_BRINGUP_TIMEOUT_SECS),
            bringup,
        )
        .await
        {
            Ok(Ok(Ok(pool))) => {
                eprintln!(
                    "[business-os] multiplexed WebRTC replication up for {collection_count} \
                     collections on one connection (room `{sync_room}`)"
                );
                pools.push(pool);
            }
            Ok(Ok(Err(err))) => {
                eprintln!(
                    "[business-os] multiplexed WebRTC replication bring-up failed: {err}"
                );
            }
            Ok(Err(join_err)) => {
                eprintln!(
                    "[business-os] multiplexed WebRTC replication bring-up task panicked: {join_err}"
                );
            }
            Err(_) => {
                eprintln!(
                    "[business-os] multiplexed WebRTC replication bring-up timed out after {}s",
                    NATIVE_COLLECTION_BRINGUP_TIMEOUT_SECS
                );
            }
        }
    }

    let command_consumer = tokio::spawn(consume_business_commands_loop(
        root.clone(),
        Arc::clone(&database),
        Arc::clone(&database_write_lock),
    ));

    let notes_sync = tokio::spawn(sync_notes_background_loop(root.clone()));
    let file_index_sync =
        if std::env::var_os("CTOX_BUSINESS_OS_DISABLE_BACKGROUND_FILE_INDEX").is_some() {
            tokio::spawn(std::future::pending())
        } else {
            tokio::spawn(sync_desktop_file_index_background_loop(
                root.clone(),
                Arc::clone(&database),
                Arc::clone(&database_write_lock),
            ))
        };
    let channel_state_sync = tokio::spawn(sync_channel_state_background_loop(
        root.clone(),
        Arc::clone(&database),
        Arc::clone(&database_write_lock),
    ));
    let business_users_sync = tokio::spawn(sync_business_users_background_loop(
        root.clone(),
        Arc::clone(&database),
        Arc::clone(&database_write_lock),
    ));
    let runtime_settings_sync = tokio::spawn(sync_runtime_settings_background_loop(
        root.clone(),
        Arc::clone(&database),
        Arc::clone(&database_write_lock),
    ));
    let module_catalog_sync = tokio::spawn(sync_module_catalog_background_loop(
        root.clone(),
        Arc::clone(&database),
        Arc::clone(&database_write_lock),
    ));
    let ticket_state_sync = tokio::spawn(sync_ticket_state_background_loop(
        root.clone(),
        Arc::clone(&database),
        Arc::clone(&database_write_lock),
    ));
    let business_record_projection_sync =
        tokio::spawn(sync_business_record_projections_background_loop(
            root.clone(),
            Arc::clone(&database),
            Arc::clone(&database_write_lock),
        ));
    // Any session left active by a previous run has no live process; reconcile.
    {
        let _guard = database_write_lock.lock().await;
        if let Err(err) = recover_stale_browser_sessions(&database).await {
            eprintln!("[business-os] browser session recovery failed: {err:#}");
        }
    }
    let browser_runtime_maintenance = tokio::spawn(browser_runtime_maintenance_loop(
        root.clone(),
        Arc::clone(&database),
        Arc::clone(&database_write_lock),
    ));
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

    // The heartbeat thread was started before bring-up (FIX 2). Move its
    // handle into the peer so `shutdown` can stop it and so it is dropped (and
    // signalled to stop) on any unwind of this function.
    let peer = Arc::new(NativePeer {
        database,
        peer_session_id,
        shutdown_tx: Mutex::new(Some(shutdown_tx)),
        _process_lock: process_lock,
        _pools: pools,
        _command_consumer: command_consumer,
        _notes_sync: notes_sync,
        _file_index_sync: file_index_sync,
        _channel_state_sync: channel_state_sync,
        _business_users_sync: business_users_sync,
        _runtime_settings_sync: runtime_settings_sync,
        _module_catalog_sync: module_catalog_sync,
        _ticket_state_sync: ticket_state_sync,
        _business_record_projection_sync: business_record_projection_sync,
        _browser_runtime_maintenance: browser_runtime_maintenance,
        _status_heartbeat: Mutex::new(Some(status_heartbeat)),
    });
    if let Ok(mut current) = NATIVE_PEER.lock() {
        *current = Some(Arc::clone(&peer));
    }
    NATIVE_PEER_RUNNING.store(true, Ordering::SeqCst);

    // FIX 2: instead of a bare `shutdown_rx.await`, select over the shutdown
    // signal and a periodic watchdog tick. The watchdog confirms the dedicated
    // heartbeat thread is still publishing a fresh status file. If the
    // heartbeat machinery is wedged (no fresh heartbeat for well over the
    // write interval and published TTL), the peer logs and shuts down cleanly
    // so the OS process lock is released for a fresh start instead of being
    // held forever by a dead-but-not-exited process. This is conservative: the
    // threshold is far above the heartbeat interval to avoid flapping.
    let mut watchdog =
        tokio::time::interval(Duration::from_secs(NATIVE_PEER_WATCHDOG_INTERVAL_SECS));
    watchdog.tick().await; // first tick fires immediately; consume it.
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                break;
            }
            _ = watchdog.tick() => {
                let heartbeat_age_ms = heartbeat_updated_at_ms(
                    read_native_peer_heartbeat(&root).as_ref(),
                )
                .map(|updated_at_ms| (now_ms() as u64).saturating_sub(updated_at_ms));
                let wedged = heartbeat_age_ms
                    .map(|age_ms| age_ms > NATIVE_PEER_WATCHDOG_MAX_HEARTBEAT_AGE_MS)
                    .unwrap_or(true);
                if wedged {
                    eprintln!(
                        "[business-os] native rxdb peer watchdog: heartbeat stale ({:?} ms); \
                         shutting down to release the process lock for a clean restart",
                        heartbeat_age_ms
                    );
                    break;
                }
            }
        }
    }

    peer.shutdown().await;
    if let Ok(mut current) = NATIVE_PEER.lock() {
        if current
            .as_ref()
            .map(|candidate| Arc::ptr_eq(candidate, &peer))
            .unwrap_or(false)
        {
            *current = None;
        }
    }
    NATIVE_PEER_RUNNING.store(false, Ordering::SeqCst);
    NATIVE_PEER_STARTED.store(false, Ordering::SeqCst);
    Ok(())
}

fn ice_servers_from_sync_config(values: &[Value]) -> Vec<RTCIceServer> {
    values
        .iter()
        .filter_map(|value| {
            let object = value.as_object()?;
            let urls = object.get("urls")?;
            let urls = if let Some(url) = urls.as_str() {
                let trimmed = url.trim();
                if trimmed.is_empty() {
                    return None;
                }
                vec![trimmed.to_owned()]
            } else if let Some(items) = urls.as_array() {
                let urls = items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(str::trim)
                    .filter(|url| !url.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if urls.is_empty() {
                    return None;
                }
                urls
            } else {
                return None;
            };
            Some(RTCIceServer {
                urls,
                username: object
                    .get("username")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                credential: object
                    .get("credential")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                ..RTCIceServer::default()
            })
        })
        .collect()
}

fn signaling_url_with_native_metadata(
    raw_url: &str,
    sync_room: &str,
    signaling_room_password: &str,
) -> String {
    let Ok(mut url) = Url::parse(raw_url) else {
        return raw_url.to_string();
    };
    let existing = url
        .query_pairs()
        .filter(|(key, _)| {
            !matches!(
                key.as_ref(),
                "client"
                    | "role"
                    | "instance_id"
                    | "protocol"
                    | "cap"
                    | "token"
                    | "token_iat"
                    | "token_exp"
            )
        })
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in existing {
            query.append_pair(&key, &value);
        }
        query.append_pair("client", "ctox-business-os-native");
        query.append_pair("role", "ctox_instance");
        if let Some(instance_id) = instance_id_from_sync_room(sync_room) {
            query.append_pair("instance_id", instance_id);
        }
        query.append_pair("protocol", CTOX_RXDB_PROTOCOL);
        if let Some(token) = signaling_token_from_room_password(signaling_room_password) {
            let issued_at = current_unix_seconds();
            query.append_pair("token", &token);
            query.append_pair("token_iat", &issued_at.to_string());
            query.append_pair(
                "token_exp",
                &(issued_at + SIGNALING_TOKEN_TTL_SECONDS).to_string(),
            );
        }
        for capability in CTOX_NATIVE_CAPABILITIES {
            query.append_pair("cap", capability);
        }
    }
    url.to_string()
}

fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn signaling_token_from_room_password(room_password: &str) -> Option<String> {
    let password = room_password.trim();
    if password.is_empty() {
        return None;
    }
    let digest = sha2::Sha256::digest(password.as_bytes());
    Some(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)[..32].to_string())
}

fn instance_id_from_sync_room(sync_room: &str) -> Option<&str> {
    let mut parts = sync_room.split(':');
    if parts.next()? != "ctox-business-os" {
        return None;
    }
    let instance_id = parts.next()?.trim();
    (!instance_id.is_empty()).then_some(instance_id)
}

fn native_peer_lock_path(root: &Path) -> PathBuf {
    root.join("runtime/business-os-rxdb-peer.lock")
}

fn native_peer_heartbeat_path(root: &Path) -> PathBuf {
    root.join("runtime/business-os-rxdb-peer.status.json")
}

fn read_native_peer_heartbeat(root: &Path) -> Option<Value> {
    let bytes = fs::read(native_peer_heartbeat_path(root)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn heartbeat_updated_at_ms(heartbeat: Option<&Value>) -> Option<u64> {
    heartbeat?.get("updated_at_ms").and_then(Value::as_u64)
}

fn heartbeat_peer_session_id(heartbeat: Option<&Value>) -> Option<String> {
    heartbeat?
        .get("peer_session_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty())
}

fn native_peer_heartbeat_is_fresh(root: &Path) -> bool {
    let Some(updated_at_ms) = heartbeat_updated_at_ms(read_native_peer_heartbeat(root).as_ref())
    else {
        return false;
    };
    let age_ms = (now_ms() as u64).saturating_sub(updated_at_ms);
    age_ms <= NATIVE_PEER_HEARTBEAT_TTL_MS
}

fn write_native_peer_heartbeat(
    root: &Path,
    peer_session_id: &str,
    database_path: &Path,
) -> anyhow::Result<()> {
    let path = native_peer_heartbeat_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create native RxDB peer status dir {}",
                parent.display()
            )
        })?;
    }
    let payload = json!({
        "version": NATIVE_PEER_STATUS_VERSION,
        "running": true,
        "pid": std::process::id(),
        "peer_session_id": peer_session_id,
        "updated_at_ms": now_ms() as u64,
        "database_path": database_path.display().to_string(),
    });
    let temporary_path = path.with_extension("status.json.tmp");
    fs::write(&temporary_path, serde_json::to_vec_pretty(&payload)?).with_context(|| {
        format!(
            "failed to write native RxDB peer status {}",
            temporary_path.display()
        )
    })?;
    fs::rename(&temporary_path, &path).with_context(|| {
        format!(
            "failed to publish native RxDB peer status {}",
            path.display()
        )
    })?;
    Ok(())
}

/// FIX 2: handle for the dedicated status-heartbeat OS thread. Holding it
/// alive keeps the heartbeat running; calling `stop()` (or dropping it) sets
/// the stop flag so the thread exits at its next wake.
struct StatusHeartbeatHandle {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl StatusHeartbeatHandle {
    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for StatusHeartbeatHandle {
    fn drop(&mut self) {
        // Ensure the heartbeat thread is signalled to stop even if the peer
        // unwinds without an explicit shutdown path.
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// FIX 2: run the status heartbeat on a DEDICATED OS thread driven by
/// `std::thread::sleep`, not a tokio task. Heartbeat liveness must be
/// independent of async-runtime health: if the tokio workers are starved or
/// the collection bring-up loop stalls, the heartbeat must still be written so
/// `business-os-rxdb-peer.status.json` stays fresh (TTL 30s). The caller
/// starts this right after the process lock + DB are ready, BEFORE bring-up.
fn spawn_native_peer_status_heartbeat(
    root: PathBuf,
    peer_session_id: String,
    database_path: PathBuf,
) -> StatusHeartbeatHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name("business-os-rxdb-heartbeat".to_string())
        .spawn(move || {
            while !stop_for_thread.load(Ordering::SeqCst) {
                if let Err(err) =
                    write_native_peer_heartbeat(&root, &peer_session_id, &database_path)
                {
                    eprintln!("[business-os] native rxdb peer status heartbeat failed: {err:#}");
                }
                // Sleep in short slices so a stop request is honored promptly
                // instead of after a full heartbeat interval.
                let mut slept_ms = 0u64;
                let interval_ms = NATIVE_PEER_HEARTBEAT_INTERVAL_SECS * 1_000;
                while slept_ms < interval_ms && !stop_for_thread.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(250));
                    slept_ms += 250;
                }
            }
        })
        .ok();
    StatusHeartbeatHandle { stop, thread }
}

fn open_native_peer_lock_file(root: &Path) -> anyhow::Result<File> {
    let lock_path = native_peer_lock_path(root);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create native RxDB peer lock dir {}",
                parent.display()
            )
        })?;
    }
    File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| {
            format!(
                "failed to open native RxDB peer lock {}",
                lock_path.display()
            )
        })
}

fn acquire_native_peer_process_lock(root: &Path) -> anyhow::Result<Option<File>> {
    let lock_file = open_native_peer_lock_file(root)?;
    match lock_file.try_lock() {
        Ok(()) => Ok(Some(lock_file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(err)) => {
            Err(err).context("failed to acquire native RxDB peer process lock")
        }
    }
}

fn native_peer_process_lock_is_held(root: &Path) -> bool {
    let Ok(lock_file) = open_native_peer_lock_file(root) else {
        return false;
    };
    match lock_file.try_lock() {
        Ok(()) => false,
        Err(std::fs::TryLockError::WouldBlock) => true,
        Err(std::fs::TryLockError::Error(_)) => false,
    }
}

impl NativePeer {
    async fn upsert_desktop_file_from_path(
        &self,
        path: PathBuf,
        policy: DesktopFileContentPolicy,
    ) -> anyhow::Result<()> {
        upsert_desktop_file_with_policy(&self.database, path, policy).await
    }

    async fn sync_desktop_files_from_scan_roots(
        &self,
        scan_roots: Vec<DesktopFileScanRoot>,
    ) -> anyhow::Result<usize> {
        sync_desktop_file_scan_roots_with_database(&self.database, scan_roots).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DesktopFileContentPolicy {
    Eager,
    Lazy,
}

#[derive(Debug, Clone)]
struct DesktopFileScanRoot {
    path: PathBuf,
    label: String,
}

#[derive(Debug, Clone)]
struct DesktopFileIndexCandidate {
    path: PathBuf,
    scan_root: DesktopFileScanRoot,
}

async fn sync_notes_background_loop(root: PathBuf) {
    loop {
        let root_clone = root.clone();
        let res =
            tokio::task::spawn_blocking(move || store::sync_local_markdown_notes(&root_clone))
                .await;
        if let Err(err) = res {
            eprintln!("[business-os] native rxdb notes sync join failed: {err:#}");
        } else if let Ok(Err(err)) = res {
            eprintln!("[business-os] native rxdb notes sync failed: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

async fn sync_desktop_file_index_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    loop {
        let result = {
            let _guard = database_write_lock.lock().await;
            sync_desktop_file_index_with_database(&root, &database).await
        };
        if let Err(err) = result {
            eprintln!("[business-os] native rxdb desktop file index failed: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(DESKTOP_FILE_SCAN_INTERVAL_SECS)).await;
    }
}

async fn sync_channel_state_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    loop {
        let result = {
            let _guard = database_write_lock.lock().await;
            sync_channel_state_with_database(&root, &database).await
        };
        if let Err(err) = result {
            eprintln!("[business-os] native rxdb channel state sync failed: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(CHANNEL_STATE_SYNC_INTERVAL_SECS)).await;
    }
}

async fn sync_business_users_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    loop {
        let result = {
            let _guard = database_write_lock.lock().await;
            sync_business_users_with_database(&root, &database).await
        };
        if let Err(err) = result {
            eprintln!("[business-os] native rxdb business users sync failed: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(BUSINESS_USERS_SYNC_INTERVAL_SECS)).await;
    }
}

async fn sync_runtime_settings_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    loop {
        let result = {
            let _guard = database_write_lock.lock().await;
            sync_runtime_settings_with_database(&root, &database).await
        };
        if let Err(err) = result {
            eprintln!("[business-os] native rxdb runtime settings sync failed: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(RUNTIME_SETTINGS_SYNC_INTERVAL_SECS)).await;
    }
}

async fn sync_module_catalog_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    loop {
        let result = {
            let _guard = database_write_lock.lock().await;
            sync_module_catalog_with_database(&root, &database).await
        };
        if let Err(err) = result {
            eprintln!("[business-os] native rxdb module catalog sync failed: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(MODULE_CATALOG_SYNC_INTERVAL_SECS)).await;
    }
}

async fn sync_ticket_state_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    loop {
        let result = {
            let _guard = database_write_lock.lock().await;
            sync_ticket_state_with_database(&root, &database).await
        };
        if let Err(err) = result {
            eprintln!("[business-os] native rxdb ticket state sync failed: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(TICKET_STATE_SYNC_INTERVAL_SECS)).await;
    }
}

async fn sync_business_record_projections_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut since_by_collection = HashMap::<String, i64>::new();
    loop {
        let result = {
            let _guard = database_write_lock.lock().await;
            sync_business_record_projections_with_database(
                &root,
                &database,
                &mut since_by_collection,
            )
            .await
        };
        if let Err(err) = result {
            eprintln!("[business-os] native rxdb business record projection sync failed: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(
            BUSINESS_RECORD_PROJECTION_SYNC_INTERVAL_SECS,
        ))
        .await;
    }
}

async fn consume_business_commands_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    loop {
        let result = {
            let _guard = database_write_lock.lock().await;
            consume_pending_business_commands(&root, &database).await
        };
        if let Err(err) = result {
            eprintln!("[business-os] native rxdb command consumer failed: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn enqueue_business_command_document_with_database(
    database: &Arc<RxDatabase>,
    mut document: Value,
) -> anyhow::Result<Value> {
    let Some(object) = document.as_object_mut() else {
        anyhow::bail!("business command document must be an object");
    };
    let command_id = object
        .get("command_id")
        .or_else(|| object.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("business_command_{}", Uuid::new_v4().simple()));
    let now = now_ms() as u64;
    object.insert("id".to_string(), Value::String(command_id.clone()));
    object.insert("command_id".to_string(), Value::String(command_id.clone()));
    object
        .entry("status".to_string())
        .or_insert_with(|| Value::String("pending_sync".to_string()));
    object
        .entry("created_at_ms".to_string())
        .or_insert_with(|| Value::from(now));
    object.insert("updated_at_ms".to_string(), Value::from(now));

    let commands = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    commands
        .incremental_upsert(document.clone())
        .await
        .map_err(|err| anyhow::anyhow!("enqueue business command {command_id}: {err}"))?;
    Ok(document)
}

async fn browser_session_status_with_database(
    database: &Arc<RxDatabase>,
    session_id: &str,
) -> anyhow::Result<Value> {
    let sessions = database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?;
    let session = sessions
        .storage_instance
        .find_documents_by_id(&[session_id.to_string()], false)
        .await
        .map_err(|err| anyhow::anyhow!("load browser session {session_id}: {err}"))?
        .into_iter()
        .next()
        .with_context(|| format!("browser session not found: {session_id}"))?;
    Ok(redacted_browser_session_status(&session))
}

async fn browser_context_snapshot_with_database(
    database: &Arc<RxDatabase>,
    session_id: &str,
) -> anyhow::Result<Value> {
    let sessions = database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?;
    let session = sessions
        .storage_instance
        .find_documents_by_id(&[session_id.to_string()], false)
        .await
        .map_err(|err| anyhow::anyhow!("load browser session {session_id}: {err}"))?
        .into_iter()
        .next()
        .with_context(|| format!("browser session not found: {session_id}"))?;
    let tab = browser_context_related_document(database, "browser_tabs", "session_id", session_id)
        .await?;
    let frame =
        browser_context_related_document(database, "browser_frames", "session_id", session_id)
            .await?;
    Ok(redacted_browser_context_capture(
        &session,
        tab.as_ref(),
        frame.as_ref(),
    ))
}

async fn browser_context_related_document(
    database: &Arc<RxDatabase>,
    collection: &str,
    field: &str,
    value: &str,
) -> anyhow::Result<Option<Value>> {
    let collection = database
        .collection(collection)
        .with_context(|| format!("{collection} collection is not registered"))?;
    let document = collection
        .find_one(Some(MangoQuery {
            selector: Some(json!({ field: { "$eq": value } })),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query browser context document: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec browser context document query: {err}"))?;
    Ok(document.is_object().then_some(document))
}

fn redacted_browser_session_status(session: &Value) -> Value {
    let payload = session.get("payload").unwrap_or(&Value::Null);
    json!({
        "ok": true,
        "session": {
            "id": session.get("id").and_then(Value::as_str).unwrap_or_default(),
            "status": session.get("status").and_then(Value::as_str).unwrap_or_default(),
            "runtime_status": session.get("runtime_status").and_then(Value::as_str).unwrap_or_default(),
            "current_url": session.get("current_url").and_then(Value::as_str).unwrap_or_default(),
            "updated_at_ms": session.get("updated_at_ms").and_then(Value::as_u64).unwrap_or_default(),
            "payload": {
                "source_id": payload.get("source_id").and_then(Value::as_str).unwrap_or_default(),
                "capture_extract_result": payload.get("capture_extract_result").cloned().unwrap_or(Value::Null),
                "secret_value_in_rxdb": payload.get("secret_value_in_rxdb").and_then(Value::as_bool).unwrap_or(false),
                "browser_stream": payload.get("browser_stream").and_then(Value::as_str).unwrap_or("rxdb")
            }
        }
    })
}

fn redacted_browser_context_capture(
    session: &Value,
    tab: Option<&Value>,
    frame: Option<&Value>,
) -> Value {
    let browser_context = json!({
        "session": redacted_browser_session_status(session).get("session").cloned().unwrap_or(Value::Null),
        "tab": tab.cloned().unwrap_or(Value::Null),
        "frame": frame.map(redact_browser_frame_data).unwrap_or(Value::Null),
    });
    json!({
        "ok": true,
        "browser_stream": "rxdb",
        "browser_context": browser_context,
        "captured_at_ms": now_ms() as u64,
    })
}

fn redact_browser_frame_data(frame: &Value) -> Value {
    let mut frame = frame.clone();
    if let Some(object) = frame.as_object_mut() {
        object.remove("data");
        object.remove("content");
        object.remove("secret");
    }
    frame
}

async fn consume_pending_business_commands(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<()> {
    let commands = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let pending = commands
        .find(Some(MangoQuery {
            selector: Some(json!({ "status": { "$eq": "pending_sync" } })),
            limit: Some(25),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query pending business_commands: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec pending business_commands query: {err}"))?;

    let Some(rows) = pending.as_array() else {
        return Ok(());
    };
    for document in rows {
        accept_pending_business_command(root, database, document.clone()).await?;
    }
    Ok(())
}

async fn accept_pending_business_command(
    root: &Path,
    database: &Arc<RxDatabase>,
    document: Value,
) -> anyhow::Result<()> {
    let command_type = document
        .get("command_type")
        .or_else(|| document.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let command_payload = document.get("payload").cloned().unwrap_or(Value::Null);

    if is_browser_runtime_command(&command_type) {
        let command_id = document
            .get("command_id")
            .or_else(|| document.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let accepted = json!({
            "id": command_id,
            "command_id": command_id,
            "status": "accepted",
            "task_status": "accepted"
        });
        if let Err(err) = apply_browser_runtime_command(root, database, &document, &accepted).await
        {
            eprintln!("[business-os] browser runtime command failed: {err:#}");
            mark_browser_runtime_command_failed(database, &command_id, &command_payload, &err)
                .await?;
        }
        return Ok(());
    }

    let root = root.to_path_buf();
    let accept_root = root.clone();
    let document_for_store = document.clone();
    let accepted_result = tokio::task::spawn_blocking(move || {
        store::accept_rxdb_business_command(&accept_root, document_for_store)
    })
    .await;

    let accepted = match accepted_result {
        Ok(Ok(val)) => val,
        Ok(Err(err)) => {
            eprintln!("[business-os] native business command store execution failed: {err:#}");
            let command_id = document
                .get("command_id")
                .or_else(|| document.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !command_id.is_empty() {
                let commands = database
                    .collection("business_commands")
                    .context("business_commands collection is not registered")?;
                let existing = commands
                    .find_one(Some(MangoQuery {
                        selector: Some(json!({ "id": { "$eq": command_id } })),
                        ..Default::default()
                    }))
                    .map_err(|err| anyhow::anyhow!("query failed business_command: {err}"))?
                    .exec(false)
                    .await
                    .map_err(|err| anyhow::anyhow!("exec failed business_command query: {err}"))?;
                let mut next = if existing.is_object() {
                    existing
                } else {
                    json!({ "id": command_id, "command_id": command_id })
                };
                if let Some(obj) = next.as_object_mut() {
                    obj.insert("status".to_string(), Value::String("failed".to_string()));
                    obj.insert("error".to_string(), Value::String(err.to_string()));
                    obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
                }
                commands.incremental_upsert(next).await.map_err(|err| {
                    anyhow::anyhow!("upsert failed business_command {command_id}: {err}")
                })?;
            }
            return Ok(());
        }
        Err(err) => {
            return Err(err.into());
        }
    };

    let command_id = accepted
        .get("command_id")
        .or_else(|| accepted.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .context("accepted command is missing command_id")?;

    let commands = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let existing = commands
        .find_one(Some(MangoQuery {
            selector: Some(json!({ "id": { "$eq": command_id } })),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query accepted business_command: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec accepted business_command query: {err}"))?;
    let accepted_status = accepted
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("accepted");
    let existing_status = existing.get("status").and_then(Value::as_str).unwrap_or("");
    if accepted_status == "already_accepted"
        && !existing_status.is_empty()
        && existing_status != "pending_sync"
    {
        return Ok(());
    }
    let mut next = if existing.is_object() {
        existing
    } else {
        json!({ "id": command_id, "command_id": command_id })
    };
    if let Some(obj) = next.as_object_mut() {
        obj.insert(
            "status".to_string(),
            Value::String(accepted_status.to_string()),
        );
        if accepted.get("task_id").is_some() || !obj.contains_key("task_id") {
            obj.insert(
                "task_id".to_string(),
                accepted
                    .get("task_id")
                    .cloned()
                    .unwrap_or_else(|| Value::String(String::new())),
            );
        }
        obj.insert(
            "task_status".to_string(),
            accepted.get("task_status").cloned().unwrap_or_else(|| {
                obj.get("status")
                    .cloned()
                    .unwrap_or(Value::String("accepted".to_string()))
            }),
        );
        if let Some(result) = accepted.get("result") {
            obj.insert("result".to_string(), result.clone());
        }
        for key in ["report_id", "report_status"] {
            if let Some(value) = accepted.get(key) {
                obj.insert(key.to_string(), value.clone());
            }
        }
        obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    }
    commands
        .incremental_upsert(next)
        .await
        .map_err(|err| anyhow::anyhow!("upsert accepted business_command {command_id}: {err}"))?;

    if let Some(task_id) = accepted.get("task_id").and_then(Value::as_str) {
        if !task_id.is_empty() {
            upsert_business_record_projection(
                root.clone(),
                database,
                "ctox_queue_tasks",
                task_id.to_string(),
            )
            .await?;
        }
    }
    if let Some(report_id) = accepted.get("report_id").and_then(Value::as_str) {
        if !report_id.is_empty() {
            upsert_business_record_projection(
                root.clone(),
                database,
                "business_module_reports",
                report_id.to_string(),
            )
            .await?;
            upsert_business_record_projection(
                root.clone(),
                database,
                "ctox_bug_reports",
                report_id.to_string(),
            )
            .await?;
        }
    }
    if let Some(source_file_ids) = accepted
        .get("result")
        .and_then(|result| result.get("source_file_ids"))
        .and_then(Value::as_array)
    {
        for source_file_id in source_file_ids.iter().filter_map(Value::as_str) {
            if !source_file_id.is_empty() {
                upsert_business_record_projection(
                    root.clone(),
                    database,
                    "business_module_source_files",
                    source_file_id.to_string(),
                )
                .await?;
            }
        }
    }
    if command_type == "ctox.business_os.user.upsert" {
        sync_business_users_with_database(&root, database).await?;
    }
    if command_type == "ctox.runtime_settings.save" {
        sync_runtime_settings_with_database(&root, database).await?;
    }
    if command_type.starts_with("ctox.ticket.") {
        sync_ticket_state_with_database(&root, database).await?;
    }
    if command_type == "ctox.file.materialize" {
        if let Some(materialized_path) = accepted
            .get("result")
            .and_then(|result| result.get("path"))
            .or_else(|| command_payload.get("path"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            upsert_desktop_file_with_policy(
                database,
                PathBuf::from(materialized_path),
                DesktopFileContentPolicy::Eager,
            )
            .await
            .with_context(|| {
                format!("project materialized desktop file {materialized_path} into native RxDB")
            })?;
        }
    }
    if matches!(
        command_type.as_str(),
        "ctox.module.save"
            | "ctox.module.delete"
            | "ctox.module.install_template"
            | "ctox.module.assign_founder"
            | "ctox.module.release"
            | "ctox.module.rollback"
            | "ctox.module.rollback_version"
            | "ctox.app_store.install"
            | "ctox.app_store.uninstall"
    ) {
        sync_module_catalog_with_database(&root, database).await?;
    }
    let mut projected_acl = false;
    if let Some(acl_ids) = accepted
        .get("result")
        .and_then(|result| result.get("business_module_acl_ids"))
        .and_then(Value::as_array)
    {
        for acl_id in acl_ids.iter().filter_map(Value::as_str) {
            if !acl_id.is_empty() {
                upsert_business_record_projection(
                    root.clone(),
                    database,
                    "business_module_acl",
                    acl_id.to_string(),
                )
                .await?;
                projected_acl = true;
            }
        }
    }
    if !projected_acl && command_type == "ctox.module.assign_founder" {
        let module_id = command_payload
            .get("module_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let user_id = command_payload
            .get("user_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if !module_id.is_empty() && !user_id.is_empty() {
            upsert_business_record_projection(
                root.clone(),
                database,
                "business_module_acl",
                format!("{module_id}:founder:{user_id}"),
            )
            .await?;
        }
    }
    if let Some(release_ids) = accepted
        .get("result")
        .and_then(|result| result.get("business_module_release_ids"))
        .and_then(Value::as_array)
    {
        for release_id in release_ids.iter().filter_map(Value::as_str) {
            if !release_id.is_empty() {
                upsert_business_record_projection(
                    root.clone(),
                    database,
                    "business_module_releases",
                    release_id.to_string(),
                )
                .await?;
            }
        }
    }
    if is_browser_runtime_command(&command_type) {
        if let Err(err) = apply_browser_runtime_command(&root, database, &document, &accepted).await
        {
            eprintln!("[business-os] browser runtime command failed: {err:#}");
            mark_browser_runtime_command_failed(database, &command_id, &command_payload, &err)
                .await?;
        }
    }

    Ok(())
}

fn is_browser_runtime_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "browser.session.start"
            | "browser.navigate"
            | "browser.reload"
            | "browser.back"
            | "browser.forward"
            | "browser.reset"
            | "browser.session.stop"
    )
}

async fn apply_browser_runtime_command(
    root: &Path,
    database: &Arc<RxDatabase>,
    document: &Value,
    accepted: &Value,
) -> anyhow::Result<()> {
    let _browser_runtime_guard = BROWSER_RUNTIME_COMMAND_LOCK.lock().await;
    let command_type = document
        .get("command_type")
        .or_else(|| document.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let payload = document.get("payload").cloned().unwrap_or(Value::Null);
    let session_id = payload
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("browser_session_default")
        .to_string();
    let tab_id = payload
        .get("tab_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("browser_tab_default")
        .to_string();
    let viewport_w = payload
        .get("viewport_w")
        .and_then(Value::as_u64)
        .unwrap_or(1280)
        .clamp(320, 3840);
    let viewport_h = payload
        .get("viewport_h")
        .and_then(Value::as_u64)
        .unwrap_or(720)
        .clamp(240, 2160);
    let command_created_at_ms = document
        .get("created_at_ms")
        .or_else(|| document.get("updated_at_ms"))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| now_ms() as u64);

    let existing_session = find_browser_document(database, "browser_sessions", &session_id).await?;
    let existing_tab = find_browser_document(database, "browser_tabs", &tab_id).await?;
    let previous_url = existing_tab
        .get("url")
        .or_else(|| existing_session.get("current_url"))
        .and_then(Value::as_str)
        .unwrap_or("https://example.com")
        .to_string();
    let target_url = payload
        .get("url")
        .and_then(Value::as_str)
        .map(normalize_browser_runtime_url)
        .filter(|value| !value.is_empty())
        .unwrap_or(previous_url);

    if command_type == "browser.session.stop" {
        let title = existing_tab
            .get("title")
            .or_else(|| existing_session.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("Remote Browser");
        browser_runtime_manager().stop(&session_id).await;
        upsert_browser_session(
            database,
            &session_id,
            &tab_id,
            "stopped",
            "stopped",
            &target_url,
            title,
            viewport_w,
            viewport_h,
            None,
            0,
            command_type,
            command_created_at_ms,
            None,
        )
        .await?;
        upsert_browser_tab(
            database,
            &tab_id,
            &session_id,
            title,
            &target_url,
            "stopped",
            false,
            false,
            false,
            None,
            0,
        )
        .await?;
        mark_browser_runtime_command_completed(
            database,
            document,
            accepted,
            json!({
                "ok": true,
                "browser_stream": "rxdb",
                "session_id": session_id,
                "tab_id": tab_id,
                "status": "stopped"
            }),
        )
        .await?;
        return Ok(());
    }

    // All remaining commands (start/navigate/reload/back/forward/reset) drive a
    // live, persistent Chromium runtime via the session registry.
    let manager = browser_runtime_manager();
    if command_type == "browser.reset" {
        manager.stop(&session_id).await;
    }

    let browser_runtime_dir = browser_runtime_reference_dir(root);
    let session = match manager
        .ensure_session(
            root.to_path_buf(),
            browser_runtime_dir,
            &session_id,
            viewport_w,
            viewport_h,
        )
        .await
    {
        Ok(session) => session,
        Err(err) => {
            let detail = format!("{err:#}");
            mark_browser_session_runtime_error(
                database,
                &session_id,
                &tab_id,
                &target_url,
                viewport_w,
                viewport_h,
                command_type,
                command_created_at_ms,
                &detail,
            )
            .await?;
            return Err(err);
        }
    };

    // Translate the lifecycle command into a runtime operation.
    let (op, op_params): (&str, Value) = match command_type {
        "browser.navigate" | "browser.session.start" | "browser.reset" => {
            ("navigate", json!({ "url": target_url, "timeoutMs": 30000 }))
        }
        "browser.reload" => ("reload", json!({ "timeoutMs": 30000 })),
        "browser.back" => ("back", json!({ "timeoutMs": 30000 })),
        "browser.forward" => ("forward", json!({ "timeoutMs": 30000 })),
        _ => ("nav_state", json!({})),
    };

    let op_result = match manager.request(&session, op, op_params).await {
        Ok(value) => value,
        Err(err) => {
            // The runtime process is unusable; drop it so the next command
            // respawns, and surface a friendly error on the session.
            manager.drop_session(&session_id);
            let detail = format!("{err:#}");
            mark_browser_session_runtime_error(
                database,
                &session_id,
                &tab_id,
                &target_url,
                viewport_w,
                viewport_h,
                command_type,
                command_created_at_ms,
                &detail,
            )
            .await?;
            return Err(err);
        }
    };

    // A failed op (e.g. invalid URL) is reported as navigation_error but does
    // not tear the session down; we still capture whatever the page shows.
    let navigation_error = if op_result.get("ok").and_then(Value::as_bool) == Some(false) {
        op_result
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        None
    };
    let nav = op_result.get("nav").cloned().unwrap_or(Value::Null);
    let final_url = nav
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&target_url)
        .to_string();
    let title = nav
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Remote Browser")
        .to_string();
    let can_go_back = nav
        .get("can_go_back")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let can_go_forward = nav
        .get("can_go_forward")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if has_newer_browser_runtime_command(database, &session_id, command_created_at_ms).await? {
        mark_browser_runtime_command_completed(
            database,
            document,
            accepted,
            json!({
                "ok": true,
                "browser_stream": "rxdb",
                "session_id": session_id,
                "tab_id": tab_id,
                "url": final_url,
                "title": title,
                "superseded_by_newer_command": true
            }),
        )
        .await?;
        return Ok(());
    }

    let screenshot = match manager.request(&session, "screenshot", json!({})).await {
        Ok(value) => value,
        Err(err) => {
            manager.drop_session(&session_id);
            let detail = format!("{err:#}");
            mark_browser_session_runtime_error(
                database,
                &session_id,
                &tab_id,
                &final_url,
                viewport_w,
                viewport_h,
                command_type,
                command_created_at_ms,
                &detail,
            )
            .await?;
            return Err(err);
        }
    };
    let data = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("base64"))
        .and_then(Value::as_str)
        .context("browser runtime did not return screenshot data")?
        .to_string();
    let mime_type = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("mimeType"))
        .and_then(Value::as_str)
        .unwrap_or("image/png")
        .to_string();
    let next_seq = existing_session
        .get("last_frame_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + 1;
    let frame_id = format!("browser_frame_{}_{}", session_id, next_seq);
    let frame_hash = browser_frame_hash(&data);
    let size_bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map(|bytes| bytes.len() as u64)
        .unwrap_or_else(|_| data.len() as u64);
    upsert_browser_frame(
        database,
        &frame_id,
        &session_id,
        &tab_id,
        next_seq,
        &mime_type,
        &data,
        viewport_w,
        viewport_h,
        size_bytes,
        &frame_hash,
    )
    .await?;
    upsert_browser_tab(
        database,
        &tab_id,
        &session_id,
        &title,
        &final_url,
        "active",
        false,
        can_go_back,
        can_go_forward,
        Some(&frame_id),
        next_seq,
    )
    .await?;
    upsert_browser_session(
        database,
        &session_id,
        &tab_id,
        "active",
        "active",
        &final_url,
        &title,
        viewport_w,
        viewport_h,
        Some(&frame_id),
        next_seq,
        command_type,
        command_created_at_ms,
        navigation_error.as_deref(),
    )
    .await?;
    mark_browser_runtime_command_completed(
        database,
        document,
        accepted,
        json!({
            "ok": true,
            "browser_stream": "rxdb",
            "session_id": session_id,
            "tab_id": tab_id,
            "frame_id": frame_id,
            "url": final_url,
            "title": title,
            "frame_hash": frame_hash,
            "size_bytes": size_bytes,
            "can_go_back": can_go_back,
            "can_go_forward": can_go_forward,
            "navigation_error": navigation_error
        }),
    )
    .await?;
    Ok(())
}

fn browser_runtime_reference_dir(root: &Path) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CTOX_WEB_BROWSER_REFERENCE_DIR").map(PathBuf::from) {
        return Some(path);
    }
    let root_candidate = root.join("runtime/browser/interactive-reference");
    if root_candidate.join("package.json").exists() || root_candidate.join("node_modules").is_dir()
    {
        return Some(root_candidate);
    }
    let home_candidate = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".local/state/ctox/browser/interactive-reference"));
    if let Some(path) = home_candidate {
        if path.join("package.json").exists() || path.join("node_modules").is_dir() {
            return Some(path);
        }
    }
    None
}

/// Record a runtime failure on a session/tab in user-friendly terms without
/// tearing down the row, so the UI can show "needs attention" instead of a
/// silent stall.
#[allow(clippy::too_many_arguments)]
async fn mark_browser_session_runtime_error(
    database: &Arc<RxDatabase>,
    session_id: &str,
    tab_id: &str,
    url: &str,
    viewport_w: u64,
    viewport_h: u64,
    command_type: &str,
    command_created_at_ms: u64,
    detail: &str,
) -> anyhow::Result<()> {
    let existing_session = find_browser_document(database, "browser_sessions", session_id).await?;
    let existing_tab = find_browser_document(database, "browser_tabs", tab_id).await?;
    let title = existing_tab
        .get("title")
        .or_else(|| existing_session.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Remote Browser")
        .to_string();
    let frame_id = existing_session
        .get("active_frame_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let frame_seq = existing_session
        .get("last_frame_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    upsert_browser_tab(
        database,
        tab_id,
        session_id,
        &title,
        url,
        "error",
        false,
        false,
        false,
        frame_id.as_deref(),
        frame_seq,
    )
    .await?;
    upsert_browser_session(
        database,
        session_id,
        tab_id,
        "error",
        "error",
        url,
        &title,
        viewport_w,
        viewport_h,
        frame_id.as_deref(),
        frame_seq,
        command_type,
        command_created_at_ms,
        Some(detail),
    )
    .await?;
    Ok(())
}

/// Background loop that keeps live browser sessions responsive: it replays
/// pending input events against the real page, refreshes frames after input,
/// and garbage-collects expired frames. Runs under the shared write lock so it
/// never races the command consumer's RxDB writes.
async fn browser_runtime_maintenance_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let _ = root;
    loop {
        {
            let _guard = database_write_lock.lock().await;
            let _browser_guard = BROWSER_RUNTIME_COMMAND_LOCK.lock().await;
            if let Err(err) = run_browser_runtime_maintenance(&database).await {
                eprintln!("[business-os] browser runtime maintenance failed: {err:#}");
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

async fn run_browser_runtime_maintenance(database: &Arc<RxDatabase>) -> anyhow::Result<()> {
    let manager = browser_runtime_manager();
    for session_id in manager.active_session_ids() {
        if let Err(err) = drain_browser_session_inputs(database, &session_id).await {
            eprintln!("[business-os] browser input drain failed for {session_id}: {err:#}");
        }
    }
    gc_expired_browser_frames(database).await?;
    Ok(())
}

/// Replay all pending `browser_input_events` for one session against its live
/// page, mark them consumed/failed, and refresh the frame if anything applied.
async fn drain_browser_session_inputs(
    database: &Arc<RxDatabase>,
    session_id: &str,
) -> anyhow::Result<()> {
    let manager = browser_runtime_manager();
    let Some(session) = manager.get(session_id) else {
        return Ok(());
    };

    let events_collection = database
        .collection("browser_input_events")
        .context("browser_input_events collection is not registered")?;
    let pending = events_collection
        .find(Some(MangoQuery {
            selector: Some(json!({
                "session_id": { "$eq": session_id },
                "status": { "$eq": "pending" }
            })),
            sort: Some(vec![[("seq".to_string(), "asc".to_string())]
                .into_iter()
                .collect()]),
            limit: Some(64),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query pending browser_input_events: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec pending browser_input_events query: {err}"))?;
    let Some(rows) = pending.as_array() else {
        return Ok(());
    };
    if rows.is_empty() {
        return Ok(());
    }

    let mut events = Vec::with_capacity(rows.len());
    let mut max_seq = 0u64;
    for row in rows {
        let seq = row.get("seq").and_then(Value::as_u64).unwrap_or(0);
        max_seq = max_seq.max(seq);
        events.push(json!({
            "type": row.get("type").and_then(Value::as_str).unwrap_or_default(),
            "x": row.get("x").and_then(Value::as_f64).unwrap_or(0.0),
            "y": row.get("y").and_then(Value::as_f64).unwrap_or(0.0),
            "button": row.get("button").and_then(Value::as_str).unwrap_or("left"),
            "buttons": row.get("buttons").and_then(Value::as_u64).unwrap_or(0),
            "dx": row.get("dx").and_then(Value::as_f64).unwrap_or(0.0),
            "dy": row.get("dy").and_then(Value::as_f64).unwrap_or(0.0),
            "key": row.get("key").and_then(Value::as_str).unwrap_or_default(),
            "code": row.get("code").and_then(Value::as_str).unwrap_or_default(),
            "text": row.get("text").and_then(Value::as_str).unwrap_or_default()
        }));
    }

    let response = match manager
        .request(&session, "input", json!({ "events": events }))
        .await
    {
        Ok(value) => value,
        Err(err) => {
            // Process is dead: drop it, fail the batch, surface the error.
            manager.drop_session(session_id);
            let now = now_ms() as u64;
            for row in rows {
                if let Some(id) = row.get("id").and_then(Value::as_str) {
                    let mut next = row.clone();
                    if let Some(obj) = next.as_object_mut() {
                        obj.insert("status".to_string(), Value::String("failed".to_string()));
                        obj.insert("error".to_string(), Value::String(format!("{err:#}")));
                        obj.insert("updated_at_ms".to_string(), Value::from(now));
                    }
                    let _ = id;
                    events_collection
                        .incremental_upsert(next)
                        .await
                        .map_err(|e| anyhow::anyhow!("mark input event failed: {e}"))?;
                }
            }
            let tab_id = find_browser_document(database, "browser_sessions", session_id)
                .await?
                .get("current_tab_id")
                .and_then(Value::as_str)
                .unwrap_or("browser_tab_default")
                .to_string();
            mark_browser_session_runtime_error(
                database,
                session_id,
                &tab_id,
                "",
                session.viewport_w,
                session.viewport_h,
                "browser.input",
                now,
                &format!("{err:#}"),
            )
            .await?;
            return Ok(());
        }
    };

    let ok = response.get("ok").and_then(Value::as_bool) == Some(true);
    let now = now_ms() as u64;
    for row in rows {
        let mut next = row.clone();
        if let Some(obj) = next.as_object_mut() {
            if ok {
                obj.insert("status".to_string(), Value::String("consumed".to_string()));
                obj.insert("consumed_at_ms".to_string(), Value::from(now));
            } else {
                obj.insert("status".to_string(), Value::String("failed".to_string()));
                obj.insert(
                    "error".to_string(),
                    Value::String(
                        response
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("input replay failed")
                            .to_string(),
                    ),
                );
            }
            obj.insert("updated_at_ms".to_string(), Value::from(now));
        }
        events_collection
            .incremental_upsert(next)
            .await
            .map_err(|err| anyhow::anyhow!("mark input event consumed: {err}"))?;
    }

    if ok {
        let nav = response.get("nav").cloned().unwrap_or(Value::Null);
        capture_and_store_browser_frame(database, &session, session_id, Some(&nav)).await?;
        update_browser_session_input_state(database, session_id, max_seq).await?;
    }
    Ok(())
}

/// Capture a fresh frame from the live page and persist it plus the derived
/// tab/session navigation state. `nav` may carry the most recent navigation
/// snapshot; otherwise it is read from the screenshot response.
async fn capture_and_store_browser_frame(
    database: &Arc<RxDatabase>,
    session: &Arc<super::browser_runtime::LiveBrowserSession>,
    session_id: &str,
    nav_hint: Option<&Value>,
) -> anyhow::Result<()> {
    let manager = browser_runtime_manager();
    let screenshot = manager.request(session, "screenshot", json!({})).await?;
    let data = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("base64"))
        .and_then(Value::as_str)
        .context("browser runtime did not return screenshot data")?
        .to_string();
    let mime_type = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("mimeType"))
        .and_then(Value::as_str)
        .unwrap_or("image/png")
        .to_string();
    let nav = nav_hint
        .cloned()
        .filter(|value| !value.is_null())
        .or_else(|| screenshot.get("nav").cloned())
        .unwrap_or(Value::Null);

    let session_doc = find_browser_document(database, "browser_sessions", session_id).await?;
    let tab_id = session_doc
        .get("current_tab_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("browser_tab_default")
        .to_string();
    let url = nav
        .get("url")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| session_doc.get("current_url").and_then(Value::as_str))
        .unwrap_or("about:blank")
        .to_string();
    let title = nav
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| session_doc.get("title").and_then(Value::as_str))
        .unwrap_or("Remote Browser")
        .to_string();
    let can_go_back = nav
        .get("can_go_back")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let can_go_forward = nav
        .get("can_go_forward")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let next_seq = session_doc
        .get("last_frame_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + 1;
    let frame_id = format!("browser_frame_{}_{}", session_id, next_seq);
    let frame_hash = browser_frame_hash(&data);
    let size_bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map(|bytes| bytes.len() as u64)
        .unwrap_or_else(|_| data.len() as u64);
    upsert_browser_frame(
        database,
        &frame_id,
        session_id,
        &tab_id,
        next_seq,
        &mime_type,
        &data,
        session.viewport_w,
        session.viewport_h,
        size_bytes,
        &frame_hash,
    )
    .await?;
    upsert_browser_tab(
        database,
        &tab_id,
        session_id,
        &title,
        &url,
        "active",
        false,
        can_go_back,
        can_go_forward,
        Some(&frame_id),
        next_seq,
    )
    .await?;
    upsert_browser_session(
        database,
        session_id,
        &tab_id,
        "active",
        "active",
        &url,
        &title,
        session.viewport_w,
        session.viewport_h,
        Some(&frame_id),
        next_seq,
        "browser.input",
        now_ms() as u64,
        None,
    )
    .await?;
    Ok(())
}

/// Recompute `last_input_seq` and the live pending input count on a session
/// after a drain pass.
async fn update_browser_session_input_state(
    database: &Arc<RxDatabase>,
    session_id: &str,
    last_input_seq: u64,
) -> anyhow::Result<()> {
    let events_collection = database
        .collection("browser_input_events")
        .context("browser_input_events collection is not registered")?;
    let pending = events_collection
        .find(Some(MangoQuery {
            selector: Some(json!({
                "session_id": { "$eq": session_id },
                "status": { "$eq": "pending" }
            })),
            limit: Some(512),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("count pending browser_input_events: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec count pending browser_input_events: {err}"))?;
    let pending_count = pending.as_array().map(|rows| rows.len()).unwrap_or(0) as u64;

    let sessions = database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?;
    let existing = find_browser_document(database, "browser_sessions", session_id).await?;
    if !existing.is_object() {
        return Ok(());
    }
    let mut next = existing;
    if let Some(obj) = next.as_object_mut() {
        obj.remove("_rev");
        obj.remove("_meta");
        let prev = obj
            .get("last_input_seq")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        obj.insert(
            "last_input_seq".to_string(),
            Value::from(prev.max(last_input_seq)),
        );
        obj.insert(
            "pending_input_count".to_string(),
            Value::from(pending_count),
        );
        obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    }
    sessions
        .incremental_upsert(next)
        .await
        .map_err(|err| anyhow::anyhow!("update browser session input state: {err}"))?;
    Ok(())
}

/// Remove expired frames so `browser_frames` does not grow without bound.
async fn gc_expired_browser_frames(database: &Arc<RxDatabase>) -> anyhow::Result<()> {
    let now = now_ms() as u64;
    let frames = database
        .collection("browser_frames")
        .context("browser_frames collection is not registered")?;
    let expired = frames
        .find(Some(MangoQuery {
            selector: Some(json!({ "expires_at_ms": { "$lt": now } })),
            limit: Some(256),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query expired browser_frames: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec expired browser_frames query: {err}"))?;
    let Some(rows) = expired.as_array() else {
        return Ok(());
    };
    let ids: Vec<String> = rows
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    frames
        .bulk_remove_by_ids(ids)
        .await
        .map_err(|err| anyhow::anyhow!("remove expired browser_frames: {err}"))?;
    Ok(())
}

/// On peer startup, no live processes exist yet. Any session row left `active`
/// from a previous run is stale; mark it disconnected so the UI does not show a
/// dead live session as running.
async fn recover_stale_browser_sessions(database: &Arc<RxDatabase>) -> anyhow::Result<()> {
    let sessions = database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?;
    let active = sessions
        .find(Some(MangoQuery {
            selector: Some(json!({ "status": { "$ne": "stopped" } })),
            limit: Some(128),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query active browser_sessions: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec active browser_sessions query: {err}"))?;
    let Some(rows) = active.as_array() else {
        return Ok(());
    };
    let manager = browser_runtime_manager();
    let now = now_ms() as u64;
    for row in rows {
        let Some(session_id) = row.get("id").and_then(Value::as_str) else {
            continue;
        };
        if manager.has_session(session_id) {
            continue;
        }
        let status = row
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if status == "error" || status == "disconnected" {
            continue;
        }
        let mut next = row.clone();
        if let Some(obj) = next.as_object_mut() {
            obj.remove("_rev");
            obj.remove("_meta");
            obj.insert(
                "status".to_string(),
                Value::String("disconnected".to_string()),
            );
            obj.insert(
                "runtime_status".to_string(),
                Value::String("disconnected".to_string()),
            );
            obj.insert("updated_at_ms".to_string(), Value::from(now));
        }
        sessions
            .incremental_upsert(next)
            .await
            .map_err(|err| anyhow::anyhow!("mark stale browser session disconnected: {err}"))?;
    }
    Ok(())
}

async fn find_browser_document(
    database: &Arc<RxDatabase>,
    collection_name: &str,
    id: &str,
) -> anyhow::Result<Value> {
    let collection = database
        .collection(collection_name)
        .with_context(|| format!("{collection_name} collection is not registered"))?;
    let existing = collection
        .find_one(Some(MangoQuery {
            selector: Some(json!({ "id": { "$eq": id } })),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query {collection_name} {id}: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec {collection_name} {id} query: {err}"))?;
    Ok(existing
        .is_object()
        .then_some(existing)
        .unwrap_or(Value::Null))
}

async fn has_newer_browser_runtime_command(
    database: &Arc<RxDatabase>,
    session_id: &str,
    command_created_at_ms: u64,
) -> anyhow::Result<bool> {
    let collection = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let rows = collection
        .find(Some(MangoQuery {
            selector: Some(json!({ "module": { "$eq": "browser" } })),
            limit: Some(500),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query newer browser commands: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec newer browser commands query: {err}"))?;
    let Some(commands) = rows.as_array() else {
        return Ok(false);
    };
    Ok(commands.iter().any(|command| {
        let Some(candidate_type) = command
            .get("command_type")
            .or_else(|| command.get("type"))
            .and_then(Value::as_str)
        else {
            return false;
        };
        if !is_browser_runtime_command(candidate_type) {
            return false;
        }
        let candidate_session_id = command
            .get("payload")
            .and_then(|payload| payload.get("session_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("browser_session_default");
        if candidate_session_id != session_id {
            return false;
        }
        let candidate_created_at_ms = command
            .get("created_at_ms")
            .or_else(|| command.get("updated_at_ms"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        candidate_created_at_ms > command_created_at_ms
    }))
}

#[allow(clippy::too_many_arguments)]
async fn upsert_browser_session(
    database: &Arc<RxDatabase>,
    session_id: &str,
    tab_id: &str,
    status: &str,
    runtime_status: &str,
    url: &str,
    title: &str,
    viewport_w: u64,
    viewport_h: u64,
    frame_id: Option<&str>,
    frame_seq: u64,
    command_type: &str,
    command_created_at_ms: u64,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let now = now_ms() as u64;
    let existing = find_browser_document(database, "browser_sessions", session_id).await?;
    let existing_command_created_at_ms = existing
        .get("payload")
        .and_then(|payload| payload.get("last_command_created_at_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if existing_command_created_at_ms > command_created_at_ms {
        return Ok(());
    }
    // Carry forward input bookkeeping so a lifecycle/navigation write does not
    // clobber counts maintained by the input-drain loop or the UI.
    let preserved_last_input_seq = existing
        .get("last_input_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let preserved_pending_input_count = existing
        .get("pending_input_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut payload = json!({
        "browser_stream": "rxdb",
        "last_command_type": command_type,
        "last_command_created_at_ms": command_created_at_ms,
        "runtime": "ctox-web-stack",
        "updated_by": "native-rxdb-peer"
    });
    if let Some(error) = error {
        payload["error"] = Value::String(error.to_string());
    }
    let doc = json!({
        "id": session_id,
        "owner_user_id": "ctox",
        "controller_user_id": "ctox",
        "status": status,
        "runtime_status": runtime_status,
        "current_tab_id": tab_id,
        "current_url": url,
        "title": title,
        "viewport_w": viewport_w,
        "viewport_h": viewport_h,
        "device_scale_factor": 1,
        "frame_rate_target": 0,
        "active_frame_id": frame_id.unwrap_or_default(),
        "last_frame_seq": frame_seq,
        "last_input_seq": preserved_last_input_seq,
        "pending_input_count": preserved_pending_input_count,
        "error": error.unwrap_or_default(),
        "payload": payload,
        "created_at_ms": now,
        "updated_at_ms": now
    });
    database
        .collection("browser_sessions")
        .context("browser_sessions collection is not registered")?
        .incremental_upsert(doc)
        .await
        .map_err(|err| anyhow::anyhow!("upsert browser session {session_id}: {err}"))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upsert_browser_tab(
    database: &Arc<RxDatabase>,
    tab_id: &str,
    session_id: &str,
    title: &str,
    url: &str,
    status: &str,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
    frame_id: Option<&str>,
    frame_seq: u64,
) -> anyhow::Result<()> {
    let now = now_ms() as u64;
    let doc = json!({
        "id": tab_id,
        "session_id": session_id,
        "title": title,
        "url": url,
        "status": status,
        "loading": loading,
        "active": true,
        "can_go_back": can_go_back,
        "can_go_forward": can_go_forward,
        "frame_seq": frame_seq,
        "last_frame_id": frame_id.unwrap_or_default(),
        "last_frame_at_ms": frame_id.map(|_| now).unwrap_or(0),
        "error": "",
        "payload": {
            "browser_stream": "rxdb",
            "updated_by": "native-rxdb-peer"
        },
        "created_at_ms": now,
        "updated_at_ms": now
    });
    database
        .collection("browser_tabs")
        .context("browser_tabs collection is not registered")?
        .incremental_upsert(doc)
        .await
        .map_err(|err| anyhow::anyhow!("upsert browser tab {tab_id}: {err}"))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upsert_browser_frame(
    database: &Arc<RxDatabase>,
    frame_id: &str,
    session_id: &str,
    tab_id: &str,
    seq: u64,
    mime_type: &str,
    data: &str,
    width: u64,
    height: u64,
    size_bytes: u64,
    frame_hash: &str,
) -> anyhow::Result<()> {
    let now = now_ms() as u64;
    let doc = json!({
        "id": frame_id,
        "session_id": session_id,
        "tab_id": tab_id,
        "seq": seq,
        "mime_type": mime_type,
        "encoding": "base64",
        "data": data,
        "width": width,
        "height": height,
        "viewport_w": width,
        "viewport_h": height,
        "quality": 100,
        "size_bytes": size_bytes,
        "frame_hash": frame_hash,
        "captured_at_ms": now,
        "expires_at_ms": now + 15 * 60 * 1000,
        "updated_at_ms": now
    });
    database
        .collection("browser_frames")
        .context("browser_frames collection is not registered")?
        .incremental_upsert(doc)
        .await
        .map_err(|err| anyhow::anyhow!("upsert browser frame {frame_id}: {err}"))?;
    Ok(())
}

async fn mark_browser_runtime_command_completed(
    database: &Arc<RxDatabase>,
    document: &Value,
    accepted: &Value,
    result: Value,
) -> anyhow::Result<()> {
    let command_id = document
        .get("command_id")
        .or_else(|| document.get("id"))
        .and_then(Value::as_str)
        .context("browser command is missing id")?;
    let mut next = document.clone();
    if let Some(object) = next.as_object_mut() {
        object.insert("status".to_string(), Value::String("completed".to_string()));
        object.insert(
            "task_status".to_string(),
            Value::String("completed".to_string()),
        );
        if let Some(task_id) = accepted.get("task_id") {
            object.insert("task_id".to_string(), task_id.clone());
        }
        object.insert("result".to_string(), result);
        object.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    }
    database
        .collection("business_commands")
        .context("business_commands collection is not registered")?
        .incremental_upsert(next)
        .await
        .map_err(|err| anyhow::anyhow!("complete browser command {command_id}: {err}"))?;
    Ok(())
}

async fn mark_browser_runtime_command_failed(
    database: &Arc<RxDatabase>,
    command_id: &str,
    payload: &Value,
    error: &anyhow::Error,
) -> anyhow::Result<()> {
    let session_id = payload
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("browser_session_default");
    let tab_id = payload
        .get("tab_id")
        .and_then(Value::as_str)
        .unwrap_or("browser_tab_default");
    let message = error.to_string();
    upsert_browser_session(
        database,
        session_id,
        tab_id,
        "failed",
        "failed",
        payload
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("https://example.com"),
        "Remote Browser",
        payload
            .get("viewport_w")
            .and_then(Value::as_u64)
            .unwrap_or(1280),
        payload
            .get("viewport_h")
            .and_then(Value::as_u64)
            .unwrap_or(720),
        None,
        0,
        "browser.runtime.failed",
        now_ms() as u64,
        Some(&message),
    )
    .await?;
    let commands = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let existing = commands
        .find_one(Some(MangoQuery {
            selector: Some(json!({ "id": { "$eq": command_id } })),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query failed browser command {command_id}: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec failed browser command {command_id}: {err}"))?;
    let mut next = if existing.is_object() {
        existing
    } else {
        json!({ "id": command_id, "command_id": command_id })
    };
    if let Some(object) = next.as_object_mut() {
        object.insert("status".to_string(), Value::String("failed".to_string()));
        object.insert(
            "task_status".to_string(),
            Value::String("failed".to_string()),
        );
        object.insert("error".to_string(), Value::String(message.clone()));
        object.insert(
            "result".to_string(),
            json!({
                "ok": false,
                "browser_stream": "rxdb",
                "error": message
            }),
        );
        object.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    }
    commands
        .incremental_upsert(next)
        .await
        .map_err(|err| anyhow::anyhow!("mark browser command {command_id} failed: {err}"))?;
    Ok(())
}

fn normalize_browser_runtime_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

fn browser_frame_hash(data: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data.as_bytes());
    let digest = hasher.finalize();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

async fn sync_business_users_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    let root = root.to_path_buf();
    let pulled = tokio::task::spawn_blocking(move || store::pull_business_users_for_rxdb(&root))
        .await
        .context("join native business users projection load")??;
    let documents = pulled
        .get("documents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // Acquire write lock specifically for database writes
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    let users = database
        .collection("business_users")
        .context("business_users collection is not registered")?;
    let count = documents.len();
    for mut document in documents {
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object.insert("is_deleted".to_string(), Value::Bool(false));
        }
        users
            .incremental_upsert(document)
            .await
            .map_err(|err| anyhow::anyhow!("upsert business user projection: {err}"))?;
    }
    Ok(count)
}

async fn sync_runtime_settings_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<()> {
    let root = root.to_path_buf();
    let mut document = tokio::task::spawn_blocking(move || store::runtime_settings_for_rxdb(&root))
        .await
        .context("join native runtime settings projection load")??;
    if let Some(object) = document.as_object_mut() {
        object.remove("_rev");
        object.remove("_meta");
        object.insert("_deleted".to_string(), Value::Bool(false));
        object.insert("is_deleted".to_string(), Value::Bool(false));
    }

    // Acquire write lock specifically for database writes
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    let runtime_settings = database
        .collection("ctox_runtime_settings")
        .context("ctox_runtime_settings collection is not registered")?;
    runtime_settings
        .incremental_upsert(document)
        .await
        .map_err(|err| anyhow::anyhow!("upsert runtime settings projection: {err}"))?;
    Ok(())
}

async fn sync_module_catalog_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<()> {
    let root = root.to_path_buf();
    let mut document = tokio::task::spawn_blocking(move || store::module_catalog_for_rxdb(&root))
        .await
        .context("join native module catalog projection load")??;
    if let Some(object) = document.as_object_mut() {
        object.remove("_rev");
        object.remove("_meta");
        object.insert("_deleted".to_string(), Value::Bool(false));
        object.insert("is_deleted".to_string(), Value::Bool(false));
    }

    // Acquire write lock specifically for database writes
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    let module_catalog = database
        .collection("business_module_catalog")
        .context("business_module_catalog collection is not registered")?;
    module_catalog
        .incremental_upsert(document)
        .await
        .map_err(|err| anyhow::anyhow!("upsert module catalog projection: {err}"))?;
    Ok(())
}

async fn sync_ticket_state_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    let root = root.to_path_buf();
    let projection = tokio::task::spawn_blocking(move || {
        tickets::business_os_ticket_projection_documents(&root, TICKET_STATE_SYNC_LIMIT)
    })
    .await
    .context("join native ticket state projection load")??;

    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    let mut count = 0usize;
    for collection_name in tickets::BUSINESS_OS_TICKET_COLLECTIONS {
        let collection = database
            .collection(collection_name)
            .with_context(|| format!("{collection_name} collection is not registered"))?;
        for mut document in projection
            .get(*collection_name)
            .cloned()
            .unwrap_or_default()
        {
            if let Some(object) = document.as_object_mut() {
                object.remove("_rev");
                object.remove("_meta");
                object.insert("_deleted".to_string(), Value::Bool(false));
                object.insert("is_deleted".to_string(), Value::Bool(false));
            }
            collection
                .incremental_upsert(document)
                .await
                .map_err(|err| {
                    anyhow::anyhow!("upsert {collection_name} ticket projection: {err}")
                })?;
            count += 1;
        }
    }
    Ok(count)
}

async fn sync_business_record_projections_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
    since_by_collection: &mut HashMap<String, i64>,
) -> anyhow::Result<usize> {
    let collections = business_record_projection_collections();
    let root = root.to_path_buf();
    let pull_jobs = collections
        .iter()
        .map(|collection| {
            let collection = collection.clone();
            let root = root.clone();
            let since_ms = *since_by_collection.get(&collection).unwrap_or(&0);
            tokio::task::spawn_blocking(move || {
                let pulled = store::pull_collection_records(
                    &root,
                    &collection,
                    Some(since_ms),
                    Some(BUSINESS_RECORD_PROJECTION_SYNC_LIMIT),
                )?;
                Ok::<_, anyhow::Error>((collection, since_ms, pulled))
            })
        })
        .collect::<Vec<_>>();

    let mut pulled_collections = Vec::with_capacity(pull_jobs.len());
    for job in pull_jobs {
        pulled_collections.push(
            job.await
                .context("join native business record projection load")??,
        );
    }

    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    let mut count = 0usize;
    for (collection_name, since_ms, pulled) in pulled_collections {
        let documents = pulled
            .get("documents")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if documents.is_empty() {
            since_by_collection
                .entry(collection_name)
                .or_insert(since_ms);
            continue;
        }
        let collection = database
            .collection(&collection_name)
            .with_context(|| format!("{collection_name} collection is not registered"))?;
        let mut max_updated_at_ms = since_ms;
        for mut document in documents {
            if let Some(object) = document.as_object_mut() {
                object.remove("_rev");
                object.remove("_meta");
                object
                    .entry("is_deleted".to_string())
                    .or_insert_with(|| Value::Bool(false));
                if let Some(updated_at_ms) = object.get("updated_at_ms").and_then(Value::as_i64) {
                    max_updated_at_ms = max_updated_at_ms.max(updated_at_ms);
                }
            }
            upsert_business_record_projection_document(&collection, document)
                .await
                .map_err(|err| {
                    anyhow::anyhow!("upsert {collection_name} business record projection: {err}")
                })?;
            count += 1;
        }
        since_by_collection.insert(collection_name, max_updated_at_ms.saturating_add(1));
    }
    Ok(count)
}

async fn upsert_business_record_projection(
    root: PathBuf,
    database: &Arc<RxDatabase>,
    collection_name: &'static str,
    record_id: String,
) -> anyhow::Result<()> {
    let document = tokio::task::spawn_blocking(move || {
        let pulled = store::pull_collection_records(&root, collection_name, Some(0), Some(2_000))?;
        let document = pulled
            .get("documents")
            .and_then(Value::as_array)
            .and_then(|documents| {
                documents
                    .iter()
                    .find(|document| {
                        document.get("id").and_then(Value::as_str) == Some(record_id.as_str())
                    })
                    .cloned()
            });
        Ok::<_, anyhow::Error>(document)
    })
    .await
    .with_context(|| format!("join native {collection_name} projection load"))??;

    let Some(mut document) = document else {
        return Ok(());
    };
    if let Some(object) = document.as_object_mut() {
        object.remove("_rev");
        object.remove("_meta");
    }
    let collection = database
        .collection(collection_name)
        .with_context(|| format!("{collection_name} collection is not registered"))?;
    if matches!(
        collection_name,
        "business_module_releases" | "business_module_acl"
    ) {
        collection
            .upsert(document)
            .await
            .map_err(|err| anyhow::anyhow!("upsert {collection_name} projection: {err}"))?;
    } else {
        upsert_business_record_projection_document(&collection, document)
            .await
            .map_err(|err| anyhow::anyhow!("upsert {collection_name} projection: {err}"))?;
    }
    Ok(())
}

async fn upsert_business_record_projection_document(
    collection: &Arc<RxCollection>,
    document: Value,
) -> anyhow::Result<()> {
    match collection.incremental_upsert(document.clone()).await {
        Ok(_) => Ok(()),
        Err(err) if is_recoverable_projection_write_error(&err) => match collection.upsert(document.clone()).await
        {
            Ok(_) => Ok(()),
            Err(_) => repair_projection_document_envelope_and_upsert(collection, document)
                .await
                .map_err(|fallback_err| {
                    anyhow::anyhow!(
                        "projection upsert fallback after document cache envelope repair failed: {fallback_err}"
                    )
                }),
        },
        Err(err) => Err(anyhow::anyhow!("{err}")),
    }
}

fn is_doc_cache_revision_error(error: &rxdb::rx_error::RxError) -> bool {
    matches!(error.code(), "DOC_CACHE_REV" | "DOC_CACHE_LWT" | "UTL2")
        || error.to_string().contains("DOC_CACHE_REV")
        || error.to_string().contains("DOC_CACHE_LWT")
        || error.to_string().contains("UTL2")
}

// A tombstone re-delete (incoming `_deleted:true` over an existing tombstone with
// a divergent `_rev`) surfaces as a 409 `CONFLICT`, because `incremental_upsert`
// queries exclude deleted docs and fall through to `insert`, which the storage
// rejects as a duplicate primary key. Route it through the same upsert/envelope
// repair fallback, which rebases the write onto the existing tombstone instead of
// failing the projection sync.
fn is_recoverable_projection_write_error(error: &rxdb::rx_error::RxError) -> bool {
    is_doc_cache_revision_error(error)
        || error.code() == "CONFLICT"
        || error.to_string().contains("CONFLICT")
}

async fn repair_projection_document_envelope_and_upsert(
    collection: &Arc<RxCollection>,
    document: Value,
) -> anyhow::Result<()> {
    let schema = collection
        .schema_required()
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let primary_path = schema.primary_path.clone();
    let write_data = fill_object_data_before_insert(schema, document)
        .map_err(|err| anyhow::anyhow!("fill projection document envelope: {err}"))?;
    let document_id = write_data
        .get(&primary_path)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("projection document missing primary key {primary_path}"))?
        .to_string();

    let existing = collection
        .storage_instance
        .find_documents_by_id(std::slice::from_ref(&document_id), true)
        .await
        .map_err(|err| anyhow::anyhow!("load existing projection document for repair: {err}"))?
        .into_iter()
        .next();

    let Some(previous) = existing else {
        collection
            .insert(write_data)
            .await
            .map(|_| ())
            .map_err(|err| anyhow::anyhow!("insert projection document after repair: {err}"))?;
        return Ok(());
    };

    let repaired_previous = fill_object_data_before_insert(schema, previous.clone())
        .map_err(|err| anyhow::anyhow!("repair existing projection document envelope: {err}"))?;
    let mut next = repaired_previous.clone();
    if let (Some(next_obj), Some(write_obj)) = (next.as_object_mut(), write_data.as_object()) {
        for (key, value) in write_obj {
            if matches!(key.as_str(), "_rev" | "_attachments") {
                continue;
            }
            next_obj.insert(key.clone(), value.clone());
        }
    } else {
        next = write_data;
    }

    let result = collection
        .storage_instance
        .bulk_write(
            vec![BulkWriteRow {
                previous: Some(repaired_previous),
                document: next,
            }],
            "business-os-projection-envelope-repair",
        )
        .await
        .map_err(|err| anyhow::anyhow!("write repaired projection document: {err}"))?;
    if let Some(err) = result.error.first() {
        anyhow::bail!("write repaired projection document conflict: {err:?}");
    }
    Ok(())
}

#[derive(Debug)]
struct ChannelStateProjection {
    accounts: Vec<Value>,
    pairing_states: Vec<Value>,
}

async fn sync_channel_state_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    let root = root.to_path_buf();
    let projection = tokio::task::spawn_blocking(move || load_channel_state_projection(&root))
        .await
        .context("join native channel state projection load")??;

    // Acquire write lock specifically for database writes
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    let accounts = database
        .collection("communication_accounts")
        .context("communication_accounts collection is not registered")?;
    let pairing_states = database
        .collection("channel_pairing_state")
        .context("channel_pairing_state collection is not registered")?;

    let now = now_ms();
    let mut synced = 0usize;
    let seen_account_keys: HashSet<String> = projection
        .accounts
        .iter()
        .filter_map(|document| {
            document
                .get("account_key")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();

    for mut document in projection.accounts {
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object.insert("is_deleted".to_string(), Value::Bool(false));
            object.insert("_deleted".to_string(), Value::Bool(false));
        }
        accounts
            .incremental_upsert(document)
            .await
            .map_err(|err| anyhow::anyhow!("upsert communication account projection: {err}"))?;
        synced += 1;
    }

    let existing_accounts = accounts
        .find(Some(MangoQuery {
            limit: Some(2_000),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query communication account tombstones: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec communication account tombstone query: {err}"))?;
    let existing_account_rows = existing_accounts
        .as_array()
        .cloned()
        .unwrap_or_else(Vec::new);
    for mut document in existing_account_rows {
        let Some(account_key) = document
            .get("account_key")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        if seen_account_keys.contains(&account_key) {
            continue;
        }
        if document
            .get("is_deleted")
            .or_else(|| document.get("_deleted"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object.insert("is_deleted".to_string(), Value::Bool(true));
            object.insert("_deleted".to_string(), Value::Bool(false));
            object.insert(
                "status".to_string(),
                Value::String("disconnected".to_string()),
            );
            object.insert("deleted_at_ms".to_string(), Value::from(now as u64));
            object.insert("updated_at_ms".to_string(), Value::from(now as u64));
        }
        accounts
            .incremental_upsert(document)
            .await
            .map_err(|err| anyhow::anyhow!("upsert communication account tombstone: {err}"))?;
        synced += 1;
    }

    for mut document in projection.pairing_states {
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
        }
        pairing_states
            .incremental_upsert(document)
            .await
            .map_err(|err| anyhow::anyhow!("upsert channel pairing state projection: {err}"))?;
        synced += 1;
    }

    Ok(synced)
}

fn load_channel_state_projection(root: &Path) -> anyhow::Result<ChannelStateProjection> {
    let pulled = channels::pull_communication_accounts_for_business_os(root, Some(0), Some(2_000))
        .context("pull communication account projection")?;
    let accounts = pulled
        .get("documents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut channel_ids: HashSet<String> = BUSINESS_OS_CHANNEL_IDS
        .iter()
        .map(|channel| (*channel).to_string())
        .collect();
    for document in &accounts {
        if let Some(channel) = document.get("channel").and_then(Value::as_str) {
            if !channel.trim().is_empty() {
                channel_ids.insert(channel.to_string());
            }
        }
    }

    let now = now_ms();
    let mut channel_ids: Vec<String> = channel_ids.into_iter().collect();
    channel_ids.sort();
    let pairing_states = channel_ids
        .into_iter()
        .map(|channel| {
            let state = channels::read_pairing_state_for_business_os(root, &channel);
            channel_pairing_state_document(channel, state, now)
        })
        .collect();

    Ok(ChannelStateProjection {
        accounts,
        pairing_states,
    })
}

fn channel_pairing_state_document(channel: String, state: Value, now: u128) -> Value {
    let status = state
        .get("status")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("idle");
    let mut document = json!({
        "channel": channel,
        "status": status,
        "updated_at_ms": now as u64,
    });
    let Some(object) = document.as_object_mut() else {
        return document;
    };
    for key in [
        "qr_payload",
        "qr_svg",
        "account_key",
        "last_inbound_ok_at",
        "last_outbound_ok_at",
        "error",
    ] {
        if let Some(value) = state.get(key) {
            if !value.is_null() {
                object.insert(key.to_string(), value.clone());
            }
        }
    }
    if let Some(step) = state
        .get("artifact")
        .and_then(|artifact| artifact.get("step"))
        .or_else(|| state.get("step"))
    {
        if !step.is_null() {
            object.insert("step".to_string(), step.clone());
        }
    }
    if let Some(artifact) = state.get("artifact") {
        if !artifact.is_null() {
            object.insert("artifact".to_string(), artifact.clone());
        }
    }
    document
}

async fn upsert_desktop_file_with_policy(
    database: &Arc<RxDatabase>,
    path: PathBuf,
    policy: DesktopFileContentPolicy,
) -> anyhow::Result<()> {
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    upsert_desktop_file_with_parent(
        database,
        path,
        policy,
        CTOX_DESKTOP_FOLDER_ID.to_string(),
        None,
    )
    .await
}

async fn upsert_desktop_file_with_parent(
    database: &Arc<RxDatabase>,
    path: PathBuf,
    policy: DesktopFileContentPolicy,
    parent_id: String,
    virtual_path: Option<String>,
) -> anyhow::Result<()> {
    let metadata = fs::metadata(&path)
        .with_context(|| format!("failed to read desktop file metadata {}", path.display()))?;
    if !metadata.is_file() {
        anyhow::bail!(
            "desktop file sync only supports regular files: {}",
            path.display()
        );
    }

    let now = now_ms();
    let file_id = desktop_file_id(&path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_string();
    let path_string = path.to_string_lossy().into_owned();
    let display_path = virtual_path.unwrap_or_else(|| path_string.clone());
    let modified_at_ms = metadata_modified_at_ms(&metadata);
    let (content_hash, content_generation_id, active_generation_id) =
        if policy == DesktopFileContentPolicy::Eager {
            let bytes = fs::read(&path)
                .with_context(|| format!("failed to read desktop file {}", path.display()))?;
            let content_hash = hex_sha256(&bytes);
            let generation_suffix = content_hash.get(..12).unwrap_or(content_hash.as_str());
            let generation_id = format!("gen_{now}_{generation_suffix}");
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let total = encoded.len().div_ceil(DESKTOP_FILE_CHUNK_SIZE).max(1);
            let chunks = database
                .collection("desktop_file_chunks")
                .context("desktop_file_chunks collection is not registered")?;

            let chunk_payloads: Vec<&str> = if encoded.is_empty() {
                vec![""]
            } else {
                encoded
                    .as_bytes()
                    .chunks(DESKTOP_FILE_CHUNK_SIZE)
                    .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
                    .collect()
            };
            for (idx, data) in chunk_payloads.into_iter().enumerate() {
                let chunk_hash = hex_sha256(data.as_bytes());
                chunks
                    .incremental_upsert(json!({
                        "id": format!("{file_id}_{generation_id}_{idx}"),
                        "file_id": file_id,
                        "generation_id": generation_id.clone(),
                        "content_hash": content_hash.clone(),
                        "content_hash_scheme": DESKTOP_FILE_CONTENT_HASH_SCHEME,
                        "idx": idx as u64,
                        "total": total as u64,
                        "encoding": "base64",
                        "data": data,
                        "chunk_hash": chunk_hash,
                        "chunk_hash_scheme": DESKTOP_FILE_CHUNK_HASH_SCHEME,
                        "size_bytes": data.len() as u64,
                        "created_at_ms": now,
                    }))
                    .await
                    .map_err(|err| anyhow::anyhow!("upsert desktop file chunk {idx}: {err}"))?;
            }
            (
                content_hash,
                Value::String(generation_id.clone()),
                Some(generation_id),
            )
        } else {
            (
                format!("mtime:{modified_at_ms}:size:{}", metadata.len()),
                Value::Null,
                None,
            )
        };

    ensure_ctox_desktop_folder(database, now).await?;
    let files = database
        .collection("desktop_files")
        .context("desktop_files collection is not registered")?;
    let now_u64 = u64::try_from(now).unwrap_or(u64::MAX);
    let content_synced_at_ms = if policy == DesktopFileContentPolicy::Eager {
        Value::from(now_u64)
    } else {
        Value::Null
    };
    let content_state = if policy == DesktopFileContentPolicy::Eager {
        "available"
    } else {
        "lazy"
    };
    files
        .incremental_upsert(json!({
            "id": file_id,
            "parent_id": parent_id,
            "path": path_string,
            "local_path": path_string,
            "virtual_path": display_path,
            "name": file_name,
            "kind": "file",
            "mime_type": mime_type_for_path(&path),
            "extension": extension,
            "size_bytes": metadata.len(),
            "owner_id": "ctox",
            "source": "ctox-core",
            "content_ref": file_id,
            "content_state": content_state,
            "content_hash": content_hash,
            "content_hash_scheme": DESKTOP_FILE_CONTENT_HASH_SCHEME,
            "content_generation_id": content_generation_id,
            "mtime_ms": modified_at_ms,
            "content_synced_at_ms": content_synced_at_ms,
            "sort_index": now,
            "is_deleted": false,
            "created_at_ms": now,
            "updated_at_ms": now,
        }))
        .await
        .map_err(|err| anyhow::anyhow!("upsert desktop file row: {err}"))?;

    if let Some(active_generation_id) = active_generation_id.as_deref() {
        prune_desktop_file_chunk_generations(database, &file_id, active_generation_id).await?;
    }

    Ok(())
}

async fn prune_desktop_file_chunk_generations(
    database: &Arc<RxDatabase>,
    file_id: &str,
    active_generation_id: &str,
) -> anyhow::Result<usize> {
    let chunks = database
        .collection("desktop_file_chunks")
        .context("desktop_file_chunks collection is not registered")?;
    let rows = chunks
        .find(Some(MangoQuery {
            selector: Some(json!({ "file_id": { "$eq": file_id } })),
            limit: Some(DESKTOP_FILE_CHUNK_CLEANUP_SCAN_LIMIT),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query desktop file chunks for cleanup: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec desktop file chunk cleanup query: {err}"))?;
    let chunk_rows = rows.as_array().cloned().unwrap_or_default();
    if chunk_rows.is_empty() {
        return Ok(0);
    }

    let mut latest_by_generation: HashMap<String, u64> = HashMap::new();
    for chunk in &chunk_rows {
        let generation = desktop_file_chunk_generation_key(chunk);
        let created_at = chunk
            .get("created_at_ms")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        latest_by_generation
            .entry(generation)
            .and_modify(|existing| *existing = (*existing).max(created_at))
            .or_insert(created_at);
    }

    if latest_by_generation.len() <= DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS {
        return Ok(0);
    }

    let mut generations: Vec<(String, u64)> = latest_by_generation.into_iter().collect();
    generations.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    let mut keep = HashSet::from([active_generation_id.to_string()]);
    for (generation, _) in generations {
        if keep.len() >= DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS {
            break;
        }
        keep.insert(generation);
    }

    let stale_chunks: Vec<Value> = chunk_rows
        .into_iter()
        .filter(|chunk| !keep.contains(&desktop_file_chunk_generation_key(chunk)))
        .filter(|chunk| chunk.get("id").and_then(Value::as_str).is_some())
        .collect();
    if stale_chunks.is_empty() {
        return Ok(0);
    }

    let removed = stale_chunks.len();
    let pruned_at_ms = now_ms();
    for mut chunk in stale_chunks {
        if let Some(object) = chunk.as_object_mut() {
            object.insert("data".to_string(), Value::String(String::new()));
            object.insert("size_bytes".to_string(), Value::from(0_u64));
            object.insert("_deleted".to_string(), Value::Bool(true));
            object.insert("pruned_at_ms".to_string(), Value::from(pruned_at_ms as u64));
            object.insert(
                "prune_reason".to_string(),
                Value::String("stale_generation".to_string()),
            );
        }
        chunks
            .incremental_upsert(chunk)
            .await
            .map_err(|err| anyhow::anyhow!("redact stale desktop file chunk: {err}"))?;
    }
    Ok(removed)
}

fn desktop_file_chunk_generation_key(chunk: &Value) -> String {
    chunk
        .get("generation_id")
        .and_then(Value::as_str)
        .filter(|generation| !generation.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let created_at = chunk
                .get("created_at_ms")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            format!("legacy_{created_at}")
        })
}

async fn ensure_ctox_desktop_folder(database: &Arc<RxDatabase>, now: u128) -> anyhow::Result<()> {
    ensure_ctox_desktop_folder_path(database, now, &[]).await?;
    Ok(())
}

async fn ensure_ctox_desktop_folder_path(
    database: &Arc<RxDatabase>,
    now: u128,
    components: &[String],
) -> anyhow::Result<String> {
    let files = database
        .collection("desktop_files")
        .context("desktop_files collection is not registered")?;
    let mut parent_id = "fs_root".to_string();
    let mut folder_id = CTOX_DESKTOP_FOLDER_ID.to_string();
    let mut virtual_path = CTOX_DESKTOP_FOLDER_PATH.to_string();
    let mut names = vec!["CTOX".to_string()];
    names.extend(
        components
            .iter()
            .filter(|component| !component.is_empty())
            .cloned(),
    );

    for (idx, name) in names.iter().enumerate() {
        if idx == 0 {
            folder_id = CTOX_DESKTOP_FOLDER_ID.to_string();
            virtual_path = CTOX_DESKTOP_FOLDER_PATH.to_string();
        } else {
            virtual_path = format!("{}/{}", virtual_path.trim_end_matches('/'), name);
            folder_id = desktop_folder_id(&virtual_path);
        }
        files
            .incremental_upsert(json!({
                "id": folder_id,
                "parent_id": parent_id,
                "path": virtual_path,
                "virtual_path": virtual_path,
                "local_path": "",
                "name": name,
                "kind": "folder",
                "mime_type": "",
                "extension": "",
                "size_bytes": 0,
                "owner_id": "ctox",
                "source": "ctox-core",
                "content_ref": "",
                "content_state": "directory",
                "content_hash": "",
                "mtime_ms": now,
                "content_synced_at_ms": now,
                "sort_index": if idx == 0 { 5 } else { now },
                "is_deleted": false,
                "created_at_ms": now,
                "updated_at_ms": now,
            }))
            .await
            .map_err(|err| anyhow::anyhow!("upsert CTOX desktop folder {virtual_path}: {err}"))?;
        parent_id = folder_id.clone();
    }
    Ok(folder_id)
}

async fn sync_desktop_file_index_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    sync_desktop_file_scan_roots_with_database(database, desktop_file_scan_roots(root)).await
}

async fn sync_desktop_file_scan_roots_with_database(
    database: &Arc<RxDatabase>,
    mut scan_roots: Vec<DesktopFileScanRoot>,
) -> anyhow::Result<usize> {
    let mut indexed = 0usize;
    normalize_desktop_file_scan_roots(&mut scan_roots);
    let scan_roots_clone = scan_roots.clone();
    let candidates = tokio::task::spawn_blocking(move || {
        collect_desktop_file_index_candidates(&scan_roots_clone)
    })
    .await
    .context("join native desktop file scanning candidates")?;

    let candidate_count = candidates.len();
    let mut seen_file_ids = HashSet::with_capacity(candidates.len());

    // Acquire write lock specifically for the DB write iteration
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    for candidate in candidates {
        let path = candidate.path;
        let metadata = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            _ => continue,
        };
        let policy = if should_eager_sync_file(&path, &metadata) {
            DesktopFileContentPolicy::Eager
        } else {
            DesktopFileContentPolicy::Lazy
        };
        let file_id = desktop_file_id(&path);
        let (folder_components, virtual_path) =
            desktop_file_virtual_location(&candidate.scan_root, &path);
        let parent_id =
            ensure_ctox_desktop_folder_path(database, now_ms(), &folder_components).await?;
        if let Err(err) = upsert_desktop_file_with_parent(
            database,
            path.clone(),
            policy,
            parent_id,
            Some(virtual_path),
        )
        .await
        {
            eprintln!(
                "[business-os] failed to index desktop file {}: {err:#}",
                path.display()
            );
            continue;
        }
        seen_file_ids.insert(file_id);
        indexed += 1;
    }
    if candidate_count < DESKTOP_FILE_SCAN_MAX_FILES {
        mark_missing_scanned_desktop_files(database, &scan_roots, &seen_file_ids).await?;
    }
    Ok(indexed)
}

fn collect_desktop_file_index_candidates(
    scan_roots: &[DesktopFileScanRoot],
) -> Vec<DesktopFileIndexCandidate> {
    let mut candidates = Vec::new();
    for scan_root in scan_roots {
        let mut paths = Vec::new();
        collect_files_bounded(&scan_root.path, &mut paths);
        candidates.extend(paths.into_iter().map(|path| DesktopFileIndexCandidate {
            path,
            scan_root: scan_root.clone(),
        }));
        if candidates.len() >= DESKTOP_FILE_SCAN_MAX_FILES {
            break;
        }
    }
    candidates.truncate(DESKTOP_FILE_SCAN_MAX_FILES);
    candidates
}

fn normalize_desktop_file_scan_roots(roots: &mut Vec<DesktopFileScanRoot>) {
    roots.sort_by(|left, right| left.path.cmp(&right.path));
    roots.dedup_by(|left, right| left.path == right.path);
}

fn desktop_file_scan_roots(root: &Path) -> Vec<DesktopFileScanRoot> {
    let mut roots = vec![
        (root.join("runtime/business-os/notes"), "Notes".to_string()),
        (
            root.join("runtime/business-os/documents/generated"),
            "Generated Documents".to_string(),
        ),
        (
            root.join("runtime/business-os-imports"),
            "Imports".to_string(),
        ),
    ];
    if let Ok(tasks) = channels::list_queue_tasks(root, &[], 128) {
        roots.extend(
            tasks
                .into_iter()
                .filter_map(|task| task.workspace_root)
                .map(PathBuf::from)
                .map(|path| {
                    let label = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .filter(|name| !name.trim().is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| desktop_file_scan_root_label(&path));
                    (path, label)
                }),
        );
    }
    roots
        .into_iter()
        .filter_map(|(path, label)| {
            path.canonicalize()
                .ok()
                .map(|path| DesktopFileScanRoot { path, label })
        })
        .filter(|root| is_safe_desktop_file_scan_root(&root.path))
        .collect()
}

fn desktop_file_scan_root_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Workspace".to_string())
}

fn is_safe_desktop_file_scan_root(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    let component_count = path.components().count();
    if component_count <= 1 {
        return false;
    }
    if path == Path::new("/tmp") || path == Path::new("/var/tmp") {
        return false;
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        if path == home {
            return false;
        }
    }
    true
}

fn collect_files_bounded(root: &Path, out: &mut Vec<PathBuf>) {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if out.len() >= DESKTOP_FILE_SCAN_MAX_FILES || depth > DESKTOP_FILE_SCAN_MAX_DEPTH {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if out.len() >= DESKTOP_FILE_SCAN_MAX_FILES {
                break;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if depth < DESKTOP_FILE_SCAN_MAX_DEPTH && !should_skip_scan_dir(&path) {
                    stack.push((path, depth + 1));
                }
            } else if file_type.is_file() && !should_skip_scan_file(&path) {
                out.push(path);
            }
        }
    }
}

fn should_skip_scan_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    name.starts_with('.')
        || matches!(
            name,
            "node_modules" | "target" | "build" | "dist" | "vendor" | "__pycache__"
        )
}

fn should_skip_scan_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name.starts_with('.')
                || name.ends_with('~')
                || name.ends_with(".tmp")
                || name.ends_with(".sqlite3")
                || name.ends_with(".db")
                || name.contains(".sqlite3-")
                || name.contains(".db-")
        })
        .unwrap_or(true)
}

fn should_eager_sync_file(path: &Path, metadata: &fs::Metadata) -> bool {
    if metadata.len() > DESKTOP_FILE_EAGER_LIMIT_BYTES {
        return false;
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "txt"
            | "md"
            | "markdown"
            | "json"
            | "jsonl"
            | "csv"
            | "tsv"
            | "log"
            | "html"
            | "htm"
            | "css"
            | "js"
            | "mjs"
            | "ts"
            | "tsx"
            | "jsx"
            | "rs"
            | "toml"
            | "yaml"
            | "yml"
            | "xml"
            | "svg"
            | "pdf"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
    )
}

fn desktop_file_virtual_location(
    scan_root: &DesktopFileScanRoot,
    path: &Path,
) -> (Vec<String>, String) {
    let mut folder_components = vec![scan_root.label.clone()];
    let relative = path.strip_prefix(&scan_root.path).unwrap_or(path);
    if let Some(parent) = relative.parent() {
        folder_components.extend(
            parent
                .components()
                .filter_map(|component| component.as_os_str().to_str())
                .filter(|component| !component.is_empty())
                .map(str::to_string),
        );
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let mut parts = vec!["CTOX".to_string()];
    parts.extend(folder_components.iter().cloned());
    parts.push(file_name.to_string());
    (folder_components, format!("/{}", parts.join("/")))
}

fn desktop_file_id(path: &Path) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    format!("ctox_file_{:x}", hasher.finalize())
}

fn desktop_folder_id(virtual_path: &str) -> String {
    if virtual_path == CTOX_DESKTOP_FOLDER_PATH {
        return CTOX_DESKTOP_FOLDER_ID.to_string();
    }
    let mut hasher = sha2::Sha256::new();
    hasher.update(virtual_path.as_bytes());
    format!("ctox_folder_{:x}", hasher.finalize())
}

async fn mark_missing_scanned_desktop_files(
    database: &Arc<RxDatabase>,
    scan_roots: &[DesktopFileScanRoot],
    seen_file_ids: &HashSet<String>,
) -> anyhow::Result<usize> {
    if scan_roots.is_empty() {
        return Ok(0);
    }
    let files = database
        .collection("desktop_files")
        .context("desktop_files collection is not registered")?;
    let documents = files
        .find(None)
        .map_err(|err| anyhow::anyhow!("query desktop_files for tombstones: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec desktop_files tombstone query: {err}"))?;
    let Some(rows) = documents.as_array() else {
        return Ok(0);
    };

    let mut marked = 0usize;
    let now = now_ms();
    for row in rows {
        let mut document = row.clone();
        if document.get("kind").and_then(Value::as_str) != Some("file") {
            continue;
        }
        if document.get("source").and_then(Value::as_str) != Some("ctox-core") {
            continue;
        }
        let Some(file_id) = document
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        if seen_file_ids.contains(&file_id) {
            continue;
        }
        if document
            .get("is_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(local_path) = document
            .get("local_path")
            .or_else(|| document.get("path"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let local_path = PathBuf::from(local_path);
        if !scan_roots
            .iter()
            .any(|scan_root| local_path.starts_with(&scan_root.path))
        {
            continue;
        }
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object.insert("_deleted".to_string(), Value::Bool(false));
            object.insert("is_deleted".to_string(), Value::Bool(true));
            object.insert(
                "content_state".to_string(),
                Value::String("missing".to_string()),
            );
            object.insert("deleted_at_ms".to_string(), Value::from(now as u64));
            object.insert(
                "tombstone_reason".to_string(),
                Value::String("missing_from_scan".to_string()),
            );
            object.insert("updated_at_ms".to_string(), Value::from(now as u64));
        }
        files
            .incremental_upsert(document)
            .await
            .map_err(|err| anyhow::anyhow!("mark missing desktop file {file_id}: {err}"))?;
        marked += 1;
    }
    Ok(marked)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn metadata_modified_at_ms(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn mime_type_for_path(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "txt" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "json" | "jsonl" => "application/json",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "html" | "htm" => "text/html",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

async fn open_database(database_path: PathBuf) -> anyhow::Result<Arc<RxDatabase>> {
    let storage = get_rx_storage_sqlite(RxStorageSqliteSettings { database_path });
    create_rx_database(RxDatabaseCreator {
        name: "ctox-business-os".to_string(),
        storage,
        multi_instance: false,
        password: None,
        hash_function: Arc::new(Sha256HashFunction),
        options: HashMap::new(),
        ignore_duplicate: false,
        close_duplicates: true,
        event_reduce: true,
        allow_slow_count: true,
    })
    .await
    .map_err(|err| anyhow::anyhow!("open native Business OS RxDB database: {err}"))
}

fn collection_creators() -> HashMap<String, RxCollectionCreator> {
    business_os_collections()
        .iter()
        .map(|(name, primary_key)| {
            (
                name.clone(),
                RxCollectionCreator {
                    schema: business_os_schema(name, primary_key),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )
        })
        .collect()
}

fn business_os_schema(name: &str, primary_key: &str) -> RxJsonSchema {
    let schema = business_os_schema_contract()
        .get(name)
        .unwrap_or_else(|| panic!("Business OS RxDB schema contract is missing {name}"));
    let actual_primary_key = schema
        .get("primaryKey")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert_eq!(
        actual_primary_key, primary_key,
        "Business OS RxDB primary key mismatch for {name}"
    );
    schema_from_json(schema.clone())
}

fn schema_from_json(mut value: Value) -> RxJsonSchema {
    normalize_schema_indexes(&mut value);
    serde_json::from_value(value).expect("Business OS RxDB schema must match rxdb-rs schema type")
}

fn normalize_schema_indexes(value: &mut Value) {
    let Some(indexes) = value.get_mut("indexes").and_then(Value::as_array_mut) else {
        return;
    };
    for index in indexes.iter_mut() {
        if let Some(field) = index.as_str() {
            *index = Value::Array(vec![Value::String(field.to_string())]);
        }
    }
}

fn business_os_schema_contract() -> &'static HashMap<String, Value> {
    static CONTRACT: OnceLock<HashMap<String, Value>> = OnceLock::new();
    CONTRACT.get_or_init(|| {
        serde_json::from_str(include_str!("business_os_schema_contract.json"))
            .expect("Business OS RxDB schema contract JSON must be valid")
    })
}

fn business_os_collections() -> Vec<(String, String)> {
    let mut collections = business_os_schema_contract()
        .iter()
        .map(|(name, schema)| {
            let primary_key = schema
                .get("primaryKey")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("Business OS RxDB schema {name} has no primaryKey"));
            (name.clone(), primary_key.to_string())
        })
        .collect::<Vec<_>>();
    collections.sort_by(|left, right| left.0.cmp(&right.0));
    collections
}

fn business_record_projection_collections() -> Vec<String> {
    business_os_collections()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|name| !matches!(name.as_str(), "browser_frames" | "desktop_file_chunks"))
        .collect()
}

/// FIX 4: the set of Business OS RxDB collections whose failure must abort the
/// peer bring-up. These carry runtime data the daemon depends on (module
/// catalog, runtime settings, command queue, queue tasks, desktop files +
/// chunks). Everything else is OPTIONAL: if it drifts or fails to register we
/// log and skip it instead of tearing down the whole peer. This mirrors the
/// required-vs-optional knowledge already encoded in
/// `repair_optional_rxdb_collection_schema_drift`.
fn is_required_native_collection(collection: &str) -> bool {
    matches!(
        collection,
        "business_module_catalog"
            | "ctox_runtime_settings"
            | "business_commands"
            | "ctox_queue_tasks"
            | "desktop_files"
            | "desktop_file_chunks"
    )
}

pub fn repair_optional_rxdb_collection_schema_drift(
    root: &Path,
    collection: &str,
    dry_run: bool,
    force: bool,
) -> anyhow::Result<Value> {
    let collection = collection.trim();
    if collection.is_empty() {
        anyhow::bail!("collection is required");
    }
    let required = is_required_native_collection(collection);
    if required && !force {
        anyhow::bail!(
            "refusing to repair required Business OS RxDB collection `{collection}` without --force"
        );
    }
    Ok(json!({
        "ok": true,
        "code": "ctox_optional_schema_drift",
        "action": "repair-optional-drift",
        "dry_run": dry_run,
        "force": force,
        "collection": collection,
        "database_path": store::rxdb_store_path(root).display().to_string(),
        "repaired": false,
        "message": "No optional schema drift repair was needed for the current isolated Business OS RxDB store."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use rusqlite::Connection;
    use rusqlite::OptionalExtension;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_RXDB_DATABASE_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn business_record_projection_skips_transient_payload_collections() {
        let collections = business_record_projection_collections();
        assert!(!collections.iter().any(|name| name == "browser_frames"));
        assert!(!collections.iter().any(|name| name == "desktop_file_chunks"));
        assert!(collections.iter().any(|name| name == "browser_tabs"));
        assert!(collections.iter().any(|name| name == "desktop_files"));
    }

    #[test]
    fn projection_upsert_detects_doc_cache_revision_errors() {
        let revision_error = rxdb::rx_error::new_rx_error("DOC_CACHE_REV", Some(json!({})));
        let lwt_error = rxdb::rx_error::new_rx_error("DOC_CACHE_LWT", Some(json!({})));
        let other_error = rxdb::rx_error::new_rx_error("COL4", Some(json!({})));

        assert!(is_doc_cache_revision_error(&revision_error));
        assert!(is_doc_cache_revision_error(&lwt_error));
        assert!(!is_doc_cache_revision_error(&other_error));
    }

    #[test]
    fn projection_upsert_recovers_from_tombstone_conflict() {
        let conflict_error = rxdb::rx_error::new_rx_error("CONFLICT", Some(json!({})));
        let revision_error = rxdb::rx_error::new_rx_error("DOC_CACHE_REV", Some(json!({})));
        let other_error = rxdb::rx_error::new_rx_error("COL4", Some(json!({})));

        assert!(is_recoverable_projection_write_error(&conflict_error));
        assert!(is_recoverable_projection_write_error(&revision_error));
        assert!(!is_recoverable_projection_write_error(&other_error));
    }

    #[test]
    fn native_peer_status_reports_fresh_heartbeat() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = root.path().join("runtime/ctox.sqlite3");
        write_native_peer_heartbeat(root.path(), "rxdb-rs-test", &database_path)
            .expect("write heartbeat");

        let status = native_peer_status(root.path());
        assert_eq!(status["running"], true);
        assert_eq!(status["heartbeat"]["fresh"], true);
        assert_eq!(status["peer_session_id"], "rxdb-rs-test");
        assert!(is_native_peer_running_for_root(root.path()));
    }

    #[test]
    fn native_peer_heartbeat_freshness_expires() {
        let root = tempfile::tempdir().expect("temp root");
        let path = native_peer_heartbeat_path(root.path());
        std::fs::create_dir_all(path.parent().expect("heartbeat parent"))
            .expect("create heartbeat dir");
        std::fs::write(
            &path,
            serde_json::to_vec(&json!({
                "version": NATIVE_PEER_STATUS_VERSION,
                "running": true,
                "pid": 1,
                "peer_session_id": "rxdb-rs-stale",
                "updated_at_ms": 1_u64,
                "database_path": root.path().join("runtime/ctox.sqlite3").display().to_string(),
            }))
            .expect("serialize heartbeat"),
        )
        .expect("write heartbeat");

        assert!(!native_peer_heartbeat_is_fresh(root.path()));
    }

    #[test]
    fn native_signaling_url_carries_control_plane_metadata() {
        let url = signaling_url_with_native_metadata(
            "wss://signaling.ctox.dev?foo=bar&role=browser",
            "ctox-business-os:inst_123:roomhash",
            "room-password",
        );

        let parsed = Url::parse(&url).expect("metadata url parses");
        assert_eq!(
            parsed
                .query_pairs()
                .find(|(key, _)| key == "foo")
                .unwrap()
                .1,
            "bar"
        );
        assert_eq!(
            parsed
                .query_pairs()
                .find(|(key, _)| key == "client")
                .unwrap()
                .1,
            "ctox-business-os-native"
        );
        assert_eq!(
            parsed
                .query_pairs()
                .find(|(key, _)| key == "role")
                .unwrap()
                .1,
            "ctox_instance"
        );
        assert_eq!(
            parsed
                .query_pairs()
                .find(|(key, _)| key == "instance_id")
                .unwrap()
                .1,
            "inst_123"
        );
        let query_pairs = parsed.query_pairs().into_owned().collect::<HashMap<_, _>>();
        let expected_token =
            signaling_token_from_room_password("room-password").expect("room password token");
        assert_eq!(
            query_pairs.get("token").map(String::as_str),
            Some(expected_token.as_str())
        );
        let issued_at = query_pairs
            .get("token_iat")
            .expect("token_iat")
            .parse::<u64>()
            .expect("token_iat number");
        let expires_at = query_pairs
            .get("token_exp")
            .expect("token_exp")
            .parse::<u64>()
            .expect("token_exp number");
        assert_eq!(expires_at - issued_at, SIGNALING_TOKEN_TTL_SECONDS);
        assert_eq!(
            query_pairs.get("protocol").map(String::as_str),
            Some(CTOX_RXDB_PROTOCOL)
        );
        let capabilities = parsed
            .query_pairs()
            .filter_map(|(key, value)| (key == "cap").then(|| value.into_owned()))
            .collect::<Vec<_>>();
        assert_eq!(capabilities, CTOX_NATIVE_CAPABILITIES);
    }

    #[tokio::test]
    async fn native_core_schema_hashes_match_browser_contract() {
        let cases = [
            (
                "business_module_catalog",
                "id",
                "332763869d93c2bb55fa6b217c36521d1c1f17be4701d8538d686cda89f5cea0",
            ),
            (
                "ctox_runtime_settings",
                "id",
                "3958bb6580e9705f3688fcf453a80ec33c486b43ac6988f015ffc16cb5ac918d",
            ),
            (
                "ctox_ticket_items",
                "id",
                "b233b5e15b0f46ccfa864976861b8e0665dcee8f3e5d920f1c2341b2a3366ba9",
            ),
        ];
        for (collection, primary_key, expected_hash) in cases {
            let schema = rxdb::rx_schema::create_rx_schema(
                business_os_schema(collection, primary_key),
                Arc::new(Sha256HashFunction),
                true,
            )
            .expect("schema creates");
            let actual_hash = schema.hash().await;
            if actual_hash != expected_hash {
                let normalized =
                    serde_json::to_string(&schema.json_schema).expect("schema serializes");
                eprintln!("{collection} native schema: {normalized}");
            }
            assert_eq!(actual_hash, expected_hash, "schema hash for {collection}");
        }
    }

    #[tokio::test]
    async fn native_all_schema_hashes_match_browser_contract_fixture() {
        let fixture: HashMap<String, String> =
            serde_json::from_str(include_str!("business_os_schema_hashes.json"))
                .expect("Business OS schema hash fixture must be valid JSON");
        let contract = business_os_schema_contract();
        let mut missing = Vec::new();
        let mut stale = Vec::new();
        for (collection, schema_json) in contract {
            let primary_key = schema_json
                .get("primaryKey")
                .and_then(Value::as_str)
                .expect("schema primaryKey");
            let schema = rxdb::rx_schema::create_rx_schema(
                business_os_schema(collection, primary_key),
                Arc::new(Sha256HashFunction),
                true,
            )
            .expect("schema creates");
            let actual = schema.hash().await;
            match fixture.get(collection) {
                Some(expected) if expected == &actual => {}
                Some(expected) => stale.push(json!({
                    "collection": collection,
                    "expected": expected,
                    "actual": actual,
                })),
                None => missing.push(json!({
                    "collection": collection,
                    "actual": actual,
                })),
            }
        }
        let extra = fixture
            .keys()
            .filter(|collection| !contract.contains_key(*collection))
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty() && stale.is_empty() && extra.is_empty(),
            "{}",
            json!({
                "message": "Business OS schema hash fixture drifted from generated schema contract",
                "missing": missing,
                "stale": stale,
                "extra": extra,
            })
        );
    }

    async fn open_test_database(database_path: PathBuf) -> anyhow::Result<Arc<RxDatabase>> {
        let test_id = TEST_RXDB_DATABASE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings { database_path });
        create_rx_database(RxDatabaseCreator {
            name: format!("ctox-business-os-test-{test_id}"),
            storage,
            multi_instance: false,
            password: None,
            hash_function: Arc::new(Sha256HashFunction),
            options: HashMap::new(),
            ignore_duplicate: false,
            close_duplicates: true,
            event_reduce: true,
            allow_slow_count: true,
        })
        .await
        .map_err(|err| anyhow::anyhow!("open test Business OS RxDB database: {err}"))
    }

    #[test]
    fn sync_desktop_file_from_path_writes_sqlite_records_without_running_peer() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("artifact.md");
        fs::write(&file_path, b"# Artifact\n\nready\n").expect("write artifact");

        sync_desktop_file_from_path(root.path(), &file_path).expect("sync desktop file");

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let mut stmt = conn
            .prepare("SELECT data FROM ctox_business_os__desktop_files__v0")
            .expect("desktop file query");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("desktop file rows");
        let mut file = None;
        for row in rows {
            let document: Value =
                serde_json::from_str(&row.expect("desktop file row")).expect("desktop file json");
            if document.get("name").and_then(Value::as_str) == Some("artifact.md") {
                file = Some(document);
                break;
            }
        }
        let file = file.expect("artifact desktop file row");
        assert_eq!(
            file.get("name").and_then(Value::as_str),
            Some("artifact.md")
        );
        assert_eq!(
            file.get("source").and_then(Value::as_str),
            Some("ctox-core")
        );

        let chunk_json: Option<String> = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("chunk query");
        let chunk: Value =
            serde_json::from_str(&chunk_json.expect("chunk row")).expect("chunk json");
        assert_eq!(
            chunk.get("encoding").and_then(Value::as_str),
            Some("base64")
        );
        assert_eq!(chunk.get("idx").and_then(Value::as_u64), Some(0));
        assert_eq!(
            chunk.get("chunk_hash_scheme").and_then(Value::as_str),
            Some(DESKTOP_FILE_CHUNK_HASH_SCHEME)
        );
    }

    #[test]
    fn sync_desktop_file_from_path_writes_empty_file_chunk() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("empty.txt");
        fs::write(&file_path, b"").expect("write empty artifact");

        sync_desktop_file_from_path(root.path(), &file_path).expect("sync desktop file");

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let chunk_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("empty chunk row");
        let chunk: Value = serde_json::from_str(&chunk_json).expect("chunk json");
        assert_eq!(chunk.get("data").and_then(Value::as_str), Some(""));
        assert_eq!(chunk.get("total").and_then(Value::as_u64), Some(1));
        assert_eq!(chunk.get("size_bytes").and_then(Value::as_u64), Some(0));
        assert_eq!(
            chunk.get("content_hash_scheme").and_then(Value::as_str),
            Some(DESKTOP_FILE_CONTENT_HASH_SCHEME)
        );
        assert_eq!(
            chunk.get("chunk_hash_scheme").and_then(Value::as_str),
            Some(DESKTOP_FILE_CHUNK_HASH_SCHEME)
        );
        assert_eq!(
            chunk.get("content_hash").and_then(Value::as_str),
            Some(hex_sha256(b"").as_str())
        );
        assert_eq!(
            chunk.get("chunk_hash").and_then(Value::as_str),
            Some(hex_sha256(b"").as_str())
        );
    }

    #[test]
    fn sync_desktop_file_from_path_reconstructs_and_rejects_invalid_chunk_integrity() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("integrity.txt");
        let payload = vec![b'z'; DESKTOP_FILE_CHUNK_SIZE + 23];
        fs::write(&file_path, &payload).expect("write integrity artifact");

        sync_desktop_file_from_path(root.path(), &file_path).expect("sync desktop file");

        let canonical = file_path.canonicalize().expect("canonical file");
        let file_id = desktop_file_id(&canonical);
        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let file = read_desktop_file_row(&conn, &file_id);
        let generation_id = file
            .get("content_generation_id")
            .and_then(Value::as_str)
            .expect("active generation id");
        let content_hash = file
            .get("content_hash")
            .and_then(Value::as_str)
            .expect("content hash");
        assert_eq!(
            file.get("content_hash_scheme").and_then(Value::as_str),
            Some(DESKTOP_FILE_CONTENT_HASH_SCHEME)
        );

        let chunks = read_desktop_file_chunks(&conn, &file_id, false);
        let decoded =
            reconstruct_desktop_file_chunks(&chunks, &file_id, generation_id, content_hash)
                .expect("valid chunks reconstruct");
        assert_eq!(decoded, payload);

        let mut missing_chunk = chunks.clone();
        missing_chunk.pop();
        assert_chunk_integrity_error(
            reconstruct_desktop_file_chunks(&missing_chunk, &file_id, generation_id, content_hash),
            "chunk set is incomplete",
        );

        let mut stale_generation = chunks.clone();
        stale_generation[0]["generation_id"] = Value::String("gen_stale".to_string());
        assert_chunk_integrity_error(
            reconstruct_desktop_file_chunks(
                &stale_generation,
                &file_id,
                generation_id,
                content_hash,
            ),
            "unexpected chunk generation",
        );

        let mut wrong_content_hash = chunks.clone();
        wrong_content_hash[0]["content_hash"] = Value::String("bad-content-hash".to_string());
        assert_chunk_integrity_error(
            reconstruct_desktop_file_chunks(
                &wrong_content_hash,
                &file_id,
                generation_id,
                content_hash,
            ),
            "unexpected chunk content hash",
        );

        let mut wrong_chunk_hash = chunks.clone();
        wrong_chunk_hash[0]["chunk_hash"] = Value::String("bad-chunk-hash".to_string());
        assert_chunk_integrity_error(
            reconstruct_desktop_file_chunks(
                &wrong_chunk_hash,
                &file_id,
                generation_id,
                content_hash,
            ),
            "unexpected chunk hash",
        );

        let mut wrong_total = chunks.clone();
        wrong_total[1]["total"] = Value::from(99_u64);
        assert_chunk_integrity_error(
            reconstruct_desktop_file_chunks(&wrong_total, &file_id, generation_id, content_hash),
            "chunk total mismatch",
        );
    }

    #[test]
    fn sync_desktop_file_from_path_indexes_large_file_lazily_until_materialized() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("large.txt");
        fs::write(
            &file_path,
            vec![b'x'; DESKTOP_FILE_EAGER_LIMIT_BYTES as usize + 1],
        )
        .expect("write large artifact");
        let canonical = file_path.canonicalize().expect("canonical file");
        let file_id = desktop_file_id(&canonical);

        sync_desktop_file_from_path(root.path(), &file_path).expect("sync large file metadata");

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let file_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [file_id.as_str()],
                |row| row.get(0),
            )
            .expect("large desktop file row");
        let file: Value = serde_json::from_str(&file_json).expect("desktop file json");
        assert_eq!(
            file.get("content_state").and_then(Value::as_str),
            Some("lazy")
        );
        assert_eq!(file.get("content_synced_at_ms"), Some(&Value::Null));
        let lazy_chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 WHERE id LIKE ?1",
                params![format!("{file_id}_%")],
                |row| row.get(0),
            )
            .expect("lazy chunks count");
        assert_eq!(lazy_chunks, 0);
        drop(conn);

        materialize_desktop_file_from_path(root.path(), &file_path)
            .expect("materialize large file content");
        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let materialized_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [file_id.as_str()],
                |row| row.get(0),
            )
            .expect("materialized desktop file row");
        let materialized: Value =
            serde_json::from_str(&materialized_json).expect("materialized file json");
        assert_eq!(
            materialized.get("content_state").and_then(Value::as_str),
            Some("available")
        );
        assert!(materialized
            .get("content_synced_at_ms")
            .and_then(Value::as_u64)
            .is_some());
        let materialized_chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 WHERE id LIKE ?1",
                params![format!("{file_id}_%")],
                |row| row.get(0),
            )
            .expect("materialized chunks count");
        assert!(materialized_chunks > 0);
    }

    #[test]
    fn sync_desktop_file_from_path_records_new_chunk_generation_after_update() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("mutable.txt");
        let initial = vec![b'a'; DESKTOP_FILE_CHUNK_SIZE + 16];
        fs::write(&file_path, initial).expect("write initial artifact");

        sync_desktop_file_from_path(root.path(), &file_path).expect("sync initial desktop file");
        let canonical = file_path.canonicalize().expect("canonical file");
        let file_id = desktop_file_id(&canonical);

        std::thread::sleep(Duration::from_millis(2));
        fs::write(&file_path, b"short").expect("write updated artifact");
        sync_desktop_file_from_path(root.path(), &file_path).expect("sync updated desktop file");

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let file_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [file_id.as_str()],
                |row| row.get(0),
            )
            .expect("updated desktop file row");
        let file: Value = serde_json::from_str(&file_json).expect("desktop file json");
        assert_eq!(
            file.get("content_state").and_then(Value::as_str),
            Some("available")
        );
        assert_eq!(file.get("size_bytes").and_then(Value::as_u64), Some(5));
        let active_generation_id = file
            .get("content_generation_id")
            .and_then(Value::as_str)
            .expect("active content generation id");

        let mut rows = conn
            .prepare(
                "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 \
                 WHERE id LIKE ?1 AND deleted = 0",
            )
            .expect("chunk query");
        let chunks: Vec<Value> = rows
            .query_map(params![format!("{file_id}_%")], |row| {
                row.get::<_, String>(0)
            })
            .expect("chunk rows")
            .map(|row| serde_json::from_str(&row.expect("chunk row")).expect("chunk json"))
            .collect();
        assert!(
            chunks.len() > 1,
            "old chunk rows should remain as historical generations"
        );
        let distinct_generations: HashSet<&str> = chunks
            .iter()
            .filter_map(|chunk| chunk.get("generation_id").and_then(Value::as_str))
            .collect();
        assert!(
            distinct_generations.len() > 1,
            "updated content should create a distinct chunk generation"
        );
        let mut latest_generation: Vec<&Value> = chunks
            .iter()
            .filter(|chunk| {
                chunk.get("generation_id").and_then(Value::as_str) == Some(active_generation_id)
            })
            .collect();
        latest_generation.sort_by_key(|chunk| chunk.get("idx").and_then(Value::as_u64));
        assert_eq!(latest_generation.len(), 1);
        assert_eq!(
            latest_generation[0].get("idx").and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(
            latest_generation[0].get("total").and_then(Value::as_u64),
            Some(1)
        );
        let encoded = latest_generation[0]
            .get("data")
            .and_then(Value::as_str)
            .expect("latest chunk data");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("decode latest chunk");
        assert_eq!(decoded, b"short");
    }

    #[test]
    fn sync_desktop_file_from_path_prunes_stale_chunk_generations() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("retained.txt");
        fs::write(&file_path, b"first").expect("write first artifact");

        sync_desktop_file_from_path(root.path(), &file_path).expect("sync first desktop file");
        let canonical = file_path.canonicalize().expect("canonical file");
        let file_id = desktop_file_id(&canonical);

        for content in [
            b"second".as_slice(),
            b"third".as_slice(),
            b"fourth".as_slice(),
        ] {
            std::thread::sleep(Duration::from_millis(2));
            fs::write(&file_path, content).expect("write updated artifact");
            sync_desktop_file_from_path(root.path(), &file_path)
                .expect("sync updated desktop file");
        }

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let file_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [file_id.as_str()],
                |row| row.get(0),
            )
            .expect("desktop file row");
        let file: Value = serde_json::from_str(&file_json).expect("desktop file json");
        let active_generation_id = file
            .get("content_generation_id")
            .and_then(Value::as_str)
            .expect("active content generation id");

        let mut rows = conn
            .prepare(
                "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 \
                 WHERE id LIKE ?1 AND deleted = 0",
            )
            .expect("chunk query");
        let chunks: Vec<Value> = rows
            .query_map(params![format!("{file_id}_%")], |row| {
                row.get::<_, String>(0)
            })
            .expect("chunk rows")
            .map(|row| serde_json::from_str(&row.expect("chunk row")).expect("chunk json"))
            .collect();
        let generations: HashSet<&str> = chunks
            .iter()
            .filter_map(|chunk| chunk.get("generation_id").and_then(Value::as_str))
            .collect();
        assert!(
            generations.len() <= DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS,
            "stale chunk generations should be pruned"
        );
        assert!(
            generations.contains(active_generation_id),
            "active generation must never be pruned"
        );

        let pruned_chunk_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 \
                 WHERE id LIKE ?1 AND deleted = 1 LIMIT 1",
                params![format!("{file_id}_%")],
                |row| row.get(0),
            )
            .expect("pruned chunk tombstone");
        let pruned_chunk: Value =
            serde_json::from_str(&pruned_chunk_json).expect("pruned chunk json");
        assert_eq!(pruned_chunk.get("data").and_then(Value::as_str), Some(""));
        assert_eq!(
            pruned_chunk.get("size_bytes").and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(
            pruned_chunk.get("prune_reason").and_then(Value::as_str),
            Some("stale_generation")
        );
    }

    #[test]
    fn materialize_desktop_file_command_writes_missing_chunks() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("materialize.md");
        fs::write(&file_path, b"# Materialize\n\nfrom ctox\n").expect("write artifact");
        let canonical = file_path.canonicalize().expect("canonical file");
        let file_id = desktop_file_id(&canonical);

        sync_desktop_file_from_path(root.path(), &file_path).expect("sync desktop file metadata");
        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        conn.execute("DELETE FROM ctox_business_os__desktop_file_chunks__v0", [])
            .expect("delete chunks");
        drop(conn);

        let accepted = store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_materialize_file",
                "command_id": "cmd_materialize_file",
                "module": "desktop",
                "command_type": "ctox.file.materialize",
                "record_id": file_id,
                "status": "pending_sync",
                "payload": {
                    "file_id": file_id,
                    "path": canonical.to_string_lossy()
                },
                "client_context": {
                    "actor": { "id": "tester", "role": "user", "display_name": "Tester" }
                },
                "updated_at_ms": now_ms() as u64
            }),
        )
        .expect("accept materialize command");
        assert_eq!(
            accepted.get("status").and_then(Value::as_str),
            Some("completed")
        );

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let chunk_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("materialized chunk row");
        let chunk: Value = serde_json::from_str(&chunk_json).expect("chunk json");
        assert_eq!(
            chunk.get("file_id").and_then(Value::as_str),
            Some(file_id.as_str())
        );
        assert_eq!(
            accepted
                .get("result")
                .and_then(|result| result.get("content_state"))
                .and_then(Value::as_str),
            Some("available")
        );
    }

    #[test]
    fn sync_channel_state_projects_accounts_and_pairing_state() {
        let root = tempfile::tempdir().expect("temp root");
        channels::list_communication_accounts_for_business_os(root.path())
            .expect("initialize channel schema");
        let conn = Connection::open(root.path().join("runtime/ctox.sqlite3"))
            .expect("open channel sqlite");
        conn.execute(
            r#"
            INSERT INTO communication_accounts (
                account_key, channel, address, provider, profile_json,
                created_at, updated_at, last_inbound_ok_at, last_outbound_ok_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, NULL, NULL)
            "#,
            params![
                "jami:alice",
                "jami",
                "alice@example.test",
                "jami",
                "{}",
                "2026-05-20T08:00:00Z",
            ],
        )
        .expect("insert communication account");
        drop(conn);

        let artifacts =
            crate::communication::runtime::artifacts_dir_for_business_os(root.path(), "jami");
        fs::create_dir_all(&artifacts).expect("create pairing artifacts");
        fs::write(
            artifacts.join("pairing-status.json"),
            r#"{"status":"qr_ready","qr_payload":"jami-pair","step":"qr"}"#,
        )
        .expect("write pairing status");
        fs::write(artifacts.join("pairing-qr.svg"), "<svg></svg>").expect("write pairing qr");

        let synced = sync_channel_state(root.path()).expect("sync channel state");
        assert!(synced >= BUSINESS_OS_CHANNEL_IDS.len() + 1);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let accounts: Vec<Value> = {
            let mut stmt = conn
                .prepare("SELECT data FROM ctox_business_os__communication_accounts__v0")
                .expect("account projection query");
            stmt.query_map([], |row| row.get::<_, String>(0))
                .expect("account projection rows")
                .map(|row| serde_json::from_str(&row.expect("account row")).expect("account json"))
                .collect()
        };
        let account = accounts
            .iter()
            .find(|doc| doc.get("account_key").and_then(Value::as_str) == Some("jami:alice"))
            .expect("projected account");
        assert_eq!(
            account.get("address").and_then(Value::as_str),
            Some("alice@example.test")
        );
        assert_eq!(
            account.get("is_deleted").and_then(Value::as_bool),
            Some(false)
        );

        let pairing_states: Vec<Value> = {
            let mut stmt = conn
                .prepare("SELECT data FROM ctox_business_os__channel_pairing_state__v0")
                .expect("pairing projection query");
            stmt.query_map([], |row| row.get::<_, String>(0))
                .expect("pairing projection rows")
                .map(|row| serde_json::from_str(&row.expect("pairing row")).expect("pairing json"))
                .collect()
        };
        let jami_state = pairing_states
            .iter()
            .find(|doc| doc.get("channel").and_then(Value::as_str) == Some("jami"))
            .expect("projected jami pairing state");
        assert_eq!(
            jami_state.get("status").and_then(Value::as_str),
            Some("qr_ready")
        );
        assert_eq!(
            jami_state.get("qr_payload").and_then(Value::as_str),
            Some("jami-pair")
        );

        channels::disconnect_communication_account_for_business_os(root.path(), "jami:alice")
            .expect("disconnect account");
        sync_channel_state(root.path()).expect("sync channel state after disconnect");
        let tombstone_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__communication_accounts__v0",
                [],
                |row| row.get(0),
            )
            .expect("account tombstone row");
        let tombstone: Value = serde_json::from_str(&tombstone_json).expect("tombstone json");
        assert_eq!(
            tombstone.get("account_key").and_then(Value::as_str),
            Some("jami:alice")
        );
        assert_eq!(
            tombstone.get("is_deleted").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn sync_business_users_projects_store_rows() {
        let root = tempfile::tempdir().expect("temp root");
        let admin = store::BusinessOsSession {
            ok: true,
            authenticated: true,
            auth_required: true,
            user: Some(store::BusinessOsSessionUser {
                id: "admin".to_string(),
                display_name: "Admin".to_string(),
                role: "admin".to_string(),
                is_admin: true,
            }),
            login_url: None,
            reason: None,
        };
        store::upsert_user(
            root.path(),
            &admin,
            store::BusinessOsUserMutation {
                id: "alice".to_string(),
                display_name: "Alice".to_string(),
                role: "founder".to_string(),
                active: true,
            },
        )
        .expect("upsert business user");

        let synced = sync_business_users(root.path()).expect("sync business users");
        assert!(synced >= 2);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let user_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__business_users__v0 WHERE id = 'alice'",
                [],
                |row| row.get(0),
            )
            .expect("projected user row");
        let user: Value = serde_json::from_str(&user_json).expect("user json");
        assert_eq!(user.get("id").and_then(Value::as_str), Some("alice"));
        assert_eq!(
            user.get("display_name").and_then(Value::as_str),
            Some("Alice")
        );
        assert_eq!(user.get("role").and_then(Value::as_str), Some("founder"));
        assert_eq!(user.get("active").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn sync_runtime_settings_projects_status_document() {
        let root = tempfile::tempdir().expect("temp root");

        sync_runtime_settings(root.path()).expect("sync runtime settings");

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let settings_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__ctox_runtime_settings__v0 WHERE id = 'runtime-settings'",
                [],
                |row| row.get(0),
            )
            .expect("projected runtime settings row");
        let settings: Value = serde_json::from_str(&settings_json).expect("runtime settings json");
        assert_eq!(
            settings.get("id").and_then(Value::as_str),
            Some("runtime-settings")
        );
        assert_eq!(settings.get("ok").and_then(Value::as_bool), Some(true));
        assert!(settings.get("runtime").and_then(Value::as_object).is_some());
        assert!(settings.get("auth").and_then(Value::as_object).is_some());
        assert!(settings
            .get("harness_flow")
            .and_then(Value::as_object)
            .is_some());
        assert!(settings
            .get("queue_health")
            .and_then(Value::as_object)
            .is_some());
        assert!(settings
            .get("diagnostics")
            .and_then(Value::as_object)
            .is_some());
    }

    #[test]
    fn sync_module_catalog_projects_modules_and_templates() {
        let root = tempfile::tempdir().expect("temp root");
        let app_root = root.path().join("src/apps/business-os");
        fs::create_dir_all(app_root.join("modules/ctox")).expect("create ctox module");
        fs::create_dir_all(app_root.join("template-store/demo-template")).expect("create template");
        fs::write(app_root.join("index.html"), "<!doctype html>").expect("write app index");
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","collections":["ctox_queue_tasks"]}"#,
        )
        .expect("write ctox manifest");
        fs::write(
            app_root.join("template-store/demo-template/template.json"),
            r#"{"id":"demo-template","title":"Demo Template","source_module":"ctox","default_title":"Demo","category":"test"}"#,
        )
        .expect("write template manifest");

        sync_module_catalog(root.path()).expect("sync module catalog");

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let catalog_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__business_module_catalog__v0 WHERE id = 'module-catalog'",
                [],
                |row| row.get(0),
            )
            .expect("projected module catalog row");
        let catalog: Value = serde_json::from_str(&catalog_json).expect("module catalog json");
        assert_eq!(
            catalog.get("id").and_then(Value::as_str),
            Some("module-catalog")
        );
        assert_eq!(
            catalog
                .get("modules")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|module| module.get("id"))
                .and_then(Value::as_str),
            Some("ctox")
        );
        assert_eq!(
            catalog
                .get("templates")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|template| template.get("id"))
                .and_then(Value::as_str),
            Some("demo-template")
        );
    }

    #[test]
    fn sync_business_record_projections_materializes_generic_collections() {
        let root = tempfile::tempdir().expect("temp root");
        let conn = store::open_store(root.path()).expect("open business store");
        store::upsert_business_record(
            &conn,
            "documents",
            "doc_projection_1",
            1_000,
            json!({
                "id": "doc_projection_1",
                "title": "Projected document",
                "filename": "projected.docx",
                "mime_type": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "status": "imported",
                "current_version_id": "doc_projection_1_v1",
                "index_text": "Projected document body",
                "is_deleted": false,
                "created_at_ms": 900,
                "updated_at_ms": 1_000
            }),
        )
        .expect("insert document business record");
        store::upsert_business_record(
            &conn,
            "document_versions",
            "doc_projection_1_v1",
            1_001,
            json!({
                "id": "doc_projection_1_v1",
                "document_id": "doc_projection_1",
                "version": 1,
                "source_kind": "import",
                "blob_id": "doc_projection_1_blob",
                "model_json": {},
                "diagnostics": [],
                "created_at_ms": 901,
                "updated_at_ms": 1_001
            }),
        )
        .expect("insert document version business record");
        drop(conn);

        let synced = sync_business_record_projections(root.path())
            .expect("sync business record projections");
        assert!(synced >= 2);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let document_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__documents__v0 WHERE id = 'doc_projection_1'",
                [],
                |row| row.get(0),
            )
            .expect("projected document row");
        let document: Value = serde_json::from_str(&document_json).expect("document json");
        assert_eq!(
            document.get("title").and_then(Value::as_str),
            Some("Projected document")
        );

        let version_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__document_versions__v0 WHERE id = 'doc_projection_1_v1'",
                [],
                |row| row.get(0),
            )
            .expect("projected version count");
        assert_eq!(version_count, 1);
    }

    #[test]
    fn sync_business_record_projections_repairs_missing_cache_metadata() {
        let root = tempfile::tempdir().expect("temp root");
        let conn = store::open_store(root.path()).expect("open business store");
        store::upsert_business_record(
            &conn,
            "documents",
            "doc_projection_repair",
            1_000,
            json!({
                "id": "doc_projection_repair",
                "title": "Projected document",
                "filename": "projected.docx",
                "mime_type": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "status": "imported",
                "current_version_id": "doc_projection_repair_v1",
                "index_text": "Projected document body",
                "is_deleted": false,
                "created_at_ms": 900,
                "updated_at_ms": 1_000
            }),
        )
        .expect("insert document business record");
        drop(conn);

        assert!(sync_business_record_projections(root.path()).expect("initial sync") >= 1);

        let rxdb_path = store::rxdb_store_path(root.path());
        let conn = Connection::open(&rxdb_path).expect("open rxdb sqlite");
        let document_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__documents__v0 WHERE id = 'doc_projection_repair'",
                [],
                |row| row.get(0),
            )
            .expect("projected document row");
        let mut document: Value = serde_json::from_str(&document_json).expect("document json");
        document
            .as_object_mut()
            .expect("document object")
            .remove("_meta");
        conn.execute(
            "UPDATE ctox_business_os__documents__v0 SET data = ? WHERE id = 'doc_projection_repair'",
            params![serde_json::to_string(&document).expect("serialize damaged document")],
        )
        .expect("damage document metadata");
        drop(conn);

        let conn = store::open_store(root.path()).expect("reopen business store");
        store::upsert_business_record(
            &conn,
            "documents",
            "doc_projection_repair",
            2_000,
            json!({
                "id": "doc_projection_repair",
                "title": "Projected document repaired",
                "filename": "projected.docx",
                "mime_type": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "status": "imported",
                "current_version_id": "doc_projection_repair_v1",
                "index_text": "Projected document body",
                "is_deleted": false,
                "created_at_ms": 900,
                "updated_at_ms": 2_000
            }),
        )
        .expect("update document business record");
        drop(conn);

        assert!(sync_business_record_projections(root.path()).expect("repair sync") >= 1);

        let conn = Connection::open(rxdb_path).expect("open repaired rxdb sqlite");
        let repaired_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__documents__v0 WHERE id = 'doc_projection_repair'",
                [],
                |row| row.get(0),
            )
            .expect("repaired document row");
        let repaired: Value = serde_json::from_str(&repaired_json).expect("repaired document json");
        assert_eq!(
            repaired.get("title").and_then(Value::as_str),
            Some("Projected document repaired")
        );
        assert!(repaired
            .get("_meta")
            .and_then(|meta| meta.get("lwt"))
            .is_some());
        assert!(repaired.get("_rev").and_then(Value::as_str).is_some());
    }

    #[test]
    fn sync_desktop_file_index_scans_runtime_outputs_with_lazy_large_files() {
        let root = tempfile::tempdir().expect("temp root");
        let output_dir = root
            .path()
            .join("runtime/business-os/documents/generated/reports");
        fs::create_dir_all(&output_dir).expect("create output dir");
        let small_path = output_dir.join("summary.md");
        let large_path = output_dir.join("archive.bin");
        fs::write(&small_path, b"# Summary\n\nvisible\n").expect("write small file");
        fs::write(
            &large_path,
            vec![b'x'; DESKTOP_FILE_EAGER_LIMIT_BYTES as usize + 1],
        )
        .expect("write large file");

        let indexed = sync_desktop_file_index(root.path()).expect("sync desktop file index");
        assert_eq!(indexed, 2);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let folder_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = 'fs_ctox'",
                [],
                |row| row.get(0),
            )
            .expect("ctox folder row");
        let folder: Value = serde_json::from_str(&folder_json).expect("folder json");
        assert_eq!(folder.get("name").and_then(Value::as_str), Some("CTOX"));

        let small_id = desktop_file_id(&small_path.canonicalize().expect("canonical small"));
        let large_id = desktop_file_id(&large_path.canonicalize().expect("canonical large"));
        let small_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [small_id.as_str()],
                |row| row.get(0),
            )
            .expect("small file row");
        let small: Value = serde_json::from_str(&small_json).expect("small json");
        assert_eq!(
            small.get("content_state").and_then(Value::as_str),
            Some("available")
        );
        assert_eq!(
            small.get("virtual_path").and_then(Value::as_str),
            Some("/CTOX/Generated Documents/reports/summary.md")
        );
        assert_eq!(
            small.get("local_path").and_then(Value::as_str),
            Some(
                small_path
                    .canonicalize()
                    .expect("canonical small again")
                    .to_string_lossy()
                    .as_ref()
            )
        );
        let reports_folder_id = desktop_folder_id("/CTOX/Generated Documents/reports");
        assert_eq!(
            small.get("parent_id").and_then(Value::as_str),
            Some(reports_folder_id.as_str())
        );

        let reports_folder_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [reports_folder_id.as_str()],
                |row| row.get(0),
            )
            .expect("reports folder row");
        let reports_folder: Value =
            serde_json::from_str(&reports_folder_json).expect("reports folder json");
        assert_eq!(
            reports_folder.get("kind").and_then(Value::as_str),
            Some("folder")
        );
        assert_eq!(
            reports_folder.get("path").and_then(Value::as_str),
            Some("/CTOX/Generated Documents/reports")
        );

        let large_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [large_id.as_str()],
                |row| row.get(0),
            )
            .expect("large file row");
        let large: Value = serde_json::from_str(&large_json).expect("large json");
        assert_eq!(
            large.get("content_state").and_then(Value::as_str),
            Some("lazy")
        );

        let small_chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 WHERE id LIKE ?1",
                params![format!("{small_id}_%")],
                |row| row.get(0),
            )
            .expect("small chunks count");
        let large_chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 WHERE id LIKE ?1",
                params![format!("{large_id}_%")],
                |row| row.get(0),
            )
            .expect("large chunks count");
        assert!(small_chunks > 0);
        assert_eq!(large_chunks, 0);

        fs::remove_file(&small_path).expect("remove small file");
        drop(conn);
        let indexed_after_delete =
            sync_desktop_file_index(root.path()).expect("sync desktop file index after delete");
        assert_eq!(indexed_after_delete, 1);
        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let deleted_small_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [small_id.as_str()],
                |row| row.get(0),
            )
            .expect("deleted small file row");
        let deleted_small: Value =
            serde_json::from_str(&deleted_small_json).expect("deleted small json");
        assert_eq!(
            deleted_small.get("is_deleted").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            deleted_small.get("content_state").and_then(Value::as_str),
            Some("missing")
        );

        fs::remove_file(&large_path).expect("remove large file");
        drop(conn);
        let indexed_after_empty_root =
            sync_desktop_file_index(root.path()).expect("sync empty desktop file index");
        assert_eq!(indexed_after_empty_root, 0);
        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let deleted_large_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [large_id.as_str()],
                |row| row.get(0),
            )
            .expect("deleted large file row");
        let deleted_large: Value =
            serde_json::from_str(&deleted_large_json).expect("deleted large json");
        assert_eq!(
            deleted_large.get("is_deleted").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn sync_desktop_files_from_workspace_root_indexes_agent_workspace() {
        let root = tempfile::tempdir().expect("temp root");
        let workspace = root.path().join("agent-workspace");
        let nested = workspace.join("reports");
        fs::create_dir_all(&nested).expect("create workspace dirs");
        let file_path = nested.join("brief.md");
        fs::write(&file_path, b"# Brief\n\nvisible from Business OS\n").expect("write file");

        let indexed = sync_desktop_files_from_workspace_root(root.path(), &workspace)
            .expect("sync workspace root");
        assert_eq!(indexed, 1);

        let file_id = desktop_file_id(&file_path.canonicalize().expect("canonical file"));
        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let file_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [file_id.as_str()],
                |row| row.get(0),
            )
            .expect("workspace file row");
        let file: Value = serde_json::from_str(&file_json).expect("file json");
        assert_eq!(
            file.get("virtual_path").and_then(Value::as_str),
            Some("/CTOX/agent-workspace/reports/brief.md")
        );
        assert_eq!(
            file.get("content_state").and_then(Value::as_str),
            Some("available")
        );
        assert_eq!(
            file.get("source").and_then(Value::as_str),
            Some("ctox-core")
        );

        let chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 WHERE id LIKE ?1",
                params![format!("{file_id}_%")],
                |row| row.get(0),
            )
            .expect("chunks count");
        assert_eq!(chunks, 1);
    }

    fn read_desktop_file_row(conn: &Connection, file_id: &str) -> Value {
        let file_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [file_id],
                |row| row.get(0),
            )
            .expect("desktop file row");
        serde_json::from_str(&file_json).expect("desktop file json")
    }

    fn read_desktop_file_chunks(
        conn: &Connection,
        file_id: &str,
        include_deleted: bool,
    ) -> Vec<Value> {
        let deleted_clause = if include_deleted {
            ""
        } else {
            "AND deleted = 0"
        };
        let sql = format!(
            "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 \
             WHERE id LIKE ?1 {deleted_clause}"
        );
        let mut rows = conn.prepare(&sql).expect("chunk query");
        rows.query_map(params![format!("{file_id}_%")], |row| {
            row.get::<_, String>(0)
        })
        .expect("chunk rows")
        .map(|row| serde_json::from_str(&row.expect("chunk row")).expect("chunk json"))
        .collect()
    }

    fn reconstruct_desktop_file_chunks(
        chunks: &[Value],
        file_id: &str,
        generation_id: &str,
        content_hash: &str,
    ) -> anyhow::Result<Vec<u8>> {
        anyhow::ensure!(!chunks.is_empty(), "chunk set is empty");
        let total = chunks[0]
            .get("total")
            .and_then(Value::as_u64)
            .context("chunk total is missing")? as usize;
        anyhow::ensure!(chunks.len() == total, "chunk set is incomplete");

        let mut ordered = chunks.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|chunk| chunk.get("idx").and_then(Value::as_u64).unwrap_or(u64::MAX));
        for (expected_idx, chunk) in ordered.iter().enumerate() {
            anyhow::ensure!(
                chunk.get("file_id").and_then(Value::as_str) == Some(file_id),
                "unexpected chunk file id"
            );
            anyhow::ensure!(
                chunk.get("generation_id").and_then(Value::as_str) == Some(generation_id),
                "unexpected chunk generation"
            );
            anyhow::ensure!(
                chunk.get("content_hash").and_then(Value::as_str) == Some(content_hash),
                "unexpected chunk content hash"
            );
            anyhow::ensure!(
                chunk.get("content_hash_scheme").and_then(Value::as_str)
                    == Some(DESKTOP_FILE_CONTENT_HASH_SCHEME),
                "unexpected chunk content hash scheme"
            );
            anyhow::ensure!(
                chunk.get("chunk_hash_scheme").and_then(Value::as_str)
                    == Some(DESKTOP_FILE_CHUNK_HASH_SCHEME),
                "unexpected chunk hash scheme"
            );
            anyhow::ensure!(
                chunk.get("idx").and_then(Value::as_u64) == Some(expected_idx as u64),
                "chunk index is not contiguous"
            );
            anyhow::ensure!(
                chunk.get("total").and_then(Value::as_u64) == Some(total as u64),
                "chunk total mismatch"
            );
            let data = chunk
                .get("data")
                .and_then(Value::as_str)
                .context("chunk data is missing")?;
            anyhow::ensure!(
                chunk.get("size_bytes").and_then(Value::as_u64) == Some(data.len() as u64),
                "chunk size does not match encoded data"
            );
            anyhow::ensure!(
                chunk.get("chunk_hash").and_then(Value::as_str)
                    == Some(hex_sha256(data.as_bytes()).as_str()),
                "unexpected chunk hash"
            );
        }

        let encoded = ordered
            .iter()
            .filter_map(|chunk| chunk.get("data").and_then(Value::as_str))
            .collect::<String>();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .context("decode desktop file chunks")?;
        anyhow::ensure!(
            hex_sha256(&decoded) == content_hash,
            "decoded content hash mismatch"
        );
        Ok(decoded)
    }

    fn assert_chunk_integrity_error(result: anyhow::Result<Vec<u8>>, expected: &str) {
        let err = result.expect_err("chunk integrity must reject invalid fixture");
        let message = format!("{err:#}");
        assert!(
            message.contains(expected),
            "expected error containing {expected:?}, got {message:?}"
        );
    }

    #[test]
    fn sync_desktop_files_from_workspace_root_tombstones_renamed_files() {
        let root = tempfile::tempdir().expect("temp root");
        let workspace = root.path().join("rename-workspace");
        fs::create_dir_all(&workspace).expect("create workspace");
        let original_path = workspace.join("draft.md");
        let renamed_path = workspace.join("final.md");
        fs::write(&original_path, b"# Draft\n").expect("write original file");

        let indexed = sync_desktop_files_from_workspace_root(root.path(), &workspace)
            .expect("sync original workspace root");
        assert_eq!(indexed, 1);
        let original_id =
            desktop_file_id(&original_path.canonicalize().expect("canonical original"));

        fs::rename(&original_path, &renamed_path).expect("rename workspace file");
        let indexed_after_rename = sync_desktop_files_from_workspace_root(root.path(), &workspace)
            .expect("sync renamed workspace root");
        assert_eq!(indexed_after_rename, 1);
        let renamed_id = desktop_file_id(&renamed_path.canonicalize().expect("canonical renamed"));

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let original_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [original_id.as_str()],
                |row| row.get(0),
            )
            .expect("original tombstone row");
        let original: Value = serde_json::from_str(&original_json).expect("original json");
        assert_eq!(
            original.get("is_deleted").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            original.get("_deleted").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            original.get("content_state").and_then(Value::as_str),
            Some("missing")
        );
        assert_eq!(
            original.get("tombstone_reason").and_then(Value::as_str),
            Some("missing_from_scan")
        );
        assert!(original
            .get("deleted_at_ms")
            .and_then(Value::as_u64)
            .is_some());

        let renamed_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [renamed_id.as_str()],
                |row| row.get(0),
            )
            .expect("renamed active row");
        let renamed: Value = serde_json::from_str(&renamed_json).expect("renamed json");
        assert_eq!(
            renamed.get("name").and_then(Value::as_str),
            Some("final.md")
        );
        assert_eq!(
            renamed.get("is_deleted").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            renamed.get("virtual_path").and_then(Value::as_str),
            Some("/CTOX/rename-workspace/final.md")
        );

        let active_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_files__v0",
                [],
                |row| row.get(0),
            )
            .expect("desktop file count");
        assert!(
            active_count >= 3,
            "expected folder, tombstone, and renamed file"
        );
    }

    #[test]
    fn bounded_workspace_scan_does_not_tombstone_when_limit_is_reached() {
        let root = tempfile::tempdir().expect("temp root");
        let workspace = root.path().join("large-workspace");
        fs::create_dir_all(&workspace).expect("create workspace");
        for idx in 0..=DESKTOP_FILE_SCAN_MAX_FILES {
            fs::write(workspace.join(format!("file-{idx:03}.md")), b"visible")
                .expect("write workspace file");
        }

        let indexed = sync_desktop_files_from_workspace_root(root.path(), &workspace)
            .expect("sync large workspace root");
        assert_eq!(indexed, DESKTOP_FILE_SCAN_MAX_FILES);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let (removed_id, removed_path) = {
            let mut rows = conn
                .prepare("SELECT id, data FROM ctox_business_os__desktop_files__v0")
                .expect("workspace file rows query");
            let mut found = None;
            for row in rows
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .expect("workspace file rows")
            {
                let Ok((id, data)) = row else {
                    continue;
                };
                let Ok(document) = serde_json::from_str::<Value>(&data) else {
                    continue;
                };
                if document.get("kind").and_then(Value::as_str) != Some("file") {
                    continue;
                }
                let Some(local_path) = document.get("local_path").and_then(Value::as_str) else {
                    continue;
                };
                found = Some((id, local_path.to_string()));
                break;
            }
            found.expect("indexed workspace file row")
        };
        drop(conn);
        fs::remove_file(&removed_path).expect("remove indexed file");

        let indexed_after_remove = sync_desktop_files_from_workspace_root(root.path(), &workspace)
            .expect("sync large workspace root after remove");
        assert_eq!(indexed_after_remove, DESKTOP_FILE_SCAN_MAX_FILES);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let row_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [removed_id.as_str()],
                |row| row.get(0),
            )
            .expect("removed file row still present");
        let row: Value = serde_json::from_str(&row_json).expect("removed row json");
        assert_eq!(row.get("is_deleted").and_then(Value::as_bool), Some(false));
    }

    #[test]
    fn sync_ticket_state_projects_local_ticket_items_and_events() {
        let root = tempfile::tempdir().expect("temp root");
        let remote = crate::mission::ticket_local_native::create_local_ticket(
            root.path(),
            "Business OS ticket smoke",
            "Ticket projection must reach native RxDB.",
            Some("open"),
            Some("normal"),
        )
        .expect("create local ticket");
        tickets::sync_ticket_system(root.path(), "local").expect("sync local tickets");

        let synced = sync_ticket_state(root.path()).expect("sync ticket projections");
        assert!(synced >= 2, "expected item and event projection");

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let ticket_key = format!("local:{}", remote.ticket_id);
        let item_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__ctox_ticket_items__v0 WHERE id = ?1",
                [ticket_key.as_str()],
                |row| row.get(0),
            )
            .expect("ticket item projection");
        let item: Value = serde_json::from_str(&item_json).expect("ticket item json");
        assert_eq!(
            item.get("title").and_then(Value::as_str),
            Some("Business OS ticket smoke")
        );
        assert_eq!(
            item.get("source_system").and_then(Value::as_str),
            Some("local")
        );
        assert!(
            item.get("updated_at_ms")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                > 0
        );

        let event_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__ctox_ticket_events__v0 WHERE json_extract(data, '$.ticket_key') = ?1",
                [ticket_key.as_str()],
                |row| row.get(0),
            )
            .expect("ticket event projection count");
        assert!(event_count >= 1, "expected projected ticket events");
    }

    #[test]
    fn native_peer_consumes_pending_business_command() {
        let root = tempfile::tempdir().expect("temp root");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let database = open_test_database(store::rxdb_store_path(root.path()))
                .await
                .expect("open rxdb sqlite");
            database
                .add_collections(collection_creators())
                .await
                .expect("register collections");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            let pending_command = json!({
                "id": "cmd_native_consumer",
                "command_id": "cmd_native_consumer",
                "module": "ctox",
                "command_type": "business_os.test",
                "record_id": "",
                "status": "pending_sync",
                "inbound_channel": "ctox",
                "payload": { "title": "Native consumer test", "instruction": "test only" },
                "client_context": {},
                "updated_at_ms": now_ms() as u64
            });
            commands
                .insert(pending_command.clone())
                .await
                .expect("insert pending command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume pending command");

            let accepted = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_native_consumer" } })),
                    ..Default::default()
                }))
                .expect("accepted query")
                .exec(false)
                .await
                .expect("accepted document");
            assert_eq!(
                accepted.get("status").and_then(Value::as_str),
                Some("accepted")
            );
            let task_id = accepted
                .get("task_id")
                .and_then(Value::as_str)
                .expect("task_id");
            assert!(!task_id.is_empty());

            let queue = database
                .collection("ctox_queue_tasks")
                .expect("ctox_queue_tasks collection");
            let task_doc = queue
                .find(None)
                .expect("task query")
                .exec(false)
                .await
                .expect("task documents");
            let task_docs = task_doc.as_array().expect("task documents array");
            assert!(
                task_docs
                    .iter()
                    .any(|doc| doc.get("id").and_then(Value::as_str) == Some(task_id)),
                "missing task projection for {task_id}: {task_docs:?}"
            );

            accept_pending_business_command(root.path(), &database, pending_command.clone())
                .await
                .expect("consume duplicate command");
            let duplicate = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_native_consumer" } })),
                    ..Default::default()
                }))
                .expect("duplicate query")
                .exec(false)
                .await
                .expect("duplicate document");
            assert_eq!(
                duplicate.get("status").and_then(Value::as_str),
                Some("accepted")
            );
            assert_eq!(
                duplicate.get("task_id").and_then(Value::as_str),
                Some(task_id)
            );

            let after_duplicate = queue
                .find(None)
                .expect("task query after duplicate")
                .exec(false)
                .await
                .expect("task documents after duplicate");
            let duplicate_task_count = after_duplicate
                .as_array()
                .expect("task documents array after duplicate")
                .iter()
                .filter(|doc| doc.get("id").and_then(Value::as_str) == Some(task_id))
                .count();
            assert_eq!(duplicate_task_count, 1);

            let mut replayed_pending = pending_command.clone();
            if let Some(obj) = replayed_pending.as_object_mut() {
                obj.insert(
                    "status".to_string(),
                    Value::String("pending_sync".to_string()),
                );
                obj.remove("task_id");
                obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
            }
            commands
                .incremental_upsert(replayed_pending.clone())
                .await
                .expect("replay pending command after restart");
            accept_pending_business_command(root.path(), &database, replayed_pending)
                .await
                .expect("consume replayed pending command");
            let replayed = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_native_consumer" } })),
                    ..Default::default()
                }))
                .expect("replayed query")
                .exec(false)
                .await
                .expect("replayed document");
            assert_eq!(
                replayed.get("status").and_then(Value::as_str),
                Some("accepted")
            );
            assert_eq!(
                replayed.get("task_id").and_then(Value::as_str),
                Some(task_id)
            );
        });
    }

    #[test]
    fn native_peer_consumes_ticket_local_create_command_and_projects_ticket() {
        let root = tempfile::tempdir().expect("temp root");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let database = open_test_database(store::rxdb_store_path(root.path()))
                .await
                .expect("open rxdb sqlite");
            database
                .add_collections(collection_creators())
                .await
                .expect("register collections");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            commands
                .insert(json!({
                    "id": "cmd_ticket_local_create",
                    "command_id": "cmd_ticket_local_create",
                    "module": "tickets",
                    "command_type": "ctox.ticket.local.create",
                    "record_id": "local:test",
                    "status": "pending_sync",
                    "inbound_channel": "tickets",
                    "payload": {
                        "title": "Business OS command ticket",
                        "body": "Created through business_commands.",
                        "status": "open",
                        "priority": "normal"
                    },
                    "client_context": {
                        "actor": {
                            "id": "tester",
                            "display_name": "Tester",
                            "role": "admin",
                            "is_admin": true
                        }
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert pending ticket command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume pending ticket command");

            let accepted = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_ticket_local_create" } })),
                    ..Default::default()
                }))
                .expect("accepted query")
                .exec(false)
                .await
                .expect("accepted document");
            assert_eq!(
                accepted.get("status").and_then(Value::as_str),
                Some("completed")
            );
            assert_eq!(
                accepted.get("task_status").and_then(Value::as_str),
                Some("completed")
            );

            let tickets = database
                .collection("ctox_ticket_items")
                .expect("ctox_ticket_items collection");
            let docs = tickets
                .find(None)
                .expect("ticket query")
                .exec(false)
                .await
                .expect("ticket documents");
            let docs = docs.as_array().expect("ticket documents array");
            assert!(
                docs.iter().any(|doc| {
                    doc.get("title").and_then(Value::as_str) == Some("Business OS command ticket")
                        && doc.get("source_system").and_then(Value::as_str) == Some("local")
                }),
                "missing projected ticket item: {docs:?}"
            );
        });
    }

    #[test]
    fn native_peer_marks_invalid_ticket_commands_failed() {
        let root = tempfile::tempdir().expect("temp root");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let database = open_test_database(store::rxdb_store_path(root.path()))
                .await
                .expect("open rxdb sqlite");
            database
                .add_collections(collection_creators())
                .await
                .expect("register collections");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");

            for (id, command_type, payload) in [
                (
                    "cmd_ticket_unsupported",
                    "ctox.ticket.unsupported",
                    json!({"case_id": "case_missing"}),
                ),
                (
                    "cmd_ticket_missing_title",
                    "ctox.ticket.local.create",
                    json!({"body": "missing title"}),
                ),
            ] {
                commands
                    .insert(json!({
                        "id": id,
                        "command_id": id,
                        "module": "tickets",
                        "command_type": command_type,
                        "record_id": "",
                        "status": "pending_sync",
                        "inbound_channel": "tickets",
                        "payload": payload,
                        "client_context": {
                            "actor": {
                                "id": "tester",
                                "display_name": "Tester",
                                "role": "admin",
                                "is_admin": true
                            }
                        },
                        "updated_at_ms": now_ms() as u64
                    }))
                    .await
                    .expect("insert invalid ticket command");
            }

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume invalid ticket commands");

            let unsupported = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_ticket_unsupported" } })),
                    ..Default::default()
                }))
                .expect("unsupported query")
                .exec(false)
                .await
                .expect("unsupported document");
            assert_eq!(
                unsupported.get("status").and_then(Value::as_str),
                Some("failed")
            );
            assert!(
                unsupported
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .contains("unsupported Business OS ticket command"),
                "unexpected unsupported error: {unsupported:?}"
            );

            let missing_title = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_ticket_missing_title" } })),
                    ..Default::default()
                }))
                .expect("missing title query")
                .exec(false)
                .await
                .expect("missing title document");
            assert_eq!(
                missing_title.get("status").and_then(Value::as_str),
                Some("failed")
            );
            assert!(
                missing_title
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .contains("title is required"),
                "unexpected missing title error: {missing_title:?}"
            );
        });
    }

    #[test]
    fn native_peer_consumes_pending_ctox_task_update_command() {
        let root = tempfile::tempdir().expect("temp root");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let database = open_test_database(store::rxdb_store_path(root.path()))
                .await
                .expect("open rxdb sqlite");
            database
                .add_collections(collection_creators())
                .await
                .expect("register collections");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            commands
                .insert(json!({
                    "id": "cmd_task_seed",
                    "command_id": "cmd_task_seed",
                    "module": "ctox",
                    "command_type": "business_os.test",
                    "record_id": "",
                    "status": "pending_sync",
                    "inbound_channel": "ctox",
                    "payload": { "title": "Task seed", "instruction": "test only" },
                    "client_context": {},
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert seed command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume seed command");

            let seeded = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_task_seed" } })),
                    ..Default::default()
                }))
                .expect("seed query")
                .exec(false)
                .await
                .expect("seed document");
            let task_id = seeded
                .get("task_id")
                .and_then(Value::as_str)
                .expect("task_id")
                .to_string();

            commands
                .insert(json!({
                    "id": "cmd_task_update",
                    "command_id": "cmd_task_update",
                    "module": "ctox",
                    "command_type": "ctox.task.update",
                    "record_id": task_id.clone(),
                    "status": "pending_sync",
                    "inbound_channel": "business_os.ctox",
                    "payload": {
                        "task_id": task_id.clone(),
                        "title": "Updated through RxDB",
                        "prompt": "updated prompt",
                        "priority": "high"
                    },
                    "client_context": {
                        "actor": {
                            "id": "chef",
                            "display_name": "Chef",
                            "role": "chef",
                            "is_admin": false
                        }
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert task update command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume task update command");

            let update = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_task_update" } })),
                    ..Default::default()
                }))
                .expect("update query")
                .exec(false)
                .await
                .expect("update document");
            assert_eq!(
                update.get("status").and_then(Value::as_str),
                Some("completed")
            );
            assert_eq!(
                update.get("task_status").and_then(Value::as_str),
                Some("updated")
            );

            let queue = database
                .collection("ctox_queue_tasks")
                .expect("ctox_queue_tasks collection");
            let task_doc = queue
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": task_id } })),
                    ..Default::default()
                }))
                .expect("task query")
                .exec(false)
                .await
                .expect("task document");
            assert_eq!(
                task_doc.get("title").and_then(Value::as_str),
                Some("Updated through RxDB")
            );
            assert_eq!(
                task_doc.get("priority").and_then(Value::as_str),
                Some("high")
            );
        });
    }

    #[test]
    fn native_peer_consumes_pending_knowledge_command() {
        let root = tempfile::tempdir().expect("temp root");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let database = open_test_database(store::rxdb_store_path(root.path()))
                .await
                .expect("open rxdb sqlite");
            database
                .add_collections(collection_creators())
                .await
                .expect("register collections");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            commands
                .insert(json!({
                    "id": "cmd_knowledge_help",
                    "command_id": "cmd_knowledge_help",
                    "module": "knowledge",
                    "command_type": "knowledge.command",
                    "record_id": "cmd_knowledge_help",
                    "status": "pending_sync",
                    "inbound_channel": "business_os.outbound",
                    "payload": { "title": "Knowledge help", "args": ["help"] },
                    "client_context": { "source_module": "outbound" },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert knowledge command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume knowledge command");

            let doc = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_knowledge_help" } })),
                    ..Default::default()
                }))
                .expect("knowledge query")
                .exec(false)
                .await
                .expect("knowledge document");
            assert_eq!(doc.get("status").and_then(Value::as_str), Some("completed"));
            assert_eq!(
                doc.get("result")
                    .and_then(|result| result.get("ok"))
                    .and_then(Value::as_bool),
                Some(true)
            );
        });
    }

    #[test]
    fn native_peer_consumes_pending_report_command() {
        let root = tempfile::tempdir().expect("temp root");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let database = open_test_database(store::rxdb_store_path(root.path()))
                .await
                .expect("open rxdb sqlite");
            database
                .add_collections(collection_creators())
                .await
                .expect("register collections");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            commands
                .insert(json!({
                    "id": "cmd_report_bug",
                    "command_id": "cmd_report_bug",
                    "module": "ctox",
                    "command_type": "ctox.report.bug",
                    "record_id": "report_rxdb_bug",
                    "status": "pending_sync",
                    "inbound_channel": "ctox",
                    "payload": {
                        "module_id": "ctox",
                        "kind": "bug",
                        "severity": "high",
                        "title": "Report through RxDB",
                        "summary": "The browser report should become a native task",
                        "expected": "Report and bug projections are available"
                    },
                    "client_context": {
                        "actor": {
                            "id": "business-user",
                            "display_name": "Business User",
                            "role": "user",
                            "is_admin": false
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert report command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume report command");

            let accepted = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_report_bug" } })),
                    ..Default::default()
                }))
                .expect("accepted report query")
                .exec(false)
                .await
                .expect("accepted report document");
            assert_eq!(
                accepted.get("status").and_then(Value::as_str),
                Some("accepted")
            );
            assert_eq!(
                accepted.get("report_id").and_then(Value::as_str),
                Some("report_rxdb_bug")
            );

            let reports = database
                .collection("business_module_reports")
                .expect("business_module_reports collection");
            let report = reports
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "report_rxdb_bug" } })),
                    ..Default::default()
                }))
                .expect("report query")
                .exec(false)
                .await
                .expect("report document");
            assert_eq!(
                report.get("title").and_then(Value::as_str),
                Some("Report through RxDB")
            );
            assert_eq!(report.get("status").and_then(Value::as_str), Some("open"));
            assert_eq!(
                report.get("ctox_command_id").and_then(Value::as_str),
                Some("cmd_report_bug")
            );

            let bugs = database
                .collection("ctox_bug_reports")
                .expect("ctox_bug_reports collection");
            let bug_docs = bugs
                .find(None)
                .expect("bug list query")
                .exec(false)
                .await
                .expect("bug documents");
            let bug = bug_docs
                .as_array()
                .and_then(|documents| {
                    documents.iter().find(|document| {
                        document.get("id").and_then(Value::as_str) == Some("report_rxdb_bug")
                    })
                })
                .expect("bug projection");
            assert_eq!(
                bug.get("title").and_then(Value::as_str),
                Some("Report through RxDB")
            );
        });
    }

    #[test]
    fn native_peer_consumes_pending_source_commands() {
        let root = tempfile::tempdir().expect("temp root");
        let module_root = root.path().join("src/apps/business-os/modules/source-demo");
        std::fs::create_dir_all(&module_root).expect("create module root");
        std::fs::write(root.path().join("src/apps/business-os/index.html"), "")
            .expect("write app index");
        std::fs::write(
            module_root.join("module.json"),
            r#"{"id":"source-demo","title":"Source Demo"}"#,
        )
        .expect("write module manifest");
        std::fs::write(module_root.join("index.js"), "export const value = 1;\n")
            .expect("write source");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let database = open_test_database(store::rxdb_store_path(root.path()))
                .await
                .expect("open rxdb sqlite");
            database
                .add_collections(collection_creators())
                .await
                .expect("register collections");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            commands
                .insert(json!({
                    "id": "cmd_source_load",
                    "command_id": "cmd_source_load",
                    "module": "ctox",
                    "command_type": "ctox.source.load",
                    "record_id": "source-demo:source",
                    "status": "pending_sync",
                    "inbound_channel": "source-demo",
                    "payload": { "module_id": "source-demo" },
                    "client_context": { "source": "test" },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert source load command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume source load command");

            let source_files = database
                .collection("business_module_source_files")
                .expect("business_module_source_files collection");
            let source_file = source_files
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "source-demo:index.js" } })),
                    ..Default::default()
                }))
                .expect("source query")
                .exec(false)
                .await
                .expect("source document");
            assert_eq!(
                source_file.get("content").and_then(Value::as_str),
                Some("export const value = 1;\n")
            );

            commands
                .insert(json!({
                    "id": "cmd_source_save",
                    "command_id": "cmd_source_save",
                    "module": "ctox",
                    "command_type": "ctox.source.save",
                    "record_id": "source-demo:index.js",
                    "status": "pending_sync",
                    "inbound_channel": "source-demo",
                    "payload": {
                        "module_id": "source-demo",
                        "path": "index.js",
                        "content": "export const value = 2;\n"
                    },
                    "client_context": {
                        "actor": {
                            "id": "chef",
                            "display_name": "Chef",
                            "role": "chef",
                            "is_admin": false
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert source save command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume source save command");

            assert_eq!(
                std::fs::read_to_string(module_root.join("index.js")).expect("read saved source"),
                "export const value = 2;\n"
            );
            let saved = source_files
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "source-demo:index.js" } })),
                    ..Default::default()
                }))
                .expect("saved source query")
                .exec(false)
                .await
                .expect("saved source document");
            assert_eq!(
                saved.get("content").and_then(Value::as_str),
                Some("export const value = 2;\n")
            );
            assert!(
                saved
                    .get("previous_sha256")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .len()
                    >= 32
            );
        });
    }

    #[test]
    fn native_peer_consumes_pending_module_governance_commands() {
        let root = tempfile::tempdir().expect("temp root");
        let module_root = root.path().join("src/apps/business-os/modules/gov-demo");
        std::fs::create_dir_all(&module_root).expect("create module root");
        std::fs::write(root.path().join("src/apps/business-os/index.html"), "")
            .expect("write app index");
        std::fs::write(
            module_root.join("module.json"),
            r#"{"id":"gov-demo","title":"Governance Demo v1"}"#,
        )
        .expect("write module manifest");
        let template_source = root
            .path()
            .join("src/apps/business-os/modules/template-source");
        std::fs::create_dir_all(&template_source).expect("create template source");
        std::fs::write(
            template_source.join("module.json"),
            r#"{"id":"template-source","title":"Template Source","entry":"modules/template-source/index.html","collections":["business_commands"]}"#,
        )
        .expect("write template source manifest");
        std::fs::write(template_source.join("index.html"), "<div></div>")
            .expect("write template source html");
        let template_root = root
            .path()
            .join("src/apps/business-os/template-store/simple");
        std::fs::create_dir_all(&template_root).expect("create template root");
        std::fs::write(
            template_root.join("template.json"),
            r#"{"id":"simple","title":"Simple Template","source_module":"template-source","default_title":"Simple Installed"}"#,
        )
        .expect("write template manifest");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let database = open_test_database(store::rxdb_store_path(root.path()))
                .await
                .expect("open rxdb sqlite");
            database
                .add_collections(collection_creators())
                .await
                .expect("register collections");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            let releases = database
                .collection("business_module_releases")
                .expect("business_module_releases collection");
            let acl = database
                .collection("business_module_acl")
                .expect("business_module_acl collection");

            commands
                .insert(json!({
                    "id": "cmd_user_upsert",
                    "command_id": "cmd_user_upsert",
                    "module": "ctox",
                    "command_type": "ctox.business_os.user.upsert",
                    "record_id": "module-owner",
                    "status": "pending_sync",
                    "inbound_channel": "ctox",
                    "payload": {
                        "id": "module-owner",
                        "display_name": "Module Owner",
                        "role": "founder",
                        "active": true
                    },
                    "client_context": {
                        "actor": {
                            "id": "admin",
                            "display_name": "Admin",
                            "role": "admin",
                            "is_admin": true
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert user command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume user command");

            let user_command = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_user_upsert" } })),
                    ..Default::default()
                }))
                .expect("user command query")
                .exec(false)
                .await
                .expect("user command document");
            assert_eq!(
                user_command.get("status").and_then(Value::as_str),
                Some("completed"),
                "user_command={user_command}"
            );
            let has_module_owner = user_command
                .get("result")
                .and_then(|result| result.get("users"))
                .and_then(Value::as_array)
                .map(|users| {
                    users.iter().any(|user| {
                        user.get("id").and_then(Value::as_str) == Some("module-owner")
                            && user.get("role").and_then(Value::as_str) == Some("founder")
                    })
                })
                .unwrap_or(false);
            assert!(has_module_owner, "user_command={user_command}");

            commands
                .insert(json!({
                    "id": "cmd_runtime_settings_save",
                    "command_id": "cmd_runtime_settings_save",
                    "module": "ctox",
                    "command_type": "ctox.runtime_settings.save",
                    "record_id": "runtime-settings",
                    "status": "pending_sync",
                    "inbound_channel": "ctox",
                    "payload": {
                        "provider": "local",
                        "auth_mode": "local",
                        "chat_model": "local-test",
                        "context": "128k",
                        "max_run_secs": 120
                    },
                    "client_context": {
                        "actor": {
                            "id": "chef",
                            "display_name": "Chef",
                            "role": "chef",
                            "is_admin": false
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert runtime settings command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume runtime settings command");

            let runtime_command = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_runtime_settings_save" } })),
                    ..Default::default()
                }))
                .expect("runtime settings command query")
                .exec(false)
                .await
                .expect("runtime settings command document");
            assert_eq!(
                runtime_command.get("status").and_then(Value::as_str),
                Some("completed"),
                "runtime_command={runtime_command}"
            );

            commands
                .insert(json!({
                    "id": "cmd_channel_jami_export",
                    "command_id": "cmd_channel_jami_export",
                    "module": "ctox",
                    "command_type": "ctox.channel.jami.export",
                    "record_id": "jami-export",
                    "status": "pending_sync",
                    "inbound_channel": "ctox",
                    "payload": {},
                    "client_context": {
                        "actor": {
                            "id": "chef",
                            "display_name": "Chef",
                            "role": "chef",
                            "is_admin": false
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert channel command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume channel command");

            let channel_command = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_channel_jami_export" } })),
                    ..Default::default()
                }))
                .expect("channel command query")
                .exec(false)
                .await
                .expect("channel command document");
            assert_eq!(
                channel_command.get("status").and_then(Value::as_str),
                Some("completed"),
                "channel_command={channel_command}"
            );
            assert_eq!(
                channel_command
                    .get("result")
                    .and_then(|result| result.get("error"))
                    .and_then(Value::as_str),
                Some("not_implemented"),
                "channel_command={channel_command}"
            );

            commands
                .insert(json!({
                    "id": "cmd_gov_assign_founder",
                    "command_id": "cmd_gov_assign_founder",
                    "module": "ctox",
                    "command_type": "ctox.module.assign_founder",
                    "record_id": "gov-demo:founder:module-owner",
                    "status": "pending_sync",
                    "inbound_channel": "gov-demo",
                    "payload": { "module_id": "gov-demo", "user_id": "module-owner", "active": true },
                    "client_context": {
                        "actor": {
                            "id": "chef",
                            "display_name": "Chef",
                            "role": "chef",
                            "is_admin": false
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert founder command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume founder command");

            let founder_acl = acl
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "gov-demo:founder:module-owner" } })),
                    ..Default::default()
                }))
                .expect("founder acl query")
                .exec(false)
                .await
                .expect("founder acl projection");
            let founder_active = founder_acl
                .get("active")
                .and_then(|value| value.as_bool().or_else(|| value.as_i64().map(|raw| raw != 0)));
            assert_eq!(founder_active, Some(true), "founder_acl={founder_acl}");

            commands
                .insert(json!({
                    "id": "cmd_gov_module_save",
                    "command_id": "cmd_gov_module_save",
                    "module": "ctox",
                    "command_type": "ctox.module.save",
                    "record_id": "new-module",
                    "status": "pending_sync",
                    "inbound_channel": "new-module",
                    "payload": {
                        "id": "new-module",
                        "title": "New Module",
                        "description": "Created via RxDB",
                        "entry": "installed-modules/new-module/index.html",
                        "collections": ["business_commands"],
                        "layout": { "shell": "pane", "center": "module workspace" }
                    },
                    "client_context": {
                        "actor": {
                            "id": "chef",
                            "display_name": "Chef",
                            "role": "chef",
                            "is_admin": false
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert module save command");
            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume module save command");
            assert!(
                root.path()
                    .join("src/apps/business-os/installed-modules/new-module/module.json")
                    .is_file()
            );

            commands
                .insert(json!({
                    "id": "cmd_gov_template_install",
                    "command_id": "cmd_gov_template_install",
                    "module": "ctox",
                    "command_type": "ctox.module.install_template",
                    "record_id": "installed-from-template",
                    "status": "pending_sync",
                    "inbound_channel": "installed-from-template",
                    "payload": {
                        "template_id": "simple",
                        "module_id": "installed-from-template",
                        "title": "Installed From Template"
                    },
                    "client_context": {
                        "actor": {
                            "id": "chef",
                            "display_name": "Chef",
                            "role": "chef",
                            "is_admin": false
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert template install command");
            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume template install command");
            let installed_template_manifest = std::fs::read_to_string(
                root.path()
                    .join("src/apps/business-os/installed-modules/installed-from-template/module.json"),
            )
            .expect("read installed template manifest");
            assert!(installed_template_manifest.contains("Installed From Template"));

            commands
                .insert(json!({
                    "id": "cmd_gov_module_delete",
                    "command_id": "cmd_gov_module_delete",
                    "module": "ctox",
                    "command_type": "ctox.module.delete",
                    "record_id": "new-module",
                    "status": "pending_sync",
                    "inbound_channel": "new-module",
                    "payload": { "module_id": "new-module" },
                    "client_context": {
                        "actor": {
                            "id": "chef",
                            "display_name": "Chef",
                            "role": "chef",
                            "is_admin": false
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert module delete command");
            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume module delete command");
            assert!(
                !root
                    .path()
                    .join("src/apps/business-os/installed-modules/new-module")
                    .exists()
            );

            for (command_id, notes) in [
                ("cmd_gov_release_v1", "release v1"),
                ("cmd_gov_release_v2", "release v2"),
            ] {
                if command_id.ends_with("_v2") {
                    std::fs::write(
                        module_root.join("module.json"),
                        r#"{"id":"gov-demo","title":"Governance Demo v2"}"#,
                    )
                    .expect("write updated manifest");
                }
                commands
                    .insert(json!({
                        "id": command_id,
                        "command_id": command_id,
                        "module": "ctox",
                        "command_type": "ctox.module.release",
                        "record_id": "gov-demo",
                        "status": "pending_sync",
                        "inbound_channel": "gov-demo",
                        "payload": { "module_id": "gov-demo", "notes": notes },
                        "client_context": {
                            "actor": {
                                "id": "chef",
                                "display_name": "Chef",
                                "role": "chef",
                                "is_admin": false
                            },
                            "source": "test"
                        },
                        "updated_at_ms": now_ms() as u64
                    }))
                    .await
                    .expect("insert release command");

                consume_pending_business_commands(root.path(), &database)
                    .await
                    .expect("consume release command");
            }

            let first_release_command = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_gov_release_v1" } })),
                    ..Default::default()
                }))
                .expect("release command query")
                .exec(false)
                .await
                .expect("release command document");
            let first_version_id = first_release_command
                .get("result")
                .and_then(|result| result.get("version_id"))
                .and_then(Value::as_str)
                .expect("first version id")
                .to_string();
            let second_release_command = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_gov_release_v2" } })),
                    ..Default::default()
                }))
                .expect("second release command query")
                .exec(false)
                .await
                .expect("second release command document");
            let second_version_id = second_release_command
                .get("result")
                .and_then(|result| result.get("version_id"))
                .and_then(Value::as_str)
                .expect("second version id")
                .to_string();

            commands
                .insert(json!({
                    "id": "cmd_gov_rollback",
                    "command_id": "cmd_gov_rollback",
                    "module": "ctox",
                    "command_type": "ctox.module.rollback",
                    "record_id": first_version_id.clone(),
                    "status": "pending_sync",
                    "inbound_channel": "gov-demo",
                    "payload": { "module_id": "gov-demo", "version_id": first_version_id.clone() },
                    "client_context": {
                        "actor": {
                            "id": "chef",
                            "display_name": "Chef",
                            "role": "chef",
                            "is_admin": false
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert rollback command");

            consume_pending_business_commands(root.path(), &database)
                .await
                .expect("consume rollback command");

            let manifest = std::fs::read_to_string(module_root.join("module.json"))
                .expect("read rolled back manifest");
            assert!(manifest.contains("Governance Demo v1"));

            let released = releases
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": first_version_id } })),
                    ..Default::default()
                }))
                .expect("v1 release query")
                .exec(false)
                .await
                .expect("v1 release projection");
            let rolled_back = releases
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": second_version_id } })),
                    ..Default::default()
                }))
                .expect("v2 release query")
                .exec(false)
                .await
                .expect("v2 release projection");
            assert_eq!(
                released.get("status").and_then(Value::as_str),
                Some("released")
            );
            assert_eq!(
                rolled_back.get("status").and_then(Value::as_str),
                Some("rolled_back")
            );
        });
    }
}
