//! ============================================================================
//! AGENT GUARDRAILS — Business OS native RxDB peer (read docs/ctox-rxdb.md)
//! ============================================================================
//! Lifecycle rules that took real outages to learn — do not regress them:
//!   * spawn_native_peer SUPERVISES run_native_peer: every non-intentional
//!     exit respawns with capped backoff and re-reads the sync config.
//!     NATIVE_PEER_STARTED is owned by the supervision loop alone.
//!   * WebRTC bring-up failure/timeout is FATAL for the run. "Log and keep
//!     running" creates a zombie: heartbeat healthy, zero replication.
//!   * Heartbeats carry replicationUp — "process alive" and "replication up"
//!     are different facts; never collapse them.
//!   * The signaling URL is produced by a PROVIDER per (re)connect attempt so
//!     token_iat/token_exp stay fresh; never bake the token window in once.
//!   * NO HTTP data path for Business OS records; NO new process-env toggles.
//! ============================================================================

// Origin: CTOX
// License: Apache-2.0

use super::app_runtime;
use super::browser_runtime::{browser_runtime_manager, BrowserSessionAutomationRequest};
use super::command_lifecycle_generated::CTOX_COMMAND_LIFECYCLE_CAPABILITY;
use super::store;
use crate::mission::channels;
use crate::mission::tickets;
use anyhow::Context;
use base64::Engine;
use chrono::{DateTime, FixedOffset};
use notify::event::{AccessKind, AccessMode, EventKind, MetadataKind, ModifyKind};
use notify::Watcher;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OpenFlags, OptionalExtension};
use rxdb::plugins::replication_webrtc::{
    file_fetch_handler::FileRange, CollectionAuthzHook, DocumentReadAuthzHook,
    DocumentWriteAuthzHook, RTCIceServer, RxWebRTCReplicationPool, WebRTCRsConnectionHandler,
};
use rxdb::rx_collection::RxCollection;
use rxdb::rx_collection_helper::fill_object_data_before_insert;
use rxdb::rx_database::{create_rx_database, RxCollectionCreator, RxDatabase, RxDatabaseCreator};
use rxdb::storage::sqlite::{get_rx_storage_sqlite, RxStorageSqliteSettings};
use rxdb::types::{BulkWriteRow, HashOutput, JsonSchema, MangoQuery, RxJsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use url::Url;
use uuid::Uuid;

static NATIVE_PEER_STARTED: AtomicBool = AtomicBool::new(false);
static NATIVE_PEER_RUNNING: AtomicBool = AtomicBool::new(false);
static NATIVE_PEER_SUPERVISOR_STOP: AtomicBool = AtomicBool::new(false);
static NATIVE_PEER: Mutex<Option<Arc<NativePeer>>> = Mutex::new(None);
static TEMPORARY_RXDB_DATABASE_LOCK: Mutex<()> = Mutex::new(());
static NATIVE_RXDB_WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static BROWSER_RUNTIME_COMMAND_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const SIGNALING_TOKEN_TTL_SECONDS: u64 = 24 * 60 * 60;

/// True while the multiplexed WebRTC replication session is up. Written by
/// `run_native_peer`, read by the status heartbeat — a peer whose process is
/// alive but whose replication bring-up failed used to be indistinguishable
/// from a healthy one ("running" status, zero sync).
static NATIVE_PEER_REPLICATION_UP: AtomicBool = AtomicBool::new(false);
static NATIVE_PEER_SIGNALING_JOIN_ACCEPTED: AtomicBool = AtomicBool::new(false);
static NATIVE_PEER_DATA_CHANNEL_OPEN: AtomicBool = AtomicBool::new(false);
static NATIVE_PEER_CRITICAL_TASKS_ALIVE: AtomicBool = AtomicBool::new(false);
static NATIVE_PEER_HEARTBEAT_THREAD_ALIVE: AtomicBool = AtomicBool::new(false);
/// Last outbox depth observed by the async watchdog. The dedicated heartbeat
/// thread must never open CTOX SQLite just to enrich its status payload:
/// waiting on a busy database would make the heartbeat itself appear stale
/// and cause a healthy WebRTC session to be cancelled mid-command.
static NATIVE_PEER_PENDING_OUTBOX: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static DESKTOP_FILE_CHUNK_COMPLETENESS_CHECKS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static CHAT_TRACKING_BATCH_DOCUMENT_LOOKUPS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static NOTES_LOOP_METRICS: NativePeerLoopMetrics = NativePeerLoopMetrics::new("notes");
static DESKTOP_FILE_INDEX_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("desktop_file_index");
static CHANNEL_STATE_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("channel_state");
static BUSINESS_USERS_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("business_users");
static RUNTIME_SETTINGS_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("runtime_settings");
static WORKSPACE_BRANDING_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("workspace_branding");
static MODULE_CATALOG_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("module_catalog");
static TICKET_STATE_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("ticket_state");
static KNOWLEDGE_TABLES_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("knowledge_tables");
static BUSINESS_RECORDS_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("business_records");
static BUSINESS_COMMANDS_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("business_commands");
static BROWSER_RUNTIME_LOOP_METRICS: NativePeerLoopMetrics =
    NativePeerLoopMetrics::new("browser_runtime");
static DEMAND_FILE_FETCH_METRICS: DemandFileFetchMetrics = DemandFileFetchMetrics::new();
static COMMAND_PLANE_METRICS: CommandPlaneMetrics = CommandPlaneMetrics::new();

/// Supervision backoff bounds for respawning the native peer after a
/// non-intentional exit (bring-up failure, watchdog-stale heartbeat,
/// transient SQLite/fs error).
const NATIVE_PEER_RESPAWN_BASE_DELAY_SECS: u64 = 5;
const NATIVE_PEER_RESPAWN_MAX_DELAY_SECS: u64 = 300;
/// A run that stayed up at least this long resets the respawn backoff.
const NATIVE_PEER_RESPAWN_HEALTHY_RUN_SECS: u64 = 600;
const NATIVE_PEER_CIRCUIT_FAILURE_THRESHOLD: u32 = 5;
const NATIVE_PEER_CIRCUIT_OPEN_SECS: u64 = 2 * 60;
const NATIVE_PEER_CIRCUIT_CONFIG_POLL_SECS: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativePeerCircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl NativePeerCircuitState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Open => "open",
            Self::HalfOpen => "half_open",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativePeerSignalingFailure {
    NotSignaling,
    Retryable,
    Permanent,
}

#[derive(Debug, Clone)]
struct NativePeerCircuitBreaker {
    state: NativePeerCircuitState,
    consecutive_failures: u32,
    next_probe_at_ms: Option<u64>,
    permanent: bool,
    config_epoch: String,
    last_error: Option<String>,
}

impl Default for NativePeerCircuitBreaker {
    fn default() -> Self {
        Self {
            state: NativePeerCircuitState::Closed,
            consecutive_failures: 0,
            next_probe_at_ms: None,
            permanent: false,
            config_epoch: String::new(),
            last_error: None,
        }
    }
}

impl NativePeerCircuitBreaker {
    fn reset_for_epoch(&mut self, config_epoch: String) {
        self.state = NativePeerCircuitState::Closed;
        self.consecutive_failures = 0;
        self.next_probe_at_ms = None;
        self.permanent = false;
        self.config_epoch = config_epoch;
        self.last_error = None;
    }

    /// Returns `None` when this supervisor owns the single allowed attempt,
    /// otherwise the bounded delay before config is checked again.
    fn before_attempt(&mut self, config_epoch: String, now_ms: u64) -> Option<Duration> {
        if self.config_epoch != config_epoch {
            self.reset_for_epoch(config_epoch);
        }
        if self.state != NativePeerCircuitState::Open {
            return None;
        }
        if self.permanent {
            return Some(Duration::from_secs(NATIVE_PEER_CIRCUIT_CONFIG_POLL_SECS));
        }
        let next_probe_at_ms = self.next_probe_at_ms.unwrap_or(now_ms);
        if now_ms < next_probe_at_ms {
            let wait_ms = next_probe_at_ms.saturating_sub(now_ms);
            return Some(Duration::from_millis(
                wait_ms.min(NATIVE_PEER_CIRCUIT_CONFIG_POLL_SECS * 1_000),
            ));
        }
        self.state = NativePeerCircuitState::HalfOpen;
        self.next_probe_at_ms = None;
        None
    }

    fn record_success(&mut self) {
        self.state = NativePeerCircuitState::Closed;
        self.consecutive_failures = 0;
        self.next_probe_at_ms = None;
        self.permanent = false;
        self.last_error = None;
    }

    fn record_failure(
        &mut self,
        failure: NativePeerSignalingFailure,
        message: String,
        now_ms: u64,
    ) {
        if failure == NativePeerSignalingFailure::NotSignaling {
            return;
        }
        let was_half_open = self.state == NativePeerCircuitState::HalfOpen;
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_error = Some(message);
        self.permanent = failure == NativePeerSignalingFailure::Permanent;
        if self.permanent
            || was_half_open
            || self.consecutive_failures >= NATIVE_PEER_CIRCUIT_FAILURE_THRESHOLD
        {
            self.state = NativePeerCircuitState::Open;
            self.next_probe_at_ms = (!self.permanent)
                .then_some(now_ms.saturating_add(NATIVE_PEER_CIRCUIT_OPEN_SECS * 1_000));
        }
    }

    fn snapshot(&self) -> Value {
        json!({
            "state": self.state.as_str(),
            "consecutiveFailures": self.consecutive_failures,
            "nextProbeAtMs": self.next_probe_at_ms,
            "permanent": self.permanent,
            "errorCode": if self.state == NativePeerCircuitState::Open {
                Value::String("ctox_signaling_circuit_open".to_string())
            } else {
                Value::Null
            },
            "lastError": self.last_error,
        })
    }
}

static NATIVE_PEER_CIRCUIT_BREAKER: OnceLock<Mutex<NativePeerCircuitBreaker>> = OnceLock::new();

fn native_peer_circuit_breaker() -> &'static Mutex<NativePeerCircuitBreaker> {
    NATIVE_PEER_CIRCUIT_BREAKER.get_or_init(|| Mutex::new(NativePeerCircuitBreaker::default()))
}

fn native_peer_circuit_snapshot() -> Value {
    native_peer_circuit_breaker()
        .lock()
        .map(|breaker| breaker.snapshot())
        .unwrap_or_else(|_| json!({ "state": "unknown" }))
}

/// How `run_native_peer` ended — drives the supervision loop's respawn
/// decision in `spawn_native_peer`.
enum NativePeerExit {
    /// Intentional stop (`restart_native_peer` / service shutdown): no respawn.
    Shutdown,
    /// The watchdog saw a stale heartbeat and tore the peer down: respawn.
    WatchdogStale,
    /// The persisted sync room/signaling config changed: respawn with fresh config.
    ConfigChanged,
    /// Runtime-installed module schemas changed: respawn so new collections are registered.
    RuntimeSchemaChanged,
    /// A load-bearing command/projection child exited unexpectedly: respawn.
    CriticalChildExited,
    /// Another process holds the peer lock: retry later (standby takeover).
    LockHeldElsewhere,
}
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
    "ctox-checkpoint-generation-v2",
    "ctox-app-runtime-v1",
    CTOX_COMMAND_LIFECYCLE_CAPABILITY,
];
const DESKTOP_FILE_CHUNK_SIZE: usize = 16 * 1024;
const SPREADSHEET_BLOB_CHUNK_SIZE: usize = 256_000;
const SPREADSHEET_CSV_IMPORT_LIMIT_BYTES: u64 = 10 * 1024 * 1024;
const SPREADSHEET_CSV_IMPORT_MAX_ROWS: usize = 50_000;
const SPREADSHEET_CSV_IMPORT_MAX_COLUMNS: usize = 512;
const DESKTOP_FILE_CHUNK_DECODED_SIZE: u64 = (DESKTOP_FILE_CHUNK_SIZE as u64 / 4) * 3;
const DESKTOP_FILE_EAGER_LIMIT_BYTES: u64 = 1024 * 1024;
const DESKTOP_FILE_SCAN_INTERVAL_SECS: u64 = 15;
/// Standby reconciliation is a safety net, not the normal data path. Runtime
/// command handlers and explicit sync paths project changes immediately; once
/// the peer has observed an unchanged round, fallback loops must stop touching
/// SQLite/RxDB on short idle windows.
const BUSINESS_OS_STANDBY_RECONCILE_INTERVAL_SECS: u64 = 30 * 60;
const DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS: u64 = BUSINESS_OS_STANDBY_RECONCILE_INTERVAL_SECS;
const DESKTOP_FILE_SCAN_MAX_DEPTH: usize = 6;
const DESKTOP_FILE_SCAN_MAX_FILES: usize = 200;
const DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS: usize = 2;
const DESKTOP_FILE_CHUNK_CLEANUP_SCAN_LIMIT: u64 = 100_000;
const DESKTOP_FILE_INDEX_MAINTENANCE_INTERVAL_SECS: u64 = 10 * 60;
const DESKTOP_FILE_INDEX_MAINTENANCE_FILE_LIMIT: usize = 1_000;
const DESKTOP_FILE_INDEX_MAINTENANCE_CHUNK_DELETE_LIMIT: usize = 5_000;
const DESKTOP_FILE_INDEX_MAINTENANCE_FILE_TOMBSTONE_DELETE_LIMIT: usize = 5_000;
const DESKTOP_FILE_INDEX_UNSAFE_TOMBSTONE_RETENTION_SECS: u64 = 24 * 60 * 60;
const DESKTOP_FILE_CHUNK_CACHE_MAX_LIVE_BYTES: u64 = 64 * 1024 * 1024;
const DESKTOP_FILE_CHUNK_CACHE_TARGET_LIVE_BYTES: u64 = 48 * 1024 * 1024;
const DESKTOP_FILE_CHUNK_CACHE_ACTIVE_MIN_AGE_SECS: u64 = 6 * 60 * 60;
const DESKTOP_FILE_CHUNK_CACHE_CHECKPOINT_MIN_INTERVAL_SECS: u64 = 30 * 60;
const DESKTOP_FILE_CHUNK_CACHE_WAL_CHECKPOINT_MIN_BYTES: u64 = 16 * 1024 * 1024;
const DESKTOP_FILE_CHUNK_CACHE_VACUUM_MIN_INTERVAL_SECS: u64 = 24 * 60 * 60;
const DESKTOP_FILE_CHUNK_CACHE_VACUUM_MIN_RECLAIM_BYTES: u64 = 32 * 1024 * 1024;
const DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE: &str = "ctox_desktop_file_chunk_cache_state";
const DESKTOP_FILE_CHUNK_CACHE_STATE_ID: &str = "desktop_file_chunks";
const DESKTOP_FILE_CONTENT_HASH_SCHEME: &str = "sha256-bytes-v1";
const DESKTOP_FILE_CHUNK_HASH_SCHEME: &str = "sha256-base64-chunk-v1";
const CTOX_DESKTOP_FOLDER_ID: &str = "fs_ctox";
const CTOX_DESKTOP_FOLDER_PATH: &str = "/CTOX";
const NOTES_SYNC_ACTIVE_INTERVAL_SECS: u64 = 3;
const NOTES_SYNC_IDLE_INTERVAL_SECS: u64 = BUSINESS_OS_STANDBY_RECONCILE_INTERVAL_SECS;
const NOTES_SYNC_IDLE_BACKOFF_AFTER_TICKS: u32 = 1;
const CHANNEL_STATE_SYNC_INTERVAL_SECS: u64 = 3;
const RXDB_SQLITE_DATABASE_NAME: &str = "ctox_business_os";
const BUSINESS_USERS_SYNC_INTERVAL_SECS: u64 = 3;
const RUNTIME_SETTINGS_SYNC_INTERVAL_SECS: u64 = 3;
const MODULE_CATALOG_SYNC_INTERVAL_SECS: u64 = 3;
const TICKET_STATE_SYNC_INTERVAL_SECS: u64 = 3;
// Command handlers project changed Business OS data synchronously; the idle
// loops are reconciliation fallbacks and must stay quiet in daemon standby.
const BUSINESS_OS_PROJECTION_IDLE_SYNC_INTERVAL_SECS: u64 =
    BUSINESS_OS_STANDBY_RECONCILE_INTERVAL_SECS;
const BUSINESS_OS_PROJECTION_IDLE_BACKOFF_AFTER_TICKS: u32 = 1;
/// Knowledge tables are record-shape parquet content that changes far less
/// often than ticket/queue state, and projecting them reads parquet rows off
/// disk. A slower interval keeps the parquet I/O light while still surfacing
/// catalog/row changes to the browser within seconds-to-tens-of-seconds.
const KNOWLEDGE_TABLES_SYNC_INTERVAL_SECS: u64 = 15;
const BUSINESS_RECORD_PROJECTION_SYNC_INTERVAL_SECS: u64 = 3;
const BUSINESS_RECORD_PROJECTION_IDLE_SYNC_INTERVAL_SECS: u64 =
    BUSINESS_OS_STANDBY_RECONCILE_INTERVAL_SECS;
const BUSINESS_RECORD_PROJECTION_IDLE_BACKOFF_AFTER_TICKS: u32 = 1;
const BUSINESS_RECORD_PROJECTION_ERROR_BACKOFF_BASE_SECS: u64 = 30;
const BUSINESS_RECORD_PROJECTION_ERROR_BACKOFF_MAX_SECS: u64 = 5 * 60;
const BUSINESS_RECORD_PROJECTION_PARTIAL_SYNC_INTERVAL_SECS: u64 = 30;
const BUSINESS_RECORD_PROJECTION_SYNC_LIMIT: usize = 2_000;
const BUSINESS_RECORD_PROJECTION_PAGE_SIZE: usize = 25;
const BUSINESS_RECORD_PROJECTION_WRITE_BATCH_SIZE: usize = 250;
const BUSINESS_RECORD_PROJECTION_CURSOR_VERSION: u32 = 1;
const QUEUE_CHAT_REPAIR_ORPHAN_EPOCH_MS: i64 = 10 * 60 * 1_000;
const BUSINESS_COMMAND_ACTIVE_POLL_SECS: u64 = 1;
// Browser-originated commands are user-visible control-plane work. Same-process
// RxDB writes wake this loop through table notifiers, but replicated browser
// writes can arrive through SQLite without tripping that notifier reliably on
// every platform. Keep a short safety poll even when idle so pending commands
// cannot sit in `pending_sync` until a standby reconcile.
const BUSINESS_COMMAND_IDLE_POLL_SECS: u64 = BUSINESS_COMMAND_ACTIVE_POLL_SECS;
const BUSINESS_COMMAND_IDLE_BACKOFF_AFTER_TICKS: u32 = 1;
const SUPPORT_COMMUNICATION_INTAKE_SINCE_KEY: &str = "__support_communication_intake";
const THREADS_CTOX_RELEVANCE_COMMANDS_SINCE_KEY: &str =
    "__threads_ctox_relevance_business_commands";
const THREADS_CTOX_RELEVANCE_TASKS_SINCE_KEY: &str = "__threads_ctox_relevance_ctox_queue_tasks";
const THREADS_APP_RELEVANCE_SINCE_KEY_PREFIX: &str = "__threads_app_relevance_";
const TICKET_STATE_SYNC_LIMIT: usize = 500;
const BROWSER_RUNTIME_ACTIVE_MAINTENANCE_INTERVAL_MS: u64 = 300;
const BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS: u64 = 10;
const BROWSER_RUNTIME_IDLE_BACKOFF_AFTER_TICKS: u32 = 1;
const BROWSER_FRAME_GC_LIMIT: u64 = 256;
const BROWSER_INPUT_EVENT_GC_LIMIT: u64 = 512;
const BROWSER_INPUT_EVENT_RETENTION_SECS: u64 = 60 * 60;
const BUSINESS_OS_CHANNEL_IDS: &[&str] = &[
    "whatsapp",
    "jami",
    "teams",
    "email",
    "meeting",
    "slack",
    "discord",
    "telegram",
    "matrix",
    "mattermost",
    "zulip",
    "google_chat",
];
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
/// Runtime-installed app schemas are an activation input, not a health probe.
/// Detect them promptly so a client-only app does not sit behind the general
/// 15-second watchdog cadence before its collections become native-visible.
const NATIVE_PEER_RUNTIME_SCHEMA_WATCH_INTERVAL_SECS: u64 = 1;
/// FIX 2: maximum tolerated heartbeat staleness before the watchdog considers
/// its own liveness machinery wedged. Generously above the write interval and
/// the published TTL so a healthy peer never trips it.
const NATIVE_PEER_WATCHDOG_MAX_HEARTBEAT_AGE_MS: u64 = 90_000;
const NATIVE_PEER_PROGRESS_WARN_AGE_MS: u64 = 60_000;
const NATIVE_PEER_PROGRESS_RESPAWN_AGE_MS: u64 = 180_000;
const NATIVE_PEER_PROGRESS_RESPAWN_TICKS: u32 = 2;

/// FIX 2: worker-thread count for the peer's tokio runtime: `max(4, cores)`.
fn native_peer_worker_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(NATIVE_PEER_MIN_WORKER_THREADS)
        .max(NATIVE_PEER_MIN_WORKER_THREADS)
}

#[derive(Debug)]
struct IotAgentBootRow {
    id: String,
    realm: String,
    kind: String,
    data: Value,
}

fn load_enabled_iot_agents(root: &Path) -> anyhow::Result<Vec<IotAgentBootRow>> {
    let conn = crate::iot::store::open_iot_store(root)?;
    crate::iot::commands::ensure_stub_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, realm, kind, data
             FROM iot_agents
             WHERE enabled != 0
             ORDER BY id ASC",
        )
        .context("prepare enabled iot agent query")?;
    let rows = stmt
        .query_map([], |row| {
            let data_raw: String = row.get(3)?;
            let data = serde_json::from_str::<Value>(&data_raw).unwrap_or(Value::Null);
            Ok(IotAgentBootRow {
                id: row.get(0)?,
                realm: row.get(1)?,
                kind: row.get(2)?,
                data,
            })
        })
        .context("query enabled iot agents")?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("read enabled iot agent")?);
    }
    Ok(out)
}

fn configured_iot_agent_links(
    data: &Value,
) -> anyhow::Result<Vec<crate::iot::adapters::AgentLink>> {
    let Some(raw) = data
        .get("links")
        .or_else(|| data.get("agentLinks"))
        .or_else(|| data.get("linkedAttributes"))
    else {
        return Ok(Vec::new());
    };
    if raw.is_null() {
        return Ok(Vec::new());
    }
    let items = raw
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("iot agent links must be an array"))?;
    let mut links = Vec::with_capacity(items.len());
    for item in items {
        let link: crate::iot::adapters::AgentLink = serde_json::from_value(item.clone())
            .context("parse iot agent link from iot_agents.data.links")?;
        anyhow::ensure!(
            !link.asset_id.trim().is_empty() && !link.attribute_name.trim().is_empty(),
            "iot agent link requires asset_id/assetId and attribute_name/attributeName"
        );
        links.push(link);
    }
    Ok(links)
}

fn record_iot_agent_runtime_status(
    root: &Path,
    agent_id: &str,
    realm: &str,
    link_state: &str,
    last_event_ms: Option<i64>,
    data: Value,
) -> anyhow::Result<()> {
    let conn = crate::iot::store::open_iot_store(root)?;
    crate::iot::commands::ensure_stub_schema(&conn)?;
    let now = crate::iot::now_iso();
    conn.execute(
        "INSERT INTO iot_agent_status
            (id, agent_id, realm, link_state, last_event_ms, error, data, created_at, updated_at)
         VALUES (?1, ?1, ?2, ?3, ?4, NULL, ?5, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET
            realm = excluded.realm,
            link_state = excluded.link_state,
            last_event_ms = COALESCE(excluded.last_event_ms, iot_agent_status.last_event_ms),
            error = NULL,
            data = excluded.data,
            updated_at = excluded.updated_at",
        rusqlite::params![
            agent_id,
            realm,
            link_state,
            last_event_ms,
            serde_json::to_string(&data)?,
            now
        ],
    )
    .context("upsert iot agent runtime status")?;
    let rows = crate::iot::projector::project_agent_status(&conn, agent_id)?;
    store::project_iot_projection_rows(root, rows)?;
    Ok(())
}

fn connection_status_label(status: crate::iot::adapters::ConnectionStatus) -> &'static str {
    match status {
        crate::iot::adapters::ConnectionStatus::Disconnected => "disconnected",
        crate::iot::adapters::ConnectionStatus::Connecting => "connecting",
        crate::iot::adapters::ConnectionStatus::Waiting => "waiting",
        crate::iot::adapters::ConnectionStatus::Connected => "connected",
        crate::iot::adapters::ConnectionStatus::Disconnecting => "disconnecting",
    }
}

fn spawn_iot_agent_supervisors(root: PathBuf) -> Vec<Arc<AtomicBool>> {
    let agents = match load_enabled_iot_agents(&root) {
        Ok(agents) => agents,
        Err(err) => {
            eprintln!("[business-os] IoT agent supervisor bootstrap skipped: {err:#}");
            return Vec::new();
        }
    };
    let mut stops = Vec::new();
    for agent_row in agents {
        let links = match configured_iot_agent_links(&agent_row.data) {
            Ok(links) => links,
            Err(err) => {
                let _ = record_iot_agent_runtime_status(
                    &root,
                    &agent_row.id,
                    &agent_row.realm,
                    "misconfigured",
                    None,
                    json!({"runtime": "supervisor", "reason": "invalid-links"}),
                );
                eprintln!(
                    "[business-os] IoT agent `{}` not started: invalid link configuration ({err:#})",
                    agent_row.id
                );
                continue;
            }
        };
        if links.is_empty() {
            let _ = record_iot_agent_runtime_status(
                &root,
                &agent_row.id,
                &agent_row.realm,
                "unconfigured",
                None,
                json!({"runtime": "supervisor", "reason": "no-links"}),
            );
            continue;
        }
        let Some(kind) = crate::iot::adapters::IotAgentKind::from_str(&agent_row.kind) else {
            let _ = record_iot_agent_runtime_status(
                &root,
                &agent_row.id,
                &agent_row.realm,
                "misconfigured",
                None,
                json!({"runtime": "supervisor", "reason": "unknown-kind"}),
            );
            eprintln!(
                "[business-os] IoT agent `{}` not started: unknown kind `{}`",
                agent_row.id, agent_row.kind
            );
            continue;
        };
        let ctx = crate::iot::adapters::AgentContext {
            root: &root,
            agent_id: agent_row.id.clone(),
            realm: agent_row.realm.clone(),
            config: agent_row.data.clone(),
        };
        let agent = match crate::iot::gateway::build_agent(kind, ctx) {
            Ok(agent) => agent,
            Err(err) => {
                let _ = record_iot_agent_runtime_status(
                    &root,
                    &agent_row.id,
                    &agent_row.realm,
                    "misconfigured",
                    None,
                    json!({"runtime": "supervisor", "reason": "agent-build-failed"}),
                );
                eprintln!(
                    "[business-os] IoT agent `{}` not started: failed to construct native agent ({err:#})",
                    agent_row.id
                );
                continue;
            }
        };
        let mut runtime = crate::iot::runtime::AgentRuntime::new(agent, agent_row.realm.clone());
        let mut link_failed = false;
        for link in links {
            if let Err(err) = runtime.link(link) {
                link_failed = true;
                let _ = record_iot_agent_runtime_status(
                    &root,
                    &agent_row.id,
                    &agent_row.realm,
                    "misconfigured",
                    None,
                    json!({"runtime": "supervisor", "reason": "link-failed"}),
                );
                eprintln!(
                    "[business-os] IoT agent `{}` not started: failed to link attribute ({err:#})",
                    agent_row.id
                );
                break;
            }
        }
        if link_failed {
            continue;
        }
        let _ = record_iot_agent_runtime_status(
            &root,
            &agent_row.id,
            &agent_row.realm,
            "starting",
            None,
            json!({"runtime": "supervisor", "kind": agent_row.kind}),
        );
        let projection_root = root.clone();
        let status_root = root.clone();
        let status_agent_id = agent_row.id.clone();
        let status_realm = agent_row.realm.clone();
        let status_kind = agent_row.kind.clone();
        let stop = crate::iot::runtime::spawn_supervisor(
            runtime,
            root.clone(),
            agent_row.id.clone(),
            agent_row.data.clone(),
            move |rows, status| {
                if !rows.is_empty() {
                    store::project_iot_projection_rows(&projection_root, rows)?;
                }
                record_iot_agent_runtime_status(
                    &status_root,
                    &status_agent_id,
                    &status_realm,
                    connection_status_label(status),
                    Some(crate::iot::now_ms()),
                    json!({"runtime": "supervisor", "kind": status_kind.clone()}),
                )?;
                Ok(())
            },
        );
        eprintln!(
            "[business-os] IoT agent supervisor started for `{}` ({})",
            agent_row.id, agent_row.kind
        );
        stops.push(stop);
    }
    stops
}

type WebRtcPool = Arc<RxWebRTCReplicationPool<WebRTCRsConnectionHandler>>;

struct NativePeerLoopMetrics {
    name: &'static str,
    ticks: std::sync::atomic::AtomicU64,
    idle_ticks: std::sync::atomic::AtomicU64,
    active_ticks: std::sync::atomic::AtomicU64,
    error_ticks: std::sync::atomic::AtomicU64,
    rows: std::sync::atomic::AtomicU64,
    total_duration_ms: std::sync::atomic::AtomicU64,
    max_duration_ms: std::sync::atomic::AtomicU64,
    last_duration_ms: std::sync::atomic::AtomicU64,
    last_success_at_ms: std::sync::atomic::AtomicU64,
    last_error_at_ms: std::sync::atomic::AtomicU64,
}

impl NativePeerLoopMetrics {
    const fn new(name: &'static str) -> Self {
        Self {
            name,
            ticks: std::sync::atomic::AtomicU64::new(0),
            idle_ticks: std::sync::atomic::AtomicU64::new(0),
            active_ticks: std::sync::atomic::AtomicU64::new(0),
            error_ticks: std::sync::atomic::AtomicU64::new(0),
            rows: std::sync::atomic::AtomicU64::new(0),
            total_duration_ms: std::sync::atomic::AtomicU64::new(0),
            max_duration_ms: std::sync::atomic::AtomicU64::new(0),
            last_duration_ms: std::sync::atomic::AtomicU64::new(0),
            last_success_at_ms: std::sync::atomic::AtomicU64::new(0),
            last_error_at_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn record(&self, rows: Option<usize>, elapsed: Duration) {
        let duration_ms = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
        self.ticks.fetch_add(1, Ordering::Relaxed);
        self.last_duration_ms.store(duration_ms, Ordering::Relaxed);
        self.total_duration_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
        update_atomic_max(&self.max_duration_ms, duration_ms);
        match rows {
            Some(0) => {
                self.last_success_at_ms
                    .store(now_ms() as u64, Ordering::Relaxed);
                self.idle_ticks.fetch_add(1, Ordering::Relaxed);
            }
            Some(rows) => {
                self.last_success_at_ms
                    .store(now_ms() as u64, Ordering::Relaxed);
                self.active_ticks.fetch_add(1, Ordering::Relaxed);
                self.rows.fetch_add(rows as u64, Ordering::Relaxed);
            }
            None => {
                self.last_error_at_ms
                    .store(now_ms() as u64, Ordering::Relaxed);
                self.error_ticks.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn snapshot(&self) -> Value {
        json!({
            "name": self.name,
            "ticks": self.ticks.load(Ordering::Relaxed),
            "idle_ticks": self.idle_ticks.load(Ordering::Relaxed),
            "active_ticks": self.active_ticks.load(Ordering::Relaxed),
            "error_ticks": self.error_ticks.load(Ordering::Relaxed),
            "rows": self.rows.load(Ordering::Relaxed),
            "last_duration_ms": self.last_duration_ms.load(Ordering::Relaxed),
            "max_duration_ms": self.max_duration_ms.load(Ordering::Relaxed),
            "total_duration_ms": self.total_duration_ms.load(Ordering::Relaxed),
            "last_success_at_ms": self.last_success_at_ms.load(Ordering::Relaxed),
            "last_error_at_ms": self.last_error_at_ms.load(Ordering::Relaxed),
        })
    }
}

struct CommandPlaneMetrics {
    attempts_total: std::sync::atomic::AtomicU64,
    processed_total: std::sync::atomic::AtomicU64,
    errors_total: std::sync::atomic::AtomicU64,
    retries_total: std::sync::atomic::AtomicU64,
    exhausted_total: std::sync::atomic::AtomicU64,
    last_attempt_at_ms: std::sync::atomic::AtomicU64,
    last_processed_at_ms: std::sync::atomic::AtomicU64,
    observation_latency_total_ms: std::sync::atomic::AtomicU64,
    observation_latency_samples: std::sync::atomic::AtomicU64,
    observation_latency_max_ms: std::sync::atomic::AtomicU64,
}

impl CommandPlaneMetrics {
    const fn new() -> Self {
        Self {
            attempts_total: std::sync::atomic::AtomicU64::new(0),
            processed_total: std::sync::atomic::AtomicU64::new(0),
            errors_total: std::sync::atomic::AtomicU64::new(0),
            retries_total: std::sync::atomic::AtomicU64::new(0),
            exhausted_total: std::sync::atomic::AtomicU64::new(0),
            last_attempt_at_ms: std::sync::atomic::AtomicU64::new(0),
            last_processed_at_ms: std::sync::atomic::AtomicU64::new(0),
            observation_latency_total_ms: std::sync::atomic::AtomicU64::new(0),
            observation_latency_samples: std::sync::atomic::AtomicU64::new(0),
            observation_latency_max_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn record_attempt(&self) {
        self.attempts_total.fetch_add(1, Ordering::Relaxed);
        self.last_attempt_at_ms
            .store(now_ms() as u64, Ordering::Relaxed);
    }

    fn record_processed(&self, document: &Value) {
        let processed_at_ms = now_ms() as u64;
        self.processed_total.fetch_add(1, Ordering::Relaxed);
        self.last_processed_at_ms
            .store(processed_at_ms, Ordering::Relaxed);
        if let Some(created_at_ms) = document.get("created_at_ms").and_then(Value::as_u64) {
            let latency_ms = processed_at_ms.saturating_sub(created_at_ms);
            self.observation_latency_total_ms
                .fetch_add(latency_ms, Ordering::Relaxed);
            self.observation_latency_samples
                .fetch_add(1, Ordering::Relaxed);
            update_atomic_max(&self.observation_latency_max_ms, latency_ms);
        }
    }

    fn snapshot(&self) -> Value {
        let processed_total = self.processed_total.load(Ordering::Relaxed);
        let latency_samples = self.observation_latency_samples.load(Ordering::Relaxed);
        let latency_total = self.observation_latency_total_ms.load(Ordering::Relaxed);
        json!({
            "schema": "ctox.command_plane.runtime_counters.v1",
            "attempts_total": self.attempts_total.load(Ordering::Relaxed),
            "processed_total": processed_total,
            "errors_total": self.errors_total.load(Ordering::Relaxed),
            "retries_total": self.retries_total.load(Ordering::Relaxed),
            "exhausted_total": self.exhausted_total.load(Ordering::Relaxed),
            "last_attempt_at_ms": self.last_attempt_at_ms.load(Ordering::Relaxed),
            "last_processed_at_ms": self.last_processed_at_ms.load(Ordering::Relaxed),
            "observation_latency_samples": latency_samples,
            "observation_latency_avg_ms": if latency_samples == 0 { 0 } else { latency_total / latency_samples },
            "observation_latency_max_ms": self.observation_latency_max_ms.load(Ordering::Relaxed),
        })
    }
}

#[derive(Default)]
struct DemandFileFetchRequestStats {
    ranged: bool,
    rows_loaded: u64,
    chunks_decoded: u64,
    bytes_requested: u64,
    bytes_decoded: u64,
    bytes_emitted: u64,
}

impl DemandFileFetchRequestStats {
    fn new(range: Option<&FileRange>) -> Self {
        Self {
            ranged: range.is_some(),
            bytes_requested: range.map(|range| range.length).unwrap_or_default(),
            ..Self::default()
        }
    }

    fn finish(&mut self) {
        if !self.ranged {
            self.bytes_requested = self.bytes_emitted;
        }
    }
}

struct DemandFileFetchMetrics {
    requests: std::sync::atomic::AtomicU64,
    ranged_requests: std::sync::atomic::AtomicU64,
    error_requests: std::sync::atomic::AtomicU64,
    rows_loaded: std::sync::atomic::AtomicU64,
    chunks_decoded: std::sync::atomic::AtomicU64,
    bytes_requested: std::sync::atomic::AtomicU64,
    bytes_decoded: std::sync::atomic::AtomicU64,
    bytes_emitted: std::sync::atomic::AtomicU64,
    max_rows_loaded: std::sync::atomic::AtomicU64,
    max_bytes_decoded: std::sync::atomic::AtomicU64,
}

impl DemandFileFetchMetrics {
    const fn new() -> Self {
        Self {
            requests: std::sync::atomic::AtomicU64::new(0),
            ranged_requests: std::sync::atomic::AtomicU64::new(0),
            error_requests: std::sync::atomic::AtomicU64::new(0),
            rows_loaded: std::sync::atomic::AtomicU64::new(0),
            chunks_decoded: std::sync::atomic::AtomicU64::new(0),
            bytes_requested: std::sync::atomic::AtomicU64::new(0),
            bytes_decoded: std::sync::atomic::AtomicU64::new(0),
            bytes_emitted: std::sync::atomic::AtomicU64::new(0),
            max_rows_loaded: std::sync::atomic::AtomicU64::new(0),
            max_bytes_decoded: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn record(&self, stats: &DemandFileFetchRequestStats, success: bool) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        if stats.ranged {
            self.ranged_requests.fetch_add(1, Ordering::Relaxed);
        }
        if !success {
            self.error_requests.fetch_add(1, Ordering::Relaxed);
        }
        self.rows_loaded
            .fetch_add(stats.rows_loaded, Ordering::Relaxed);
        self.chunks_decoded
            .fetch_add(stats.chunks_decoded, Ordering::Relaxed);
        self.bytes_requested
            .fetch_add(stats.bytes_requested, Ordering::Relaxed);
        self.bytes_decoded
            .fetch_add(stats.bytes_decoded, Ordering::Relaxed);
        self.bytes_emitted
            .fetch_add(stats.bytes_emitted, Ordering::Relaxed);
        update_atomic_max(&self.max_rows_loaded, stats.rows_loaded);
        update_atomic_max(&self.max_bytes_decoded, stats.bytes_decoded);
    }

    fn snapshot(&self) -> Value {
        json!({
            "schema": "ctox.native_peer.file_fetch.performance.v1",
            "requests": self.requests.load(Ordering::Relaxed),
            "ranged_requests": self.ranged_requests.load(Ordering::Relaxed),
            "error_requests": self.error_requests.load(Ordering::Relaxed),
            "rows_loaded": self.rows_loaded.load(Ordering::Relaxed),
            "chunks_decoded": self.chunks_decoded.load(Ordering::Relaxed),
            "bytes_requested": self.bytes_requested.load(Ordering::Relaxed),
            "bytes_decoded": self.bytes_decoded.load(Ordering::Relaxed),
            "bytes_emitted": self.bytes_emitted.load(Ordering::Relaxed),
            "max_rows_loaded": self.max_rows_loaded.load(Ordering::Relaxed),
            "max_bytes_decoded": self.max_bytes_decoded.load(Ordering::Relaxed),
        })
    }

    #[cfg(test)]
    fn reset(&self) {
        self.requests.store(0, Ordering::Relaxed);
        self.ranged_requests.store(0, Ordering::Relaxed);
        self.error_requests.store(0, Ordering::Relaxed);
        self.rows_loaded.store(0, Ordering::Relaxed);
        self.chunks_decoded.store(0, Ordering::Relaxed);
        self.bytes_requested.store(0, Ordering::Relaxed);
        self.bytes_decoded.store(0, Ordering::Relaxed);
        self.bytes_emitted.store(0, Ordering::Relaxed);
        self.max_rows_loaded.store(0, Ordering::Relaxed);
        self.max_bytes_decoded.store(0, Ordering::Relaxed);
    }
}

fn update_atomic_max(value: &std::sync::atomic::AtomicU64, candidate: u64) {
    let mut current = value.load(Ordering::Relaxed);
    while candidate > current {
        match value.compare_exchange_weak(current, candidate, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => break,
            Err(next_current) => current = next_current,
        }
    }
}

fn record_native_peer_loop_result(
    metrics: &NativePeerLoopMetrics,
    result: &anyhow::Result<usize>,
    elapsed: Duration,
) {
    match result {
        Ok(rows) => metrics.record(Some(*rows), elapsed),
        Err(_) => metrics.record(None, elapsed),
    }
}

fn record_native_peer_bool_loop_result(
    metrics: &NativePeerLoopMetrics,
    result: &anyhow::Result<bool>,
    elapsed: Duration,
) {
    match result {
        Ok(true) => metrics.record(Some(1), elapsed),
        Ok(false) => metrics.record(Some(0), elapsed),
        Err(_) => metrics.record(None, elapsed),
    }
}

fn native_peer_performance_snapshot() -> Value {
    json!({
        "schema": "ctox.native_peer.performance.v1",
        "loops": {
            "notes": NOTES_LOOP_METRICS.snapshot(),
            "desktop_file_index": DESKTOP_FILE_INDEX_LOOP_METRICS.snapshot(),
            "channel_state": CHANNEL_STATE_LOOP_METRICS.snapshot(),
            "business_users": BUSINESS_USERS_LOOP_METRICS.snapshot(),
            "runtime_settings": RUNTIME_SETTINGS_LOOP_METRICS.snapshot(),
            "workspace_branding": WORKSPACE_BRANDING_LOOP_METRICS.snapshot(),
            "module_catalog": MODULE_CATALOG_LOOP_METRICS.snapshot(),
            "ticket_state": TICKET_STATE_LOOP_METRICS.snapshot(),
            "knowledge_tables": KNOWLEDGE_TABLES_LOOP_METRICS.snapshot(),
            "business_records": BUSINESS_RECORDS_LOOP_METRICS.snapshot(),
            "business_commands": BUSINESS_COMMANDS_LOOP_METRICS.snapshot(),
            "browser_runtime": BROWSER_RUNTIME_LOOP_METRICS.snapshot(),
        },
        "file_fetch": DEMAND_FILE_FETCH_METRICS.snapshot(),
        "command_plane": COMMAND_PLANE_METRICS.snapshot(),
        "rxdb_sqlite": rxdb::storage::sqlite::instance::sqlite_runtime_counters_snapshot(),
        "rxdb_subjects": {
            "schema": "ctox.rxdb.subjects.runtime_counters.v1",
            "lagged_items_total": rxdb::rxjs_compat::rx_subject_lagged_items_total(),
        },
    })
}

fn native_peer_loop_metrics(name: &str) -> Option<&'static NativePeerLoopMetrics> {
    match name {
        "notes" => Some(&NOTES_LOOP_METRICS),
        "desktop_file_index" => Some(&DESKTOP_FILE_INDEX_LOOP_METRICS),
        "channel_state" => Some(&CHANNEL_STATE_LOOP_METRICS),
        "business_users" => Some(&BUSINESS_USERS_LOOP_METRICS),
        "runtime_settings" => Some(&RUNTIME_SETTINGS_LOOP_METRICS),
        "workspace_branding" => Some(&WORKSPACE_BRANDING_LOOP_METRICS),
        "module_catalog" => Some(&MODULE_CATALOG_LOOP_METRICS),
        "ticket_state" => Some(&TICKET_STATE_LOOP_METRICS),
        "knowledge_tables" => Some(&KNOWLEDGE_TABLES_LOOP_METRICS),
        "business_records" => Some(&BUSINESS_RECORDS_LOOP_METRICS),
        "business_commands" => Some(&BUSINESS_COMMANDS_LOOP_METRICS),
        "browser_runtime" => Some(&BROWSER_RUNTIME_LOOP_METRICS),
        _ => None,
    }
}

fn native_peer_performance_status(heartbeat: Option<&Value>, heartbeat_fresh: bool) -> Value {
    if is_native_peer_running() {
        return native_peer_performance_snapshot();
    }
    if heartbeat_fresh {
        return heartbeat
            .and_then(|value| value.get("performance"))
            .cloned()
            .unwrap_or(Value::Null);
    }
    Value::Null
}

struct NativePeer {
    root: PathBuf,
    database: Arc<RxDatabase>,
    peer_session_id: String,
    shutdown_tx: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    _process_lock: File,
    _pools: Vec<WebRtcPool>,
    _command_consumer: tokio::task::JoinHandle<()>,
    _command_outbox: tokio::task::JoinHandle<()>,
    _notes_sync: tokio::task::JoinHandle<()>,
    _file_index_sync: tokio::task::JoinHandle<()>,
    _channel_state_sync: tokio::task::JoinHandle<()>,
    _business_users_sync: tokio::task::JoinHandle<()>,
    _runtime_settings_sync: tokio::task::JoinHandle<()>,
    _workspace_branding_sync: tokio::task::JoinHandle<()>,
    _module_catalog_sync: tokio::task::JoinHandle<()>,
    _ticket_state_sync: tokio::task::JoinHandle<()>,
    _knowledge_tables_sync: tokio::task::JoinHandle<()>,
    _business_record_projection_sync: tokio::task::JoinHandle<()>,
    _iot_agent_supervisors: Vec<Arc<AtomicBool>>,
    _browser_runtime_maintenance: tokio::task::JoinHandle<()>,
    // FIX 2: the status heartbeat now runs on a dedicated OS thread (see
    // `StatusHeartbeatHandle`) so its liveness is independent of the tokio
    // runtime. `Mutex` lets `shutdown` take/stop it through `&self`.
    _status_heartbeat: Mutex<Option<StatusHeartbeatHandle>>,
}

impl NativePeer {
    fn task_liveness(&self) -> [(&'static str, bool); 14] {
        [
            ("business_commands", !self._command_consumer.is_finished()),
            (
                "business_command_outbox",
                !self._command_outbox.is_finished(),
            ),
            ("notes", !self._notes_sync.is_finished()),
            ("desktop_file_index", !self._file_index_sync.is_finished()),
            ("channel_state", !self._channel_state_sync.is_finished()),
            ("business_users", !self._business_users_sync.is_finished()),
            (
                "runtime_settings",
                !self._runtime_settings_sync.is_finished(),
            ),
            (
                "workspace_branding",
                !self._workspace_branding_sync.is_finished(),
            ),
            ("module_catalog", !self._module_catalog_sync.is_finished()),
            ("ticket_state", !self._ticket_state_sync.is_finished()),
            (
                "knowledge_tables",
                !self._knowledge_tables_sync.is_finished(),
            ),
            (
                "business_records",
                !self._business_record_projection_sync.is_finished(),
            ),
            (
                "browser_runtime",
                !self._browser_runtime_maintenance.is_finished(),
            ),
            (
                "status_heartbeat",
                NATIVE_PEER_HEARTBEAT_THREAD_ALIVE.load(Ordering::SeqCst),
            ),
        ]
    }

    fn critical_tasks_alive(&self) -> bool {
        self.task_liveness().iter().all(|(_, alive)| *alive)
    }

    fn finished_critical_tasks(&self) -> Vec<&'static str> {
        self.task_liveness()
            .into_iter()
            .filter_map(|(name, alive)| (!alive).then_some(name))
            .collect()
    }

    fn task_liveness_json(&self) -> Value {
        let command_backlog = NATIVE_PEER_PENDING_OUTBOX.load(Ordering::Relaxed);
        Value::Array(self.task_liveness().into_iter().map(|(name, alive)| {
            let metrics = native_peer_loop_metrics(name).map(NativePeerLoopMetrics::snapshot);
            json!({
                "name": name,
                "alive": alive,
                "lastSuccessAtMs": metrics.as_ref().and_then(|value| value.get("last_success_at_ms")).cloned().unwrap_or(Value::Null),
                "lastErrorAtMs": metrics.as_ref().and_then(|value| value.get("last_error_at_ms")).cloned().unwrap_or(Value::Null),
                "backlog": if name == "business_commands" { Value::from(command_backlog) } else { Value::Null },
                "metrics": metrics,
            })
        }).collect())
    }

    fn refresh_liveness_signals(&self) {
        let transport = self
            ._pools
            .first()
            .map(|pool| pool.connection_handler.frame_transport_status_json())
            .unwrap_or(Value::Null);
        let join_accepted = transport
            .get("signalingJoinAccepted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let data_channel_open = transport
            .get("openDataChannels")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            > 0;
        let tasks_alive = self.critical_tasks_alive();
        NATIVE_PEER_SIGNALING_JOIN_ACCEPTED.store(join_accepted, Ordering::SeqCst);
        NATIVE_PEER_DATA_CHANNEL_OPEN.store(data_channel_open, Ordering::SeqCst);
        NATIVE_PEER_CRITICAL_TASKS_ALIVE.store(tasks_alive, Ordering::SeqCst);
        NATIVE_PEER_REPLICATION_UP.store(
            !self._pools.is_empty() && join_accepted && data_channel_open && tasks_alive,
            Ordering::SeqCst,
        );
    }

    async fn shutdown(&self) {
        for pool in &self._pools {
            pool.cancel().await;
        }
        self._command_consumer.abort();
        self._command_outbox.abort();
        self._notes_sync.abort();
        self._file_index_sync.abort();
        self._channel_state_sync.abort();
        self._business_users_sync.abort();
        self._runtime_settings_sync.abort();
        self._workspace_branding_sync.abort();
        self._module_catalog_sync.abort();
        self._ticket_state_sync.abort();
        self._knowledge_tables_sync.abort();
        self._business_record_projection_sync.abort();
        for stop in &self._iot_agent_supervisors {
            stop.store(true, Ordering::SeqCst);
        }
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
    let circuit_breaker = native_peer_circuit_snapshot();
    let circuit_open = circuit_breaker.get("state").and_then(Value::as_str) == Some("open");
    let circuit_permanent = circuit_breaker
        .get("permanent")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let active_peer = current_peer();
    let transport = active_peer
        .as_ref()
        .and_then(|peer| peer._pools.first())
        .map(|pool| pool.connection_handler.frame_transport_status_json())
        .unwrap_or(Value::Null);
    let critical_tasks_alive = active_peer
        .as_ref()
        .is_some_and(|peer| peer.critical_tasks_alive());
    let command_consumer_alive = active_peer
        .as_ref()
        .is_some_and(|peer| !peer._command_consumer.is_finished());
    let in_process_started = NATIVE_PEER_STARTED.load(Ordering::SeqCst);
    let in_process_running = NATIVE_PEER_RUNNING.load(Ordering::SeqCst);
    let process_lock_held = native_peer_process_lock_is_held(root);
    let heartbeat = read_native_peer_heartbeat(root);
    let task_liveness = active_peer
        .as_ref()
        .map(|peer| peer.task_liveness_json())
        .or_else(|| {
            heartbeat
                .as_ref()
                .and_then(|value| value.get("criticalTasks").cloned())
        })
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let heartbeat_updated_at_ms = heartbeat_updated_at_ms(heartbeat.as_ref());
    let heartbeat_age_ms = heartbeat_updated_at_ms.map(|updated_at_ms| {
        let now = now_ms() as u64;
        now.saturating_sub(updated_at_ms)
    });
    let heartbeat_fresh = heartbeat_age_ms
        .map(|age_ms| age_ms <= NATIVE_PEER_HEARTBEAT_TTL_MS)
        .unwrap_or(false);
    let running = in_process_running || in_process_started || heartbeat_fresh;
    // Replication liveness: in-process reads the static directly; an
    // out-of-process reader falls back to the heartbeat field. `false` when
    // unknown — a missing signal must read as "not proven up".
    let replication_up = if in_process_running {
        NATIVE_PEER_REPLICATION_UP.load(Ordering::SeqCst)
            && transport
                .get("signalingJoinAccepted")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && transport
                .get("openDataChannels")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 0
            && critical_tasks_alive
    } else {
        heartbeat
            .as_ref()
            .and_then(|value| value.get("replicationUp"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && heartbeat_fresh
    };
    let health_errors = if circuit_open {
        vec![native_peer_health_error(
            "ctox_signaling_circuit_open",
            "CtoxSignalingCircuitOpen",
            "signaling",
            if circuit_permanent {
                "terminal"
            } else {
                "recoverable"
            },
            !circuit_permanent,
            if circuit_permanent {
                "Native signaling is blocked after a terminal room/auth/protocol error until configuration or credentials change."
            } else {
                "Native signaling is temporarily blocked after repeated failures; exactly one probe will run after the open interval."
            },
        )]
    } else if running {
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
    let turn_readiness = store::turn_config_status(root).unwrap_or_else(|error| {
        json!({
            "active": false,
            "error": error.to_string(),
        })
    });
    let turn_credential_ready = turn_readiness
        .pointer("/ice_diagnostics/iceServersHaveCredentialedTurn")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    json!({
        "version": NATIVE_PEER_STATUS_VERSION,
        "running": running,
        "replicationUp": replication_up,
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
        "criticalTasks": task_liveness,
        "circuitBreaker": circuit_breaker,
        "nativePeerRecovery": {
            "code": "ctox_optional_schema_drift",
            "action": "repair-optional-drift",
            "status": "available",
        },
        "performance": native_peer_performance_status(heartbeat.as_ref(), heartbeat_fresh),
        "command_plane": command_plane_status(root),
        "health_stages": {
            "process_alive": running,
            "signaling_socket_connected": transport
                .get("signalingSocketConnected").and_then(Value::as_bool).unwrap_or(false),
            "signaling_join_accepted": transport
                .get("signalingJoinAccepted").and_then(Value::as_bool).unwrap_or(false),
            "peer_authenticated": transport
                .get("peerCount").and_then(Value::as_u64).unwrap_or_default() > 0,
            "data_channel_open": transport
                .get("openDataChannels").and_then(Value::as_u64).unwrap_or_default() > 0,
            "command_consumer_alive": command_consumer_alive,
            "last_command_ingestion_progress_ms": COMMAND_PLANE_METRICS.last_processed_at_ms.load(Ordering::Relaxed),
            "projection_outbox": crate::mission::channels::business_command_core_diagnostics(root)
                .ok()
                .and_then(|value| value.get("oldest_outbox_age_ms").cloned())
                .unwrap_or(Value::Null),
            "turn_credential_ready": turn_credential_ready,
        },
        "turn_readiness": turn_readiness,
        "transport": transport,
        "peer_session_id": current_peer()
            .map(|peer| peer.peer_session_id.clone())
            .or_else(|| heartbeat_peer_session_id(heartbeat.as_ref()))
            .unwrap_or_default(),
        "lock_path": native_peer_lock_path(root).display().to_string(),
        "database_path": store::rxdb_store_path(root).display().to_string(),
    })
}

fn command_plane_status(root: &Path) -> Value {
    let runtime = COMMAND_PLANE_METRICS.snapshot();
    let path = store::rxdb_store_path(root);
    if !path.exists() {
        return json!({
            "runtime": runtime,
            "pending_sync_count": 0,
            "oldest_pending_age_ms": 0,
        });
    }
    let result = (|| -> anyhow::Result<(u64, u64)> {
        let conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
        let Some(table) = latest_rxdb_collection_table(&conn, "business_commands")? else {
            return Ok((0, 0));
        };
        let quoted = sqlite_quote_identifier(&table);
        let query = format!(
            "SELECT COUNT(*), MIN(CAST(COALESCE(json_extract(data, '$.created_at_ms'), json_extract(data, '$.updated_at_ms'), 0) AS INTEGER))
             FROM {quoted}
             WHERE json_extract(data, '$.status') = 'pending_sync'"
        );
        let (count, oldest_at_ms): (u64, Option<u64>) =
            conn.query_row(&query, [], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok((
            count,
            oldest_at_ms
                .filter(|value| *value > 0)
                .map(|value| (now_ms() as u64).saturating_sub(value))
                .unwrap_or_default(),
        ))
    })();
    match result {
        Ok((pending_sync_count, oldest_pending_age_ms)) => json!({
            "runtime": runtime,
            "pending_sync_count": pending_sync_count,
            "oldest_pending_age_ms": oldest_pending_age_ms,
        }),
        Err(error) => json!({
            "runtime": runtime,
            "pending_sync_count": Value::Null,
            "oldest_pending_age_ms": Value::Null,
            "diagnostic_error": error.to_string(),
        }),
    }
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
    } else if NATIVE_PEER_STARTED.load(Ordering::SeqCst) {
        // A circuit-open supervisor has no current peer and must still be
        // interruptible by the explicit recovery action.
        NATIVE_PEER_SUPERVISOR_STOP.store(true, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while NATIVE_PEER_STARTED.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        if NATIVE_PEER_STARTED.load(Ordering::SeqCst) {
            anyhow::bail!("native RxDB peer supervisor did not stop before restart deadline");
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
    runtime
        .block_on(run_native_peer(
            root,
            config.sync_room.clone(),
            config.signaling_urls.clone(),
            config.signaling_room_password.clone(),
        ))
        .map(|_| ())
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
    NATIVE_PEER_SUPERVISOR_STOP.store(false, Ordering::SeqCst);
    let root = root.to_path_buf();
    if let Err(err) = std::thread::Builder::new()
        .name("business-os-rxdb-peer".to_string())
        .spawn(move || {
            // Supervision loop. The peer used to die PERMANENTLY on any exit
            // (bring-up failure, watchdog-stale heartbeat, transient SQLite
            // error): nothing respawned it and `ensure_native_peer` only runs
            // at daemon boot, so a boot race against the signaling server
            // cost the entire daemon lifetime of sync. Every non-intentional
            // exit now respawns with capped backoff. The sync config is
            // re-read per attempt so room-password rotation and signaling
            // changes reach the respawned peer without a daemon restart.
            let mut delay = Duration::from_secs(NATIVE_PEER_RESPAWN_BASE_DELAY_SECS);
            loop {
                if NATIVE_PEER_SUPERVISOR_STOP.load(Ordering::SeqCst) {
                    break;
                }
                let (room, urls, password) = match store::sync_config(&root) {
                    Ok(config) => (
                        config.sync_room.clone(),
                        config.signaling_urls.clone(),
                        config.signaling_room_password.clone(),
                    ),
                    Err(err) => {
                        eprintln!(
                            "[business-os] native rxdb peer: sync config re-read failed \
                             ({err:#}); using boot-time values"
                        );
                        (
                            sync_room.clone(),
                            signaling_urls.clone(),
                            signaling_room_password.clone(),
                        )
                    }
                };
                let config_epoch = native_peer_config_epoch(&room, &urls, &password);
                let circuit_wait = native_peer_circuit_breaker()
                    .lock()
                    .ok()
                    .and_then(|mut breaker| {
                        breaker.before_attempt(config_epoch.clone(), now_ms() as u64)
                    });
                if let Some(wait) = circuit_wait {
                    sleep_native_peer_supervisor(wait);
                    continue;
                }
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .worker_threads(native_peer_worker_threads())
                    .thread_name("business-os-rxdb-peer")
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        eprintln!("[business-os] native rxdb peer runtime failed: {err:#}");
                        break;
                    }
                };
                let started_at = std::time::Instant::now();
                let result = runtime.block_on(run_native_peer(root.clone(), room, urls, password));
                NATIVE_PEER_RUNNING.store(false, Ordering::SeqCst);
                // Drop the runtime before sleeping so tasks leaked by a
                // wedged run cannot hold sockets/filehandles across the
                // backoff window.
                drop(runtime);
                if let Err(error) = &result {
                    let message = format!("{error:#}");
                    let failure = classify_native_peer_failure(&message);
                    if let Ok(mut breaker) = native_peer_circuit_breaker().lock() {
                        breaker.record_failure(failure, message, now_ms() as u64);
                    }
                } else if !matches!(&result, Ok(NativePeerExit::LockHeldElsewhere)) {
                    if let Ok(mut breaker) = native_peer_circuit_breaker().lock() {
                        breaker.record_success();
                    }
                }
                match &result {
                    Ok(NativePeerExit::Shutdown) => break,
                    Ok(NativePeerExit::WatchdogStale) => {
                        eprintln!(
                            "[business-os] native rxdb peer exited after stale heartbeat; \
                             respawning in {}s",
                            delay.as_secs()
                        );
                    }
                    Ok(NativePeerExit::ConfigChanged) => {
                        eprintln!(
                            "[business-os] native rxdb peer sync config changed; \
                             reconfiguring immediately"
                        );
                    }
                    Ok(NativePeerExit::RuntimeSchemaChanged) => {
                        eprintln!(
                            "[business-os] native rxdb peer runtime app schemas changed; \
                             reconfiguring immediately"
                        );
                    }
                    Ok(NativePeerExit::CriticalChildExited) => {
                        eprintln!(
                            "[business-os] native rxdb peer critical child exited; \
                             respawning in {}s",
                            delay.as_secs()
                        );
                    }
                    Ok(NativePeerExit::LockHeldElsewhere) => {
                        eprintln!(
                            "[business-os] native rxdb peer lock held by another process; \
                             retrying in {}s",
                            delay.as_secs()
                        );
                    }
                    Err(err) => {
                        let circuit = native_peer_circuit_snapshot();
                        eprintln!(
                            "[business-os] native rxdb peer failed: {err:#}; circuit={}; next retry backoff={}s",
                            circuit.get("state").and_then(Value::as_str).unwrap_or("unknown"),
                            delay.as_secs(),
                        );
                    }
                }
                if started_at.elapsed() >= Duration::from_secs(NATIVE_PEER_RESPAWN_HEALTHY_RUN_SECS)
                {
                    delay = Duration::from_secs(NATIVE_PEER_RESPAWN_BASE_DELAY_SECS);
                }
                let immediate_reconfigure = matches!(
                    result,
                    Ok(NativePeerExit::ConfigChanged | NativePeerExit::RuntimeSchemaChanged)
                );
                if !immediate_reconfigure
                    && native_peer_circuit_snapshot()
                    .get("state")
                    .and_then(Value::as_str)
                    != Some("open")
                {
                    sleep_native_peer_supervisor(native_peer_retry_delay(delay));
                }
                if immediate_reconfigure {
                    delay = Duration::from_secs(NATIVE_PEER_RESPAWN_BASE_DELAY_SECS);
                } else {
                    delay =
                        (delay * 2).min(Duration::from_secs(NATIVE_PEER_RESPAWN_MAX_DELAY_SECS));
                }
            }
            NATIVE_PEER_RUNNING.store(false, Ordering::SeqCst);
            NATIVE_PEER_STARTED.store(false, Ordering::SeqCst);
        })
    {
        NATIVE_PEER_STARTED.store(false, Ordering::SeqCst);
        eprintln!("[business-os] native rxdb peer thread failed: {err:#}");
    }
}

fn sleep_native_peer_supervisor(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline && !NATIVE_PEER_SUPERVISOR_STOP.load(Ordering::SeqCst) {
        std::thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(250)),
        );
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
    ensure_safe_desktop_file_index_path(&path, "desktop file")?;
    if !metadata.is_file() {
        anyhow::bail!("desktop file path is not a file: {}", path.display());
    }
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
        upsert_desktop_file_with_policy(root, &database, path, policy).await?;
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
        let indexed =
            sync_desktop_file_scan_roots_with_database(root, &database, scan_roots).await?;
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
fn sync_desktop_file_index_if_changed(
    root: &Path,
    last_projection_stamp: &mut Option<DesktopFileIndexProjectionStamp>,
) -> anyhow::Result<usize> {
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
            return sync_desktop_file_index_with_database_if_changed(
                root,
                &peer.database,
                last_projection_stamp,
            )
            .await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let indexed = sync_desktop_file_index_with_database_if_changed(
            root,
            &database,
            last_projection_stamp,
        )
        .await?;
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
fn sync_channel_state_if_changed(
    root: &Path,
    last_projection_stamp: &mut Option<ChannelStateProjectionStamp>,
) -> anyhow::Result<usize> {
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
            return sync_channel_state_with_database_if_changed(
                root,
                &peer.database,
                last_projection_stamp,
            )
            .await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced =
            sync_channel_state_with_database_if_changed(root, &database, last_projection_stamp)
                .await?;
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
fn sync_business_users_if_changed(
    root: &Path,
    last_projection_stamp: &mut Option<store::BusinessUsersProjectionStamp>,
) -> anyhow::Result<usize> {
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
            return sync_business_users_with_database_if_changed(
                root,
                &peer.database,
                last_projection_stamp,
            )
            .await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced =
            sync_business_users_with_database_if_changed(root, &database, last_projection_stamp)
                .await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

#[cfg(test)]
fn sync_runtime_settings(root: &Path) -> anyhow::Result<usize> {
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
        let synced = sync_runtime_settings_with_database(root, &database).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

#[cfg(test)]
fn sync_runtime_settings_if_changed(
    root: &Path,
    last_projection_stamp: &mut Option<store::RuntimeSettingsProjectionStamp>,
) -> anyhow::Result<usize> {
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
            return sync_runtime_settings_with_database_if_changed(
                root,
                &peer.database,
                last_projection_stamp,
            )
            .await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced =
            sync_runtime_settings_with_database_if_changed(root, &database, last_projection_stamp)
                .await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

#[cfg(test)]
fn sync_module_catalog(root: &Path) -> anyhow::Result<usize> {
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
        let synced = sync_module_catalog_with_database(root, &database).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

#[cfg(test)]
fn sync_module_catalog_if_changed(
    root: &Path,
    last_projection_stamp: &mut Option<store::ModuleCatalogProjectionStamp>,
) -> anyhow::Result<usize> {
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
            return sync_module_catalog_with_database_if_changed(
                root,
                &peer.database,
                last_projection_stamp,
            )
            .await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced =
            sync_module_catalog_with_database_if_changed(root, &database, last_projection_stamp)
                .await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
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
fn sync_ticket_state_if_changed(
    root: &Path,
    last_source_stamp: &mut Option<tickets::TicketStoreChangeStamp>,
) -> anyhow::Result<usize> {
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
            return sync_ticket_state_with_database_if_changed(
                root,
                &peer.database,
                last_source_stamp,
            )
            .await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced =
            sync_ticket_state_with_database_if_changed(root, &database, last_source_stamp).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

pub(crate) fn sync_knowledge_tables(root: &Path) -> anyhow::Result<usize> {
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS knowledge tables sync runtime")?;
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        if let Some(peer) = current_peer() {
            return sync_knowledge_tables_with_database(root, &peer.database).await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced = sync_knowledge_tables_with_database(root, &database).await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

#[cfg(test)]
fn sync_knowledge_tables_if_changed(
    root: &Path,
    last_source_stamp: &mut Option<crate::knowledge::KnowledgeTablesProjectionSourceStamp>,
) -> anyhow::Result<usize> {
    let database_path = store::rxdb_store_path(root);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS knowledge tables sync runtime")?;
    let _database_guard = TEMPORARY_RXDB_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.block_on(async move {
        if let Some(peer) = current_peer() {
            return sync_knowledge_tables_with_database_if_changed(
                root,
                &peer.database,
                last_source_stamp,
            )
            .await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced =
            sync_knowledge_tables_with_database_if_changed(root, &database, last_source_stamp)
                .await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

pub(crate) fn sync_business_record_projections(root: &Path) -> anyhow::Result<usize> {
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
        let database_write_lock = Arc::new(AsyncMutex::new(()));
        if let Some(peer) = current_peer() {
            let mut since_by_collection = HashMap::new();
            let mut queue_chat_repair_stamp = None;
            return sync_business_record_projections_with_database(
                root,
                &peer.database,
                &database_write_lock,
                &mut since_by_collection,
                &mut queue_chat_repair_stamp,
            )
            .await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let mut since_by_collection = HashMap::new();
        let mut queue_chat_repair_stamp = None;
        let synced = sync_business_record_projections_with_database(
            root,
            &database,
            &database_write_lock,
            &mut since_by_collection,
            &mut queue_chat_repair_stamp,
        )
        .await?;
        database
            .close()
            .await
            .map_err(|err| anyhow::anyhow!("close temporary Business OS RxDB database: {err}"))?;
        Ok(synced)
    })
}

#[cfg(test)]
fn sync_business_record_projections_if_changed(
    root: &Path,
    since_by_collection: &mut HashMap<String, i64>,
    queue_chat_repair_stamp: &mut Option<QueueChatRepairProjectionStamp>,
    last_source_stamp: &mut Option<BusinessRecordProjectionSourceStamp>,
) -> anyhow::Result<usize> {
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
        let database_write_lock = Arc::new(AsyncMutex::new(()));
        if let Some(peer) = current_peer() {
            return sync_business_record_projections_with_database_if_changed(
                root,
                &peer.database,
                &database_write_lock,
                since_by_collection,
                queue_chat_repair_stamp,
                last_source_stamp,
            )
            .await;
        }

        let database = open_database(database_path).await?;
        database
            .add_collections(collection_creators())
            .await
            .map_err(|err| anyhow::anyhow!("register Business OS RxDB collections: {err}"))?;
        let synced = sync_business_record_projections_with_database_if_changed(
            root,
            &database,
            &database_write_lock,
            since_by_collection,
            queue_chat_repair_stamp,
            last_source_stamp,
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

pub fn browser_session_automation(
    root: &Path,
    request: BrowserSessionAutomationRequest,
) -> anyhow::Result<Value> {
    let session_id = request.session_id.trim().to_string();
    if session_id.is_empty() {
        anyhow::bail!("session_id is required for persistent browser automation");
    }
    if request.source.trim().is_empty() {
        anyhow::bail!("browser automation source is empty");
    }
    let database_path = store::rxdb_store_path(root);
    let root = root.to_path_buf();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Business OS browser automation runtime")?;
    if let Some(peer) = current_peer() {
        return runtime.block_on(async move {
            browser_session_automation_with_database(root, &peer.database, request).await
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
        let session_id = request.session_id.trim().to_string();
        let output = browser_session_automation_with_database(root, &database, request).await;
        browser_runtime_manager().stop(&session_id).await;
        // This CLI fallback is short-lived. Awaiting RxDB close here can leave
        // browser-automation commands stuck after evidence is already produced,
        // which blocks deployment-audit task execution. Let process teardown
        // reclaim the temporary handle instead of making CLI completion depend
        // on close liveness.
        output
    })
}

fn current_peer() -> Option<Arc<NativePeer>> {
    NATIVE_PEER.lock().ok().and_then(|guard| guard.clone())
}

async fn run_native_peer(
    root: PathBuf,
    sync_room: String,
    signaling_urls: Vec<String>,
    signaling_room_password: String,
) -> anyhow::Result<NativePeerExit> {
    NATIVE_PEER_REPLICATION_UP.store(false, Ordering::SeqCst);
    NATIVE_PEER_SIGNALING_JOIN_ACCEPTED.store(false, Ordering::SeqCst);
    NATIVE_PEER_DATA_CHANNEL_OPEN.store(false, Ordering::SeqCst);
    NATIVE_PEER_CRITICAL_TASKS_ALIVE.store(false, Ordering::SeqCst);
    NATIVE_PEER_PENDING_OUTBOX.store(0, Ordering::Relaxed);
    let Some(process_lock) = acquire_native_peer_process_lock(&root)? else {
        eprintln!("[business-os] native rxdb peer already runs in another process");
        return Ok(NativePeerExit::LockHeldElsewhere);
    };
    let configured_signaling_urls = signaling_urls.clone();
    let runtime_schema_fingerprint = runtime_installed_module_schema_fingerprint(&root)?;
    let signaling_base_url = signaling_urls
        .into_iter()
        .find(|url| !url.trim().is_empty())
        .context("Business OS native RxDB peer requires a signaling URL")?;
    let peer_session_id = format!("rxdb-rs-{}", Uuid::new_v4().simple());
    // The provider re-derives the URL — including fresh `token_iat`/
    // `token_exp` — on EVERY signaling (re)connect attempt. Baking the token
    // window in once meant that after >24h uptime any socket drop became a
    // permanent join-rejection loop (server: "control plane token expired").
    let signaling_url_provider: std::sync::Arc<dyn Fn() -> String + Send + Sync> = {
        let base_url = signaling_base_url.clone();
        let sync_room = sync_room.clone();
        let password = signaling_room_password.clone();
        let peer_session_id = peer_session_id.clone();
        std::sync::Arc::new(move || {
            signaling_url_with_native_metadata(&base_url, &sync_room, &password, &peer_session_id)
        })
    };
    let ice_servers = {
        let mut sync = store::sync_config(&root)?;
        // Mint an ephemeral TURN credential for the native peer too (no-op unless
        // a TURN URL + secret are configured). Re-derived on each peer bring-up.
        if let Some(turn) = store::ephemeral_turn_server(&root, &peer_session_id) {
            sync.ice_servers.push(turn);
        }
        ice_servers_from_sync_config(&sync.ice_servers)
    };
    let database_path = store::rxdb_store_path(&root);
    // Publish process liveness before opening or repairing the potentially
    // large SQLite store. SQLite has to parse the complete schema on first
    // access, which can take noticeable time for long-lived installations.
    // Holding the peer lock without a heartbeat during that work makes a
    // healthy startup indistinguishable from a wedged peer.
    let status_heartbeat = spawn_native_peer_status_heartbeat(
        root.clone(),
        peer_session_id.clone(),
        database_path.clone(),
    );
    match repair_stale_rxdb_collection_schema_versions(&root) {
        Ok(result) => {
            let repaired_tables = result
                .get("repaired_tables")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let repaired_triggers = result
                .get("repaired_triggers")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if repaired_tables > 0 || repaired_triggers > 0 {
                eprintln!(
                    "[business-os] repaired {repaired_tables} stale RxDB collection schema table(s) \
                     and {repaired_triggers} trigger(s) before native peer startup"
                );
            }
        }
        Err(err) => {
            eprintln!(
                "[business-os] stale RxDB schema table repair failed before peer startup: {err:#}"
            );
        }
    }
    let database = open_database(database_path.clone()).await?;
    let database_write_lock = Arc::new(AsyncMutex::new(()));

    // FIX 4: register collections fault tolerantly. A drifted/failing OPTIONAL
    // collection is logged and skipped; a failing REQUIRED collection still
    // aborts the peer (the daemon depends on those). The strict
    // all-or-nothing `add_collections` is no longer used here.
    let (collections, failed_collections) = database
        .add_collections_tolerant(collection_creators_for_root(&root))
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
    match compact_desktop_file_index_store(&root).await {
        Ok(stats) if stats.changed() => {
            log_desktop_file_index_maintenance_stats(&stats);
        }
        Ok(_) => {}
        Err(err) => {
            eprintln!("[business-os] desktop file index maintenance failed: {err:#}");
        }
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
    let collection_names: Vec<String> = collections
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();
    store::ensure_legacy_collection_grants(&root, &collection_names)
        .context("materialize exact legacy collection grants")?;
    let collection_list: Vec<Arc<RxCollection>> = collections
        .into_iter()
        .map(|(_, collection)| collection)
        .collect();
    let collection_count = collection_list.len();
    let mut pools: Vec<WebRtcPool> = Vec::with_capacity(1);
    if collection_count == 0 {
        eprintln!(
            "[business-os] no Business OS RxDB collections to replicate; skipping WebRTC bring-up"
        );
    } else {
        let topic = sync_room.clone();
        let multi_signaling_url_provider = std::sync::Arc::clone(&signaling_url_provider);
        let multi_peer_session_id = peer_session_id.clone();
        let multi_ice_servers = ice_servers.clone();
        // Server-authoritative per-device revocation: deny any signaling peer id
        // present in the revocation registry at connect time. The browser cannot
        // override this — the gate runs on the native (master-authoritative) peer.
        let revocation_root = root.clone();
        let is_peer_valid: std::sync::Arc<dyn Fn(&String) -> bool + Send + Sync> =
            std::sync::Arc::new(move |peer_id: &String| {
                !store::is_business_peer_revoked(&revocation_root, peer_id)
            });
        // Server-authoritative exact per-collection read authz. Missing,
        // expired, revoked, or stale-epoch capabilities fail closed.
        let collection_authz: Option<CollectionAuthzHook> =
            if store::collection_authz_enabled(&root) {
                let authz_root = root.clone();
                Some(std::sync::Arc::new(move |token: &str, collection: &str| {
                    store::capability_allows_collection_permission(
                        &authz_root,
                        token,
                        collection,
                        crate::business_os::policy::BusinessOsPermission::DataRead,
                    )
                }))
            } else {
                None
            };
        let collection_write_authz: Option<CollectionAuthzHook> = {
            let write_authz_root = root.clone();
            Some(std::sync::Arc::new(move |token: &str, collection: &str| {
                super::threads::may_accept_peer_write(&write_authz_root, token, collection)
            }))
        };
        let document_read_authz: Option<DocumentReadAuthzHook> = {
            let doc_authz_root = root.clone();
            Some(std::sync::Arc::new(
                move |token: &str, collection: &str, document: &Value| {
                    super::threads::may_replicate_document(
                        &doc_authz_root,
                        token,
                        collection,
                        document,
                    )
                },
            ))
        };
        let document_write_authz: Option<DocumentWriteAuthzHook> = {
            let doc_write_authz_root = root.clone();
            Some(std::sync::Arc::new(
                move |token: &str, collection: &str, document: &Value| {
                    super::threads::may_accept_peer_document_write(
                        &doc_write_authz_root,
                        token,
                        collection,
                        document,
                    )
                },
            ))
        };
        let mut bringup = tokio::spawn(async move {
            rxdb::plugins::replication_webrtc::replicate_web_rtc_rs_multi_with_url_provider(
                collection_list,
                multi_signaling_url_provider,
                topic,
                multi_peer_session_id,
                multi_ice_servers,
                Some(is_peer_valid),
                collection_authz,
                collection_write_authz,
                document_read_authz,
                document_write_authz,
                20,
                20,
                5_000,
            )
            .await
        });
        // Bring-up failure is FATAL for this run: returning the error hands
        // control to the supervision loop, which respawns with backoff. The
        // previous behavior — log and keep running with an empty pool list —
        // produced the canonical zombie: heartbeat "running", zero
        // replication, no retry, until a manual daemon restart.
        match tokio::time::timeout(
            Duration::from_secs(NATIVE_COLLECTION_BRINGUP_TIMEOUT_SECS),
            &mut bringup,
        )
        .await
        {
            Ok(Ok(Ok(pool))) => {
                if let Ok(mut breaker) = native_peer_circuit_breaker().lock() {
                    breaker.record_success();
                }
                eprintln!(
                    "[business-os] multiplexed WebRTC replication up for {collection_count} \
                     collections on one connection (room `{sync_room}`)"
                );
                // Phase 4: register demand-fetch file SOURCES on the pool's file
                // fetch registry so `rxdb.file.fetch` actually serves bytes for
                // the file-bearing chunk collections (without a source the
                // dispatcher always returns FILE_NOT_FOUND). The query registry
                // already auto-registers every multiplexed collection inside
                // `RxWebRTCReplicationPool::new_multi`.
                register_demand_file_sources(&pool, &database, &root);
                pools.push(pool);
            }
            Ok(Ok(Err(err))) => {
                anyhow::bail!("multiplexed WebRTC replication bring-up failed: {err}");
            }
            Ok(Err(join_err)) => {
                anyhow::bail!("multiplexed WebRTC replication bring-up task panicked: {join_err}");
            }
            Err(_) => {
                // Abort the in-flight attempt: letting it run detached used
                // to leak a LIVE orphan replication session (joined to the
                // room under this peer's session id, answering handshakes,
                // no demand-file sources, uncancelable).
                bringup.abort();
                anyhow::bail!(
                    "multiplexed WebRTC replication bring-up timed out after {}s",
                    NATIVE_COLLECTION_BRINGUP_TIMEOUT_SECS
                );
            }
        }
    }

    let command_consumer = tokio::spawn(consume_business_commands_loop(
        root.clone(),
        Arc::clone(&database),
    ));
    let command_outbox = tokio::spawn(deliver_business_command_outbox_background_loop(
        root.clone(),
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
    let workspace_branding_sync = tokio::spawn(sync_workspace_branding_background_loop(
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
    let knowledge_tables_sync = tokio::spawn(sync_knowledge_tables_background_loop(
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
    let iot_agent_supervisors = spawn_iot_agent_supervisors(root.clone());
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
        root: root.clone(),
        database,
        peer_session_id,
        shutdown_tx: Mutex::new(Some(shutdown_tx)),
        _process_lock: process_lock,
        _pools: pools,
        _command_consumer: command_consumer,
        _command_outbox: command_outbox,
        _notes_sync: notes_sync,
        _file_index_sync: file_index_sync,
        _channel_state_sync: channel_state_sync,
        _business_users_sync: business_users_sync,
        _runtime_settings_sync: runtime_settings_sync,
        _workspace_branding_sync: workspace_branding_sync,
        _module_catalog_sync: module_catalog_sync,
        _ticket_state_sync: ticket_state_sync,
        _knowledge_tables_sync: knowledge_tables_sync,
        _business_record_projection_sync: business_record_projection_sync,
        _iot_agent_supervisors: iot_agent_supervisors,
        _browser_runtime_maintenance: browser_runtime_maintenance,
        _status_heartbeat: Mutex::new(Some(status_heartbeat)),
    });
    if let Ok(mut current) = NATIVE_PEER.lock() {
        *current = Some(Arc::clone(&peer));
    }
    NATIVE_PEER_RUNNING.store(true, Ordering::SeqCst);
    peer.refresh_liveness_signals();

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
    let mut runtime_schema_watch = tokio::time::interval(Duration::from_secs(
        NATIVE_PEER_RUNTIME_SCHEMA_WATCH_INTERVAL_SECS,
    ));
    runtime_schema_watch.tick().await;
    let mut exit = NativePeerExit::Shutdown;
    let mut progress_stall_ticks = 0_u32;
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                break;
            }
            _ = watchdog.tick() => {
                peer.refresh_liveness_signals();
                let finished_tasks = peer.finished_critical_tasks();
                if !finished_tasks.is_empty() {
                    eprintln!(
                        "[business-os] native rxdb peer watchdog: critical children exited ({}); \
                         shutting down for a supervised respawn",
                        finished_tasks.join(", ")
                    );
                    exit = NativePeerExit::CriticalChildExited;
                    break;
                }
                let command_diagnostics = crate::mission::channels::business_command_core_diagnostics(&root)
                    .unwrap_or(Value::Null);
                let pending_outbox = command_diagnostics.get("pending_outbox")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                NATIVE_PEER_PENDING_OUTBOX.store(pending_outbox, Ordering::Relaxed);
                let outbox_age_ms = command_diagnostics.get("oldest_outbox_age_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let transport = peer._pools.first()
                    .map(|pool| pool.connection_handler.frame_transport_status_json())
                    .unwrap_or(Value::Null);
                let transport_backlog = transport.get("priorityQueueDepth")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let transport_age_ms = transport.get("oldestQueuedAgeMs")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let has_backlog = pending_outbox > 0 || transport_backlog > 0;
                let progress_age_ms = outbox_age_ms.max(transport_age_ms);
                if has_backlog && progress_age_ms >= NATIVE_PEER_PROGRESS_WARN_AGE_MS {
                    eprintln!(
                        "[business-os] native rxdb peer watchdog: backlog without progress for {progress_age_ms}ms \
                         (outbox={pending_outbox}, transport={transport_backlog})"
                    );
                }
                if has_backlog && progress_age_ms >= NATIVE_PEER_PROGRESS_RESPAWN_AGE_MS {
                    progress_stall_ticks = progress_stall_ticks.saturating_add(1);
                } else {
                    progress_stall_ticks = 0;
                }
                if progress_stall_ticks >= NATIVE_PEER_PROGRESS_RESPAWN_TICKS {
                    eprintln!(
                        "[business-os] native rxdb peer watchdog: durable backlog stalled across \
                         {progress_stall_ticks} watchdog ticks; shutting down for supervised respawn"
                    );
                    exit = NativePeerExit::CriticalChildExited;
                    break;
                }
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
                         shutting down for a supervised respawn",
                        heartbeat_age_ms
                    );
                    exit = NativePeerExit::WatchdogStale;
                    break;
                }
                match native_peer_sync_config_changed(
                    &root,
                    &sync_room,
                    &signaling_room_password,
                    &configured_signaling_urls,
                ) {
                    Ok(true) => {
                        eprintln!(
                            "[business-os] native rxdb peer watchdog: sync config changed; \
                             shutting down for a supervised respawn"
                        );
                        exit = NativePeerExit::ConfigChanged;
                        break;
                    }
                    Ok(false) => {}
                    Err(err) => {
                        eprintln!(
                            "[business-os] native rxdb peer watchdog: sync config check failed: {err:#}"
                        );
                    }
                }
            }
            _ = runtime_schema_watch.tick() => {
                match native_peer_runtime_installed_schemas_changed(
                    &root,
                    &runtime_schema_fingerprint,
                ) {
                    Ok(true) => {
                        eprintln!(
                            "[business-os] native rxdb peer: runtime app schemas changed; \
                             shutting down for immediate supervised reconfiguration"
                        );
                        exit = NativePeerExit::RuntimeSchemaChanged;
                        break;
                    }
                    Ok(false) => {}
                    Err(err) => {
                        eprintln!(
                            "[business-os] native rxdb peer runtime app schema check failed: {err:#}"
                        );
                    }
                }
            }
        }
    }

    NATIVE_PEER_REPLICATION_UP.store(false, Ordering::SeqCst);
    NATIVE_PEER_SIGNALING_JOIN_ACCEPTED.store(false, Ordering::SeqCst);
    NATIVE_PEER_DATA_CHANNEL_OPEN.store(false, Ordering::SeqCst);
    NATIVE_PEER_CRITICAL_TASKS_ALIVE.store(false, Ordering::SeqCst);
    NATIVE_PEER_PENDING_OUTBOX.store(0, Ordering::Relaxed);
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
    // NATIVE_PEER_STARTED stays true here: the supervision loop in
    // `spawn_native_peer` owns that flag and only clears it when it decides
    // not to respawn. Clearing it from inside a run opened a window where a
    // concurrent `ensure_native_peer` spawned a SECOND peer thread.
    NATIVE_PEER_RUNNING.store(false, Ordering::SeqCst);
    Ok(exit)
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

fn native_peer_sync_config_changed(
    root: &Path,
    active_sync_room: &str,
    active_signaling_room_password: &str,
    active_signaling_urls: &[String],
) -> anyhow::Result<bool> {
    let config = store::sync_connection_config(root)?;
    Ok(config.sync_room != active_sync_room
        || config.signaling_room_password != active_signaling_room_password
        || normalized_signaling_urls(&config.signaling_urls)
            != normalized_signaling_urls(active_signaling_urls))
}

fn native_peer_runtime_installed_schemas_changed(
    root: &Path,
    active_fingerprint: &str,
) -> anyhow::Result<bool> {
    Ok(runtime_installed_module_schema_fingerprint(root)? != active_fingerprint)
}

fn runtime_installed_module_schema_fingerprint(root: &Path) -> anyhow::Result<String> {
    let modules_root =
        resolve_business_os_installed_app_root_for_native_peer(root).join("installed-modules");
    let mut files = BTreeSet::new();
    if modules_root.is_dir() {
        for entry in fs::read_dir(&modules_root).with_context(|| {
            format!(
                "failed to read runtime-installed module root {}",
                modules_root.display()
            )
        })? {
            let entry = entry?;
            let module_dir = entry.path();
            if !module_dir.is_dir() {
                continue;
            }
            for file_name in ["module.json", "collections.schema.json"] {
                let path = module_dir.join(file_name);
                if path.is_file() {
                    files.insert(path);
                }
            }
        }
    }

    let mut hasher = sha2::Sha256::new();
    hasher.update(b"ctox-runtime-installed-module-schemas-v1");
    for path in files {
        let rel = path.strip_prefix(&modules_root).unwrap_or(&path);
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update([0]);
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read runtime app schema {}", path.display()))?;
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
        hasher.update([0xff]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn normalized_signaling_urls(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn native_peer_config_epoch(sync_room: &str, signaling_urls: &[String], password: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"ctox-native-peer-circuit-config-v1");
    hasher.update(sync_room.trim().as_bytes());
    hasher.update([0]);
    for url in normalized_signaling_urls(signaling_urls) {
        hasher.update(url.as_bytes());
        hasher.update([0]);
    }
    // Hashing the credential into the epoch lets rotations close a permanent
    // circuit without ever exposing or persisting the credential itself.
    hasher.update(sha2::Sha256::digest(password.as_bytes()));
    format!("{:x}", hasher.finalize())
}

fn classify_native_peer_failure(message: &str) -> NativePeerSignalingFailure {
    let normalized = message.to_ascii_lowercase();
    let signaling_related = [
        "signaling",
        "webrtc",
        "web rtc",
        "replication bring-up",
        "multiplexed webrtc",
        "join rejected",
    ]
    .iter()
    .any(|needle| normalized.contains(needle));
    if !signaling_related {
        return NativePeerSignalingFailure::NotSignaling;
    }
    if [
        "expired",
        "timeout",
        "timed out",
        "temporar",
        "unavailable",
        "connection reset",
        "connection refused",
        "network",
        "server error",
        "status 5",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
    {
        return NativePeerSignalingFailure::Retryable;
    }
    if [
        "revoked",
        "revocation",
        "unauthorized",
        "forbidden",
        "invalid credential",
        "invalid token",
        "authentication failed",
        "role mismatch",
        "instance mismatch",
        "protocol mismatch",
        "incompatible",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
    {
        return NativePeerSignalingFailure::Permanent;
    }
    NativePeerSignalingFailure::Retryable
}

fn native_peer_retry_delay(base: Duration) -> Duration {
    let base_ms = base.as_millis() as u64;
    let jitter_span = (base_ms / 4).max(1);
    let jitter = (now_ms() as u64) % jitter_span;
    Duration::from_millis(base_ms.saturating_add(jitter))
}

fn signaling_url_with_native_metadata(
    raw_url: &str,
    sync_room: &str,
    signaling_room_password: &str,
    native_peer_id: &str,
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
        let native_peer_id = native_peer_id.trim();
        query.append_pair(
            "client",
            if native_peer_id.is_empty() {
                "ctox-business-os-native"
            } else {
                native_peer_id
            },
        );
        if !native_peer_id.is_empty() {
            query.append_pair("native_peer_id", native_peer_id);
        }
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
    let replication_up = NATIVE_PEER_REPLICATION_UP.load(Ordering::SeqCst)
        && NATIVE_PEER_SIGNALING_JOIN_ACCEPTED.load(Ordering::SeqCst)
        && NATIVE_PEER_DATA_CHANNEL_OPEN.load(Ordering::SeqCst)
        && NATIVE_PEER_CRITICAL_TASKS_ALIVE.load(Ordering::SeqCst);
    let critical_tasks = current_peer()
        .map(|peer| peer.task_liveness_json())
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let payload = json!({
        "version": NATIVE_PEER_STATUS_VERSION,
        "running": true,
        "pid": std::process::id(),
        "peer_session_id": peer_session_id,
        "updated_at_ms": now_ms() as u64,
        "database_path": database_path.display().to_string(),
        // Replication liveness rides on every heartbeat: "process alive" and
        // "replication session up" are different facts, and conflating them
        // hid bring-up failures behind a healthy-looking status.
        "replicationUp": replication_up,
        "replicationSignals": {
            "poolCreated": current_peer().is_some_and(|peer| !peer._pools.is_empty()),
            "signalingJoinAccepted": NATIVE_PEER_SIGNALING_JOIN_ACCEPTED.load(Ordering::SeqCst),
            "dataChannelOpen": NATIVE_PEER_DATA_CHANNEL_OPEN.load(Ordering::SeqCst),
            "criticalTasksAlive": NATIVE_PEER_CRITICAL_TASKS_ALIVE.load(Ordering::SeqCst),
        },
        "circuitBreaker": native_peer_circuit_snapshot(),
        "criticalTasks": critical_tasks,
        "performance": native_peer_performance_snapshot(),
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
            NATIVE_PEER_HEARTBEAT_THREAD_ALIVE.store(true, Ordering::SeqCst);
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
            NATIVE_PEER_HEARTBEAT_THREAD_ALIVE.store(false, Ordering::SeqCst);
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
        upsert_desktop_file_with_policy(&self.root, &self.database, path, policy).await
    }

    async fn sync_desktop_files_from_scan_roots(
        &self,
        scan_roots: Vec<DesktopFileScanRoot>,
    ) -> anyhow::Result<usize> {
        sync_desktop_file_scan_roots_with_database(&self.root, &self.database, scan_roots).await
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopFileIndexProjectionStamp {
    scan_root_count: usize,
    candidate_count: usize,
    truncated: bool,
    content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopFileScanRootsStamp {
    scan_root_count: usize,
    content_hash: String,
}

#[derive(Debug)]
struct DesktopFileIndexScan {
    scan_roots: Vec<DesktopFileScanRoot>,
    candidates: Vec<DesktopFileIndexCandidate>,
    stamp: DesktopFileIndexProjectionStamp,
}

struct DesktopFileIndexWatch {
    _watcher: notify::RecommendedWatcher,
    rx: mpsc::UnboundedReceiver<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchEventWait {
    Event,
    Timeout,
    Closed,
}

fn watch_event_from_drain(saw_event: bool) -> WatchEventWait {
    if saw_event {
        WatchEventWait::Event
    } else {
        WatchEventWait::Timeout
    }
}

fn is_sync_relevant_watch_event(event: &notify::Event) -> bool {
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

fn forward_sync_relevant_watch_event(
    tx: &mpsc::UnboundedSender<()>,
    event: notify::Result<notify::Event>,
) {
    if event.as_ref().is_ok_and(is_sync_relevant_watch_event) {
        let _ = tx.send(());
    }
}

impl DesktopFileIndexWatch {
    fn new(scan_roots: &[DesktopFileScanRoot]) -> anyhow::Result<Option<Self>> {
        if scan_roots.is_empty() {
            return Ok(None);
        }
        let roots = scan_roots
            .iter()
            .map(|root| root.path.clone())
            .collect::<Vec<_>>();
        let (tx, rx) = mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            forward_sync_relevant_watch_event(&tx, event);
        })
        .context("create desktop file index watcher")?;
        for root in &roots {
            watcher
                .watch(root, notify::RecursiveMode::Recursive)
                .with_context(|| format!("watch desktop file scan root {}", root.display()))?;
        }
        Ok(Some(Self {
            _watcher: watcher,
            rx,
        }))
    }

    fn drain_pending(&mut self) -> bool {
        let mut saw_event = false;
        while self.rx.try_recv().is_ok() {
            saw_event = true;
        }
        saw_event
    }

    async fn wait_for_event(&mut self, timeout: Duration) -> WatchEventWait {
        if timeout.is_zero() {
            return watch_event_from_drain(self.drain_pending());
        }
        tokio::select! {
            _ = tokio::time::sleep(timeout) => watch_event_from_drain(self.drain_pending()),
            event = self.rx.recv() => {
                match event {
                    Some(_) => {
                        let _ = self.drain_pending();
                        WatchEventWait::Event
                    }
                    None => {
                        tokio::time::sleep(timeout).await;
                        WatchEventWait::Closed
                    }
                }
            }
        }
    }
}

struct NotesSyncWatch {
    _watcher: notify::RecommendedWatcher,
    rx: mpsc::UnboundedReceiver<()>,
    notes_dir_exists: bool,
}

impl NotesSyncWatch {
    fn new(root: &Path) -> anyhow::Result<Self> {
        let business_os_dir = root.join("runtime/business-os");
        std::fs::create_dir_all(&business_os_dir)
            .with_context(|| format!("create notes watch parent {}", business_os_dir.display()))?;
        let notes_dir = business_os_dir.join("notes");
        let notes_dir_exists = notes_dir.is_dir();
        let (tx, rx) = mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            forward_sync_relevant_watch_event(&tx, event);
        })
        .context("create notes sync watcher")?;
        watcher
            .watch(&business_os_dir, notify::RecursiveMode::NonRecursive)
            .with_context(|| {
                format!(
                    "watch Business OS notes parent {}",
                    business_os_dir.display()
                )
            })?;
        if notes_dir_exists {
            watcher
                .watch(&notes_dir, notify::RecursiveMode::Recursive)
                .with_context(|| format!("watch notes directory {}", notes_dir.display()))?;
        }
        Ok(Self {
            _watcher: watcher,
            rx,
            notes_dir_exists,
        })
    }

    fn drain_pending(&mut self) -> bool {
        let mut saw_event = false;
        while self.rx.try_recv().is_ok() {
            saw_event = true;
        }
        saw_event
    }

    async fn wait_for_event(&mut self, timeout: Duration) -> WatchEventWait {
        if timeout.is_zero() {
            return watch_event_from_drain(self.drain_pending());
        }
        tokio::select! {
            _ = tokio::time::sleep(timeout) => watch_event_from_drain(self.drain_pending()),
            event = self.rx.recv() => {
                match event {
                    Some(_) => {
                        let _ = self.drain_pending();
                        WatchEventWait::Event
                    }
                    None => {
                        tokio::time::sleep(timeout).await;
                        WatchEventWait::Closed
                    }
                }
            }
        }
    }
}

fn desktop_file_scan_root_paths(scan_roots: &[DesktopFileScanRoot]) -> Vec<PathBuf> {
    scan_roots
        .iter()
        .map(|scan_root| scan_root.path.clone())
        .collect()
}

async fn sync_notes_background_loop(root: PathBuf) {
    let mut last_source_stamp: Option<store::LocalMarkdownNotesSourceStamp> = None;
    let mut unchanged_ticks = 0u32;
    let mut dirty_notes = true;
    let mut notes_watch: Option<NotesSyncWatch> = None;
    let mut last_watch_error: Option<String> = None;
    loop {
        let started = Instant::now();
        let result: anyhow::Result<bool> = async {
            let notes_dir_exists = root.join("runtime/business-os/notes").is_dir();
            let refresh_watch = notes_watch
                .as_ref()
                .map(|watch| watch.notes_dir_exists != notes_dir_exists)
                .unwrap_or(true);
            if refresh_watch {
                match NotesSyncWatch::new(&root) {
                    Ok(next_watch) => {
                        notes_watch = Some(next_watch);
                        last_watch_error = None;
                        dirty_notes = true;
                    }
                    Err(err) => {
                        let message = format!("{err:#}");
                        if last_watch_error.as_deref() != Some(message.as_str()) {
                            eprintln!("[business-os] notes sync watcher unavailable: {message}");
                        }
                        last_watch_error = Some(message);
                        notes_watch = None;
                        dirty_notes = true;
                    }
                }
            }
            if notes_watch
                .as_mut()
                .map(NotesSyncWatch::drain_pending)
                .unwrap_or(false)
            {
                dirty_notes = true;
            }
            if !dirty_notes {
                return Ok(false);
            }

            let root_for_stamp = root.clone();
            let source_stamp = tokio::task::spawn_blocking(move || {
                store::local_markdown_notes_source_stamp(&root_for_stamp)
            })
            .await
            .context("join native notes source stamp")??;
            dirty_notes = false;
            if last_source_stamp.as_ref() == Some(&source_stamp) {
                return Ok(false);
            }

            let root_for_sync = root.clone();
            tokio::task::spawn_blocking(move || store::sync_local_markdown_notes(&root_for_sync))
                .await
                .context("join native rxdb notes sync")??;
            let root_for_stamp = root.clone();
            last_source_stamp = Some(
                tokio::task::spawn_blocking(move || {
                    store::local_markdown_notes_source_stamp(&root_for_stamp)
                })
                .await
                .context("join native notes source stamp after sync")??,
            );
            dirty_notes = false;
            Ok(true)
        }
        .await;
        record_native_peer_bool_loop_result(&NOTES_LOOP_METRICS, &result, started.elapsed());
        match result {
            Ok(source_changed) => {
                notes_sync_update_idle_ticks(&mut unchanged_ticks, source_changed);
            }
            Err(err) => {
                unchanged_ticks = 0;
                eprintln!("[business-os] native rxdb notes sync failed: {err:#}");
            }
        }
        let sleep_interval = notes_sync_sleep_interval(unchanged_ticks);
        let event_driven = notes_watch.is_some();
        let watch_wait = if let Some(watch) = notes_watch.as_mut() {
            watch.wait_for_event(sleep_interval).await
        } else {
            tokio::time::sleep(sleep_interval).await;
            WatchEventWait::Timeout
        };
        if watch_wait == WatchEventWait::Closed {
            notes_watch = None;
            dirty_notes = true;
            unchanged_ticks = 0;
            continue;
        }
        if watch_wait == WatchEventWait::Event
            || !event_driven
            || unchanged_ticks >= NOTES_SYNC_IDLE_BACKOFF_AFTER_TICKS
        {
            dirty_notes = true;
        }
    }
}

fn notes_sync_update_idle_ticks(unchanged_ticks: &mut u32, source_changed: bool) {
    if source_changed {
        *unchanged_ticks = 0;
    } else {
        *unchanged_ticks = unchanged_ticks.saturating_add(1);
    }
}

fn notes_sync_sleep_interval(unchanged_ticks: u32) -> Duration {
    if unchanged_ticks >= NOTES_SYNC_IDLE_BACKOFF_AFTER_TICKS {
        Duration::from_secs(NOTES_SYNC_IDLE_INTERVAL_SECS)
    } else {
        Duration::from_secs(NOTES_SYNC_ACTIVE_INTERVAL_SECS)
    }
}

async fn sync_desktop_file_index_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut last_maintenance_at = SystemTime::UNIX_EPOCH;
    let mut last_projection_stamp: Option<DesktopFileIndexProjectionStamp> = None;
    let mut last_scan_roots_stamp: Option<DesktopFileScanRootsStamp> = None;
    let mut last_full_scan_at: Option<SystemTime> = None;
    let mut dirty_scan_roots = true;
    let mut file_watch: Option<DesktopFileIndexWatch> = None;
    let mut file_watch_roots: Option<Vec<PathBuf>> = None;
    let mut last_watch_error: Option<String> = None;
    loop {
        let started = Instant::now();
        let mut has_scan_roots = false;
        let result: anyhow::Result<usize> = async {
            let run_maintenance = last_maintenance_at
                .elapsed()
                .map(|elapsed| {
                    elapsed >= Duration::from_secs(DESKTOP_FILE_INDEX_MAINTENANCE_INTERVAL_SECS)
                })
                .unwrap_or(true);
            let scan_roots = desktop_file_scan_roots(&root);
            has_scan_roots = !scan_roots.is_empty();
            let scan_root_paths = desktop_file_scan_root_paths(&scan_roots);
            if file_watch_roots.as_ref() != Some(&scan_root_paths) {
                match DesktopFileIndexWatch::new(&scan_roots) {
                    Ok(next_watch) => {
                        file_watch = next_watch;
                        file_watch_roots = Some(scan_root_paths);
                        last_watch_error = None;
                        dirty_scan_roots = true;
                    }
                    Err(err) => {
                        let message = format!("{err:#}");
                        if last_watch_error.as_deref() != Some(message.as_str()) {
                            eprintln!(
                                "[business-os] desktop file index watcher unavailable: {message}"
                            );
                        }
                        last_watch_error = Some(message);
                        file_watch = None;
                        file_watch_roots = None;
                    }
                }
            }
            if file_watch
                .as_mut()
                .map(DesktopFileIndexWatch::drain_pending)
                .unwrap_or(false)
            {
                dirty_scan_roots = true;
            }
            let scan_roots_stamp = desktop_file_scan_roots_stamp(&scan_roots);
            let should_collect_scan = desktop_file_index_should_collect_scan(
                last_scan_roots_stamp.as_ref(),
                last_full_scan_at,
                &scan_roots_stamp,
                dirty_scan_roots,
                SystemTime::now(),
            );
            if !run_maintenance && !should_collect_scan {
                return Ok(0);
            }

            if run_maintenance {
                match compact_desktop_file_index_store(&root).await {
                    Ok(stats) if stats.changed() => {
                        log_desktop_file_index_maintenance_stats(&stats);
                    }
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("[business-os] desktop file index maintenance failed: {err:#}");
                    }
                }
                last_maintenance_at = SystemTime::now();
            }
            if !should_collect_scan {
                return Ok(0);
            }

            let scan = collect_desktop_file_index_scan(scan_roots).await?;
            last_scan_roots_stamp = Some(scan_roots_stamp);
            last_full_scan_at = Some(SystemTime::now());
            let projection_changed = last_projection_stamp.as_ref() != Some(&scan.stamp);
            if !projection_changed {
                dirty_scan_roots = false;
                return Ok(0);
            }
            let projection_stamp = scan.stamp.clone();
            let _guard = database_write_lock.lock().await;
            let indexed = sync_desktop_file_scan_with_database(&root, &database, scan).await?;
            last_projection_stamp = Some(projection_stamp);
            dirty_scan_roots = false;
            Ok(indexed)
        }
        .await;
        record_native_peer_loop_result(
            &DESKTOP_FILE_INDEX_LOOP_METRICS,
            &result,
            started.elapsed(),
        );
        let result_failed = result.is_err();
        if let Err(err) = &result {
            eprintln!("[business-os] native rxdb desktop file index failed: {err:#}");
        }
        let sleep_for = if result_failed {
            Duration::from_secs(DESKTOP_FILE_SCAN_INTERVAL_SECS)
        } else {
            desktop_file_index_sleep_interval(
                has_scan_roots,
                last_maintenance_at,
                last_full_scan_at,
                SystemTime::now(),
            )
        };
        let watch_wait = if let Some(watch) = file_watch.as_mut() {
            watch.wait_for_event(sleep_for).await
        } else {
            tokio::time::sleep(sleep_for).await;
            WatchEventWait::Timeout
        };
        match watch_wait {
            WatchEventWait::Event => {
                dirty_scan_roots = true;
            }
            WatchEventWait::Closed => {
                file_watch = None;
                file_watch_roots = None;
                dirty_scan_roots = true;
            }
            WatchEventWait::Timeout => {}
        }
    }
}

async fn sync_channel_state_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut last_projection_stamp: Option<ChannelStateProjectionStamp> = None;
    let mut consecutive_idle_rounds = 0u32;
    loop {
        let started = Instant::now();
        let result: anyhow::Result<usize> = async {
            let projection_stamp = channel_state_projection_stamp_async(&root).await?;
            if last_projection_stamp.as_ref() == Some(&projection_stamp) {
                return Ok(0);
            }

            let _guard = database_write_lock.lock().await;
            let synced = sync_channel_state_with_database(&root, &database).await?;
            last_projection_stamp = Some(projection_stamp);
            Ok(synced)
        }
        .await;
        record_native_peer_loop_result(&CHANNEL_STATE_LOOP_METRICS, &result, started.elapsed());
        update_projection_idle_rounds(
            result,
            &mut consecutive_idle_rounds,
            "[business-os] native rxdb channel state sync failed",
        );
        tokio::time::sleep(Duration::from_secs(business_os_projection_sleep_secs(
            CHANNEL_STATE_SYNC_INTERVAL_SECS,
            consecutive_idle_rounds,
        )))
        .await;
    }
}

async fn sync_business_users_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut last_projection_stamp: Option<store::BusinessUsersProjectionStamp> = None;
    let mut consecutive_idle_rounds = 0u32;
    loop {
        let started = Instant::now();
        let result: anyhow::Result<usize> = async {
            let projection_stamp = business_users_projection_stamp(&root).await?;
            if last_projection_stamp.as_ref() == Some(&projection_stamp) {
                return Ok(0);
            }

            let _guard = database_write_lock.lock().await;
            let synced = sync_business_users_with_database(&root, &database).await?;
            last_projection_stamp = Some(business_users_projection_stamp(&root).await?);
            Ok(synced)
        }
        .await;
        record_native_peer_loop_result(&BUSINESS_USERS_LOOP_METRICS, &result, started.elapsed());
        update_projection_idle_rounds(
            result,
            &mut consecutive_idle_rounds,
            "[business-os] native rxdb business users sync failed",
        );
        tokio::time::sleep(Duration::from_secs(business_os_projection_sleep_secs(
            BUSINESS_USERS_SYNC_INTERVAL_SECS,
            consecutive_idle_rounds,
        )))
        .await;
    }
}

async fn sync_runtime_settings_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut last_projection_stamp: Option<store::RuntimeSettingsProjectionStamp> = None;
    let mut consecutive_idle_rounds = 0u32;
    loop {
        let started = Instant::now();
        let result: anyhow::Result<usize> = async {
            let projection_stamp = runtime_settings_projection_stamp(&root).await?;
            if last_projection_stamp.as_ref() == Some(&projection_stamp) {
                return Ok(0);
            }

            let _guard = database_write_lock.lock().await;
            let synced = sync_runtime_settings_with_database(&root, &database).await?;
            last_projection_stamp = Some(projection_stamp);
            Ok(synced)
        }
        .await;
        record_native_peer_loop_result(&RUNTIME_SETTINGS_LOOP_METRICS, &result, started.elapsed());
        update_projection_idle_rounds(
            result,
            &mut consecutive_idle_rounds,
            "[business-os] native rxdb runtime settings sync failed",
        );
        tokio::time::sleep(Duration::from_secs(business_os_projection_sleep_secs(
            RUNTIME_SETTINGS_SYNC_INTERVAL_SECS,
            consecutive_idle_rounds,
        )))
        .await;
    }
}

async fn sync_workspace_branding_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut last_projection_stamp: Option<store::WorkspaceBrandingProjectionStamp> = None;
    let mut consecutive_idle_rounds = 0u32;
    loop {
        let started = Instant::now();
        let result: anyhow::Result<usize> = async {
            let projection_stamp = workspace_branding_projection_stamp(&root).await?;
            if last_projection_stamp.as_ref() == Some(&projection_stamp) {
                return Ok(0);
            }

            let _guard = database_write_lock.lock().await;
            let synced = sync_workspace_branding_with_database(&root, &database).await?;
            last_projection_stamp = Some(projection_stamp);
            Ok(synced)
        }
        .await;
        record_native_peer_loop_result(
            &WORKSPACE_BRANDING_LOOP_METRICS,
            &result,
            started.elapsed(),
        );
        update_projection_idle_rounds(
            result,
            &mut consecutive_idle_rounds,
            "[business-os] native rxdb workspace branding sync failed",
        );
        tokio::time::sleep(Duration::from_secs(business_os_projection_sleep_secs(
            RUNTIME_SETTINGS_SYNC_INTERVAL_SECS,
            consecutive_idle_rounds,
        )))
        .await;
    }
}

async fn sync_module_catalog_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut last_projection_stamp: Option<store::ModuleCatalogProjectionStamp> = None;
    let mut consecutive_idle_rounds = 0u32;
    loop {
        let started = Instant::now();
        let result: anyhow::Result<usize> = async {
            let projection_stamp = module_catalog_projection_stamp(&root).await?;
            if last_projection_stamp.as_ref() == Some(&projection_stamp) {
                return Ok(0);
            }

            let _guard = database_write_lock.lock().await;
            let synced = sync_module_catalog_with_database(&root, &database).await?;
            last_projection_stamp = Some(module_catalog_projection_stamp(&root).await?);
            Ok(synced)
        }
        .await;
        record_native_peer_loop_result(&MODULE_CATALOG_LOOP_METRICS, &result, started.elapsed());
        update_projection_idle_rounds(
            result,
            &mut consecutive_idle_rounds,
            "[business-os] native rxdb module catalog sync failed",
        );
        tokio::time::sleep(Duration::from_secs(business_os_projection_sleep_secs(
            MODULE_CATALOG_SYNC_INTERVAL_SECS,
            consecutive_idle_rounds,
        )))
        .await;
    }
}

async fn sync_ticket_state_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut consecutive_idle_rounds = 0u32;
    let mut last_source_stamp = None;
    loop {
        let started = Instant::now();
        let result = async {
            let source_stamp = ticket_state_source_stamp(&root).await?;
            if last_source_stamp.as_ref() == Some(&source_stamp) {
                return Ok(0);
            }

            let _guard = database_write_lock.lock().await;
            let synced = sync_ticket_state_with_database(&root, &database).await?;
            last_source_stamp = Some(source_stamp);
            Ok(synced)
        }
        .await;
        record_native_peer_loop_result(&TICKET_STATE_LOOP_METRICS, &result, started.elapsed());
        update_projection_idle_rounds(
            result,
            &mut consecutive_idle_rounds,
            "[business-os] native rxdb ticket state sync failed",
        );
        tokio::time::sleep(Duration::from_secs(business_os_projection_sleep_secs(
            TICKET_STATE_SYNC_INTERVAL_SECS,
            consecutive_idle_rounds,
        )))
        .await;
    }
}

async fn sync_knowledge_tables_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let mut consecutive_idle_rounds = 0u32;
    let mut last_source_stamp = None;
    loop {
        let started = Instant::now();
        let result = async {
            let source_stamp = knowledge_tables_source_stamp(&root).await?;
            if last_source_stamp.as_ref() == Some(&source_stamp) {
                return Ok(0);
            }

            let _guard = database_write_lock.lock().await;
            let synced = sync_knowledge_tables_with_database(&root, &database).await?;
            last_source_stamp = Some(source_stamp);
            Ok(synced)
        }
        .await;
        record_native_peer_loop_result(&KNOWLEDGE_TABLES_LOOP_METRICS, &result, started.elapsed());
        update_projection_idle_rounds(
            result,
            &mut consecutive_idle_rounds,
            "[business-os] native rxdb knowledge tables sync failed",
        );
        tokio::time::sleep(Duration::from_secs(business_os_projection_sleep_secs(
            KNOWLEDGE_TABLES_SYNC_INTERVAL_SECS,
            consecutive_idle_rounds,
        )))
        .await;
    }
}

async fn sync_business_record_projections_background_loop(
    root: PathBuf,
    database: Arc<RxDatabase>,
    database_write_lock: Arc<AsyncMutex<()>>,
) {
    let persisted_progress = load_business_record_projection_progress(&root);
    let mut since_by_collection = persisted_progress.since_by_collection;
    let mut after_record_id_by_collection = persisted_progress.after_record_id_by_collection;
    let mut next_collection_index = persisted_progress.next_collection_index;
    let mut queue_chat_repair_stamp = None;
    let mut last_source_stamp = None;
    let mut consecutive_idle_rounds = 0u32;
    let mut consecutive_failure_rounds = 0u32;
    loop {
        let started = Instant::now();
        let mut slice_incomplete = false;
        let result = async {
            let source_stamp = business_record_projection_source_stamp(&root).await?;
            if last_source_stamp.as_ref() == Some(&source_stamp) {
                return Ok(0);
            }

            let (synced, caught_up) = sync_business_record_projections_slice_with_database(
                &root,
                &database,
                &database_write_lock,
                &mut since_by_collection,
                &mut after_record_id_by_collection,
                &mut next_collection_index,
                &mut queue_chat_repair_stamp,
                Some(BUSINESS_RECORD_PROJECTION_SYNC_LIMIT),
            )
            .await?;
            if caught_up {
                last_source_stamp = Some(source_stamp);
            } else {
                slice_incomplete = true;
            }
            persist_business_record_projection_progress(
                &root,
                &BusinessRecordProjectionProgress {
                    version: BUSINESS_RECORD_PROJECTION_CURSOR_VERSION,
                    since_by_collection: since_by_collection.clone(),
                    after_record_id_by_collection: after_record_id_by_collection.clone(),
                    next_collection_index,
                },
            )?;
            // An incomplete slice is active reconciliation work even when it
            // only advanced across empty collections. Keep the short active
            // interval until all source collections have been visited.
            Ok(if caught_up { synced } else { synced.max(1) })
        }
        .await;
        record_native_peer_loop_result(&BUSINESS_RECORDS_LOOP_METRICS, &result, started.elapsed());
        if result.is_err() {
            consecutive_failure_rounds = consecutive_failure_rounds.saturating_add(1);
        } else {
            consecutive_failure_rounds = 0;
        }
        update_projection_idle_rounds(
            result,
            &mut consecutive_idle_rounds,
            "[business-os] native rxdb business record projection sync failed",
        );
        let sleep_secs = business_record_projection_loop_sleep_secs(
            consecutive_idle_rounds,
            consecutive_failure_rounds,
            slice_incomplete,
        );
        tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct BusinessRecordProjectionProgress {
    version: u32,
    #[serde(default)]
    since_by_collection: HashMap<String, i64>,
    #[serde(default)]
    after_record_id_by_collection: HashMap<String, String>,
    #[serde(default)]
    next_collection_index: usize,
}

fn business_record_projection_progress_path(root: &Path) -> PathBuf {
    root.join("runtime")
        .join("business-record-projection-progress.json")
}

fn load_business_record_projection_progress(root: &Path) -> BusinessRecordProjectionProgress {
    let path = business_record_projection_progress_path(root);
    let Ok(bytes) = fs::read(&path) else {
        return BusinessRecordProjectionProgress {
            version: BUSINESS_RECORD_PROJECTION_CURSOR_VERSION,
            ..Default::default()
        };
    };
    match serde_json::from_slice::<BusinessRecordProjectionProgress>(&bytes) {
        Ok(progress) if progress.version == BUSINESS_RECORD_PROJECTION_CURSOR_VERSION => progress,
        Ok(_) => BusinessRecordProjectionProgress {
            version: BUSINESS_RECORD_PROJECTION_CURSOR_VERSION,
            ..Default::default()
        },
        Err(err) => {
            eprintln!(
                "[business-os] ignoring corrupt business record projection progress {}: {err}",
                path.display()
            );
            BusinessRecordProjectionProgress {
                version: BUSINESS_RECORD_PROJECTION_CURSOR_VERSION,
                ..Default::default()
            }
        }
    }
}

fn persist_business_record_projection_progress(
    root: &Path,
    progress: &BusinessRecordProjectionProgress,
) -> anyhow::Result<()> {
    let path = business_record_projection_progress_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary_path = path.with_extension("json.tmp");
    fs::write(&temporary_path, serde_json::to_vec_pretty(progress)?)
        .with_context(|| format!("write projection progress {}", temporary_path.display()))?;
    replace_file_atomically(&temporary_path, &path)
        .with_context(|| format!("publish projection progress {}", path.display()))?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file_atomically(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file_atomically(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn update_projection_idle_rounds(
    result: anyhow::Result<usize>,
    consecutive_idle_rounds: &mut u32,
    failure_prefix: &str,
) {
    match result {
        Ok(0) => {
            *consecutive_idle_rounds = consecutive_idle_rounds.saturating_add(1);
        }
        Ok(_) => {
            *consecutive_idle_rounds = 0;
        }
        Err(err) => {
            *consecutive_idle_rounds = 0;
            eprintln!("{failure_prefix}: {err:#}");
        }
    }
}

fn business_os_projection_sleep_secs(
    active_interval_secs: u64,
    consecutive_idle_rounds: u32,
) -> u64 {
    projection_sleep_secs(
        active_interval_secs,
        BUSINESS_OS_PROJECTION_IDLE_SYNC_INTERVAL_SECS,
        BUSINESS_OS_PROJECTION_IDLE_BACKOFF_AFTER_TICKS,
        consecutive_idle_rounds,
    )
}

fn business_record_projection_sleep_secs(
    consecutive_idle_rounds: u32,
    consecutive_failure_rounds: u32,
) -> u64 {
    if consecutive_failure_rounds > 0 {
        let exponent = consecutive_failure_rounds.saturating_sub(1).min(10);
        return BUSINESS_RECORD_PROJECTION_ERROR_BACKOFF_BASE_SECS
            .saturating_mul(1u64 << exponent)
            .min(BUSINESS_RECORD_PROJECTION_ERROR_BACKOFF_MAX_SECS);
    }
    projection_sleep_secs(
        BUSINESS_RECORD_PROJECTION_SYNC_INTERVAL_SECS,
        BUSINESS_RECORD_PROJECTION_IDLE_SYNC_INTERVAL_SECS,
        BUSINESS_RECORD_PROJECTION_IDLE_BACKOFF_AFTER_TICKS,
        consecutive_idle_rounds,
    )
}

fn business_record_projection_loop_sleep_secs(
    consecutive_idle_rounds: u32,
    consecutive_failure_rounds: u32,
    slice_incomplete: bool,
) -> u64 {
    if slice_incomplete {
        BUSINESS_RECORD_PROJECTION_PARTIAL_SYNC_INTERVAL_SECS
    } else {
        business_record_projection_sleep_secs(consecutive_idle_rounds, consecutive_failure_rounds)
    }
}

fn projection_sleep_secs(
    active_interval_secs: u64,
    idle_interval_secs: u64,
    idle_backoff_after_ticks: u32,
    consecutive_idle_rounds: u32,
) -> u64 {
    if consecutive_idle_rounds >= idle_backoff_after_ticks {
        idle_interval_secs
    } else {
        active_interval_secs
    }
}

async fn consume_business_commands_loop(root: PathBuf, database: Arc<RxDatabase>) {
    // Per-command failure budget. A command that keeps failing to accept
    // (e.g. a corrupt document) used to abort the WHOLE round via `?`, get
    // re-sorted to the head on the next 1s tick, and starve every command
    // behind it — browser-issued commands then appeared to hang forever.
    let mut accept_failures: HashMap<String, u32> = HashMap::new();
    let mut last_source_stamp: Option<BusinessCommandsSourceStamp> = None;
    // Do not run the comparatively expensive invariant sweep before the first
    // command intake opportunity after peer startup.
    let mut consecutive_idle_rounds = 1u32;
    loop {
        let started = Instant::now();
        let result: anyhow::Result<usize> = async {
            if business_commands_source_change(&root, &mut last_source_stamp)
                .await?
                .is_some()
            {
                let consumed =
                    consume_pending_business_commands(&root, &database, &mut accept_failures)
                        .await?;
                refresh_business_commands_source_stamp(&root, &mut last_source_stamp).await?;
                return Ok(consumed);
            }

            Ok(0)
        }
        .await;
        record_native_peer_loop_result(&BUSINESS_COMMANDS_LOOP_METRICS, &result, started.elapsed());
        match result {
            Ok(0) => {
                consecutive_idle_rounds = consecutive_idle_rounds.saturating_add(1);
            }
            Ok(_) => {
                consecutive_idle_rounds = 0;
            }
            Err(err) => {
                consecutive_idle_rounds = 0;
                eprintln!("[business-os] native rxdb command consumer failed: {err:#}");
            }
        }
        wait_for_business_command_wake(&root, last_source_stamp.as_ref(), consecutive_idle_rounds)
            .await;
    }
}

async fn deliver_business_command_outbox_background_loop(root: PathBuf) {
    let mut idle_rounds = 0u32;
    loop {
        let result: anyhow::Result<usize> = async {
            if idle_rounds > 0 && idle_rounds % 60 == 0 {
                let reconcile_root = root.clone();
                tokio::task::spawn_blocking(move || {
                    crate::mission::channels::reconcile_business_command_invariants(
                        &reconcile_root,
                        true,
                    )
                })
                .await
                .context("join business command invariant reconciliation")??;
            }
            let outbox_root = root.clone();
            let outbox = tokio::task::spawn_blocking(move || {
                store::deliver_business_command_outbox(&outbox_root, 10)
            })
            .await
            .context("join business command outbox delivery")??;
            Ok(outbox
                .get("processed")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize)
        }
        .await;
        match result {
            Ok(0) => {
                idle_rounds = idle_rounds.saturating_add(1);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Ok(_) => {
                idle_rounds = 0;
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(err) => {
                idle_rounds = 0;
                eprintln!("[business-os] native business command outbox delivery failed: {err:#}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

fn business_command_poll_sleep_secs(consecutive_idle_rounds: u32) -> u64 {
    projection_sleep_secs(
        BUSINESS_COMMAND_ACTIVE_POLL_SECS,
        BUSINESS_COMMAND_IDLE_POLL_SECS,
        BUSINESS_COMMAND_IDLE_BACKOFF_AFTER_TICKS,
        consecutive_idle_rounds,
    )
}

async fn wait_for_business_command_wake(
    root: &Path,
    last_source_stamp: Option<&BusinessCommandsSourceStamp>,
    consecutive_idle_rounds: u32,
) {
    let sleep_for = Duration::from_secs(business_command_poll_sleep_secs(consecutive_idle_rounds));
    if consecutive_idle_rounds < BUSINESS_COMMAND_IDLE_BACKOFF_AFTER_TICKS {
        tokio::time::sleep(sleep_for).await;
        return;
    }
    let Some(table_name) = last_source_stamp
        .and_then(|stamp| stamp.table.table_name.as_deref())
        .filter(|name| !name.is_empty())
    else {
        tokio::time::sleep(sleep_for).await;
        return;
    };
    let database_path = store::rxdb_store_path(root);
    let seen_generation = rxdb::storage::sqlite::instance::table_change_generation_for_path(
        &database_path,
        table_name,
    )
    .unwrap_or(0);
    rxdb::storage::sqlite::instance::wait_for_table_change_for_path(
        &database_path,
        table_name,
        seen_generation,
        sleep_for,
    )
    .await;
}

/// How often a single command may fail `accept_pending_business_command`
/// before it is marked `failed` and dropped from the pending queue.
const BUSINESS_COMMAND_ACCEPT_RETRY_BUDGET: u32 = 5;

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

async fn browser_session_automation_with_database(
    root: PathBuf,
    database: &Arc<RxDatabase>,
    request: BrowserSessionAutomationRequest,
) -> anyhow::Result<Value> {
    let session_id = request.session_id.trim().to_string();
    anyhow::ensure!(
        !session_id.is_empty(),
        "session_id is required for persistent browser automation"
    );
    let source = request.source.trim().to_string();
    anyhow::ensure!(!source.is_empty(), "browser automation source is empty");
    let timeout_ms = request.timeout_ms.unwrap_or(30_000).clamp(1_000, 300_000);
    let command_created_at_ms = now_ms() as u64;
    let manager = browser_runtime_manager();
    let session = manager
        .ensure_session(root, request.dir, &session_id, 1920, 947, "ctox", false)
        .await?;
    let mut output = manager
        .request(
            &session,
            "automation",
            json!({
                "source": source,
                "timeoutMs": timeout_ms,
            }),
        )
        .await?;

    let session_doc = find_browser_document(database, "browser_sessions", &session_id).await?;
    let tab_id = session_doc
        .get("current_tab_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("browser_tab_{session_id}"));
    let nav = output.get("nav").cloned().unwrap_or(Value::Null);
    let page_meta = output.get("page").cloned().unwrap_or(Value::Null);
    let url = nav
        .get("url")
        .and_then(Value::as_str)
        .or_else(|| page_meta.get("url").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("about:blank")
        .to_string();
    let title = nav
        .get("title")
        .and_then(Value::as_str)
        .or_else(|| page_meta.get("title").and_then(Value::as_str))
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
    let next_seq = session_doc
        .get("last_frame_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + 1;
    let screenshot = manager.request(&session, "screenshot", json!({})).await?;
    let data = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("base64"))
        .and_then(Value::as_str)
        .context("browser runtime did not return screenshot data after automation")?
        .to_string();
    let mime_type = screenshot
        .get("screenshot")
        .and_then(|frame| frame.get("mimeType"))
        .and_then(Value::as_str)
        .unwrap_or("image/png")
        .to_string();
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
        session.viewport_w,
        session.viewport_h,
        size_bytes,
        &frame_hash,
    )
    .await?;
    upsert_browser_tab(
        database,
        &tab_id,
        &session_id,
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
        &session_id,
        &tab_id,
        "active",
        "active",
        &url,
        &title,
        session.viewport_w,
        session.viewport_h,
        Some(&frame_id),
        next_seq,
        "browser.automation",
        command_created_at_ms,
        output.get("error").and_then(Value::as_str),
    )
    .await?;
    if let Some(object) = output.as_object_mut() {
        object.insert("session_id".to_string(), Value::String(session_id));
        object.insert("tab_id".to_string(), Value::String(tab_id));
        object.insert("frame_id".to_string(), Value::String(frame_id));
        object.insert("frame_hash".to_string(), Value::String(frame_hash));
        object.insert("size_bytes".to_string(), Value::from(size_bytes));
        object.insert(
            "browser_stream".to_string(),
            Value::String("rxdb".to_string()),
        );
        object.insert("timeout_ms".to_string(), Value::from(timeout_ms));
    }
    Ok(output)
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
    accept_failures: &mut HashMap<String, u32>,
) -> anyhow::Result<usize> {
    let rows = pending_business_command_documents(root, 25)
        .await
        .context("load pending business_commands from RxDB SQLite")?;
    let pending_count = rows.len();
    for document in rows {
        COMMAND_PLANE_METRICS.record_attempt();
        // Isolate failures per command: one broken document must not stall
        // the entire queue (it would be re-sorted to the head every tick).
        let command_id = document
            .get("command_id")
            .or_else(|| document.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        match accept_pending_business_command(root, database, document.clone()).await {
            Ok(()) => {
                COMMAND_PLANE_METRICS.record_processed(&document);
                if !command_id.is_empty() {
                    accept_failures.remove(&command_id);
                    let resolve_root = root.to_path_buf();
                    let resolved_command_id = command_id.clone();
                    match tokio::task::spawn_blocking(move || {
                        store::resolve_business_command_intake_failures(
                            &resolve_root,
                            &resolved_command_id,
                        )
                    })
                    .await
                    {
                        Ok(Ok(_)) => {}
                        Ok(Err(error)) => eprintln!(
                            "[business-os] resolving intake failure history for `{command_id}` failed: {error:#}"
                        ),
                        Err(error) => eprintln!(
                            "[business-os] joining intake failure resolution for `{command_id}` failed: {error}"
                        ),
                    }
                }
            }
            Err(err) => {
                COMMAND_PLANE_METRICS
                    .errors_total
                    .fetch_add(1, Ordering::Relaxed);
                eprintln!(
                    "[business-os] accepting business command `{command_id}` failed: {err:#}"
                );
                if command_id.is_empty() {
                    continue;
                }
                let failure_root = root.to_path_buf();
                let failed_document = document.clone();
                let failure_message = format!("{err:#}");
                let persisted_failure = match tokio::task::spawn_blocking(move || {
                    store::record_business_command_intake_failure(
                        &failure_root,
                        &failed_document,
                        &failure_message,
                        BUSINESS_COMMAND_ACCEPT_RETRY_BUDGET,
                    )
                })
                .await
                {
                    Ok(Ok(value)) => value,
                    Ok(Err(persist_error)) => {
                        eprintln!(
                            "[business-os] persisting intake failure for `{command_id}` failed: {persist_error:#}"
                        );
                        let fallback_attempt = accept_failures
                            .get(&command_id)
                            .copied()
                            .unwrap_or_default()
                            .saturating_add(1);
                        json!({
                            "attempt": fallback_attempt,
                            "exhausted": false,
                            "canonical_exists": false,
                            "canonical_failure_created": false,
                        })
                    }
                    Err(join_error) => {
                        eprintln!(
                            "[business-os] joining intake failure persistence for `{command_id}` failed: {join_error}"
                        );
                        let fallback_attempt = accept_failures
                            .get(&command_id)
                            .copied()
                            .unwrap_or_default()
                            .saturating_add(1);
                        json!({
                            "attempt": fallback_attempt,
                            "exhausted": false,
                            "canonical_exists": false,
                            "canonical_failure_created": false,
                        })
                    }
                };
                let failures = persisted_failure
                    .get("attempt")
                    .and_then(Value::as_u64)
                    .unwrap_or(1) as u32;
                accept_failures.insert(command_id.clone(), failures);
                let exhausted = persisted_failure
                    .get("exhausted")
                    .and_then(Value::as_bool)
                    .unwrap_or(failures >= BUSINESS_COMMAND_ACCEPT_RETRY_BUDGET);
                if exhausted {
                    COMMAND_PLANE_METRICS
                        .exhausted_total
                        .fetch_add(1, Ordering::Relaxed);
                    accept_failures.remove(&command_id);
                    let canonical_failure_created = persisted_failure
                        .get("canonical_failure_created")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if canonical_failure_created {
                        let failed_patch = persisted_failure
                            .get("failure_document")
                            .cloned()
                            .unwrap_or_else(|| document.clone());
                        let commands = database
                            .collection("business_commands")
                            .context("business_commands collection is not registered")?;
                        if let Err(write_err) = incremental_upsert_document_with_repair(
                            &commands,
                            failed_patch,
                            "failed business_command",
                        )
                        .await
                        {
                            eprintln!(
                                "[business-os] marking command `{command_id}` failed did not stick: {write_err}"
                            );
                        }
                    } else if persisted_failure
                        .get("canonical_exists")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        // Acceptance may already be canonical while only its
                        // RxDB projection is failing. Never overwrite that
                        // aggregate with a projection-only terminal failure.
                        if let Err(write_err) = upsert_business_record_projection(
                            root.to_path_buf(),
                            database,
                            "business_commands",
                            command_id.clone(),
                        )
                        .await
                        {
                            eprintln!(
                                "[business-os] replaying canonical command `{command_id}` after projection failure did not stick: {write_err:#}"
                            );
                        }
                    }
                } else {
                    COMMAND_PLANE_METRICS
                        .retries_total
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    Ok(pending_count)
}

async fn pending_business_command_documents(
    root: &Path,
    limit: usize,
) -> anyhow::Result<Vec<Value>> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || pending_business_command_documents_sync(&root, limit))
        .await
        .context("join pending business_commands SQLite load")?
}

fn pending_business_command_documents_sync(
    root: &Path,
    limit: usize,
) -> anyhow::Result<Vec<Value>> {
    let path = store::rxdb_store_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "open Business OS RxDB store for pending commands {}",
            path.display()
        )
    })?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("configure pending business command busy_timeout")?;
    let Some(table) = latest_rxdb_collection_table(&conn, "business_commands")? else {
        return Ok(Vec::new());
    };
    let quoted = sqlite_quote_identifier(&table);
    let deleted_expr = if sqlite_table_has_column(&conn, &table, "deleted")? {
        "deleted"
    } else {
        "0"
    };
    let lwt_expr = if sqlite_table_has_column(&conn, &table, "lastWriteTime")? {
        "COALESCE(lastWriteTime, 0)"
    } else {
        "CAST(COALESCE(json_extract(data, '$._meta.lwt'), json_extract(data, '$.updated_at_ms'), 0) AS REAL)"
    };
    let oldest_limit = limit.saturating_add(1) / 2;
    let newest_limit = limit.saturating_sub(oldest_limit);
    let mut documents = Vec::new();
    let mut seen_ids = HashSet::new();
    for (direction, batch_limit) in [("ASC", oldest_limit), ("DESC", newest_limit)] {
        if batch_limit == 0 {
            continue;
        }
        let mut stmt = conn
            .prepare(&format!(
                "SELECT data
                 FROM {quoted}
                 WHERE {deleted_expr} = 0
                   AND json_extract(data, '$.status') IN ('pending_sync', 'waiting_dependencies')
                 ORDER BY {lwt_expr} {direction}
                 LIMIT ?1"
            ))
            .with_context(|| {
                format!("prepare pending business_commands {direction} scan in {table}")
            })?;
        let rows = stmt
            .query_map([batch_limit as i64], |row| row.get::<_, String>(0))
            .with_context(|| format!("query pending business_commands {direction} in {table}"))?;
        for row in rows {
            let raw = row.context("read pending business_command row")?;
            let document = serde_json::from_str::<Value>(&raw)
                .with_context(|| format!("parse pending business_command JSON in {table}"))?;
            let id = document
                .get("command_id")
                .or_else(|| document.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if id.is_empty() || seen_ids.insert(id) {
                documents.push(document);
            }
        }
    }
    Ok(documents)
}

async fn incremental_upsert_document_with_repair(
    collection: &Arc<RxCollection>,
    document: Value,
    label: &str,
) -> anyhow::Result<()> {
    match collection.incremental_upsert(document.clone()).await {
        Ok(_) => Ok(()),
        Err(err) if is_recoverable_projection_write_error(&err) => {
            let original_error = err.to_string();
            repair_projection_document_envelope_and_upsert(collection, document)
                .await
                .map_err(|fallback_err| {
                    anyhow::anyhow!(
                        "repair {label} after recoverable RxDB write error ({original_error}): {fallback_err}"
                    )
                })
        }
        Err(err) => Err(anyhow::anyhow!("{err}")),
    }
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
    // This document was replicated from a browser/device peer over WebRTC/RxDB:
    // its client_context (incl. actor) is attacker-controllable, so it is tagged
    // ReplicatedPeer and cannot authorize a privileged role without a verified
    // capability token (see store::rxdb_session_from_command).
    let accepted_result = tokio::task::spawn_blocking(move || {
        store::accept_rxdb_business_command_with_origin(
            &accept_root,
            document_for_store,
            store::CommandOrigin::ReplicatedPeer,
        )
    })
    .await;

    let mut accepted = match accepted_result {
        Ok(Ok(val)) => val,
        Ok(Err(err)) if is_transient_business_command_store_error(&err) => {
            return Err(err).context("transient native business command store contention");
        }
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
                let mut next = if document.is_object() {
                    document.clone()
                } else {
                    json!({ "id": command_id, "command_id": command_id })
                };
                if let Some(obj) = next.as_object_mut() {
                    let error_message = err.to_string();
                    obj.insert("status".to_string(), Value::String("failed".to_string()));
                    obj.insert("error".to_string(), Value::String(error_message.clone()));
                    if let Some(code) = typed_app_action_error_code(&error_message) {
                        obj.insert("error_code".to_string(), Value::String(code.to_owned()));
                    }
                    obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
                }
                incremental_upsert_document_with_repair(&commands, next, "failed business_command")
                    .await
                    .map_err(|err| {
                        anyhow::anyhow!("upsert failed business_command {command_id}: {err}")
                    })?;
            }
            return Ok(());
        }
        Err(err) => {
            return Err(err.into());
        }
    };

    if command_type == app_runtime::APP_ACTION_COMMAND_TYPE
        && accepted.get("already_accepted").and_then(Value::as_bool) != Some(true)
    {
        let snapshot = accepted
            .get("_app_action_snapshot")
            .cloned()
            .context("app_runtime_reconfiguring: admitted action has no immutable snapshot")?;
        let execution = app_runtime::execute(
            root.as_path(),
            database,
            &command_id_from_document(&document)?,
            &snapshot,
        )
        .await?;
        let mut result = execution.result;
        if let Some(object) = result.as_object_mut() {
            if let Some(code) = execution.error_code {
                object.insert("error_code".to_owned(), Value::String(code.to_owned()));
            }
            if let Some(message) = execution.error_message {
                object.insert("error".to_owned(), Value::String(message));
            }
        }
        accepted = store::finalize_runtime_app_action(
            root.as_path(),
            &document,
            execution.status,
            result,
        )?;
    }

    let command_id = accepted
        .get("command_id")
        .or_else(|| accepted.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .context("accepted command is missing command_id")?;

    if accepted
        .get("already_accepted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        upsert_business_record_projection(
            root.to_path_buf(),
            database,
            "business_commands",
            command_id.clone(),
        )
        .await?;
        if let Some(task_id) = accepted
            .get("task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            upsert_business_record_projection(
                root.to_path_buf(),
                database,
                "ctox_queue_tasks",
                task_id.to_string(),
            )
            .await?;
        }
        if let Some(chat_id) = accepted
            .get("chat_id")
            .or_else(|| {
                accepted
                    .get("result")
                    .and_then(|result| result.get("chat_id"))
            })
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            upsert_business_record_projection(
                root.to_path_buf(),
                database,
                "business_chats",
                chat_id.to_string(),
            )
            .await?;
        }
        return Ok(());
    }

    let commands = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let accepted_status = accepted
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("accepted");
    let existing_status = document.get("status").and_then(Value::as_str).unwrap_or("");
    if accepted_status == "already_accepted"
        && !existing_status.is_empty()
        && existing_status != "pending_sync"
    {
        return Ok(());
    }
    let mut next = if document.is_object() {
        document.clone()
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
        for key in ["outbound_text", "response", "answer", "summary"] {
            if let Some(value) = accepted.get(key) {
                obj.insert(key.to_string(), value.clone());
            }
        }
        for key in ["report_id", "report_status"] {
            if let Some(value) = accepted.get(key) {
                obj.insert(key.to_string(), value.clone());
            }
        }
        obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    }
    enrich_native_command_lifecycle(&mut next, &accepted)?;
    if next.get("contract_version").and_then(Value::as_u64) == Some(2) {
        let persist_root = root.clone();
        let persisted = next.clone();
        tokio::task::spawn_blocking(move || {
            store::persist_business_command_lifecycle_projection(&persist_root, &persisted)
        })
        .await
        .context("join native command lifecycle projection persistence")??;
    }
    incremental_upsert_document_with_repair(&commands, next, "accepted business_command")
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
    if command_type == "ctox.business_os.branding.update" {
        sync_workspace_branding_with_database(&root, database).await?;
    }
    if command_type.starts_with("ctox.ticket.") {
        sync_ticket_state_with_database(&root, database).await?;
    }
    if command_type.starts_with("support.") {
        project_support_command_result(root.clone(), database, &accepted).await?;
    }
    if command_type.starts_with("ctox.appsec.") {
        project_appsec_command_result(root.clone(), database, &accepted).await?;
    }
    if command_type.starts_with("threads.") {
        project_threads_command_result(root.clone(), database, &accepted).await?;
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
                &root,
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
            | "ctox.module.repair_lifecycle_projection"
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

fn command_id_from_document(document: &Value) -> anyhow::Result<String> {
    document
        .get("command_id")
        .or_else(|| document.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .context("business command id is required")
}

fn typed_app_action_error_code(message: &str) -> Option<&'static str> {
    [
        "app_action_not_registered",
        "app_action_input_invalid",
        "app_action_permission_denied",
        "app_action_definition_changed",
        "app_runtime_reconfiguring",
        "app_action_compensation_failed",
    ]
    .into_iter()
    .find(|code| message.contains(code))
}

fn enrich_native_command_lifecycle(document: &mut Value, accepted: &Value) -> anyhow::Result<()> {
    if document.get("contract_version").and_then(Value::as_u64) != Some(2) {
        return Ok(());
    }
    let object = document
        .as_object_mut()
        .context("v2 business command lifecycle document must be an object")?;
    let status = accepted
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("accepted");
    let execution_mode = accepted
        .get("execution_mode")
        .and_then(Value::as_str)
        .unwrap_or("queue")
        .to_string();
    let execution_task_id = accepted
        .get("execution_task_id")
        .or_else(|| {
            (execution_mode == "queue")
                .then_some(accepted.get("task_id"))
                .flatten()
        })
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let terminal_status = match status {
        "completed" => "completed",
        "failed" => "failed",
        "cancelled" => "cancelled",
        _ => "none",
    };
    let execution_phase = if terminal_status != "none" {
        "terminal"
    } else if execution_mode == "queue" && !execution_task_id.is_empty() {
        "queued"
    } else {
        "accepted"
    };
    let previous_version = object
        .get("projection_version")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let target_task_id = accepted
        .get("target_task_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let target_record_id = accepted
        .get("target_record_id")
        .and_then(Value::as_str)
        .or_else(|| object.get("record_id").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();

    object.insert("execution_mode".to_string(), Value::String(execution_mode));
    object.insert(
        "execution_task_id".to_string(),
        Value::String(execution_task_id),
    );
    object.insert("target_task_id".to_string(), Value::String(target_task_id));
    object.insert(
        "target_record_id".to_string(),
        Value::String(target_record_id),
    );
    object.insert(
        "replication_phase".to_string(),
        Value::String("native_observed".to_string()),
    );
    object.insert(
        "execution_phase".to_string(),
        Value::String(execution_phase.to_string()),
    );
    object.insert(
        "terminal_status".to_string(),
        Value::String(terminal_status.to_string()),
    );
    object.insert(
        "projection_version".to_string(),
        Value::from(previous_version.saturating_add(1)),
    );
    object
        .entry("attempt".to_string())
        .or_insert_with(|| Value::from(0_u64));
    if terminal_status == "failed" {
        object.insert(
            "error_code".to_string(),
            Value::String("command_terminal_failure".to_string()),
        );
        object.insert("retryable".to_string(), Value::Bool(false));
    }
    super::command_lifecycle::validate_document(document).map_err(|error| anyhow::anyhow!(error))
}

async fn project_support_command_result(
    root: PathBuf,
    database: &Arc<RxDatabase>,
    accepted: &Value,
) -> anyhow::Result<()> {
    let projections = accepted
        .get("result")
        .and_then(|result| result.get("projections"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for projection in projections {
        let Some(collection) = projection
            .get("collection")
            .and_then(Value::as_str)
            .and_then(support_projection_collection)
        else {
            continue;
        };
        let Some(record_id) = projection
            .get("record_id")
            .or_else(|| projection.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };
        upsert_business_record_projection(root.clone(), database, collection, record_id).await?;
    }
    Ok(())
}

fn support_projection_collection(collection: &str) -> Option<&'static str> {
    match collection {
        "support_inboxes" => Some("support_inboxes"),
        "support_conversations" => Some("support_conversations"),
        "support_thread_links" => Some("support_thread_links"),
        "support_identity_links" => Some("support_identity_links"),
        "support_notes" => Some("support_notes"),
        "support_conversation_events" => Some("support_conversation_events"),
        "support_labels" => Some("support_labels"),
        "support_label_assignments" => Some("support_label_assignments"),
        "support_views" => Some("support_views"),
        "support_view_filters" => Some("support_view_filters"),
        "support_assignment_policies" => Some("support_assignment_policies"),
        "support_assignment_events" => Some("support_assignment_events"),
        "support_macros" => Some("support_macros"),
        "support_automation_rules" => Some("support_automation_rules"),
        "support_sla_policies" => Some("support_sla_policies"),
        "support_applied_slas" => Some("support_applied_slas"),
        "support_sla_events" => Some("support_sla_events"),
        "support_agent_requests" => Some("support_agent_requests"),
        "support_agent_suggestions" => Some("support_agent_suggestions"),
        "support_reporting_events" => Some("support_reporting_events"),
        "support_reporting_rollups" => Some("support_reporting_rollups"),
        _ => None,
    }
}

async fn project_threads_command_result(
    root: PathBuf,
    database: &Arc<RxDatabase>,
    accepted: &Value,
) -> anyhow::Result<()> {
    let projections = accepted
        .get("result")
        .and_then(|result| result.get("projections"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for projection in projections {
        let Some(collection) = projection
            .get("collection")
            .and_then(Value::as_str)
            .and_then(threads_projection_collection)
        else {
            continue;
        };
        let Some(record_id) = projection
            .get("record_id")
            .or_else(|| projection.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };
        upsert_business_record_projection(root.clone(), database, collection, record_id).await?;
    }
    Ok(())
}

fn is_transient_business_command_store_error(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}").to_ascii_lowercase();
    [
        "database is locked",
        "database table is locked",
        "sqlite_busy",
        "cannot promote read transaction",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

async fn project_appsec_command_result(
    root: PathBuf,
    database: &Arc<RxDatabase>,
    accepted: &Value,
) -> anyhow::Result<()> {
    let projections = accepted
        .pointer("/result/ctox_durable_projection/business_os_projection/projected_records")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for projection in projections {
        let Some(collection) = projection
            .get("collection")
            .and_then(Value::as_str)
            .and_then(appsec_projection_collection)
        else {
            continue;
        };
        let Some(record_id) = projection
            .get("record_id")
            .or_else(|| projection.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };
        upsert_business_record_projection(root.clone(), database, collection, record_id).await?;
    }
    Ok(())
}

fn appsec_projection_collection(collection: &str) -> Option<&'static str> {
    match collection {
        "appsec_assessments" => Some("appsec_assessments"),
        "appsec_runs" => Some("appsec_runs"),
        "appsec_artifacts" => Some("appsec_artifacts"),
        "appsec_findings" => Some("appsec_findings"),
        "appsec_investigations" => Some("appsec_investigations"),
        "appsec_coverage" => Some("appsec_coverage"),
        "appsec_pipeline_stages" => Some("appsec_pipeline_stages"),
        "appsec_scanner_inventory" => Some("appsec_scanner_inventory"),
        "appsec_approvals" => Some("appsec_approvals"),
        _ => None,
    }
}

fn threads_projection_collection(collection: &str) -> Option<&'static str> {
    match collection {
        "user_threads" => Some("user_threads"),
        "user_thread_messages" => Some("user_thread_messages"),
        "user_thread_links" => Some("user_thread_links"),
        "user_notifications" => Some("user_notifications"),
        "ctox_task_approval_requests" => Some("ctox_task_approval_requests"),
        _ => None,
    }
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
    let capability_token = document
        .get("client_context")
        .and_then(|context| context.get("capability_token"))
        .and_then(Value::as_str)
        .context("browser command capability token is required")?;
    let (profile_owner, _) = store::verify_capability_actor(root, capability_token)
        .context("browser command capability token is invalid")?;
    let private_profile = payload
        .get("profile_mode")
        .and_then(Value::as_str)
        .is_some_and(|mode| mode == "private");

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
            &profile_owner,
            private_profile,
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
    let mut consecutive_idle_rounds = 0u32;
    loop {
        if browser_runtime_manager().active_session_ids().is_empty() {
            consecutive_idle_rounds = 0;
            tokio::time::sleep(Duration::from_secs(
                BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS,
            ))
            .await;
            continue;
        }
        let did_work = {
            let _guard = database_write_lock.lock().await;
            let _browser_guard = BROWSER_RUNTIME_COMMAND_LOCK.lock().await;
            let started = Instant::now();
            let result = run_browser_runtime_maintenance(&database).await;
            record_native_peer_loop_result(
                &BROWSER_RUNTIME_LOOP_METRICS,
                &result,
                started.elapsed(),
            );
            match result {
                Ok(rows) => rows > 0,
                Err(err) => {
                    eprintln!("[business-os] browser runtime maintenance failed: {err:#}");
                    true
                }
            }
        };
        if did_work {
            consecutive_idle_rounds = 0;
        } else {
            consecutive_idle_rounds = consecutive_idle_rounds.saturating_add(1);
        }
        wait_for_browser_runtime_maintenance_wake(&root, consecutive_idle_rounds).await;
    }
}

fn browser_runtime_maintenance_sleep(consecutive_idle_rounds: u32) -> Duration {
    if consecutive_idle_rounds >= BROWSER_RUNTIME_IDLE_BACKOFF_AFTER_TICKS {
        Duration::from_secs(BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS)
    } else {
        Duration::from_millis(BROWSER_RUNTIME_ACTIVE_MAINTENANCE_INTERVAL_MS)
    }
}

async fn wait_for_browser_runtime_maintenance_wake(root: &Path, consecutive_idle_rounds: u32) {
    let sleep_for = browser_runtime_maintenance_sleep(consecutive_idle_rounds);
    if consecutive_idle_rounds < BROWSER_RUNTIME_IDLE_BACKOFF_AFTER_TICKS {
        tokio::time::sleep(sleep_for).await;
        return;
    }
    let table_name = rxdb_collection_version_table_name("browser_input_events", 0);
    let database_path = store::rxdb_store_path(root);
    let seen_generation = rxdb::storage::sqlite::instance::table_change_generation_for_path(
        &database_path,
        &table_name,
    )
    .unwrap_or(0);
    rxdb::storage::sqlite::instance::wait_for_table_change_for_path(
        &database_path,
        &table_name,
        seen_generation,
        sleep_for,
    )
    .await;
}

async fn run_browser_runtime_maintenance(database: &Arc<RxDatabase>) -> anyhow::Result<usize> {
    let manager = browser_runtime_manager();
    let mut rows_touched = 0usize;
    for session_id in manager.active_session_ids() {
        match drain_browser_session_inputs(database, &session_id).await {
            Ok(session_rows) => {
                rows_touched = rows_touched.saturating_add(session_rows);
            }
            Err(err) => {
                eprintln!("[business-os] browser input drain failed for {session_id}: {err:#}");
            }
        }
    }
    rows_touched = rows_touched.saturating_add(gc_expired_browser_frames(database).await?);
    rows_touched = rows_touched.saturating_add(gc_consumed_browser_input_events(database).await?);
    Ok(rows_touched)
}

/// Replay all pending `browser_input_events` for one session against its live
/// page, mark them consumed/failed, and refresh the frame if anything applied.
async fn drain_browser_session_inputs(
    database: &Arc<RxDatabase>,
    session_id: &str,
) -> anyhow::Result<usize> {
    let manager = browser_runtime_manager();
    let Some(session) = manager.get(session_id) else {
        return Ok(0);
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
        return Ok(0);
    };
    if rows.is_empty() {
        return Ok(0);
    }
    let touched_rows = rows.len();

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
            return Ok(touched_rows);
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
    Ok(touched_rows)
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
    let nav = browser_capture_navigation(&screenshot, nav_hint);

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

fn browser_capture_navigation(screenshot: &Value, nav_hint: Option<&Value>) -> Value {
    screenshot
        .get("nav")
        .cloned()
        .filter(|value| !value.is_null())
        .or_else(|| nav_hint.cloned().filter(|value| !value.is_null()))
        .unwrap_or(Value::Null)
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
        .count(Some(MangoQuery {
            selector: Some(json!({
                "session_id": { "$eq": session_id },
                "status": { "$eq": "pending" }
            })),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("count pending browser_input_events: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec count pending browser_input_events: {err}"))?;
    let pending_count = pending.as_u64().unwrap_or(0);

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
async fn gc_expired_browser_frames(database: &Arc<RxDatabase>) -> anyhow::Result<usize> {
    let now = now_ms() as u64;
    let frames = database
        .collection("browser_frames")
        .context("browser_frames collection is not registered")?;
    let expired = frames
        .find(Some(MangoQuery {
            selector: Some(json!({ "expires_at_ms": { "$lt": now } })),
            limit: Some(BROWSER_FRAME_GC_LIMIT),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query expired browser_frames: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec expired browser_frames query: {err}"))?;
    let Some(rows) = expired.as_array() else {
        return Ok(0);
    };
    let ids: Vec<String> = rows
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    let redacted = rows
        .iter()
        .filter_map(redacted_expired_browser_frame)
        .collect::<Vec<_>>();
    if !redacted.is_empty() {
        frames
            .bulk_upsert(redacted)
            .await
            .map_err(|err| anyhow::anyhow!("redact expired browser_frames: {err}"))?;
    }
    frames
        .bulk_remove_by_ids(ids)
        .await
        .map_err(|err| anyhow::anyhow!("remove expired browser_frames: {err}"))?;
    Ok(rows.len())
}

fn redacted_expired_browser_frame(row: &Value) -> Option<Value> {
    let mut next = row.clone();
    let obj = next.as_object_mut()?;
    obj.remove("_rev");
    obj.remove("_meta");
    obj.insert("data".to_string(), Value::String(String::new()));
    obj.insert(
        "encoding".to_string(),
        Value::String("redacted".to_string()),
    );
    obj.insert("size_bytes".to_string(), Value::from(0));
    obj.insert("frame_hash".to_string(), Value::String(String::new()));
    obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    Some(next)
}

/// Drop consumed/failed input events after a retention window. Pending events
/// stay until drained, and the bounded query keeps active browser maintenance
/// from repeatedly touching old input-event history while idle.
async fn gc_consumed_browser_input_events(database: &Arc<RxDatabase>) -> anyhow::Result<usize> {
    let cutoff = (now_ms() as u64).saturating_sub(BROWSER_INPUT_EVENT_RETENTION_SECS * 1_000);
    let events = database
        .collection("browser_input_events")
        .context("browser_input_events collection is not registered")?;
    let mut removed = 0usize;
    for status in ["consumed", "failed"] {
        let stale = events
            .find(Some(MangoQuery {
                selector: Some(json!({
                    "status": { "$eq": status },
                    "created_at_ms": { "$lt": cutoff }
                })),
                sort: Some(vec![[("created_at_ms".to_string(), "asc".to_string())]
                    .into_iter()
                    .collect()]),
                limit: Some(BROWSER_INPUT_EVENT_GC_LIMIT),
                ..Default::default()
            }))
            .map_err(|err| anyhow::anyhow!("query stale browser_input_events: {err}"))?
            .exec(false)
            .await
            .map_err(|err| anyhow::anyhow!("exec stale browser_input_events query: {err}"))?;
        let Some(rows) = stale.as_array() else {
            continue;
        };
        let ids = rows
            .iter()
            .filter_map(|row| row.get("id").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            continue;
        }
        events
            .bulk_remove_by_ids(ids)
            .await
            .map_err(|err| anyhow::anyhow!("remove stale browser_input_events: {err}"))?;
        removed = removed.saturating_add(rows.len());
    }
    Ok(removed)
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
            selector: Some(json!({ "status": { "$eq": "active" } })),
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
    let updates_command_watermark = browser_updates_command_watermark(command_type);
    if updates_command_watermark && existing_command_created_at_ms > command_created_at_ms {
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
    let mut payload = existing
        .get("payload")
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    payload["browser_stream"] = Value::String("rxdb".to_string());
    if updates_command_watermark {
        payload["last_command_type"] = Value::String(command_type.to_string());
        payload["last_command_created_at_ms"] = Value::from(command_created_at_ms);
    }
    payload["runtime"] = Value::String("ctox-web-stack".to_string());
    payload["updated_by"] = Value::String("native-rxdb-peer".to_string());
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

fn browser_updates_command_watermark(command_type: &str) -> bool {
    command_type != "browser.input"
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
    let mut count = 0usize;
    for mut document in documents {
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object.insert("is_deleted".to_string(), Value::Bool(false));
        }
        if incremental_upsert_projection_if_changed(&users, document, "business user").await? {
            count += 1;
        }
    }
    Ok(count)
}

async fn sync_business_users_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    last_projection_stamp: &mut Option<store::BusinessUsersProjectionStamp>,
) -> anyhow::Result<usize> {
    let projection_stamp = business_users_projection_stamp(root).await?;
    if last_projection_stamp.as_ref() == Some(&projection_stamp) {
        return Ok(0);
    }

    let synced = sync_business_users_with_database(root, database).await?;
    *last_projection_stamp = Some(business_users_projection_stamp(root).await?);
    Ok(synced)
}

async fn business_users_projection_stamp(
    root: &Path,
) -> anyhow::Result<store::BusinessUsersProjectionStamp> {
    let root_for_stamp = root.to_path_buf();
    tokio::task::spawn_blocking(move || store::business_users_projection_stamp(&root_for_stamp))
        .await
        .context("join native business users projection stamp")?
}

async fn sync_runtime_settings_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
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
    let changed =
        incremental_upsert_projection_if_changed(&runtime_settings, document, "runtime settings")
            .await?;
    Ok(usize::from(changed))
}

async fn sync_runtime_settings_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    last_projection_stamp: &mut Option<store::RuntimeSettingsProjectionStamp>,
) -> anyhow::Result<usize> {
    let projection_stamp = runtime_settings_projection_stamp(root).await?;
    if last_projection_stamp.as_ref() == Some(&projection_stamp) {
        return Ok(0);
    }

    let synced = sync_runtime_settings_with_database(root, database).await?;
    *last_projection_stamp = Some(projection_stamp);
    Ok(synced)
}

async fn runtime_settings_projection_stamp(
    root: &Path,
) -> anyhow::Result<store::RuntimeSettingsProjectionStamp> {
    let root_for_stamp = root.to_path_buf();
    tokio::task::spawn_blocking(move || store::runtime_settings_projection_stamp(&root_for_stamp))
        .await
        .context("join native runtime settings projection stamp")
}

async fn sync_workspace_branding_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    let root = root.to_path_buf();
    let mut document =
        tokio::task::spawn_blocking(move || store::workspace_branding_for_rxdb(&root))
            .await
            .context("join native workspace branding projection load")??;
    if let Some(object) = document.as_object_mut() {
        object.remove("_rev");
        object.remove("_meta");
        object.insert("_deleted".to_string(), Value::Bool(false));
        object.insert("is_deleted".to_string(), Value::Bool(false));
    }

    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    let workspace_branding = database
        .collection("business_workspace_branding")
        .context("business_workspace_branding collection is not registered")?;
    let changed = incremental_upsert_projection_if_changed(
        &workspace_branding,
        document,
        "workspace branding",
    )
    .await?;
    Ok(usize::from(changed))
}

async fn sync_workspace_branding_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    last_projection_stamp: &mut Option<store::WorkspaceBrandingProjectionStamp>,
) -> anyhow::Result<usize> {
    let projection_stamp = workspace_branding_projection_stamp(root).await?;
    if last_projection_stamp.as_ref() == Some(&projection_stamp) {
        return Ok(0);
    }

    let synced = sync_workspace_branding_with_database(root, database).await?;
    *last_projection_stamp = Some(projection_stamp);
    Ok(synced)
}

async fn workspace_branding_projection_stamp(
    root: &Path,
) -> anyhow::Result<store::WorkspaceBrandingProjectionStamp> {
    let root_for_stamp = root.to_path_buf();
    tokio::task::spawn_blocking(move || store::workspace_branding_projection_stamp(&root_for_stamp))
        .await
        .context("join native workspace branding projection stamp")
}

async fn sync_module_catalog_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
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
    let changed =
        incremental_upsert_projection_if_changed(&module_catalog, document, "module catalog")
            .await?;
    Ok(usize::from(changed))
}

async fn sync_module_catalog_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    last_projection_stamp: &mut Option<store::ModuleCatalogProjectionStamp>,
) -> anyhow::Result<usize> {
    let projection_stamp = module_catalog_projection_stamp(root).await?;
    if last_projection_stamp.as_ref() == Some(&projection_stamp) {
        return Ok(0);
    }

    let synced = sync_module_catalog_with_database(root, database).await?;
    *last_projection_stamp = Some(module_catalog_projection_stamp(root).await?);
    Ok(synced)
}

async fn module_catalog_projection_stamp(
    root: &Path,
) -> anyhow::Result<store::ModuleCatalogProjectionStamp> {
    let root_for_stamp = root.to_path_buf();
    tokio::task::spawn_blocking(move || store::module_catalog_projection_stamp(&root_for_stamp))
        .await
        .context("join native module catalog projection stamp")?
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
            if incremental_upsert_projection_if_changed(
                &collection,
                document,
                &format!("{collection_name} ticket"),
            )
            .await?
            {
                count += 1;
            }
        }
    }
    Ok(count)
}

async fn sync_ticket_state_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    last_source_stamp: &mut Option<tickets::TicketStoreChangeStamp>,
) -> anyhow::Result<usize> {
    let source_stamp = ticket_state_source_stamp(root).await?;
    if last_source_stamp.as_ref() == Some(&source_stamp) {
        return Ok(0);
    }

    let synced = sync_ticket_state_with_database(root, database).await?;
    *last_source_stamp = Some(source_stamp);
    Ok(synced)
}

async fn ticket_state_source_stamp(root: &Path) -> anyhow::Result<tickets::TicketStoreChangeStamp> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || tickets::ticket_store_change_stamp(&root))
        .await
        .context("join native ticket state source stamp")
}

/// Project the record-shape knowledge catalog (`knowledge_data_tables`) into
/// the `knowledge_tables` RxDB collection, embedding the parquet rows directly
/// in each doc's payload.
///
/// This is the SINGLE native writer of the `knowledge_tables` collection.
/// `knowledge_tables` is therefore excluded from the generic business-record
/// projection in [`business_record_projection_collections`] so the two paths do
/// not fight over the same docs.
///
/// Business OS Web Research / Knowledge modules read rows exclusively from the
/// synced doc payload over RxDB/WebRTC — there is no HTTP data path — so the
/// rows must ride inside the doc, which is exactly what
/// [`crate::knowledge::knowledge_tables_rxdb_documents`] produces (with the
/// parquet path re-resolved to the live state dir, not the possibly-stale path
/// persisted in the catalog).
async fn sync_knowledge_tables_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    let root_owned = root.to_path_buf();
    let documents = tokio::task::spawn_blocking(move || {
        crate::knowledge::knowledge_tables_rxdb_documents(&root_owned)
    })
    .await
    .context("join native knowledge tables projection load")??;

    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    let collection = database
        .collection("knowledge_tables")
        .context("knowledge_tables collection is not registered")?;
    let mut count = 0usize;
    let mut current_ids = HashSet::new();
    for mut document in documents {
        if let Some(id) = document
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            current_ids.insert(id);
        }
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object.insert("_deleted".to_string(), Value::Bool(false));
            object.insert("is_deleted".to_string(), Value::Bool(false));
        }
        if incremental_upsert_projection_if_changed(&collection, document, "knowledge table")
            .await?
        {
            count += 1;
        }
    }
    let existing = collection
        .find(Some(MangoQuery {
            limit: Some(10_000),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query stale knowledge_tables projections: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec stale knowledge_tables projection query: {err}"))?;
    for mut stale in existing.as_array().cloned().unwrap_or_default() {
        let Some(id) = stale.get("id").and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        if current_ids.contains(&id) {
            continue;
        }
        if let Some(object) = stale.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
        }
        upsert_business_record_projection_tombstone(&collection, stale)
            .await
            .map_err(|err| anyhow::anyhow!("tombstone stale knowledge_tables projection: {err}"))?;
        count += 1;
    }
    Ok(count)
}

async fn sync_knowledge_tables_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    last_source_stamp: &mut Option<crate::knowledge::KnowledgeTablesProjectionSourceStamp>,
) -> anyhow::Result<usize> {
    let source_stamp = knowledge_tables_source_stamp(root).await?;
    if last_source_stamp.as_ref() == Some(&source_stamp) {
        return Ok(0);
    }

    let synced = sync_knowledge_tables_with_database(root, database).await?;
    *last_source_stamp = Some(source_stamp);
    Ok(synced)
}

async fn knowledge_tables_source_stamp(
    root: &Path,
) -> anyhow::Result<crate::knowledge::KnowledgeTablesProjectionSourceStamp> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        crate::knowledge::knowledge_tables_projection_source_stamp(&root)
    })
    .await
    .context("join native knowledge tables source stamp")?
}

async fn sync_knowledge_catalog_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    let root = root.to_path_buf();
    let payload =
        tokio::task::spawn_blocking(move || super::server::knowledge_index_payload(&root))
            .await
            .context("join native procedural knowledge projection load")??;
    let item_documents = payload
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|document| document.get("kind").and_then(Value::as_str) != Some("dataframe"))
        .collect::<Vec<_>>();
    let runbook_documents = payload
        .get("runbooks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    let mut count = 0usize;
    count += sync_knowledge_catalog_collection(database, "knowledge_items", item_documents).await?;
    count += sync_knowledge_catalog_collection(database, "knowledge_runbooks", runbook_documents)
        .await?;
    Ok(count)
}

async fn sync_knowledge_catalog_collection(
    database: &Arc<RxDatabase>,
    collection_name: &str,
    documents: Vec<Value>,
) -> anyhow::Result<usize> {
    let collection = database
        .collection(collection_name)
        .with_context(|| format!("{collection_name} collection is not registered"))?;
    let mut count = 0usize;
    let mut current_ids = HashSet::new();
    for mut document in documents {
        let Some(id) = document
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        current_ids.insert(id);
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object.insert("_deleted".to_string(), Value::Bool(false));
            object.insert("is_deleted".to_string(), Value::Bool(false));
            let updated_at_ms = object
                .get("updated_at")
                .and_then(Value::as_str)
                .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                .map(|timestamp| timestamp.timestamp_millis())
                .unwrap_or(0);
            object.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
        }
        if incremental_upsert_projection_if_changed(&collection, document, "procedural knowledge")
            .await?
        {
            count += 1;
        }
    }

    let existing = collection
        .find(Some(MangoQuery {
            limit: Some(100_000),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query stale {collection_name} projections: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec stale {collection_name} projection query: {err}"))?;
    for mut stale in existing.as_array().cloned().unwrap_or_default() {
        let Some(id) = stale.get("id").and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        let managed_id = match collection_name {
            "knowledge_items" => {
                id.starts_with("skill:")
                    || id.starts_with("skillbook:")
                    || id.starts_with("runbook:")
                    || id.starts_with("resource:")
            }
            "knowledge_runbooks" => id.starts_with("runbook:"),
            _ => false,
        };
        if !managed_id || current_ids.contains(&id) {
            continue;
        }
        if let Some(object) = stale.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
        }
        upsert_business_record_projection_tombstone(&collection, stale)
            .await
            .map_err(|err| {
                anyhow::anyhow!("tombstone stale {collection_name} projection: {err}")
            })?;
        count += 1;
    }
    Ok(count)
}

async fn sync_business_record_projections_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    database_write_lock: &Arc<AsyncMutex<()>>,
    since_by_collection: &mut HashMap<String, i64>,
    queue_chat_repair_stamp: &mut Option<QueueChatRepairProjectionStamp>,
    last_source_stamp: &mut Option<BusinessRecordProjectionSourceStamp>,
) -> anyhow::Result<usize> {
    let source_stamp = business_record_projection_source_stamp(root).await?;
    if last_source_stamp.as_ref() == Some(&source_stamp) {
        return Ok(0);
    }

    let synced = sync_business_record_projections_with_database(
        root,
        database,
        database_write_lock,
        since_by_collection,
        queue_chat_repair_stamp,
    )
    .await?;
    *last_source_stamp = Some(source_stamp);
    Ok(synced)
}

async fn business_record_projection_source_stamp(
    root: &Path,
) -> anyhow::Result<BusinessRecordProjectionSourceStamp> {
    let queue_stamp_root = root.to_path_buf();
    let store_stamp_root = root.to_path_buf();
    let knowledge_stamp_root = root.to_path_buf();
    let collections = business_record_projection_collections();
    let queue_chat_repair = queue_chat_repair_projection_stamp_async(&queue_stamp_root).await?;
    let (records, communication, knowledge) = tokio::task::spawn_blocking(move || {
        Ok::<_, anyhow::Error>((
            store::business_records_projection_stamp(&store_stamp_root, &collections)?,
            channels::communication_intake_source_stamp(&store_stamp_root)?,
            knowledge_catalog_projection_stamp(&knowledge_stamp_root)?,
        ))
    })
    .await
    .context("join native business record projection source stamp")??;
    Ok(BusinessRecordProjectionSourceStamp {
        records,
        communication,
        queue_chat_repair,
        knowledge,
    })
}

fn knowledge_catalog_projection_stamp(
    root: &Path,
) -> anyhow::Result<KnowledgeCatalogProjectionStamp> {
    let database_path = root.join("runtime").join("ctox.sqlite3");
    if !database_path.is_file() {
        return Ok(KnowledgeCatalogProjectionStamp::default());
    }
    let conn = Connection::open_with_flags(
        database_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    let mut stamp = KnowledgeCatalogProjectionStamp::default();
    for (table, count, updated_at) in [
        (
            "knowledge_main_skills",
            &mut stamp.main_skill_count,
            &mut stamp.main_skill_updated_at,
        ),
        (
            "knowledge_skillbooks",
            &mut stamp.skillbook_count,
            &mut stamp.skillbook_updated_at,
        ),
        (
            "knowledge_runbooks",
            &mut stamp.runbook_count,
            &mut stamp.runbook_updated_at,
        ),
        (
            "knowledge_resources",
            &mut stamp.resource_count,
            &mut stamp.resource_updated_at,
        ),
    ] {
        let exists = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get::<_, i64>(0),
        )? != 0;
        if !exists {
            continue;
        }
        let sql = format!("SELECT COUNT(*), COALESCE(MAX(updated_at), '') FROM {table}");
        (*count, *updated_at) = conn.query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?)))?;
    }
    Ok(stamp)
}

fn threads_app_relevance_cursor_key(collection: &str) -> String {
    format!("{THREADS_APP_RELEVANCE_SINCE_KEY_PREFIX}{collection}")
}

async fn sync_business_record_projections_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
    database_write_lock: &Arc<AsyncMutex<()>>,
    since_by_collection: &mut HashMap<String, i64>,
    queue_chat_repair_stamp: &mut Option<QueueChatRepairProjectionStamp>,
) -> anyhow::Result<usize> {
    let mut after_record_id_by_collection = HashMap::<String, String>::new();
    let mut next_collection_index = 0usize;
    let mut count = 0usize;
    loop {
        let (synced, caught_up) = sync_business_record_projections_slice_with_database(
            root,
            database,
            database_write_lock,
            since_by_collection,
            &mut after_record_id_by_collection,
            &mut next_collection_index,
            queue_chat_repair_stamp,
            None,
        )
        .await?;
        count = count.saturating_add(synced);
        if caught_up {
            return Ok(count);
        }
    }
}

async fn sync_business_record_projections_slice_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
    database_write_lock: &Arc<AsyncMutex<()>>,
    since_by_collection: &mut HashMap<String, i64>,
    after_record_id_by_collection: &mut HashMap<String, String>,
    next_collection_index: &mut usize,
    queue_chat_repair_stamp: &mut Option<QueueChatRepairProjectionStamp>,
    document_budget: Option<usize>,
) -> anyhow::Result<(usize, bool)> {
    let mut count = sync_knowledge_catalog_with_database(root, database).await?;
    let support_intake_root = root.to_path_buf();
    let support_intake_since_ms = *since_by_collection
        .get(SUPPORT_COMMUNICATION_INTAKE_SINCE_KEY)
        .unwrap_or(&0);
    let support_intake_count = tokio::task::spawn_blocking(move || {
        super::support::project_communication_intake(
            &support_intake_root,
            support_intake_since_ms,
            BUSINESS_RECORD_PROJECTION_SYNC_LIMIT,
        )
    })
    .await
    .context("join support communication intake projection")??;
    if support_intake_count.max_updated_at_ms >= support_intake_since_ms {
        since_by_collection.insert(
            SUPPORT_COMMUNICATION_INTAKE_SINCE_KEY.to_string(),
            support_intake_count.max_updated_at_ms.saturating_add(1),
        );
    }
    let collections = business_record_projection_collections();
    let root = root.to_path_buf();
    let threads_relevance_commands_since_ms = *since_by_collection
        .get(THREADS_CTOX_RELEVANCE_COMMANDS_SINCE_KEY)
        .unwrap_or(&0);
    let threads_relevance_tasks_since_ms = *since_by_collection
        .get(THREADS_CTOX_RELEVANCE_TASKS_SINCE_KEY)
        .unwrap_or(&0);
    let threads_app_relevance_cursors = super::threads::app_relevance_source_collections()
        .iter()
        .map(|collection| {
            (
                *collection,
                *since_by_collection
                    .get(&threads_app_relevance_cursor_key(collection))
                    .unwrap_or(&0),
            )
        })
        .collect::<Vec<_>>();
    let threads_relevance_root = root.clone();
    let threads_relevance = tokio::task::spawn_blocking(move || {
        super::threads::project_ctox_relevance(
            &threads_relevance_root,
            threads_relevance_commands_since_ms,
            threads_relevance_tasks_since_ms,
            BUSINESS_RECORD_PROJECTION_SYNC_LIMIT,
        )
    })
    .await
    .context("join native threads ctox relevance projection")??;
    let threads_app_relevance_root = root.clone();
    let threads_app_relevance = tokio::task::spawn_blocking(move || {
        super::threads::project_app_relevance(
            &threads_app_relevance_root,
            &threads_app_relevance_cursors,
            BUSINESS_RECORD_PROJECTION_SYNC_LIMIT,
        )
    })
    .await
    .context("join native threads app relevance projection")??;

    count += support_intake_count.changed_count
        + threads_relevance.changed_count
        + threads_app_relevance.changed_count;
    for (collection, record_id) in &threads_relevance.projections {
        let _database_guard = database_write_lock.lock().await;
        let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
        upsert_business_record_projection(root.clone(), database, *collection, record_id.clone())
            .await?;
    }
    for (collection, record_id) in &threads_app_relevance.projections {
        let _database_guard = database_write_lock.lock().await;
        let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
        upsert_business_record_projection(root.clone(), database, *collection, record_id.clone())
            .await?;
    }
    for (source_collection, max_updated_at_ms) in &threads_relevance.source_cursors {
        let cursor_key = match *source_collection {
            "business_commands" => THREADS_CTOX_RELEVANCE_COMMANDS_SINCE_KEY,
            "ctox_queue_tasks" => THREADS_CTOX_RELEVANCE_TASKS_SINCE_KEY,
            _ => continue,
        };
        let previous = *since_by_collection.get(cursor_key).unwrap_or(&0);
        if *max_updated_at_ms >= previous {
            since_by_collection.insert(cursor_key.to_string(), max_updated_at_ms.saturating_add(1));
        }
    }
    for (source_collection, max_updated_at_ms) in &threads_app_relevance.source_cursors {
        let cursor_key = threads_app_relevance_cursor_key(source_collection);
        let previous = *since_by_collection.get(&cursor_key).unwrap_or(&0);
        if *max_updated_at_ms >= previous {
            since_by_collection.insert(cursor_key, max_updated_at_ms.saturating_add(1));
        }
    }

    let mut projected_documents = 0usize;
    while *next_collection_index < collections.len() {
        let collection_name = collections[*next_collection_index].clone();
        let mut since_ms = *since_by_collection.get(&collection_name).unwrap_or(&0);
        let mut after_record_id = after_record_id_by_collection
            .get(&collection_name)
            .cloned()
            .unwrap_or_default();
        loop {
            // Keep the cross-loop lock bounded to one small page. Holding it
            // while comparing every Business OS record blocked command
            // ingestion and browser writes for minutes on a large store.
            let _database_guard = database_write_lock.lock().await;
            let pull_root = root.clone();
            let pull_collection = collection_name.clone();
            let pull_after_record_id = after_record_id.clone();
            let pulled = tokio::task::spawn_blocking(move || {
                store::pull_collection_records_for_projection_after(
                    &pull_root,
                    &pull_collection,
                    Some(since_ms),
                    Some(&pull_after_record_id),
                    Some(BUSINESS_RECORD_PROJECTION_PAGE_SIZE),
                )
            })
            .await
            .context("join native business record projection page load")??;
            let documents = pulled
                .get("documents")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if documents.is_empty() {
                since_by_collection.insert(
                    collection_name.clone(),
                    if after_record_id.is_empty() {
                        since_ms
                    } else {
                        since_ms.saturating_add(1)
                    },
                );
                after_record_id_by_collection.remove(&collection_name);
                *next_collection_index = next_collection_index.saturating_add(1);
                break;
            }
            let document_count = documents.len();
            let page_is_full = documents.len() >= BUSINESS_RECORD_PROJECTION_PAGE_SIZE;
            let collection = database
                .collection(&collection_name)
                .with_context(|| format!("{collection_name} collection is not registered"))?;
            let last_document = documents
                .last()
                .context("projection page unexpectedly empty")?;
            let page_cursor_ms = last_document
                .get("updated_at_ms")
                .and_then(Value::as_i64)
                .unwrap_or(since_ms);
            let page_cursor_id = last_document
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
            count += bulk_upsert_business_record_projection_documents(
                &collection,
                &collection_name,
                documents,
            )
            .await
            .map_err(|err| {
                anyhow::anyhow!("bulk upsert {collection_name} business record projections: {err}")
            })?;
            since_ms = page_cursor_ms;
            after_record_id = page_cursor_id;
            since_by_collection.insert(collection_name.clone(), since_ms);
            after_record_id_by_collection.insert(collection_name.clone(), after_record_id.clone());
            projected_documents = projected_documents.saturating_add(document_count);
            drop(_write_guard);
            drop(_database_guard);
            if !page_is_full {
                since_by_collection.insert(collection_name.clone(), since_ms.saturating_add(1));
                after_record_id_by_collection.remove(&collection_name);
                *next_collection_index = next_collection_index.saturating_add(1);
                break;
            }
            if document_budget.is_some_and(|budget| projected_documents >= budget) {
                return Ok((count, false));
            }
            tokio::task::yield_now().await;
        }
        if document_budget.is_some_and(|budget| projected_documents >= budget) {
            return Ok((count, false));
        }
    }
    *next_collection_index = 0;
    {
        let _database_guard = database_write_lock.lock().await;
        let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
        count += reconcile_queue_chat_tracking_projections_if_changed(
            root.as_path(),
            database,
            queue_chat_repair_stamp,
        )
        .await?;
    }
    Ok((count, true))
}

async fn reconcile_ctox_queue_task_projections(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    let queue = database
        .collection("ctox_queue_tasks")
        .context("ctox_queue_tasks collection is not registered")?;
    let queue_docs = queue
        .find(Some(MangoQuery {
            selector: Some(json!({
                "status": { "$in": ["queued", "running", "accepted"] }
            })),
            limit: Some(500),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query ctox_queue_tasks for reconciliation: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec ctox_queue_tasks reconciliation query: {err}"))?;
    let Some(queue_docs) = queue_docs.as_array() else {
        return Ok(0);
    };

    let mut repaired_documents = Vec::new();
    let mut orphaned_commands = Vec::new();
    for queue_doc in queue_docs {
        if queue_doc
            .get("_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let queue_status = queue_doc
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(queue_status, "queued" | "running" | "accepted") {
            continue;
        }
        let command_id = queue_doc
            .get("command_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(command_id) = command_id else {
            continue;
        };
        // This is an exact primary-key lookup. Routing it through Mango
        // needlessly enters the query/doc-cache path and can surface UTL2
        // when a long-lived cache still holds an older envelope revision.
        // A single poisoned command then made the whole projection loop retry
        // every three seconds forever. Read the authoritative storage row
        // directly; reconciliation writes still use the collection API below.
        let Some(command_doc) =
            find_rxdb_document_by_id(database, "business_commands", command_id, false).await?
        else {
            continue;
        };

        let canonical_task = queue_doc
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|task_id| channels::load_queue_task(root, task_id).ok().flatten());

        let command_status = command_doc
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let command_updated_at_ms = command_doc
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| now_ms() as i64);
        let repaired_status = if let Some(task) = canonical_task.as_ref() {
            let canonical_status = queue_projection_status_for_route_status(&task.route_status);
            let projection_route_status = queue_doc
                .get("route_status")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if queue_status != canonical_status || projection_route_status != task.route_status {
                Some((
                    canonical_status,
                    now_ms() as i64,
                    task.status_note.as_deref(),
                    Some(task.route_status.as_str()),
                    canonical_task.as_ref(),
                ))
            } else {
                None
            }
        } else if let Some(status) = terminal_queue_status_for_command(command_status) {
            Some((status, command_updated_at_ms, None, None, None))
        } else if matches!(command_status, "accepted" | "pending" | "pending_sync")
            && projection_queue_task_is_orphaned(root, queue_doc)
            && projection_document_age_ms(queue_doc, command_updated_at_ms)
                > QUEUE_CHAT_REPAIR_ORPHAN_EPOCH_MS
        {
            Some((
                "failed",
                now_ms() as i64,
                Some(
                    "Queue task is no longer present in the CTOX harness queue; marking the orphaned Business OS projection as failed.",
                ),
                Some("failed"),
                None,
            ))
        } else {
            None
        };
        let Some((repaired_status, repaired_at_ms, error_note, route_status, canonical_task)) =
            repaired_status
        else {
            continue;
        };
        let mut next = queue_doc.clone();
        if let Some(object) = next.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object.remove("_attachments");
            object.insert(
                "status".to_string(),
                Value::String(repaired_status.to_string()),
            );
            object.insert(
                "route_status".to_string(),
                Value::String(
                    route_status
                        .unwrap_or_else(|| route_status_for_queue_projection(repaired_status))
                        .to_string(),
                ),
            );
            object.insert(
                "task_status".to_string(),
                Value::String(repaired_status.to_string()),
            );
            object.insert("updated_at_ms".to_string(), Value::from(repaired_at_ms));
            if let Some(error_note) = error_note {
                object.insert(
                    "status_note".to_string(),
                    Value::String(error_note.to_string()),
                );
                if repaired_status == "failed" {
                    object.insert("error".to_string(), Value::String(error_note.to_string()));
                }
            }
            if let Some(task) = canonical_task {
                if let Some(owner) = task
                    .lease_owner
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    object.insert("lease_owner".to_string(), Value::String(owner.to_string()));
                }
                if let Some(leased_at) = task
                    .leased_at
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    object.insert(
                        "leased_at".to_string(),
                        Value::String(leased_at.to_string()),
                    );
                }
                if let Some(acked_at) = task
                    .acked_at
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    object.insert("acked_at".to_string(), Value::String(acked_at.to_string()));
                }
            }
        }
        upsert_business_record_projection_document(&queue, "ctox_queue_tasks", next.clone())
            .await
            .map_err(|err| anyhow::anyhow!("repair ctox_queue_tasks projection: {err}"))?;
        repaired_documents.push(next);
        if command_status == "accepted" && repaired_status == "failed" {
            orphaned_commands.push((command_id.to_string(), repaired_at_ms));
        }
    }

    if !repaired_documents.is_empty() {
        let root_for_writeback = root.to_path_buf();
        let documents = repaired_documents.clone();
        tokio::task::spawn_blocking(move || {
            store::push_collection_records(
                &root_for_writeback,
                json!({
                    "collection": "ctox_queue_tasks",
                    "documents": documents
                }),
            )
        })
        .await
        .context("join ctox_queue_tasks reconciliation writeback")??;
    }
    for (command_id, failed_at_ms) in orphaned_commands {
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || {
            store::mark_business_command_failed(
                &root,
                &command_id,
                "Queue task is no longer present in the CTOX harness queue; no tracked execution is available.",
                failed_at_ms,
            )
        })
        .await
        .context("join orphaned business command repair")??;
    }
    Ok(repaired_documents.len())
}

fn terminal_queue_status_for_command(status: &str) -> Option<&'static str> {
    match status {
        "completed" | "handled" | "done" => Some("completed"),
        "failed" | "error" => Some("failed"),
        "cancelled" | "canceled" => Some("cancelled"),
        "blocked" => Some("blocked"),
        _ => None,
    }
}

fn route_status_for_queue_projection(status: &str) -> &str {
    match status {
        "completed" => "handled",
        "cancelled" => "cancelled",
        "failed" => "failed",
        "blocked" => "blocked",
        "running" => "leased",
        _ => "pending",
    }
}

fn queue_projection_status_for_route_status(route_status: &str) -> &'static str {
    match route_status {
        "pending" => "queued",
        "leased" => "running",
        "handled" => "completed",
        "cancelled" => "cancelled",
        "failed" => "failed",
        "blocked" => "blocked",
        _ => "queued",
    }
}

fn projection_queue_task_is_orphaned(root: &Path, document: &Value) -> bool {
    let task_id = document
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(task_id) = task_id else {
        return false;
    };
    channels::load_queue_task(root, task_id)
        .map(|task| task.is_none())
        .unwrap_or(false)
}

fn projection_document_age_ms(document: &Value, fallback_updated_at_ms: i64) -> i64 {
    let updated_at_ms = document
        .get("updated_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(fallback_updated_at_ms);
    (now_ms() as i64).saturating_sub(updated_at_ms)
}

async fn reconcile_business_chat_tracking_projections(
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    let chats = database
        .collection("business_chats")
        .context("business_chats collection is not registered")?;
    let commands = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let queue = database
        .collection("ctox_queue_tasks")
        .context("ctox_queue_tasks collection is not registered")?;
    let chat_docs = chats
        .find(Some(MangoQuery {
            selector: Some(json!({ "tracking_active": true })),
            limit: Some(200),
            ..Default::default()
        }))
        .map_err(|err| anyhow::anyhow!("query business_chats for reconciliation: {err}"))?
        .exec(false)
        .await
        .map_err(|err| anyhow::anyhow!("exec business_chats reconciliation query: {err}"))?;
    let Some(chat_docs) = chat_docs.as_array() else {
        return Ok(0);
    };

    let mut command_ids = BTreeSet::new();
    let mut task_ids = BTreeSet::new();
    for chat_doc in chat_docs {
        if chat_doc
            .get("_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(messages) = chat_doc.get("messages").and_then(Value::as_array) else {
            continue;
        };
        for message in messages {
            let Some(message_object) = message.as_object() else {
                continue;
            };
            let status = chat_tracking_message_status(message_object);
            if !chat_tracking_status_is_active(&status) {
                continue;
            }
            let (command_id, task_id) = chat_tracking_message_command_and_task_ids(message_object);
            if let Some(command_id) = command_id {
                command_ids.insert(command_id.to_string());
            }
            if let Some(task_id) = task_id {
                task_ids.insert(task_id.to_string());
            }
        }
    }
    let command_docs =
        find_projection_documents_by_id(&commands, "business_commands", command_ids).await?;
    for command_doc in command_docs.values() {
        if let Some(task_id) = command_doc
            .get("task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            task_ids.insert(task_id.to_string());
        }
    }
    let task_docs = find_projection_documents_by_id(&queue, "ctox_queue_tasks", task_ids).await?;

    let mut repaired = 0;
    for chat_doc in chat_docs {
        if chat_doc
            .get("_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let mut next = chat_doc.clone();
        let Some(object) = next.as_object_mut() else {
            continue;
        };
        let Some(messages) = object.get_mut("messages").and_then(Value::as_array_mut) else {
            continue;
        };

        let mut changed = false;
        for message in messages.iter_mut() {
            let Some(message_object) = message.as_object_mut() else {
                continue;
            };
            let status = chat_tracking_message_status(message_object);
            if !chat_tracking_status_is_active(&status) {
                continue;
            }
            let (command_id, task_id) = chat_tracking_message_command_and_task_ids(message_object);
            if command_id.is_none() && task_id.is_none() {
                continue;
            }

            let command_doc = command_id.and_then(|command_id| command_docs.get(command_id));
            let resolved_task_id = task_id.map(str::to_string).or_else(|| {
                command_doc
                    .and_then(|doc| doc.get("task_id").and_then(Value::as_str))
                    .map(str::to_string)
            });
            let task_doc = resolved_task_id
                .as_deref()
                .and_then(|task_id| task_docs.get(task_id));
            let next_status = task_doc
                .and_then(|doc| doc.get("status").and_then(Value::as_str))
                .or_else(|| {
                    command_doc.and_then(|doc| doc.get("task_status").and_then(Value::as_str))
                })
                .or_else(|| command_doc.and_then(|doc| doc.get("status").and_then(Value::as_str)))
                .map(normalize_chat_tracking_status);
            let orphaned = command_doc.is_none()
                && task_doc.is_none()
                && chat_tracking_message_age_ms(message_object) > QUEUE_CHAT_REPAIR_ORPHAN_EPOCH_MS;
            let Some(next_status) = next_status.or_else(|| orphaned.then(|| "failed".to_string()))
            else {
                continue;
            };

            if Some(next_status.as_str()) != message_object.get("status").and_then(Value::as_str) {
                message_object.insert("status".to_string(), Value::String(next_status.clone()));
                changed = true;
            }
            if let Some(task_id) = resolved_task_id.as_deref() {
                if message_object.get("taskId").and_then(Value::as_str) != Some(task_id) {
                    message_object.insert("taskId".to_string(), Value::String(task_id.to_string()));
                    changed = true;
                }
            }
            if orphaned {
                message_object.insert(
                    "text".to_string(),
                    Value::String("CTOX kann diese ältere Aufgabe nicht mehr verfolgen: kein passender Command oder Queue-Task ist vorhanden.".to_string()),
                );
                message_object.insert("trackable".to_string(), Value::Bool(false));
                changed = true;
            }
        }

        changed |= apply_business_chat_tracking_summary(object);
        if !changed {
            continue;
        }
        object.remove("_rev");
        object.remove("_meta");
        object.remove("_attachments");
        object.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
        upsert_business_record_projection_document(&chats, "business_chats", next)
            .await
            .map_err(|err| anyhow::anyhow!("repair business_chats tracking projection: {err}"))?;
        repaired += 1;
    }
    Ok(repaired)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BusinessChatTrackingSummary {
    active: bool,
    status: String,
    tracking_id: String,
    command_id: String,
    task_id: String,
    message_id: String,
}

fn apply_business_chat_tracking_summary(object: &mut serde_json::Map<String, Value>) -> bool {
    let messages = object
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let summary = business_chat_tracking_summary(messages);
    let mut changed = false;
    changed |= set_json_bool_field(object, "tracking_active", summary.active);
    changed |= set_json_string_field(object, "tracking_status", &summary.status);
    changed |= set_json_string_field(object, "tracking_id", &summary.tracking_id);
    changed |= set_json_string_field(object, "tracking_command_id", &summary.command_id);
    changed |= set_json_string_field(object, "tracking_task_id", &summary.task_id);
    changed |= set_json_string_field(object, "tracking_message_id", &summary.message_id);
    changed
}

fn business_chat_tracking_summary(messages: &[Value]) -> BusinessChatTrackingSummary {
    for message in messages.iter().rev() {
        let Some(message_object) = message.as_object() else {
            continue;
        };
        let trackable = message_object
            .get("trackable")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let command_id = message_object
            .get("commandId")
            .or_else(|| message_object.get("command_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string();
        let task_id = message_object
            .get("taskId")
            .or_else(|| message_object.get("task_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string();
        if command_id.is_empty() && task_id.is_empty() {
            continue;
        }
        let status = message_object
            .get("status")
            .and_then(Value::as_str)
            .map(normalize_chat_tracking_status)
            .unwrap_or_else(|| "queued".to_string());
        let message_id = message_object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string();
        return BusinessChatTrackingSummary {
            active: trackable && chat_tracking_status_is_active(&status),
            status,
            tracking_id: if task_id.is_empty() {
                command_id.clone()
            } else {
                task_id.clone()
            },
            command_id,
            task_id,
            message_id,
        };
    }
    BusinessChatTrackingSummary {
        active: false,
        status: String::new(),
        tracking_id: String::new(),
        command_id: String::new(),
        task_id: String::new(),
        message_id: String::new(),
    }
}

fn set_json_bool_field(
    object: &mut serde_json::Map<String, Value>,
    field: &str,
    value: bool,
) -> bool {
    if object.get(field).and_then(Value::as_bool) == Some(value) {
        return false;
    }
    object.insert(field.to_string(), Value::Bool(value));
    true
}

fn set_json_string_field(
    object: &mut serde_json::Map<String, Value>,
    field: &str,
    value: &str,
) -> bool {
    if object.get(field).and_then(Value::as_str) == Some(value) {
        return false;
    }
    object.insert(field.to_string(), Value::String(value.to_string()));
    true
}

fn chat_tracking_message_status(message: &serde_json::Map<String, Value>) -> String {
    message
        .get("status")
        .and_then(Value::as_str)
        .map(normalize_chat_tracking_status)
        .unwrap_or_else(|| "queued".to_string())
}

fn chat_tracking_message_command_and_task_ids<'a>(
    message: &'a serde_json::Map<String, Value>,
) -> (Option<&'a str>, Option<&'a str>) {
    let command_id = message
        .get("commandId")
        .or_else(|| message.get("command_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let task_id = message
        .get("taskId")
        .or_else(|| message.get("task_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    (command_id, task_id)
}

async fn find_projection_documents_by_id(
    collection: &Arc<RxCollection>,
    collection_name: &str,
    ids: BTreeSet<String>,
) -> anyhow::Result<HashMap<String, Value>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    #[cfg(test)]
    CHAT_TRACKING_BATCH_DOCUMENT_LOOKUPS.fetch_add(1, Ordering::Relaxed);
    let ids = ids.into_iter().collect::<Vec<_>>();
    let documents = collection
        .storage_instance
        .find_documents_by_id(&ids, false)
        .await
        .map_err(|err| anyhow::anyhow!("find {collection_name} projection documents: {err}"))?;
    let mut by_id = HashMap::with_capacity(documents.len());
    for document in documents {
        let Some(id) = document
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        by_id.insert(id, document);
    }
    Ok(by_id)
}

fn normalize_chat_tracking_status(status: &str) -> String {
    match status.trim().to_lowercase().as_str() {
        "accepted" | "pending" | "pending_sync" | "waiting" => "queued".to_string(),
        "processing" | "executing" | "active" | "working" | "leased" => "running".to_string(),
        "success" | "done" | "erledigt" => "completed".to_string(),
        "error" => "failed".to_string(),
        value if value.is_empty() => "queued".to_string(),
        value => value.to_string(),
    }
}

fn chat_tracking_status_is_active(status: &str) -> bool {
    matches!(status, "queued" | "running")
}

fn chat_tracking_message_age_ms(message: &serde_json::Map<String, Value>) -> i64 {
    let created_at = message
        .get("createdAt")
        .or_else(|| message.get("created_at_ms"))
        .and_then(Value::as_i64)
        .unwrap_or_else(|| now_ms() as i64);
    (now_ms() as i64).saturating_sub(created_at)
}

async fn upsert_business_record_projection(
    root: PathBuf,
    database: &Arc<RxDatabase>,
    collection_name: &'static str,
    record_id: String,
) -> anyhow::Result<()> {
    let document = tokio::task::spawn_blocking(move || {
        store::pull_collection_record(&root, collection_name, &record_id)
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
        incremental_upsert_projection_if_changed(&collection, document, collection_name).await?;
    } else {
        if is_projection_tombstone(&document) {
            upsert_business_record_projection_tombstone(&collection, document)
                .await
                .map_err(|err| anyhow::anyhow!("upsert {collection_name} tombstone: {err}"))?;
        } else {
            normalize_business_record_projection_document(
                &collection,
                collection_name,
                &mut document,
            )?;
            incremental_upsert_projection_if_changed(&collection, document, collection_name)
                .await?;
        }
    }
    Ok(())
}

async fn upsert_business_record_projection_document(
    collection: &Arc<RxCollection>,
    collection_name: &str,
    mut document: Value,
) -> anyhow::Result<()> {
    if is_projection_tombstone(&document) {
        remove_projection_rxdb_envelope(&mut document);
        return upsert_business_record_projection_tombstone(collection, document).await;
    }
    normalize_business_record_projection_document(collection, collection_name, &mut document)?;
    match collection.upsert(document.clone()).await {
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

/// Project a pulled collection batch without turning every record into its own
/// SQLite transaction. The old per-document `collection.upsert()` loop made a
/// cold native-peer start perform tens of thousands of transactions while it
/// held the projection writer lock; on a realistic Business OS store this
/// blocked command ingestion and browser replication for 6-7 minutes.
async fn bulk_upsert_business_record_projection_documents(
    collection: &Arc<RxCollection>,
    collection_name: &str,
    documents: Vec<Value>,
) -> anyhow::Result<usize> {
    if documents.is_empty() {
        return Ok(0);
    }
    let schema = collection
        .schema_required()
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let primary_path = schema.primary_path.clone();
    let mut normal_documents = Vec::with_capacity(documents.len());
    let mut tombstones = Vec::new();
    for mut document in documents {
        if let Some(object) = document.as_object_mut() {
            object.remove("_rev");
            object.remove("_meta");
            object
                .entry("is_deleted".to_string())
                .or_insert_with(|| Value::Bool(false));
        }
        if is_projection_tombstone(&document) {
            tombstones.push(document);
            continue;
        }
        normalize_business_record_projection_document(collection, collection_name, &mut document)?;
        normal_documents.push(document);
    }

    let mut existing_by_id = HashMap::<String, Value>::new();
    for id_batch in normal_documents
        .iter()
        .filter_map(|document| {
            document
                .get(&primary_path)
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>()
        .chunks(BUSINESS_RECORD_PROJECTION_WRITE_BATCH_SIZE)
    {
        for existing in collection
            .storage_instance
            .find_documents_by_id(id_batch, true)
            .await
            .map_err(|err| anyhow::anyhow!("load existing projection batch: {err}"))?
        {
            if let Some(id) = existing
                .get(&primary_path)
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                existing_by_id.insert(id, existing);
            }
        }
    }

    let changed_documents = normal_documents
        .into_iter()
        .filter(|document| {
            let Some(id) = document.get(&primary_path).and_then(Value::as_str) else {
                return true;
            };
            existing_by_id.get(id).map_or(true, |existing| {
                canonical_projection_document_for_compare(existing)
                    != canonical_projection_document_for_compare(document)
            })
        })
        .collect::<Vec<_>>();

    let mut changed = 0usize;
    for batch in changed_documents.chunks(BUSINESS_RECORD_PROJECTION_WRITE_BATCH_SIZE) {
        let batch_documents = batch.to_vec();
        let result = collection
            .bulk_upsert(batch_documents.clone())
            .await
            .map_err(|err| anyhow::anyhow!("bulk upsert projection batch: {err}"))?;
        changed = changed.saturating_add(result.success.len());
        if result.error.is_empty() {
            continue;
        }
        let failed_ids = result
            .error
            .iter()
            .map(|error| error.document_id.as_str())
            .collect::<HashSet<_>>();
        for document in batch_documents {
            let Some(id) = document.get(&primary_path).and_then(Value::as_str) else {
                continue;
            };
            if !failed_ids.contains(id) {
                continue;
            }
            upsert_business_record_projection_document(collection, collection_name, document)
                .await?;
            changed = changed.saturating_add(1);
        }
    }

    for tombstone in tombstones {
        upsert_business_record_projection_document(collection, collection_name, tombstone).await?;
        changed = changed.saturating_add(1);
    }
    Ok(changed)
}

fn normalize_business_record_projection_document(
    collection: &Arc<RxCollection>,
    collection_name: &str,
    document: &mut Value,
) -> anyhow::Result<()> {
    let schema = collection
        .schema_required()
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let Some(object) = document.as_object_mut() else {
        return Ok(());
    };
    let timestamp_default = projection_timestamp_default(object);
    object.remove("_rev");
    object.remove("_meta");
    normalize_projection_required_fields(
        &schema.json_schema.required,
        &schema.json_schema.properties,
        &schema.primary_path,
        timestamp_default,
        object,
    );
    normalize_projection_schema_values(
        &schema.json_schema.required,
        &schema.json_schema.properties,
        timestamp_default,
        object,
    );
    if collection_name == "document_versions" {
        object
            .entry("diagnostics".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        object
            .entry("model_json".to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        object
            .entry("source_kind".to_string())
            .or_insert_with(|| Value::String(String::new()));
        object
            .entry("blob_id".to_string())
            .or_insert_with(|| Value::String(String::new()));
        object
            .entry("version".to_string())
            .or_insert_with(|| Value::from(0));
        let updated_at_ms = object
            .get("updated_at_ms")
            .cloned()
            .unwrap_or(Value::from(0));
        object
            .entry("created_at_ms".to_string())
            .or_insert(updated_at_ms);
    }
    Ok(())
}

fn remove_projection_rxdb_envelope(document: &mut Value) {
    let Some(object) = document.as_object_mut() else {
        return;
    };
    object.remove("_rev");
    object.remove("_meta");
}

fn normalize_projection_required_fields(
    required: &[String],
    properties: &HashMap<String, JsonSchema>,
    primary_path: &str,
    timestamp_default: i64,
    object: &mut serde_json::Map<String, Value>,
) {
    for field in required {
        if field == primary_path || object.contains_key(field) {
            continue;
        }
        if let Some(property) = properties.get(field) {
            object.insert(
                field.clone(),
                projection_default_value_for_field(field, property, timestamp_default),
            );
        }
    }
}

fn normalize_projection_schema_values(
    required: &[String],
    properties: &HashMap<String, JsonSchema>,
    timestamp_default: i64,
    object: &mut serde_json::Map<String, Value>,
) {
    let mut remove_fields = Vec::new();
    for (field, property) in properties {
        let is_required = required
            .iter()
            .any(|required_field| required_field == field);
        let Some(value) = object.get_mut(field) else {
            continue;
        };
        if value.is_null() && !is_required {
            remove_fields.push(field.clone());
            continue;
        }
        match projection_normalized_value_for_schema_field(
            field,
            property,
            value,
            is_required,
            timestamp_default,
        ) {
            ProjectionValueNormalization::Keep => {}
            ProjectionValueNormalization::Replace(next) => *value = next,
            ProjectionValueNormalization::Remove => remove_fields.push(field.clone()),
        }
    }
    for field in remove_fields {
        object.remove(&field);
    }
}

fn projection_timestamp_default(object: &serde_json::Map<String, Value>) -> i64 {
    object
        .get("updated_at_ms")
        .or_else(|| object.get("created_at_ms"))
        .or_else(|| object.get("createdAt"))
        .and_then(json_number_as_i64)
        .or_else(|| {
            object
                .get("_meta")
                .and_then(Value::as_object)
                .and_then(|meta| meta.get("lwt"))
                .and_then(json_number_as_i64)
        })
        .unwrap_or(0)
}

fn json_number_as_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        value
            .as_f64()
            .filter(|number| number.is_finite())
            .map(|number| number as i64)
    })
}

enum ProjectionValueNormalization {
    Keep,
    Replace(Value),
    Remove,
}

fn projection_normalized_value_for_schema_field(
    field: &str,
    property: &JsonSchema,
    value: &Value,
    is_required: bool,
    timestamp_default: i64,
) -> ProjectionValueNormalization {
    let fallback = || {
        if is_required {
            ProjectionValueNormalization::Replace(projection_default_value_for_field(
                field,
                property,
                timestamp_default,
            ))
        } else {
            ProjectionValueNormalization::Remove
        }
    };
    match property.schema_type.as_deref() {
        Some("number") => {
            if value.is_number() {
                ProjectionValueNormalization::Keep
            } else if let Some(number) = projection_value_as_f64(value, field) {
                match projection_json_number_from_f64(number) {
                    Some(number) => ProjectionValueNormalization::Replace(number),
                    None => fallback(),
                }
            } else {
                fallback()
            }
        }
        Some("integer") => {
            if value.as_i64().is_some() || value.as_u64().is_some() {
                ProjectionValueNormalization::Keep
            } else if let Some(number) = projection_value_as_i64(value, field) {
                ProjectionValueNormalization::Replace(Value::from(number))
            } else {
                fallback()
            }
        }
        Some("string") => match value {
            Value::String(_) => ProjectionValueNormalization::Keep,
            Value::Number(number) => {
                ProjectionValueNormalization::Replace(Value::String(number.to_string()))
            }
            Value::Bool(flag) => {
                ProjectionValueNormalization::Replace(Value::String(flag.to_string()))
            }
            _ => fallback(),
        },
        Some("boolean") => {
            if value.is_boolean() {
                ProjectionValueNormalization::Keep
            } else if let Some(flag) = projection_value_as_bool(value) {
                ProjectionValueNormalization::Replace(Value::Bool(flag))
            } else {
                fallback()
            }
        }
        Some("array") => {
            if value.is_array() {
                ProjectionValueNormalization::Keep
            } else {
                fallback()
            }
        }
        Some("object") => {
            if value.is_object() {
                ProjectionValueNormalization::Keep
            } else {
                fallback()
            }
        }
        _ => ProjectionValueNormalization::Keep,
    }
}

fn projection_value_as_f64(value: &Value, field: &str) -> Option<f64> {
    value.as_f64().or_else(|| {
        value
            .as_str()
            .and_then(|raw| projection_string_as_f64(raw, field))
    })
}

fn projection_value_as_i64(value: &Value, field: &str) -> Option<i64> {
    value.as_i64().or_else(|| {
        value
            .as_u64()
            .and_then(|number| i64::try_from(number).ok())
            .or_else(|| {
                value
                    .as_str()
                    .and_then(|raw| projection_string_as_f64(raw, field))
                    .filter(|number| number.is_finite() && number.fract() == 0.0)
                    .map(|number| number as i64)
            })
    })
}

fn projection_json_number_from_f64(number: f64) -> Option<Value> {
    if !number.is_finite() {
        return None;
    }
    if number.fract() == 0.0 && number >= i64::MIN as f64 && number <= i64::MAX as f64 {
        return Some(Value::from(number as i64));
    }
    serde_json::Number::from_f64(number).map(Value::Number)
}

fn projection_string_as_f64(raw: &str, field: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok().or_else(|| {
        projection_field_accepts_datetime(field).then(|| {
            DateTime::parse_from_rfc3339(trimmed)
                .map(|timestamp: DateTime<FixedOffset>| timestamp.timestamp_millis() as f64)
                .ok()
        })?
    })
}

fn projection_field_accepts_datetime(field: &str) -> bool {
    matches!(field, "start_time" | "end_time" | "createdAt")
        || field.ends_with("_at")
        || field.ends_with("_at_ms")
        || field.ends_with("_time")
}

fn projection_value_as_bool(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| {
        value
            .as_str()
            .and_then(|raw| match raw.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            })
    })
}

fn projection_default_value_for_field(
    field: &str,
    property: &JsonSchema,
    timestamp_default: i64,
) -> Value {
    if let Some(default) = &property.default {
        return default.clone();
    }
    if field.ends_with("_at_ms") || field == "createdAt" {
        return Value::from(timestamp_default);
    }
    match property.schema_type.as_deref() {
        Some("array") => Value::Array(Vec::new()),
        Some("boolean") => Value::Bool(false),
        Some("integer") | Some("number") => Value::from(0),
        Some("object") => Value::Object(serde_json::Map::new()),
        Some("string") => Value::String(String::new()),
        _ if property.items.is_some() => Value::Array(Vec::new()),
        _ if !property.properties.is_empty() => Value::Object(serde_json::Map::new()),
        _ => Value::Null,
    }
}

fn is_projection_tombstone(document: &Value) -> bool {
    document
        .get("_deleted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

async fn upsert_business_record_projection_tombstone(
    collection: &Arc<RxCollection>,
    document: Value,
) -> anyhow::Result<()> {
    let schema = collection
        .schema_required()
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let primary_path = schema.primary_path.clone();
    let document_id = document
        .get(&primary_path)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("projection tombstone missing primary key {primary_path}"))?
        .to_string();

    let existing = collection
        .storage_instance
        .find_documents_by_id(std::slice::from_ref(&document_id), true)
        .await
        .map_err(|err| anyhow::anyhow!("load existing projection tombstone target: {err}"))?
        .into_iter()
        .next();

    let Some(previous) = existing else {
        let mut write_data = document;
        prepare_projection_tombstone_document(schema, &mut write_data);
        let write_data = fill_object_data_before_insert(schema, write_data)
            .map_err(|err| anyhow::anyhow!("fill projection tombstone envelope: {err}"))?;
        collection
            .insert(write_data)
            .await
            .map(|_| ())
            .map_err(|err| anyhow::anyhow!("insert projection tombstone: {err}"))?;
        return Ok(());
    };

    let repaired_previous = fill_object_data_before_insert(schema, previous.clone())
        .map_err(|err| anyhow::anyhow!("repair existing projection tombstone envelope: {err}"))?;
    let mut next = repaired_previous.clone();
    if let (Some(next_obj), Some(write_obj)) = (next.as_object_mut(), document.as_object()) {
        for (key, value) in write_obj {
            if matches!(key.as_str(), "_rev" | "_attachments" | "_meta") {
                continue;
            }
            next_obj.insert(key.clone(), value.clone());
        }
    } else {
        next = document;
    }
    prepare_projection_tombstone_document(schema, &mut next);

    let result = collection
        .storage_instance
        .bulk_write(
            vec![BulkWriteRow {
                previous: Some(repaired_previous),
                document: next,
            }],
            "business-os-projection-tombstone-upsert",
        )
        .await
        .map_err(|err| anyhow::anyhow!("write projection tombstone: {err}"))?;
    if let Some(err) = result.error.first() {
        anyhow::bail!("write projection tombstone conflict: {err:?}");
    }
    Ok(())
}

fn prepare_projection_tombstone_document(schema: &rxdb::rx_schema::RxSchema, document: &mut Value) {
    let Some(object) = document.as_object_mut() else {
        return;
    };
    object.insert("_deleted".to_string(), Value::Bool(true));
    if schema.json_schema.properties.contains_key("is_deleted") {
        object.insert("is_deleted".to_string(), Value::Bool(true));
    }
    for field in &schema.json_schema.required {
        if field.starts_with('_') {
            continue;
        }
        let missing = object.get(field).map(Value::is_null).unwrap_or(true);
        if missing {
            object.insert(
                field.clone(),
                projection_tombstone_required_default(&schema.json_schema, field),
            );
        }
    }
}

fn projection_tombstone_required_default(schema: &RxJsonSchema, field: &str) -> Value {
    let Some(property) = schema.properties.get(field) else {
        return Value::String(String::new());
    };
    if let Some(default) = &property.default {
        return default.clone();
    }
    match property.schema_type.as_deref() {
        Some("boolean") => Value::Bool(false),
        Some("number") | Some("integer") => Value::from(0),
        Some("array") => Value::Array(Vec::new()),
        Some("object") => Value::Object(serde_json::Map::new()),
        _ => Value::String(String::new()),
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
    let message = error.to_string();
    is_doc_cache_revision_error(error)
        || error.code() == "CONFLICT"
        || message.contains("CONFLICT")
        || message.contains("UNIQUE constraint failed")
        || message.contains("PRIMARY KEY constraint failed")
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChannelStateProjectionStamp {
    accounts: ChannelAccountsProjectionStamp,
    pairing_artifacts: Vec<ChannelPairingArtifactStamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChannelAccountsProjectionStamp {
    table_exists: bool,
    row_count: usize,
    latest_updated_at_ms: i64,
    content_hash: String,
    channels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChannelPairingArtifactStamp {
    channel: String,
    status: ProjectionFileStamp,
    qr: ProjectionFileStamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectionFileStamp {
    len: u64,
    modified_ns: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BusinessCommandsSourceStamp {
    table: BusinessCommandsTableStamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BusinessCommandsTableStamp {
    table_name: Option<String>,
    pending_count: i64,
    latest_pending_lwt_bits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BusinessRecordProjectionSourceStamp {
    records: store::BusinessRecordsProjectionStamp,
    communication: channels::CommunicationIntakeSourceStamp,
    queue_chat_repair: QueueChatRepairProjectionStamp,
    knowledge: KnowledgeCatalogProjectionStamp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct KnowledgeCatalogProjectionStamp {
    main_skill_count: i64,
    main_skill_updated_at: String,
    skillbook_count: i64,
    skillbook_updated_at: String,
    runbook_count: i64,
    runbook_updated_at: String,
    resource_count: i64,
    resource_updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueueChatRepairProjectionStamp {
    rxdb_queue_tasks: RxdbCollectionSummaryStamp,
    rxdb_business_commands: RxdbCollectionSummaryStamp,
    rxdb_business_chats: RxdbCollectionSummaryStamp,
    canonical_queue: CanonicalQueueRepairStamp,
    orphan_repair_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RxdbCollectionSummaryStamp {
    table_name: Option<String>,
    row_count: i64,
    deleted_count: i64,
    latest_lwt_bits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CanonicalQueueRepairStamp {
    database: SqliteProjectionFilesStamp,
    queue_message_count: i64,
    routing_count: i64,
    latest_queue_observed_at: String,
    latest_routing_updated_at: String,
    routing_status_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SqliteProjectionFilesStamp {
    db: ProjectionFileStamp,
    wal: ProjectionFileStamp,
    shm: ProjectionFileStamp,
    journal: ProjectionFileStamp,
}

async fn business_commands_source_change(
    root: &Path,
    last_source_stamp: &mut Option<BusinessCommandsSourceStamp>,
) -> anyhow::Result<Option<BusinessCommandsSourceStamp>> {
    let source_stamp = business_commands_source_stamp(root).await?;
    if source_stamp.table.pending_count == 0 {
        *last_source_stamp = Some(source_stamp);
        return Ok(None);
    }
    Ok(Some(source_stamp))
}

async fn refresh_business_commands_source_stamp(
    root: &Path,
    last_source_stamp: &mut Option<BusinessCommandsSourceStamp>,
) -> anyhow::Result<()> {
    *last_source_stamp = Some(business_commands_source_stamp(root).await?);
    Ok(())
}

async fn business_commands_source_stamp(
    root: &Path,
) -> anyhow::Result<BusinessCommandsSourceStamp> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        Ok(BusinessCommandsSourceStamp {
            table: business_commands_table_stamp(&root)?,
        })
    })
    .await
    .context("join business commands source stamp")?
}

fn business_commands_table_stamp(root: &Path) -> anyhow::Result<BusinessCommandsTableStamp> {
    let path = store::rxdb_store_path(root);
    if !path.exists() {
        return Ok(empty_business_commands_table_stamp(None));
    }
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "open Business OS RxDB store for command source stamp {}",
            path.display()
        )
    })?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("configure business command source stamp busy_timeout")?;
    let Some(table) = latest_rxdb_collection_table(&conn, "business_commands")? else {
        return Ok(empty_business_commands_table_stamp(None));
    };
    let quoted = sqlite_quote_identifier(&table);
    let deleted_expr = if sqlite_table_has_column(&conn, &table, "deleted")? {
        "deleted"
    } else {
        "0"
    };
    let lwt_expr = if sqlite_table_has_column(&conn, &table, "lastWriteTime")? {
        "COALESCE(lastWriteTime, 0)"
    } else {
        "CAST(COALESCE(json_extract(data, '$._meta.lwt'), json_extract(data, '$.updated_at_ms'), 0) AS REAL)"
    };
    let (pending_count, latest_pending_lwt): (i64, f64) = conn
        .query_row(
            &format!(
                "SELECT COUNT(*), COALESCE(MAX({lwt_expr}), 0)
                 FROM {quoted}
                 WHERE {deleted_expr} = 0
                   AND json_extract(data, '$.status') = 'pending_sync'"
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .with_context(|| format!("stamp pending business_commands rows in {table}"))?;
    Ok(BusinessCommandsTableStamp {
        table_name: Some(table),
        pending_count,
        latest_pending_lwt_bits: latest_pending_lwt.to_bits(),
    })
}

fn empty_business_commands_table_stamp(table_name: Option<String>) -> BusinessCommandsTableStamp {
    BusinessCommandsTableStamp {
        table_name,
        pending_count: 0,
        latest_pending_lwt_bits: 0,
    }
}

async fn reconcile_queue_chat_tracking_projections_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    last_projection_stamp: &mut Option<QueueChatRepairProjectionStamp>,
) -> anyhow::Result<usize> {
    let projection_stamp = queue_chat_repair_projection_stamp_async(root).await?;
    if last_projection_stamp.as_ref() == Some(&projection_stamp) {
        return Ok(0);
    }

    let mut count = reconcile_ctox_queue_task_projections(root, database).await?;
    count += reconcile_business_chat_tracking_projections(database).await?;
    *last_projection_stamp = Some(queue_chat_repair_projection_stamp_async(root).await?);
    Ok(count)
}

async fn queue_chat_repair_projection_stamp_async(
    root: &Path,
) -> anyhow::Result<QueueChatRepairProjectionStamp> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || queue_chat_repair_projection_stamp(&root))
        .await
        .context("join queue/chat repair projection stamp")?
}

fn queue_chat_repair_projection_stamp(
    root: &Path,
) -> anyhow::Result<QueueChatRepairProjectionStamp> {
    let now = now_ms() as i64;
    let orphan_repair_epoch = (now / QUEUE_CHAT_REPAIR_ORPHAN_EPOCH_MS.max(1)).max(0) as u64;
    let rxdb_path = store::rxdb_store_path(root);
    let (rxdb_queue_tasks, rxdb_business_commands, rxdb_business_chats) =
        rxdb_queue_chat_repair_stamps(&rxdb_path)?;
    Ok(QueueChatRepairProjectionStamp {
        rxdb_queue_tasks,
        rxdb_business_commands,
        rxdb_business_chats,
        canonical_queue: canonical_queue_repair_stamp(root)?,
        orphan_repair_epoch,
    })
}

fn rxdb_queue_chat_repair_stamps(
    rxdb_path: &Path,
) -> anyhow::Result<(
    RxdbCollectionSummaryStamp,
    RxdbCollectionSummaryStamp,
    RxdbCollectionSummaryStamp,
)> {
    if !rxdb_path.exists() {
        return Ok((
            empty_rxdb_collection_summary_stamp(None),
            empty_rxdb_collection_summary_stamp(None),
            empty_rxdb_collection_summary_stamp(None),
        ));
    }
    let conn = Connection::open_with_flags(
        rxdb_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "open Business OS RxDB store for queue/chat repair stamp {}",
            rxdb_path.display()
        )
    })?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("configure queue/chat repair stamp busy_timeout")?;
    Ok((
        rxdb_collection_summary_stamp(&conn, "ctox_queue_tasks")?,
        rxdb_collection_summary_stamp(&conn, "business_commands")?,
        rxdb_collection_summary_stamp(&conn, "business_chats")?,
    ))
}

fn rxdb_collection_summary_stamp(
    conn: &Connection,
    collection: &str,
) -> anyhow::Result<RxdbCollectionSummaryStamp> {
    let Some(table) = latest_rxdb_collection_table(conn, collection)? else {
        return Ok(empty_rxdb_collection_summary_stamp(None));
    };
    let quoted = sqlite_quote_identifier(&table);
    let deleted_expr = if sqlite_table_has_column(conn, &table, "deleted")? {
        "COALESCE(deleted, 0)"
    } else {
        "0"
    };
    let lwt_expr = if sqlite_table_has_column(conn, &table, "lastWriteTime")? {
        "COALESCE(lastWriteTime, 0)"
    } else {
        "CAST(COALESCE(json_extract(data, '$._meta.lwt'), json_extract(data, '$.updated_at_ms'), 0) AS REAL)"
    };
    let (row_count, deleted_count, latest_lwt): (i64, i64, f64) = conn
        .query_row(
            &format!(
                "SELECT COUNT(*), COALESCE(SUM({deleted_expr}), 0), COALESCE(MAX({lwt_expr}), 0)
                 FROM {quoted}"
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .with_context(|| format!("stamp {collection} repair table {table}"))?;
    Ok(RxdbCollectionSummaryStamp {
        table_name: Some(table),
        row_count,
        deleted_count,
        latest_lwt_bits: latest_lwt.to_bits(),
    })
}

fn empty_rxdb_collection_summary_stamp(table_name: Option<String>) -> RxdbCollectionSummaryStamp {
    RxdbCollectionSummaryStamp {
        table_name,
        row_count: 0,
        deleted_count: 0,
        latest_lwt_bits: 0,
    }
}

fn canonical_queue_repair_stamp(root: &Path) -> anyhow::Result<CanonicalQueueRepairStamp> {
    let db_path = crate::paths::core_db(root);
    let database = sqlite_projection_files_stamp(&db_path);
    if !db_path.exists() {
        return Ok(empty_canonical_queue_repair_stamp(database));
    }
    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open core DB for queue repair stamp {}", db_path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("configure canonical queue repair stamp busy_timeout")?;
    if !sqlite_table_exists(&conn, "communication_messages")?
        || !sqlite_table_exists(&conn, "communication_routing_state")?
    {
        return Ok(empty_canonical_queue_repair_stamp(database));
    }

    let (queue_message_count, latest_queue_observed_at): (i64, Option<String>) = conn
        .query_row(
            "SELECT COUNT(*), MAX(observed_at)
             FROM communication_messages
             WHERE channel = 'queue'
               AND direction = 'inbound'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("stamp canonical queue messages")?;
    let (routing_count, latest_routing_updated_at): (i64, Option<String>) = conn
        .query_row(
            "SELECT COUNT(*), MAX(r.updated_at)
             FROM communication_routing_state r
             JOIN communication_messages m ON m.message_key = r.message_key
             WHERE m.channel = 'queue'
               AND m.direction = 'inbound'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("stamp canonical queue routing")?;
    let routing_status_hash = canonical_queue_routing_status_hash(&conn)?;

    Ok(CanonicalQueueRepairStamp {
        database,
        queue_message_count,
        routing_count,
        latest_queue_observed_at: latest_queue_observed_at.unwrap_or_default(),
        latest_routing_updated_at: latest_routing_updated_at.unwrap_or_default(),
        routing_status_hash,
    })
}

fn canonical_queue_routing_status_hash(conn: &Connection) -> anyhow::Result<String> {
    let mut hasher = sha2::Sha256::new();
    let mut stmt = conn
        .prepare(
            "SELECT r.route_status, COUNT(*), COALESCE(MAX(r.updated_at), '')
             FROM communication_routing_state r
             JOIN communication_messages m ON m.message_key = r.message_key
             WHERE m.channel = 'queue'
               AND m.direction = 'inbound'
             GROUP BY r.route_status
             ORDER BY r.route_status ASC",
        )
        .context("prepare canonical queue routing status stamp")?;
    let mut rows = stmt
        .query([])
        .context("query canonical queue routing status stamp")?;
    while let Some(row) = rows.next()? {
        let route_status: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        let latest_updated_at: String = row.get(2)?;
        update_hash_with_string(&mut hasher, &route_status);
        hasher.update(count.to_le_bytes());
        update_hash_with_string(&mut hasher, &latest_updated_at);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn empty_canonical_queue_repair_stamp(
    database: SqliteProjectionFilesStamp,
) -> CanonicalQueueRepairStamp {
    CanonicalQueueRepairStamp {
        database,
        queue_message_count: 0,
        routing_count: 0,
        latest_queue_observed_at: String::new(),
        latest_routing_updated_at: String::new(),
        routing_status_hash: String::new(),
    }
}

fn sqlite_projection_files_stamp(path: &Path) -> SqliteProjectionFilesStamp {
    SqliteProjectionFilesStamp {
        db: projection_file_stamp(path),
        wal: projection_file_stamp(&sqlite_sidecar_path(path, "-wal")),
        shm: projection_file_stamp(&sqlite_sidecar_path(path, "-shm")),
        journal: projection_file_stamp(&sqlite_sidecar_path(path, "-journal")),
    }
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut os = path.as_os_str().to_owned();
    os.push(suffix);
    PathBuf::from(os)
}

fn latest_rxdb_collection_table(
    conn: &Connection,
    collection: &str,
) -> anyhow::Result<Option<String>> {
    let marker = format!("__{collection}__v");
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table'",
        )
        .with_context(|| format!("prepare {collection} RxDB table lookup"))?;
    let mut rows = stmt
        .query([])
        .with_context(|| format!("query {collection} RxDB tables"))?;
    let mut latest: Option<(i64, String)> = None;
    while let Some(row) = rows.next()? {
        let table: String = row.get(0)?;
        let Some(version) = rxdb_collection_version_from_dynamic_table_name(&table, &marker) else {
            continue;
        };
        if latest
            .as_ref()
            .is_none_or(|(latest_version, _)| version > *latest_version)
        {
            latest = Some((version, table));
        }
    }
    Ok(latest.map(|(_, table)| table))
}

fn rxdb_collection_version_from_dynamic_table_name(table: &str, marker: &str) -> Option<i64> {
    let (_, suffix) = table.rsplit_once(marker)?;
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

fn sqlite_quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

async fn sync_channel_state_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    last_projection_stamp: &mut Option<ChannelStateProjectionStamp>,
) -> anyhow::Result<usize> {
    let projection_stamp = channel_state_projection_stamp_async(root).await?;
    if last_projection_stamp.as_ref() == Some(&projection_stamp) {
        return Ok(0);
    }

    let synced = sync_channel_state_with_database(root, database).await?;
    *last_projection_stamp = Some(projection_stamp);
    Ok(synced)
}

async fn channel_state_projection_stamp_async(
    root: &Path,
) -> anyhow::Result<ChannelStateProjectionStamp> {
    let root_for_stamp = root.to_path_buf();
    tokio::task::spawn_blocking(move || channel_state_projection_stamp(&root_for_stamp))
        .await
        .context("join native channel state projection stamp")?
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
        if incremental_upsert_projection_if_changed(&accounts, document, "communication account")
            .await?
        {
            synced += 1;
        }
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
        if incremental_upsert_projection_if_changed(
            &pairing_states,
            document,
            "channel pairing state",
        )
        .await?
        {
            synced += 1;
        }
    }

    Ok(synced)
}

async fn incremental_upsert_projection_if_changed(
    collection: &Arc<RxCollection>,
    document: Value,
    label: &str,
) -> anyhow::Result<bool> {
    let schema = collection
        .schema_required()
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let primary_path = schema.primary_path.clone();
    let document_id = document
        .get(&primary_path)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("{label} projection missing primary key {primary_path}"))?
        .to_string();
    let existing = collection
        .storage_instance
        .find_documents_by_id(std::slice::from_ref(&document_id), true)
        .await
        .map_err(|err| anyhow::anyhow!("load existing {label} projection: {err}"))?
        .into_iter()
        .next();
    if let Some(existing) = existing {
        if canonical_projection_document_for_compare(&existing)
            == canonical_projection_document_for_compare(&document)
        {
            return Ok(false);
        }
    }
    match collection.incremental_upsert(document.clone()).await {
        Ok(_) => {}
        Err(err) if is_recoverable_projection_write_error(&err) => {
            repair_projection_document_envelope_and_upsert(collection, document)
                .await
                .map_err(|fallback_err| {
                    anyhow::anyhow!(
                        "upsert {label} projection after document cache envelope repair failed: {fallback_err}"
                    )
                })?;
        }
        Err(err) => return Err(anyhow::anyhow!("upsert {label} projection: {err}")),
    }
    Ok(true)
}

fn canonical_projection_document_for_compare(document: &Value) -> Value {
    let mut value = document.clone();
    remove_projection_compare_metadata(&mut value);
    value
}

fn remove_projection_compare_metadata(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("_rev");
            object.remove("_meta");
            object.remove("_attachments");
            object.remove("_deleted");
            object.remove("updated_at_ms");
            object.remove("generated_at_ms");
            for value in object.values_mut() {
                remove_projection_compare_metadata(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                remove_projection_compare_metadata(item);
            }
        }
        _ => {}
    }
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

fn channel_state_projection_stamp(root: &Path) -> anyhow::Result<ChannelStateProjectionStamp> {
    let accounts = channel_accounts_projection_stamp(root)?;
    let mut channel_ids: BTreeSet<String> = BUSINESS_OS_CHANNEL_IDS
        .iter()
        .map(|channel| (*channel).to_string())
        .collect();
    channel_ids.extend(accounts.channels.iter().cloned());
    let pairing_artifacts = channel_ids
        .into_iter()
        .map(|channel| {
            let artifacts =
                crate::communication::runtime::artifacts_dir_for_business_os(root, &channel);
            ChannelPairingArtifactStamp {
                channel,
                status: projection_file_stamp(&artifacts.join("pairing-status.json")),
                qr: projection_file_stamp(&artifacts.join("pairing-qr.svg")),
            }
        })
        .collect();
    Ok(ChannelStateProjectionStamp {
        accounts,
        pairing_artifacts,
    })
}

fn channel_accounts_projection_stamp(
    root: &Path,
) -> anyhow::Result<ChannelAccountsProjectionStamp> {
    let db_path = crate::paths::core_db(root);
    if !db_path.exists() {
        return Ok(empty_channel_accounts_projection_stamp(false));
    }
    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open communication accounts DB {}", db_path.display()))?;
    let table_exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'communication_accounts' LIMIT 1",
            [],
            |_| Ok(()),
        )
        .optional()
        .context("check communication_accounts table")?
        .is_some();
    if !table_exists {
        return Ok(empty_channel_accounts_projection_stamp(false));
    }

    let mut hasher = sha2::Sha256::new();
    let mut channels = BTreeSet::new();
    let mut row_count = 0usize;
    let mut latest_updated_at_ms = 0i64;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                account_key, channel, address, provider, profile_json,
                created_at, updated_at, last_inbound_ok_at, last_outbound_ok_at,
                CAST(strftime('%s', COALESCE(updated_at, created_at)) AS INTEGER) * 1000 AS updated_at_ms
            FROM communication_accounts
            ORDER BY account_key ASC
            "#,
        )
        .context("prepare communication account stamp query")?;
    let mut rows = stmt
        .query([])
        .context("query communication account stamp")?;
    while let Some(row) = rows
        .next()
        .context("read communication account stamp row")?
    {
        row_count += 1;
        for index in 0..9 {
            let value = row.get::<_, Option<String>>(index)?.unwrap_or_default();
            if index == 1 && !value.trim().is_empty() {
                channels.insert(value.clone());
            }
            hasher.update(value.len().to_le_bytes());
            hasher.update(value.as_bytes());
        }
        let updated_at_ms = row.get::<_, Option<i64>>(9)?.unwrap_or(0);
        latest_updated_at_ms = latest_updated_at_ms.max(updated_at_ms);
        hasher.update(updated_at_ms.to_le_bytes());
    }

    Ok(ChannelAccountsProjectionStamp {
        table_exists: true,
        row_count,
        latest_updated_at_ms,
        content_hash: format!("{:x}", hasher.finalize()),
        channels: channels.into_iter().collect(),
    })
}

fn empty_channel_accounts_projection_stamp(table_exists: bool) -> ChannelAccountsProjectionStamp {
    ChannelAccountsProjectionStamp {
        table_exists,
        row_count: 0,
        latest_updated_at_ms: 0,
        content_hash: String::new(),
        channels: Vec::new(),
    }
}

fn projection_file_stamp(path: &Path) -> ProjectionFileStamp {
    let Ok(metadata) = fs::metadata(path) else {
        return ProjectionFileStamp {
            len: 0,
            modified_ns: 0,
        };
    };
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    ProjectionFileStamp {
        len: metadata.len(),
        modified_ns,
    }
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
    root: &Path,
    database: &Arc<RxDatabase>,
    path: PathBuf,
    policy: DesktopFileContentPolicy,
) -> anyhow::Result<()> {
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    upsert_desktop_file_with_parent(
        root,
        database,
        path,
        policy,
        CTOX_DESKTOP_FOLDER_ID.to_string(),
        None,
    )
    .await
}

/// Expected number of chunk documents for an eagerly-synced desktop file of
/// `size_bytes`: base64 with padding (4 chars per 3 input bytes), split into
/// DESKTOP_FILE_CHUNK_SIZE-character chunks; empty files still get one empty
/// chunk. Mirrors the write path's `total` computation exactly.
fn expected_desktop_file_chunk_total(size_bytes: u64) -> u64 {
    let encoded_len = size_bytes.div_ceil(3) * 4;
    encoded_len.div_ceil(DESKTOP_FILE_CHUNK_SIZE as u64).max(1)
}

fn desktop_file_generation_verified_by_metadata(
    document: &Value,
    generation_id: &str,
    size_bytes: u64,
) -> bool {
    if generation_id.is_empty() {
        return false;
    }
    document
        .get("content_generation_id")
        .and_then(Value::as_str)
        == Some(generation_id)
        && document.get("content_state").and_then(Value::as_str) == Some("available")
        && document.get("chunk_count").and_then(Value::as_u64)
            == Some(expected_desktop_file_chunk_total(size_bytes))
        && document
            .get("generation_verified_at_ms")
            .and_then(Value::as_u64)
            .is_some_and(|value| value > 0)
}

async fn mark_desktop_file_chunk_generation_verified(
    files: &Arc<RxCollection>,
    document: &Value,
    expected_total: u64,
    now: u128,
) -> anyhow::Result<()> {
    let mut next = document.clone();
    let Some(object) = next.as_object_mut() else {
        return Ok(());
    };
    object.insert("chunk_count".to_string(), Value::from(expected_total));
    object.insert(
        "generation_verified_at_ms".to_string(),
        Value::from(u64::try_from(now).unwrap_or(u64::MAX)),
    );
    files
        .incremental_upsert(next)
        .await
        .map_err(|err| anyhow::anyhow!("mark desktop file chunks verified: {err}"))?;
    Ok(())
}

async fn find_rxdb_document_by_id(
    database: &Arc<RxDatabase>,
    collection_name: &str,
    document_id: &str,
    with_deleted: bool,
) -> anyhow::Result<Option<Value>> {
    let collection = database
        .collection(collection_name)
        .with_context(|| format!("{collection_name} collection is not registered"))?;
    let ids = vec![document_id.to_string()];
    let mut documents = collection
        .storage_instance
        .find_documents_by_id(&ids, with_deleted)
        .await
        .map_err(|err| anyhow::anyhow!("find {collection_name} document {document_id}: {err}"))?;
    Ok(documents.pop())
}

/// True when the chunk store holds the complete LIVE chunk set for the given
/// generation. The scan's change-detection fast path must not skip a file
/// whose chunks went missing (crash window, manual cleanup,
/// `ctox.file.materialize` repair) just because the file-doc fingerprint
/// still matches — the index has to stay self-healing.
async fn desktop_file_chunk_generation_is_complete(
    database: &Arc<RxDatabase>,
    file_id: &str,
    generation_id: &str,
    size_bytes: u64,
) -> bool {
    #[cfg(test)]
    DESKTOP_FILE_CHUNK_COMPLETENESS_CHECKS.fetch_add(1, Ordering::Relaxed);
    if generation_id.is_empty() {
        return false;
    }
    let Some(chunks) = database.collection("desktop_file_chunks") else {
        return false;
    };
    let expected_total = expected_desktop_file_chunk_total(size_bytes);
    let Ok(expected_total_usize) = usize::try_from(expected_total) else {
        return false;
    };
    let ids: Vec<String> = (0..expected_total_usize)
        .map(|idx| format!("{file_id}_{generation_id}_{idx}"))
        .collect();
    let Ok(documents) = chunks
        .storage_instance
        .find_documents_by_id(&ids, false)
        .await
    else {
        return false;
    };
    if documents.len() != expected_total_usize {
        return false;
    }
    let mut seen_indices = HashSet::with_capacity(documents.len());
    for document in documents {
        if document.get("file_id").and_then(Value::as_str) != Some(file_id) {
            return false;
        }
        if document.get("generation_id").and_then(Value::as_str) != Some(generation_id) {
            return false;
        }
        if document.get("total").and_then(Value::as_u64) != Some(expected_total) {
            return false;
        }
        let Some(idx) = document.get("idx").and_then(Value::as_u64) else {
            return false;
        };
        if idx >= expected_total || !seen_indices.insert(idx) {
            return false;
        }
    }
    true
}

async fn upsert_desktop_file_with_parent(
    root: &Path,
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
    let display_path = virtual_path
        .unwrap_or_else(|| format!("{}/{}", CTOX_DESKTOP_FOLDER_PATH, file_name.as_str()));
    let modified_at_ms = metadata_modified_at_ms(&metadata);

    // Change detection: the desktop-file index rescans every workspace root
    // every DESKTOP_FILE_SCAN_INTERVAL_SECS. Without this check every scan
    // minted a fresh (timestamped) generation id for EVERY file, re-wrote all
    // its chunks and tombstoned the previous generation — a permanent
    // insert/tombstone churn that browser-side replication (batchSize 2 for
    // desktop_file_chunks) could never catch up with. Skip files whose
    // on-disk fingerprint still matches the indexed document; below, reuse
    // the stored generation when only metadata changed but content did not.
    let files = database
        .collection("desktop_files")
        .context("desktop_files collection is not registered")?;
    let existing_file_doc = find_rxdb_document_by_id(database, "desktop_files", &file_id, false)
        .await
        .map_err(|err| anyhow::anyhow!("read desktop file doc {file_id}: {err}"))?;
    // Materialization is sticky: once a file was explicitly materialized
    // (ctox.file.materialize set content_state 'available'), the periodic
    // scan must NOT demote it back to its size/extension policy 'lazy'.
    // Demoting rewrote the file doc with an empty content_generation_id,
    // stranded the already-replicated chunks and reverted the browser file
    // viewer to an unreadable lazy state ~15s after every materialize
    // (rxdb-soak workspace-large-file-viewer-restart). Keep maintaining such
    // files eagerly; a content change re-chunks them below.
    let policy = if policy == DesktopFileContentPolicy::Lazy
        && existing_file_doc
            .as_ref()
            .and_then(|doc| doc.get("content_state"))
            .and_then(Value::as_str)
            == Some("available")
    {
        DesktopFileContentPolicy::Eager
    } else {
        policy
    };
    if let Some(doc) = existing_file_doc.as_ref() {
        let same_location = doc.get("parent_id").and_then(Value::as_str)
            == Some(parent_id.as_str())
            && doc.get("virtual_path").and_then(Value::as_str) == Some(display_path.as_str())
            && doc.get("path").and_then(Value::as_str) == Some(path_string.as_str());
        let same_stat = doc.get("mtime_ms").and_then(Value::as_u64)
            == u64::try_from(modified_at_ms).ok()
            && doc.get("size_bytes").and_then(Value::as_u64) == Some(metadata.len());
        let not_deleted = !doc
            .get("is_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let content_ready = match policy {
            DesktopFileContentPolicy::Eager => {
                doc.get("content_state").and_then(Value::as_str) == Some("available")
                    && doc
                        .get("content_generation_id")
                        .and_then(Value::as_str)
                        .is_some_and(|generation| !generation.is_empty())
            }
            DesktopFileContentPolicy::Lazy => {
                doc.get("content_state").and_then(Value::as_str) == Some("lazy")
            }
        };
        if same_location && same_stat && not_deleted && content_ready {
            // Self-healing is expensive for large materialized files, so use
            // the persisted verification marker first. Full chunk verification
            // is reserved for generations that have not been verified yet.
            let chunks_complete = match policy {
                DesktopFileContentPolicy::Eager => {
                    let generation = doc
                        .get("content_generation_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if desktop_file_generation_verified_by_metadata(doc, generation, metadata.len())
                    {
                        true
                    } else if desktop_file_chunk_generation_is_complete(
                        database,
                        &file_id,
                        generation,
                        metadata.len(),
                    )
                    .await
                    {
                        mark_desktop_file_chunk_generation_verified(
                            &files,
                            doc,
                            expected_desktop_file_chunk_total(metadata.len()),
                            now,
                        )
                        .await?;
                        true
                    } else {
                        false
                    }
                }
                DesktopFileContentPolicy::Lazy => true,
            };
            if chunks_complete {
                return Ok(());
            }
        }
    }

    let (content_hash, content_generation_id, active_generation_id) = if policy
        == DesktopFileContentPolicy::Eager
    {
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read desktop file {}", path.display()))?;
        let content_hash = hex_sha256(&bytes);
        // Same content as the indexed generation (e.g. touch / metadata
        // change): keep the replicated generation and its chunks instead
        // of rotating a byte-identical copy through the data plane.
        let reused_generation_id = existing_file_doc.as_ref().and_then(|doc| {
            if doc.get("content_hash").and_then(Value::as_str) != Some(content_hash.as_str())
                || doc.get("content_state").and_then(Value::as_str) != Some("available")
            {
                return None;
            }
            doc.get("content_generation_id")
                .and_then(Value::as_str)
                .filter(|generation| !generation.is_empty())
                .map(str::to_string)
        });
        // Reuse only a generation whose chunks are complete; otherwise
        // fall through to a full rewrite (self-healing repair).
        let mut reused_generation_id = reused_generation_id;
        if let Some(generation) = reused_generation_id.as_deref() {
            let metadata_verified = existing_file_doc.as_ref().is_some_and(|doc| {
                desktop_file_generation_verified_by_metadata(doc, generation, metadata.len())
            });
            if !metadata_verified
                && !desktop_file_chunk_generation_is_complete(
                    database,
                    &file_id,
                    generation,
                    metadata.len(),
                )
                .await
            {
                reused_generation_id = None;
            }
        }
        if let Some(generation_id) = reused_generation_id {
            (
                content_hash,
                Value::String(generation_id.clone()),
                Some(generation_id),
            )
        } else {
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
            let mut chunk_documents = Vec::with_capacity(chunk_payloads.len());
            for (idx, data) in chunk_payloads.into_iter().enumerate() {
                let chunk_hash = hex_sha256(data.as_bytes());
                chunk_documents.push(json!({
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
                }));
            }
            bulk_upsert_or_error(&chunks, chunk_documents, "upsert desktop file chunks").await?;
            (
                content_hash,
                Value::String(generation_id.clone()),
                Some(generation_id),
            )
        }
    } else {
        (
            format!("mtime:{modified_at_ms}:size:{}", metadata.len()),
            Value::Null,
            None,
        )
    };

    ensure_ctox_desktop_folder(database, now).await?;
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
            "chunk_count": if policy == DesktopFileContentPolicy::Eager {
                Value::from(expected_desktop_file_chunk_total(metadata.len()))
            } else {
                Value::Null
            },
            "generation_verified_at_ms": if policy == DesktopFileContentPolicy::Eager {
                Value::from(now_u64)
            } else {
                Value::Null
            },
            "mtime_ms": modified_at_ms,
            "content_synced_at_ms": content_synced_at_ms,
            "sort_index": now,
            "is_deleted": false,
            // Keep the original creation time stable across index updates.
            "created_at_ms": existing_file_doc
                .as_ref()
                .and_then(|doc| doc.get("created_at_ms"))
                .and_then(Value::as_u64)
                .map(Value::from)
                .unwrap_or_else(|| json!(now)),
            "updated_at_ms": now,
        }))
        .await
        .map_err(|err| anyhow::anyhow!("upsert desktop file row: {err}"))?;

    if let Some(active_generation_id) = active_generation_id.as_deref() {
        prune_desktop_file_chunk_generations(root, database, &file_id, active_generation_id)
            .await?;
    }

    if extension.eq_ignore_ascii_case("csv") {
        if let Err(err) =
            upsert_workspace_csv_spreadsheet(database, &path, &file_id, &display_path, now_u64)
                .await
        {
            eprintln!(
                "[business-os] published CSV to Files but skipped automatic Spreadsheet projection for {}: {err:#}",
                path.display()
            );
        }
    }

    Ok(())
}

async fn upsert_workspace_csv_spreadsheet(
    database: &Arc<RxDatabase>,
    path: &Path,
    desktop_file_id: &str,
    virtual_path: &str,
    now: u64,
) -> anyhow::Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read CSV metadata {}", path.display()))?;
    anyhow::ensure!(
        metadata.len() <= SPREADSHEET_CSV_IMPORT_LIMIT_BYTES,
        "CSV is too large for automatic Spreadsheet import ({} bytes, limit {}): {}",
        metadata.len(),
        SPREADSHEET_CSV_IMPORT_LIMIT_BYTES,
        path.display()
    );
    let bytes = fs::read(path).with_context(|| {
        format!(
            "failed to read CSV for Spreadsheet import {}",
            path.display()
        )
    })?;
    let delimiter = detect_csv_delimiter(&bytes);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(bytes.as_slice());
    let mut rows = Vec::<Vec<String>>::new();
    let mut column_count = 0usize;
    for record in reader.records() {
        let record = record.with_context(|| format!("failed to parse CSV {}", path.display()))?;
        anyhow::ensure!(
            record.len() <= SPREADSHEET_CSV_IMPORT_MAX_COLUMNS,
            "CSV has too many columns for automatic Spreadsheet import ({} > {}): {}",
            record.len(),
            SPREADSHEET_CSV_IMPORT_MAX_COLUMNS,
            path.display()
        );
        column_count = column_count.max(record.len());
        rows.push(record.iter().map(str::to_string).collect());
        anyhow::ensure!(
            rows.len() <= SPREADSHEET_CSV_IMPORT_MAX_ROWS,
            "CSV has too many rows for automatic Spreadsheet import (>{}): {}",
            SPREADSHEET_CSV_IMPORT_MAX_ROWS,
            path.display()
        );
    }
    anyhow::ensure!(
        !rows.is_empty(),
        "CSV is empty and cannot be imported into Spreadsheets: {}",
        path.display()
    );

    let content_hash = hex_sha256(&bytes);
    let path_hash = hex_sha256(path.to_string_lossy().as_bytes());
    let spreadsheet_id = format!("sheet_ctox_{}", &path_hash[..40]);
    let version_id = format!("{spreadsheet_id}_v_{}", &content_hash[..16]);
    let blob_id = format!("{version_id}_blob");
    let existing_spreadsheet =
        find_rxdb_document_by_id(database, "spreadsheets", &spreadsheet_id, false).await?;
    let existing_version_number = if let Some(current_version_id) = existing_spreadsheet
        .as_ref()
        .and_then(|record| record.get("current_version_id"))
        .and_then(Value::as_str)
    {
        find_rxdb_document_by_id(database, "spreadsheet_versions", current_version_id, false)
            .await?
            .and_then(|version| version.get("version").and_then(Value::as_u64))
            .unwrap_or_default()
    } else {
        0
    };
    let version_number = if existing_spreadsheet
        .as_ref()
        .and_then(|record| record.get("current_version_id"))
        .and_then(Value::as_str)
        == Some(version_id.as_str())
    {
        existing_version_number.max(1)
    } else {
        existing_version_number.saturating_add(1).max(1)
    };
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("export.csv");
    let title = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("CSV export")
        .replace(['_', '-'], " ");
    let columns = (0..column_count)
        .map(|index| {
            json!({
                "type": "text",
                "title": spreadsheet_column_label(index),
                "width": "120px"
            })
        })
        .collect::<Vec<_>>();
    let model_json = json!({
        "data": rows,
        "columns": columns,
        "nestedHeaders": Value::Null,
        "mergeCells": Value::Null,
        "style": Value::Null,
    });
    let index_text = model_json
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(10)
        .filter_map(Value::as_array)
        .map(|row| {
            row.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");

    let chunks = database
        .collection("spreadsheet_blob_chunks")
        .context("spreadsheet_blob_chunks collection is not registered")?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let encoded_chunks = if encoded.is_empty() {
        vec![""]
    } else {
        encoded
            .as_bytes()
            .chunks(SPREADSHEET_BLOB_CHUNK_SIZE)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
            .collect::<Vec<_>>()
    };
    let total = encoded_chunks.len();
    let chunk_documents = encoded_chunks
        .into_iter()
        .enumerate()
        .map(|(idx, data)| {
            json!({
                "id": format!("{blob_id}_{idx}"),
                "blob_id": blob_id,
                "spreadsheet_id": spreadsheet_id,
                "version_id": version_id,
                "idx": idx,
                "total": total,
                "mime_type": "text/csv",
                "encoding": "base64",
                "data": data,
                "created_at_ms": now,
            })
        })
        .collect::<Vec<_>>();
    bulk_upsert_or_error(
        &chunks,
        chunk_documents,
        "upsert workspace CSV Spreadsheet chunks",
    )
    .await?;

    database
        .collection("spreadsheet_versions")
        .context("spreadsheet_versions collection is not registered")?
        .incremental_upsert(json!({
            "id": version_id,
            "spreadsheet_id": spreadsheet_id,
            "version": version_number,
            "source_kind": "workspace_csv",
            "blob_id": blob_id,
            "model_json": model_json,
            "diagnostics": [],
            "created_at_ms": now,
            "updated_at_ms": now,
        }))
        .await
        .map_err(|err| anyhow::anyhow!("upsert workspace CSV Spreadsheet version: {err}"))?;
    database
        .collection("spreadsheets")
        .context("spreadsheets collection is not registered")?
        .incremental_upsert(json!({
            "id": spreadsheet_id,
            "title": title,
            "filename": filename,
            "mime_type": "text/csv",
            "status": "Imported",
            "spreadsheet_type": "jspreadsheet",
            "owner_id": "ctox",
            "current_version_id": version_id,
            "source_sha256": content_hash,
            "row_count": model_json.get("data").and_then(Value::as_array).map_or(0, Vec::len),
            "col_count": column_count,
            "diagnostics_count": 0,
            "linked_records": [{
                "collection": "desktop_files",
                "record_id": desktop_file_id,
                "path": virtual_path,
            }],
            "tags": ["ctox-export", "csv"],
            "display_cache": {},
            "index_text": format!("{title}\n{index_text}"),
            "is_deleted": false,
            "created_at_ms": existing_spreadsheet
                .as_ref()
                .and_then(|record| record.get("created_at_ms"))
                .and_then(Value::as_u64)
                .unwrap_or(now),
            "updated_at_ms": now,
        }))
        .await
        .map_err(|err| anyhow::anyhow!("upsert workspace CSV Spreadsheet record: {err}"))?;
    Ok(())
}

fn detect_csv_delimiter(bytes: &[u8]) -> u8 {
    let first_line = bytes
        .split(|byte| *byte == b'\n' || *byte == b'\r')
        .find(|line| !line.iter().all(u8::is_ascii_whitespace))
        .unwrap_or_default();
    [b',', b';', b'\t']
        .into_iter()
        .max_by_key(|delimiter| {
            first_line
                .iter()
                .filter(|byte| **byte == *delimiter)
                .count()
        })
        .unwrap_or(b',')
}

fn spreadsheet_column_label(mut index: usize) -> String {
    let mut label = String::new();
    loop {
        label.insert(0, char::from(b'A' + (index % 26) as u8));
        if index < 26 {
            return label;
        }
        index = (index / 26) - 1;
    }
}

/// Phase 4: file-demand sources exposed to the browser. The request collection
/// can be metadata (`desktop_files`) while the bytes still live in a separate
/// chunk collection (`desktop_file_chunks`); this keeps large payloads off the
/// normal background replication path.
struct DemandFileChunkCollection {
    request_collection: &'static str,
    storage_collection: &'static str,
    key_field: &'static str,
}

const DEMAND_FILE_CHUNK_COLLECTIONS: &[DemandFileChunkCollection] = &[
    DemandFileChunkCollection {
        request_collection: "desktop_files",
        storage_collection: "desktop_file_chunks",
        key_field: "file_id",
    },
    DemandFileChunkCollection {
        request_collection: "desktop_file_chunks",
        storage_collection: "desktop_file_chunks",
        key_field: "file_id",
    },
    DemandFileChunkCollection {
        request_collection: "document_blob_chunks",
        storage_collection: "document_blob_chunks",
        key_field: "blob_id",
    },
    DemandFileChunkCollection {
        request_collection: "spreadsheet_blob_chunks",
        storage_collection: "spreadsheet_blob_chunks",
        key_field: "blob_id",
    },
];

/// Phase 4: register a bounded-memory file stream source on the pool's file
/// fetch registry for each file-bearing chunk collection that is actually
/// registered on this database. Without this, `rxdb.file.fetch` always returns
/// FILE_NOT_FOUND (no source). The source closure is sync and reads the local
/// RxDB SQLite store through read-only queries; the file-fetch dispatcher runs
/// it on a blocking worker and applies async transport backpressure.
fn register_demand_file_sources(pool: &WebRtcPool, database: &Arc<RxDatabase>, root: &Path) {
    for source_config in DEMAND_FILE_CHUNK_COLLECTIONS {
        // Only register sources whose backing storage collection exists (the
        // catalog is fault-tolerant; optional chunk collections may be absent).
        if database
            .collection(source_config.storage_collection)
            .is_none()
        {
            continue;
        }
        let root = root.to_path_buf();
        let request_collection = source_config.request_collection;
        let storage_collection = source_config.storage_collection.to_string();
        let key_field = source_config.key_field.to_string();
        let closure_key_field = key_field.clone();
        let source: Arc<rxdb::plugins::replication_webrtc::file_fetch_handler::FileChunkStreamFn> =
            Arc::new(move |_collection, file_id, range, emit| {
                stream_demand_file_chunks(
                    &root,
                    &storage_collection,
                    &closure_key_field,
                    file_id,
                    range,
                    emit,
                )
            });
        pool.file_fetch_registry
            .register_stream_source(request_collection, source);
        eprintln!(
            "[business-os] demand-fetch file source registered for `{request_collection}` \
             via `{}` (key `{key_field}`)",
            source_config.storage_collection
        );
    }
}

/// Phase 4: stream the bytes of `file_id` from `collection`'s chunk documents.
/// Reads the chunk docs by the collection's key field, orders by `idx`,
/// base64-decodes each `data`, and emits one chunk of raw bytes at a time
/// (honoring an optional byte range). Returns `Err` when the collection is
/// missing or the query fails; emits nothing (→ FILE_NOT_FOUND upstream) when
/// the file has no chunks.
fn stream_demand_file_chunks(
    root: &Path,
    collection: &str,
    key_field: &str,
    file_id: &str,
    range: Option<&FileRange>,
    emit: &mut dyn FnMut(&[u8]) -> rxdb::rx_error::RxResult<bool>,
) -> rxdb::rx_error::RxResult<()> {
    let mut stats = DemandFileFetchRequestStats::new(range);
    let result = stream_demand_file_chunks_inner(
        root, collection, key_field, file_id, range, emit, &mut stats,
    );
    stats.finish();
    DEMAND_FILE_FETCH_METRICS.record(&stats, result.is_ok());
    result
}

fn stream_demand_file_chunks_inner(
    root: &Path,
    collection: &str,
    key_field: &str,
    file_id: &str,
    range: Option<&FileRange>,
    emit: &mut dyn FnMut(&[u8]) -> rxdb::rx_error::RxResult<bool>,
    stats: &mut DemandFileFetchRequestStats,
) -> rxdb::rx_error::RxResult<()> {
    let (mut chunk_rows, loaded_base_offset) = if collection == "desktop_file_chunks" {
        active_desktop_file_chunk_rows_from_sqlite(root, file_id, range, stats)?
    } else {
        (
            demand_file_chunk_rows_for_key_from_sqlite(
                root, collection, key_field, file_id, None, None, stats,
            )?,
            0,
        )
    };
    // Order by `idx` so the reassembled byte stream is correct.
    chunk_rows.sort_by_key(|chunk: &Value| chunk.get("idx").and_then(Value::as_u64).unwrap_or(0));

    // Range support: skip/take a byte window across the decoded chunk stream.
    let (range_start, range_end) = match range {
        Some(r) => (r.offset, r.offset.saturating_add(r.length)),
        None => (0u64, u64::MAX),
    };
    let mut emitted_offset: u64 = loaded_base_offset;
    for chunk in chunk_rows {
        // Skip redacted/pruned chunks (empty data) so they do not corrupt the
        // stream; the browser tracks presence separately.
        let data = chunk.get("data").and_then(Value::as_str).unwrap_or("");
        if data.is_empty() {
            continue;
        }
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .map_err(|err| {
                rxdb::rx_error::new_rx_error(
                    "RC_WEBRTC_PEER",
                    Some(json!({
                        "message": format!("decode {collection} chunk for {file_id}: {err}"),
                    })),
                )
            })?;
        stats.chunks_decoded = stats.chunks_decoded.saturating_add(1);
        stats.bytes_decoded = stats
            .bytes_decoded
            .saturating_add(u64::try_from(decoded.len()).unwrap_or(u64::MAX));
        let chunk_start = emitted_offset;
        let chunk_end = emitted_offset.saturating_add(decoded.len() as u64);
        emitted_offset = chunk_end;
        // Clip this chunk to the requested byte window.
        if chunk_end <= range_start || chunk_start >= range_end {
            continue;
        }
        let slice_start = range_start.saturating_sub(chunk_start) as usize;
        let slice_end = (range_end.min(chunk_end) - chunk_start) as usize;
        let slice = &decoded[slice_start.min(decoded.len())..slice_end.min(decoded.len())];
        if slice.is_empty() {
            continue;
        }
        // `emit` returns Ok(false) to stop early (cancel / known-sequence skip).
        if !emit(slice)? {
            break;
        }
        stats.bytes_emitted = stats
            .bytes_emitted
            .saturating_add(u64::try_from(slice.len()).unwrap_or(u64::MAX));
    }
    Ok(())
}

fn desktop_file_chunk_index_window(size_bytes: u64, range: Option<&FileRange>) -> (u64, u64) {
    let expected_total = expected_desktop_file_chunk_total(size_bytes);
    let Some(range) = range else {
        return (0, expected_total);
    };
    if range.length == 0 || range.offset >= size_bytes {
        return (0, 0);
    }
    let end = range.offset.saturating_add(range.length).min(size_bytes);
    if end <= range.offset {
        return (0, 0);
    }
    let start_idx = range.offset / DESKTOP_FILE_CHUNK_DECODED_SIZE;
    let end_idx = end
        .saturating_sub(1)
        .checked_div(DESKTOP_FILE_CHUNK_DECODED_SIZE)
        .unwrap_or(0)
        .saturating_add(1)
        .min(expected_total);
    (start_idx.min(expected_total), end_idx)
}

struct DesktopFileDemandMetadata {
    generation_id: String,
    size_bytes: u64,
    content_hash: String,
}

fn demand_file_source_error(message: impl Into<String>) -> rxdb::rx_error::RxError {
    rxdb::rx_error::new_rx_error("RC_WEBRTC_PEER", Some(json!({ "message": message.into() })))
}

fn active_desktop_file_chunk_rows_from_sqlite(
    root: &Path,
    file_id: &str,
    range: Option<&FileRange>,
    stats: &mut DemandFileFetchRequestStats,
) -> rxdb::rx_error::RxResult<(Vec<Value>, u64)> {
    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok((Vec::new(), 0));
    }
    let conn = Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|err| {
            demand_file_source_error(format!(
                "open RxDB store {} for desktop file fetch: {err}",
                database_path.display()
            ))
        })?;
    if !sqlite_table_exists(&conn, "ctox_business_os__desktop_files__v0")
        .map_err(|err| demand_file_source_error(format!("inspect desktop_files table: {err}")))?
        || !sqlite_table_exists(&conn, "ctox_business_os__desktop_file_chunks__v0").map_err(
            |err| demand_file_source_error(format!("inspect desktop_file_chunks table: {err}")),
        )?
    {
        return Ok((Vec::new(), 0));
    }
    let Some(metadata) = active_desktop_file_metadata_from_sqlite(&conn, file_id)? else {
        return Ok((Vec::new(), 0));
    };
    let index_window = desktop_file_chunk_index_window(metadata.size_bytes, range);
    if index_window.0 >= index_window.1 {
        return Ok((Vec::new(), 0));
    }
    let loaded_base_offset = index_window
        .0
        .saturating_mul(DESKTOP_FILE_CHUNK_DECODED_SIZE);
    let canonical = desktop_file_chunk_rows_by_id_from_sqlite(
        &conn,
        file_id,
        &metadata.generation_id,
        metadata.size_bytes,
        index_window,
        stats,
    )?;
    let expected_total = expected_desktop_file_chunk_total(metadata.size_bytes);
    let expected_range_total = index_window.1.saturating_sub(index_window.0);
    let canonical = dedupe_desktop_file_chunks_by_idx(
        canonical,
        expected_total,
        index_window,
        metadata.size_bytes,
        &metadata.content_hash,
    );
    if u64::try_from(canonical.len()).unwrap_or_default() >= expected_range_total {
        return Ok((canonical, loaded_base_offset));
    }

    let fallback = demand_file_chunk_rows_for_key_from_sqlite(
        root,
        "desktop_file_chunks",
        "file_id",
        file_id,
        Some(metadata.generation_id.as_str()),
        Some(index_window),
        stats,
    )?
    .into_iter()
    .filter(|chunk| {
        chunk.get("generation_id").and_then(Value::as_str) == Some(metadata.generation_id.as_str())
            && chunk
                .get("idx")
                .and_then(Value::as_u64)
                .is_some_and(|idx| idx >= index_window.0 && idx < index_window.1)
    })
    .collect::<Vec<_>>();
    let fallback = dedupe_desktop_file_chunks_by_idx(
        fallback,
        expected_total,
        index_window,
        metadata.size_bytes,
        &metadata.content_hash,
    );
    if u64::try_from(fallback.len()).unwrap_or_default() >= expected_range_total {
        return Ok((fallback, loaded_base_offset));
    }

    if !metadata.content_hash.is_empty() {
        let equivalent = equivalent_desktop_file_chunk_rows_from_sqlite(
            &conn,
            file_id,
            &metadata,
            index_window,
            stats,
        )?;
        if u64::try_from(equivalent.len()).unwrap_or_default() >= expected_range_total {
            return Ok((equivalent, loaded_base_offset));
        }
    }
    Ok((canonical, loaded_base_offset))
}

fn dedupe_desktop_file_chunks_by_idx(
    chunks: Vec<Value>,
    expected_total: u64,
    index_window: (u64, u64),
    size_bytes: u64,
    content_hash: &str,
) -> Vec<Value> {
    let mut by_idx: BTreeMap<u64, Value> = BTreeMap::new();
    for chunk in chunks {
        let Some(idx) = chunk.get("idx").and_then(Value::as_u64) else {
            continue;
        };
        if idx < index_window.0 || idx >= index_window.1 || idx >= expected_total {
            continue;
        }
        if desktop_file_chunk_stream_score(&chunk, expected_total, size_bytes, content_hash)
            .is_none()
        {
            continue;
        }
        match by_idx.get(&idx) {
            Some(previous)
                if desktop_file_chunk_stream_score(
                    &chunk,
                    expected_total,
                    size_bytes,
                    content_hash,
                ) >= desktop_file_chunk_stream_score(
                    previous,
                    expected_total,
                    size_bytes,
                    content_hash,
                ) => {}
            _ => {
                by_idx.insert(idx, chunk);
            }
        }
    }
    by_idx.into_values().collect()
}

fn desktop_file_chunk_stream_score(
    chunk: &Value,
    expected_total: u64,
    size_bytes: u64,
    content_hash: &str,
) -> Option<u8> {
    if chunk
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or("base64")
        != "base64"
    {
        return None;
    }
    if !content_hash.is_empty()
        && chunk
            .get("content_hash")
            .and_then(Value::as_str)
            .is_some_and(|hash| hash != content_hash)
    {
        return None;
    }
    let data = chunk.get("data").and_then(Value::as_str).unwrap_or("");
    if size_bytes > 0 && data.is_empty() {
        return None;
    }
    if let Some(size) = chunk.get("size_bytes").and_then(Value::as_u64) {
        if size != data.len() as u64 {
            return None;
        }
    }
    if let Some(chunk_hash) = chunk.get("chunk_hash").and_then(Value::as_str) {
        if hex_sha256(data.as_bytes()) != chunk_hash {
            return None;
        }
    }
    let mut score = 0_u8;
    let chunk_total = chunk
        .get("total")
        .and_then(Value::as_u64)
        .unwrap_or(expected_total);
    if chunk_total != expected_total {
        score = score.saturating_add(if chunk_total > expected_total { 1 } else { 8 });
    }
    Some(score)
}

fn equivalent_desktop_file_chunk_rows_from_sqlite(
    conn: &Connection,
    file_id: &str,
    metadata: &DesktopFileDemandMetadata,
    index_window: (u64, u64),
    stats: &mut DemandFileFetchRequestStats,
) -> rxdb::rx_error::RxResult<Vec<Value>> {
    let expected_total = expected_desktop_file_chunk_total(metadata.size_bytes);
    let expected_range_total = index_window.1.saturating_sub(index_window.0);
    let mut stmt = conn
        .prepare(
            "SELECT id, data FROM ctox_business_os__desktop_files__v0 \
             WHERE id != ?1 AND COALESCE(deleted, 0) = 0",
        )
        .map_err(|err| {
            demand_file_source_error(format!("prepare equivalent file lookup: {err}"))
        })?;
    let rows = stmt
        .query_map([file_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|err| demand_file_source_error(format!("query equivalent files: {err}")))?;

    let mut candidates = Vec::new();
    for row in rows {
        let (candidate_id, raw) =
            row.map_err(|err| demand_file_source_error(format!("load equivalent file: {err}")))?;
        let value = serde_json::from_str::<Value>(&raw).map_err(|err| {
            demand_file_source_error(format!("decode equivalent file {candidate_id}: {err}"))
        })?;
        if value.get("content_state").and_then(Value::as_str) != Some("available") {
            continue;
        }
        if value.get("kind").and_then(Value::as_str).unwrap_or("file") != "file" {
            continue;
        }
        if value
            .get("content_hash")
            .and_then(Value::as_str)
            .map(str::trim)
            != Some(metadata.content_hash.as_str())
        {
            continue;
        }
        if value
            .get("size_bytes")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            != metadata.size_bytes
        {
            continue;
        }
        let generation_id = value
            .get("content_generation_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if generation_id.is_empty() {
            continue;
        }
        candidates.push((candidate_id, generation_id));
    }

    for (candidate_id, generation_id) in candidates {
        let chunks = desktop_file_chunk_rows_by_id_from_sqlite(
            conn,
            &candidate_id,
            &generation_id,
            metadata.size_bytes,
            index_window,
            stats,
        )?;
        let chunks = dedupe_desktop_file_chunks_by_idx(
            chunks,
            expected_total,
            index_window,
            metadata.size_bytes,
            &metadata.content_hash,
        );
        if u64::try_from(chunks.len()).unwrap_or_default() >= expected_range_total {
            return Ok(chunks);
        }
    }

    Ok(Vec::new())
}

fn active_desktop_file_metadata_from_sqlite(
    conn: &Connection,
    file_id: &str,
) -> rxdb::rx_error::RxResult<Option<DesktopFileDemandMetadata>> {
    let file_json = conn
        .query_row(
            "SELECT data FROM ctox_business_os__desktop_files__v0 \
             WHERE id = ?1 AND COALESCE(deleted, 0) = 0",
            [file_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|err| demand_file_source_error(format!("read desktop file {file_id}: {err}")))?;
    let Some(file_json) = file_json else {
        return Ok(None);
    };
    let file_row: Value = serde_json::from_str(&file_json).map_err(|err| {
        demand_file_source_error(format!("decode desktop file {file_id} metadata: {err}"))
    })?;
    if file_row.get("content_state").and_then(Value::as_str) != Some("available") {
        return Ok(None);
    }
    let generation_id = file_row
        .get("content_generation_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if generation_id.is_empty() {
        return Ok(None);
    }
    Ok(Some(DesktopFileDemandMetadata {
        generation_id: generation_id.to_string(),
        size_bytes: file_row
            .get("size_bytes")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        content_hash: file_row
            .get("content_hash")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }))
}

fn desktop_file_chunk_rows_by_id_from_sqlite(
    conn: &Connection,
    file_id: &str,
    generation_id: &str,
    size_bytes: u64,
    index_window: (u64, u64),
    stats: &mut DemandFileFetchRequestStats,
) -> rxdb::rx_error::RxResult<Vec<Value>> {
    let expected_total = expected_desktop_file_chunk_total(size_bytes);
    let mut rows = desktop_file_chunk_rows_by_row_id_from_sqlite(
        conn,
        file_id,
        generation_id,
        expected_total,
        index_window,
        stats,
    )?;
    let expected_range_total = index_window.1.saturating_sub(index_window.0);
    if u64::try_from(rows.len()).unwrap_or_default() >= expected_range_total {
        return Ok(rows);
    }
    let start_idx = i64::try_from(index_window.0).map_err(|err| {
        demand_file_source_error(format!(
            "desktop file chunk start overflow for {file_id}: {err}"
        ))
    })?;
    let end_idx = i64::try_from(index_window.1).map_err(|err| {
        demand_file_source_error(format!(
            "desktop file chunk end overflow for {file_id}: {err}"
        ))
    })?;
    let mut stmt = conn
        .prepare(
            "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 \
             WHERE COALESCE(deleted, 0) = 0 \
               AND json_extract(data, '$.file_id') = ?1 \
               AND json_extract(data, '$.generation_id') = ?2 \
               AND json_extract(data, '$.idx') >= ?3 \
               AND json_extract(data, '$.idx') < ?4 \
             ORDER BY json_extract(data, '$.idx') ASC",
        )
        .map_err(|err| demand_file_source_error(format!("prepare chunk lookup: {err}")))?;
    let query_rows = stmt
        .query_map(params![file_id, generation_id, start_idx, end_idx], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|err| demand_file_source_error(format!("query desktop file chunks: {err}")))?;
    rows.reserve(usize::try_from(index_window.1.saturating_sub(index_window.0)).unwrap_or(0));
    for row in query_rows {
        let raw =
            row.map_err(|err| demand_file_source_error(format!("load desktop file chunk: {err}")))?;
        stats.rows_loaded = stats.rows_loaded.saturating_add(1);
        let value = serde_json::from_str::<Value>(&raw).map_err(|err| {
            demand_file_source_error(format!("decode desktop file chunk for {file_id}: {err}"))
        })?;
        rows.push(value);
    }
    rows.retain(|chunk| {
        chunk.get("file_id").and_then(Value::as_str) == Some(file_id)
            && chunk.get("generation_id").and_then(Value::as_str) == Some(generation_id)
            && chunk
                .get("idx")
                .and_then(Value::as_u64)
                .is_some_and(|idx| idx < expected_total)
    });
    Ok(rows)
}

fn desktop_file_chunk_rows_by_row_id_from_sqlite(
    conn: &Connection,
    file_id: &str,
    generation_id: &str,
    expected_total: u64,
    index_window: (u64, u64),
    stats: &mut DemandFileFetchRequestStats,
) -> rxdb::rx_error::RxResult<Vec<Value>> {
    let mut rows = Vec::with_capacity(
        usize::try_from(index_window.1.saturating_sub(index_window.0)).unwrap_or(0),
    );
    let canonical_prefix = format!("{file_id}_{generation_id}_");
    let canonical_upper = format!("{canonical_prefix}`");
    let legacy_prefix = format!("{file_id}_");
    let legacy_upper = format!("{legacy_prefix}\u{10ffff}");
    for (lower, upper) in [
        (canonical_prefix.as_str(), canonical_upper.as_str()),
        (legacy_prefix.as_str(), legacy_upper.as_str()),
    ] {
        let mut stmt = conn
            .prepare(
                "SELECT data FROM ctox_business_os__desktop_file_chunks__v0 \
                 WHERE id >= ?1 AND id < ?2 AND COALESCE(deleted, 0) = 0 \
                 ORDER BY id ASC",
            )
            .map_err(|err| {
                demand_file_source_error(format!("prepare chunk row-id lookup: {err}"))
            })?;
        let query_rows = stmt
            .query_map(params![lower, upper], |row| row.get::<_, String>(0))
            .map_err(|err| {
                demand_file_source_error(format!("query desktop file chunks by row id: {err}"))
            })?;
        for row in query_rows {
            let raw = row.map_err(|err| {
                demand_file_source_error(format!("load desktop file chunk by row id: {err}"))
            })?;
            stats.rows_loaded = stats.rows_loaded.saturating_add(1);
            let value = serde_json::from_str::<Value>(&raw).map_err(|err| {
                demand_file_source_error(format!(
                    "decode desktop file chunk by row id for {file_id}: {err}"
                ))
            })?;
            rows.push(value);
        }
    }
    rows.retain(|chunk| {
        chunk.get("file_id").and_then(Value::as_str) == Some(file_id)
            && chunk.get("generation_id").and_then(Value::as_str) == Some(generation_id)
            && chunk.get("idx").and_then(Value::as_u64).is_some_and(|idx| {
                idx < expected_total && idx >= index_window.0 && idx < index_window.1
            })
    });
    Ok(rows)
}

fn demand_file_chunk_rows_for_key_from_sqlite(
    root: &Path,
    collection: &str,
    key_field: &str,
    file_id: &str,
    generation_id: Option<&str>,
    index_window: Option<(u64, u64)>,
    stats: &mut DemandFileFetchRequestStats,
) -> rxdb::rx_error::RxResult<Vec<Value>> {
    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|err| {
            demand_file_source_error(format!(
                "open RxDB store {} for file fetch: {err}",
                database_path.display()
            ))
        })?;
    let table = rxdb_collection_version_table_name(collection, 0);
    if !sqlite_table_exists(&conn, &table)
        .map_err(|err| demand_file_source_error(format!("inspect {table} table: {err}")))?
    {
        return Ok(Vec::new());
    }
    if !key_field
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(demand_file_source_error(format!(
            "invalid demand-fetch key field {key_field}"
        )));
    }
    let quoted_table = sqlite_quote_identifier(&table);
    let (chunk_id_lower, chunk_id_upper) = chunk_id_prefix_bounds(file_id);
    let query_params = vec![
        SqlValue::Text(chunk_id_lower),
        SqlValue::Text(chunk_id_upper),
        SqlValue::Integer(i64::try_from(DESKTOP_FILE_CHUNK_CLEANUP_SCAN_LIMIT).unwrap_or(i64::MAX)),
    ];
    let mut stmt = conn
        .prepare(&format!(
            "SELECT data FROM {quoted_table}
             WHERE id >= ?1
               AND id < ?2
               AND deleted = 0
             ORDER BY id
             LIMIT ?3",
        ))
        .map_err(|err| demand_file_source_error(format!("prepare {collection} fetch: {err}")))?;
    let rows = stmt
        .query_map(params_from_iter(query_params), |row| {
            row.get::<_, String>(0)
        })
        .map_err(|err| demand_file_source_error(format!("query {collection} chunks: {err}")))?;
    let mut chunks = Vec::new();
    for row in rows {
        let raw =
            row.map_err(|err| demand_file_source_error(format!("read {collection} chunk: {err}")))?;
        stats.rows_loaded = stats.rows_loaded.saturating_add(1);
        let value = serde_json::from_str::<Value>(&raw).map_err(|err| {
            demand_file_source_error(format!("decode {collection} chunk for {file_id}: {err}"))
        })?;
        let key_matches = value.get(key_field).and_then(Value::as_str) == Some(file_id);
        let generation_matches = generation_id.is_none_or(|expected| {
            value.get("generation_id").and_then(Value::as_str) == Some(expected)
        });
        let index_matches = index_window.is_none_or(|(start_idx, end_idx)| {
            value
                .get("idx")
                .and_then(Value::as_u64)
                .is_some_and(|idx| idx >= start_idx && idx < end_idx)
        });
        if key_matches && generation_matches && index_matches {
            chunks.push(value);
        }
    }
    Ok(chunks)
}

fn desktop_file_chunk_rows_for_file_id(root: &Path, file_id: &str) -> anyhow::Result<Vec<Value>> {
    const CHUNKS_TABLE: &str = "\"ctox_business_os__desktop_file_chunks__v0\"";

    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open RxDB store {}", database_path.display()))?;
    if !sqlite_table_exists(&conn, "ctox_business_os__desktop_file_chunks__v0")? {
        return Ok(Vec::new());
    }
    let (chunk_id_lower, chunk_id_upper) = desktop_file_chunk_id_bounds(file_id);
    let mut stmt = conn.prepare(&format!(
        "SELECT data FROM {CHUNKS_TABLE}
         WHERE id >= ?1
           AND id < ?2
           AND COALESCE(deleted, 0) = 0
         LIMIT ?3"
    ))?;
    let rows = stmt.query_map(
        params![
            chunk_id_lower,
            chunk_id_upper,
            DESKTOP_FILE_CHUNK_CLEANUP_SCAN_LIMIT as i64
        ],
        |row| row.get::<_, String>(0),
    )?;
    let mut chunks = Vec::new();
    for row in rows {
        let raw = row?;
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if value.get("file_id").and_then(Value::as_str) == Some(file_id) {
            chunks.push(value);
        }
    }
    Ok(chunks)
}

async fn prune_desktop_file_chunk_generations(
    root: &Path,
    database: &Arc<RxDatabase>,
    file_id: &str,
    active_generation_id: &str,
) -> anyhow::Result<usize> {
    let chunks = database
        .collection("desktop_file_chunks")
        .context("desktop_file_chunks collection is not registered")?;
    let chunk_rows = desktop_file_chunk_rows_for_file_id(root, file_id)?;
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
    let mut pruned_chunks = Vec::with_capacity(stale_chunks.len());
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
        pruned_chunks.push(chunk);
    }
    bulk_upsert_or_error(&chunks, pruned_chunks, "redact stale desktop file chunks").await?;
    Ok(removed)
}

async fn bulk_upsert_or_error(
    collection: &Arc<RxCollection>,
    documents: Vec<Value>,
    context: &str,
) -> anyhow::Result<()> {
    if documents.is_empty() {
        return Ok(());
    }
    let result = collection
        .bulk_upsert(documents)
        .await
        .map_err(|err| anyhow::anyhow!("{context}: {err}"))?;
    if let Some(error) = result.error.first() {
        anyhow::bail!(
            "{context}: {} write error(s), first document {} status {}",
            result.error.len(),
            error.document_id,
            error.status
        );
    }
    Ok(())
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
        let existing_folder =
            find_rxdb_document_by_id(database, "desktop_files", &folder_id, false)
                .await
                .map_err(|err| anyhow::anyhow!("read CTOX desktop folder {virtual_path}: {err}"))?;
        if existing_folder.as_ref().is_some_and(|doc| {
            desktop_folder_doc_is_current(doc, &folder_id, &parent_id, &virtual_path, name)
        }) {
            parent_id = folder_id.clone();
            continue;
        }
        let created_at_ms = existing_folder
            .as_ref()
            .and_then(|doc| doc.get("created_at_ms"))
            .and_then(Value::as_u64)
            .map(Value::from)
            .unwrap_or_else(|| json!(now));
        let sort_index = if idx == 0 {
            Value::from(5_u64)
        } else {
            existing_folder
                .as_ref()
                .and_then(|doc| doc.get("sort_index"))
                .and_then(Value::as_u64)
                .map(Value::from)
                .unwrap_or_else(|| Value::from(u64::try_from(now).unwrap_or(u64::MAX)))
        };
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
                "sort_index": sort_index,
                "is_deleted": false,
                "created_at_ms": created_at_ms,
                "updated_at_ms": now,
            }))
            .await
            .map_err(|err| anyhow::anyhow!("upsert CTOX desktop folder {virtual_path}: {err}"))?;
        parent_id = folder_id.clone();
    }
    Ok(folder_id)
}

fn desktop_folder_doc_is_current(
    doc: &Value,
    folder_id: &str,
    parent_id: &str,
    virtual_path: &str,
    name: &str,
) -> bool {
    doc.get("id").and_then(Value::as_str) == Some(folder_id)
        && doc.get("parent_id").and_then(Value::as_str) == Some(parent_id)
        && doc.get("path").and_then(Value::as_str) == Some(virtual_path)
        && doc.get("virtual_path").and_then(Value::as_str) == Some(virtual_path)
        && doc.get("local_path").and_then(Value::as_str) == Some("")
        && doc.get("name").and_then(Value::as_str) == Some(name)
        && doc.get("kind").and_then(Value::as_str) == Some("folder")
        && doc.get("source").and_then(Value::as_str) == Some("ctox-core")
        && doc.get("content_ref").and_then(Value::as_str) == Some("")
        && doc.get("content_state").and_then(Value::as_str) == Some("directory")
        && doc.get("content_hash").and_then(Value::as_str) == Some("")
        && !doc
            .get("is_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

async fn sync_desktop_file_index_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
) -> anyhow::Result<usize> {
    sync_desktop_file_scan_roots_with_database(root, database, desktop_file_scan_roots(root)).await
}

async fn sync_desktop_file_index_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    last_projection_stamp: &mut Option<DesktopFileIndexProjectionStamp>,
) -> anyhow::Result<usize> {
    sync_desktop_file_scan_roots_with_database_if_changed(
        root,
        database,
        desktop_file_scan_roots(root),
        last_projection_stamp,
    )
    .await
}

async fn sync_desktop_file_scan_roots_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
    scan_roots: Vec<DesktopFileScanRoot>,
) -> anyhow::Result<usize> {
    let scan = collect_desktop_file_index_scan(scan_roots).await?;
    sync_desktop_file_scan_with_database(root, database, scan).await
}

async fn sync_desktop_file_scan_roots_with_database_if_changed(
    root: &Path,
    database: &Arc<RxDatabase>,
    scan_roots: Vec<DesktopFileScanRoot>,
    last_projection_stamp: &mut Option<DesktopFileIndexProjectionStamp>,
) -> anyhow::Result<usize> {
    let scan = collect_desktop_file_index_scan(scan_roots).await?;
    if last_projection_stamp.as_ref() == Some(&scan.stamp) {
        return Ok(0);
    }

    let projection_stamp = scan.stamp.clone();
    let indexed = sync_desktop_file_scan_with_database(root, database, scan).await?;
    *last_projection_stamp = Some(projection_stamp);
    Ok(indexed)
}

async fn sync_desktop_file_scan_with_database(
    root: &Path,
    database: &Arc<RxDatabase>,
    scan: DesktopFileIndexScan,
) -> anyhow::Result<usize> {
    let candidate_count = scan.candidates.len();
    let mut seen_file_ids = HashSet::with_capacity(scan.candidates.len());
    let mut indexed = 0usize;

    // Acquire write lock specifically for the DB write iteration
    let _write_guard = NATIVE_RXDB_WRITE_LOCK.lock().await;
    for candidate in scan.candidates {
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
            root,
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
        mark_missing_scanned_desktop_files(root, database, &scan.scan_roots, &seen_file_ids)
            .await?;
    }
    Ok(indexed)
}

#[derive(Debug, Default)]
struct DesktopFileIndexMaintenanceStats {
    tombstoned_unsafe_files: usize,
    removed_unsafe_chunks: usize,
    removed_stale_chunks: usize,
    removed_deleted_chunks: usize,
    removed_unsafe_file_tombstones: usize,
    evicted_cache_files: usize,
    removed_cache_chunks: usize,
    removed_cache_bytes: u64,
    cache_live_bytes_before: u64,
    cache_live_bytes_after: u64,
    cache_pinned_bytes: u64,
    cache_over_quota_pinned_bytes: u64,
    wal_checkpoint_ran: bool,
    vacuum_ran: bool,
}

impl DesktopFileIndexMaintenanceStats {
    fn changed(&self) -> bool {
        self.tombstoned_unsafe_files > 0
            || self.removed_unsafe_chunks > 0
            || self.removed_stale_chunks > 0
            || self.removed_deleted_chunks > 0
            || self.removed_unsafe_file_tombstones > 0
            || self.evicted_cache_files > 0
            || self.removed_cache_chunks > 0
            || self.wal_checkpoint_ran
            || self.vacuum_ran
    }
}

fn log_desktop_file_index_maintenance_stats(stats: &DesktopFileIndexMaintenanceStats) {
    eprintln!(
        "[business-os] desktop file index maintenance: tombstoned {} unsafe file(s), \
         removed {} unsafe chunk(s), {} stale chunk(s), {} deleted chunk tombstone(s), \
         {} unsafe file tombstone(s), evicted {} cached file(s), removed {} cache chunk(s) \
         ({} byte(s), live {} -> {}, pinned {}, over-quota pinned {}, checkpoint {}, vacuum {})",
        stats.tombstoned_unsafe_files,
        stats.removed_unsafe_chunks,
        stats.removed_stale_chunks,
        stats.removed_deleted_chunks,
        stats.removed_unsafe_file_tombstones,
        stats.evicted_cache_files,
        stats.removed_cache_chunks,
        stats.removed_cache_bytes,
        stats.cache_live_bytes_before,
        stats.cache_live_bytes_after,
        stats.cache_pinned_bytes,
        stats.cache_over_quota_pinned_bytes,
        stats.wal_checkpoint_ran,
        stats.vacuum_ran
    );
}

#[derive(Debug, Clone, Copy)]
struct DesktopFileChunkCacheConfig {
    max_live_bytes: u64,
    target_live_bytes: u64,
    active_min_age_secs: u64,
    max_files_per_pass: usize,
    max_chunks_per_pass: usize,
    checkpoint_min_interval_secs: u64,
    wal_checkpoint_min_bytes: u64,
    vacuum_min_interval_secs: u64,
    vacuum_min_reclaim_bytes: u64,
}

impl Default for DesktopFileChunkCacheConfig {
    fn default() -> Self {
        Self {
            max_live_bytes: DESKTOP_FILE_CHUNK_CACHE_MAX_LIVE_BYTES,
            target_live_bytes: DESKTOP_FILE_CHUNK_CACHE_TARGET_LIVE_BYTES,
            active_min_age_secs: DESKTOP_FILE_CHUNK_CACHE_ACTIVE_MIN_AGE_SECS,
            max_files_per_pass: DESKTOP_FILE_INDEX_MAINTENANCE_FILE_LIMIT,
            max_chunks_per_pass: DESKTOP_FILE_INDEX_MAINTENANCE_CHUNK_DELETE_LIMIT,
            checkpoint_min_interval_secs: DESKTOP_FILE_CHUNK_CACHE_CHECKPOINT_MIN_INTERVAL_SECS,
            wal_checkpoint_min_bytes: DESKTOP_FILE_CHUNK_CACHE_WAL_CHECKPOINT_MIN_BYTES,
            vacuum_min_interval_secs: DESKTOP_FILE_CHUNK_CACHE_VACUUM_MIN_INTERVAL_SECS,
            vacuum_min_reclaim_bytes: DESKTOP_FILE_CHUNK_CACHE_VACUUM_MIN_RECLAIM_BYTES,
        }
    }
}

impl DesktopFileChunkCacheConfig {
    fn normalized(self) -> Self {
        let max_live_bytes = self.max_live_bytes.max(1);
        let target_live_bytes = self.target_live_bytes.min(max_live_bytes);
        Self {
            max_live_bytes,
            target_live_bytes,
            active_min_age_secs: self.active_min_age_secs,
            max_files_per_pass: self.max_files_per_pass.max(1),
            max_chunks_per_pass: self.max_chunks_per_pass.max(1),
            checkpoint_min_interval_secs: self.checkpoint_min_interval_secs,
            wal_checkpoint_min_bytes: self.wal_checkpoint_min_bytes,
            vacuum_min_interval_secs: self.vacuum_min_interval_secs,
            vacuum_min_reclaim_bytes: self.vacuum_min_reclaim_bytes,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct DesktopFileChunkCacheState {
    last_eviction_at_ms: u64,
    last_checkpoint_at_ms: u64,
    last_vacuum_at_ms: u64,
    last_live_bytes: u64,
    last_pinned_bytes: u64,
    last_deleted_bytes: u64,
    last_deleted_chunks: u64,
}

async fn compact_desktop_file_index_store(
    root: &Path,
) -> anyhow::Result<DesktopFileIndexMaintenanceStats> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || compact_desktop_file_index_store_sync(&root, None))
        .await
        .context("join desktop file index maintenance")?
}

fn compact_desktop_file_index_store_sync(
    root: &Path,
    home: Option<&Path>,
) -> anyhow::Result<DesktopFileIndexMaintenanceStats> {
    compact_desktop_file_index_store_sync_with_config(
        root,
        home,
        DesktopFileChunkCacheConfig::default(),
    )
}

fn compact_desktop_file_index_store_sync_with_config(
    root: &Path,
    home: Option<&Path>,
    cache_config: DesktopFileChunkCacheConfig,
) -> anyhow::Result<DesktopFileIndexMaintenanceStats> {
    const FILES_TABLE: &str = "\"ctox_business_os__desktop_files__v0\"";
    const CHUNKS_TABLE: &str = "\"ctox_business_os__desktop_file_chunks__v0\"";

    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok(DesktopFileIndexMaintenanceStats::default());
    }
    let mut conn = Connection::open(&database_path)
        .with_context(|| format!("open RxDB store {}", database_path.display()))?;
    conn.busy_timeout(Duration::from_secs(10))
        .context("set RxDB maintenance busy timeout")?;
    let has_tables = sqlite_table_exists(&conn, "ctox_business_os__desktop_files__v0")?
        && sqlite_table_exists(&conn, "ctox_business_os__desktop_file_chunks__v0")?;
    if !has_tables {
        return Ok(DesktopFileIndexMaintenanceStats::default());
    }
    ensure_desktop_file_index_query_indexes(&conn)?;

    let unsafe_files = {
        let unsafe_candidates_sql = unsafe_desktop_file_index_candidates_sql(FILES_TABLE);
        let mut stmt = conn.prepare(&unsafe_candidates_sql)?;
        let rows = stmt.query_map(
            params![DESKTOP_FILE_INDEX_MAINTENANCE_FILE_LIMIT as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?;
        let mut unsafe_files = Vec::new();
        for row in rows {
            let (id, revision, data) = row?;
            let Ok(document) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            if desktop_file_index_document_is_unsafe(&document, home) {
                unsafe_files.push((id, revision, document));
                if unsafe_files.len() >= DESKTOP_FILE_INDEX_MAINTENANCE_FILE_LIMIT {
                    break;
                }
            }
        }
        unsafe_files
    };

    let tx = conn.transaction()?;
    let now = now_ms() as f64;
    let mut stats = DesktopFileIndexMaintenanceStats::default();
    for (file_id, revision, document) in &unsafe_files {
        let mut document = document.clone();
        let next_revision = maintenance_revision(
            document
                .get("_rev")
                .and_then(Value::as_str)
                .or(revision.as_deref()),
        );
        prepare_unsafe_desktop_file_tombstone(&mut document, &next_revision, now);
        let data = serde_json::to_string(&document)?;
        let changed = tx.execute(
            &format!(
                "UPDATE {FILES_TABLE}
                 SET revision = ?2, deleted = 1, lastWriteTime = ?3, data = ?4
                 WHERE id = ?1"
            ),
            params![file_id, next_revision, now, data],
        )?;
        if changed > 0 {
            stats.tombstoned_unsafe_files += changed;
        }
    }

    let mut remaining_chunk_delete_limit = DESKTOP_FILE_INDEX_MAINTENANCE_CHUNK_DELETE_LIMIT;
    for (file_id, _, _) in &unsafe_files {
        if remaining_chunk_delete_limit == 0 {
            break;
        }
        let (chunk_id_lower, chunk_id_upper) = desktop_file_chunk_id_bounds(file_id);
        let removed = tx.execute(
            &format!(
                "DELETE FROM {CHUNKS_TABLE}
                 WHERE rowid IN (
                   SELECT rowid FROM {CHUNKS_TABLE}
                   WHERE id >= ?1 AND id < ?2
                   LIMIT ?3
                 )"
            ),
            params![
                chunk_id_lower,
                chunk_id_upper,
                remaining_chunk_delete_limit as i64
            ],
        )?;
        stats.removed_unsafe_chunks += removed;
        remaining_chunk_delete_limit = remaining_chunk_delete_limit.saturating_sub(removed);
    }
    stats.removed_deleted_chunks = tx.execute(
        &format!(
            "DELETE FROM {CHUNKS_TABLE}
             WHERE rowid IN (
               SELECT rowid FROM {CHUNKS_TABLE}
               WHERE COALESCE(deleted, 0) = 1
               LIMIT {DESKTOP_FILE_INDEX_MAINTENANCE_CHUNK_DELETE_LIMIT}
             )"
        ),
        [],
    )?;
    let unsafe_tombstone_cutoff = now
        - Duration::from_secs(DESKTOP_FILE_INDEX_UNSAFE_TOMBSTONE_RETENTION_SECS).as_millis()
            as f64;
    stats.removed_unsafe_file_tombstones = tx.execute(
        &format!(
            "DELETE FROM {FILES_TABLE}
             WHERE rowid IN (
               SELECT rowid FROM {FILES_TABLE}
               INDEXED BY ctox_business_os_desktop_files_deleted_unsafe_idx
               WHERE COALESCE(deleted, 0) = 1
                 AND json_extract(data, '$.source') = 'ctox-core'
                 AND COALESCE(json_extract(data, '$.is_deleted'), 0) = 1
                 AND json_extract(data, '$.tombstone_reason') = 'unsafe_internal_ctox_path'
                 AND COALESCE(lastWriteTime, 0) <= ?1
               ORDER BY lastWriteTime, id
               LIMIT ?2
             )"
        ),
        params![
            unsafe_tombstone_cutoff,
            DESKTOP_FILE_INDEX_MAINTENANCE_FILE_TOMBSTONE_DELETE_LIMIT as i64,
        ],
    )?;
    if stats.tombstoned_unsafe_files == 0
        && stats.removed_unsafe_chunks == 0
        && stats.removed_deleted_chunks == 0
        && stats.removed_unsafe_file_tombstones == 0
    {
        stats.removed_stale_chunks = tx.execute(
            &format!(
                "DELETE FROM {CHUNKS_TABLE}
                 WHERE rowid IN (
                   SELECT c.rowid FROM {CHUNKS_TABLE} AS c
                   WHERE COALESCE(c.deleted, 0) = 0
                     AND NOT EXISTS (
                       SELECT 1 FROM {FILES_TABLE} AS f
                       WHERE f.id = json_extract(c.data, '$.file_id')
                         AND COALESCE(f.deleted, 0) = 0
                         AND COALESCE(json_extract(f.data, '$._deleted'), 0) = 0
                         AND COALESCE(json_extract(f.data, '$.is_deleted'), 0) = 0
                         AND json_extract(f.data, '$.content_generation_id') =
                             json_extract(c.data, '$.generation_id')
                     )
                   LIMIT {DESKTOP_FILE_INDEX_MAINTENANCE_CHUNK_DELETE_LIMIT}
                 )"
            ),
            [],
        )?;
    }
    tx.commit()?;
    apply_desktop_file_chunk_cache_policy(root, &mut conn, &mut stats, cache_config.normalized())?;
    Ok(stats)
}

#[derive(Debug)]
struct DesktopFileChunkCacheCandidate {
    file_id: String,
    revision: Option<String>,
    document: Value,
    generation_id: String,
    chunk_count: usize,
    bytes: u64,
    created_at_ms: u64,
}

struct DesktopFileChunkCacheEviction {
    candidate: DesktopFileChunkCacheCandidate,
    metadata: fs::Metadata,
}

fn apply_desktop_file_chunk_cache_policy(
    root: &Path,
    conn: &mut Connection,
    stats: &mut DesktopFileIndexMaintenanceStats,
    config: DesktopFileChunkCacheConfig,
) -> anyhow::Result<()> {
    let live_bytes_before = desktop_file_chunk_cache_live_bytes(conn)?;
    stats.cache_live_bytes_before = live_bytes_before;
    stats.cache_live_bytes_after = live_bytes_before;
    if live_bytes_before <= config.max_live_bytes {
        return Ok(());
    }

    ensure_desktop_file_chunk_cache_state_table(conn)?;
    let mut state = desktop_file_chunk_cache_state(conn)?;
    let now = now_ms() as u64;
    let cutoff_ms = now.saturating_sub(config.active_min_age_secs.saturating_mul(1_000));
    let scan_roots = desktop_file_scan_roots(root);
    let candidates = desktop_file_chunk_cache_candidates(conn, config.max_files_per_pass)?;
    let mut projected_live_bytes = live_bytes_before;
    let mut selected_chunk_count = 0usize;
    let mut selected = Vec::new();
    let mut pinned_bytes = 0u64;

    for candidate in candidates {
        if projected_live_bytes <= config.target_live_bytes {
            break;
        }
        if selected.len() >= config.max_files_per_pass {
            pinned_bytes = pinned_bytes.saturating_add(candidate.bytes);
            continue;
        }
        if selected_chunk_count.saturating_add(candidate.chunk_count) > config.max_chunks_per_pass {
            pinned_bytes = pinned_bytes.saturating_add(candidate.bytes);
            continue;
        }
        if candidate.created_at_ms > cutoff_ms {
            pinned_bytes = pinned_bytes.saturating_add(candidate.bytes);
            continue;
        }
        let Some(metadata) =
            desktop_file_chunk_cache_eviction_metadata(root, &scan_roots, &candidate.document)
        else {
            pinned_bytes = pinned_bytes.saturating_add(candidate.bytes);
            continue;
        };
        selected_chunk_count = selected_chunk_count.saturating_add(candidate.chunk_count);
        projected_live_bytes = projected_live_bytes.saturating_sub(candidate.bytes);
        selected.push(DesktopFileChunkCacheEviction {
            candidate,
            metadata,
        });
    }

    if selected.is_empty() {
        stats.cache_pinned_bytes = pinned_bytes.max(live_bytes_before);
        stats.cache_over_quota_pinned_bytes =
            live_bytes_before.saturating_sub(config.max_live_bytes);
        return Ok(());
    }

    {
        let tx = conn.transaction()?;
        let now_f64 = now as f64;
        for eviction in &selected {
            let next_revision = maintenance_revision(
                eviction
                    .candidate
                    .document
                    .get("_rev")
                    .and_then(Value::as_str)
                    .or(eviction.candidate.revision.as_deref()),
            );
            let mut document = eviction.candidate.document.clone();
            prepare_desktop_file_cache_eviction(
                &mut document,
                &next_revision,
                now,
                &eviction.metadata,
            );
            let data = serde_json::to_string(&document)?;
            let updated = tx.execute(
                "UPDATE \"ctox_business_os__desktop_files__v0\"
                 SET revision = ?2, lastWriteTime = ?3, data = ?4
                 WHERE id = ?1
                   AND COALESCE(deleted, 0) = 0
                   AND json_extract(data, '$.content_generation_id') = ?5",
                params![
                    eviction.candidate.file_id,
                    next_revision,
                    now_f64,
                    data,
                    eviction.candidate.generation_id,
                ],
            )?;
            if updated == 0 {
                continue;
            }
            let (chunk_id_lower, chunk_id_upper) =
                desktop_file_chunk_id_bounds(&eviction.candidate.file_id);
            let removed_chunks = tx.execute(
                "DELETE FROM \"ctox_business_os__desktop_file_chunks__v0\"
                 WHERE rowid IN (
                   SELECT rowid FROM \"ctox_business_os__desktop_file_chunks__v0\"
                   WHERE id >= ?1
                     AND id < ?2
                     AND COALESCE(deleted, 0) = 0
                     AND json_extract(data, '$.generation_id') = ?3
                   LIMIT ?4
                 )",
                params![
                    chunk_id_lower,
                    chunk_id_upper,
                    eviction.candidate.generation_id,
                    config.max_chunks_per_pass as i64,
                ],
            )?;
            stats.evicted_cache_files = stats.evicted_cache_files.saturating_add(1);
            stats.removed_cache_chunks = stats.removed_cache_chunks.saturating_add(removed_chunks);
            stats.removed_cache_bytes = stats
                .removed_cache_bytes
                .saturating_add(eviction.candidate.bytes);
        }
        tx.commit()?;
    }

    let live_bytes_after = desktop_file_chunk_cache_live_bytes(conn)?;
    stats.cache_live_bytes_after = live_bytes_after;
    stats.cache_pinned_bytes = live_bytes_after;
    stats.cache_over_quota_pinned_bytes = live_bytes_after.saturating_sub(config.max_live_bytes);
    if stats.removed_cache_chunks == 0 {
        return Ok(());
    }

    state.last_eviction_at_ms = now;
    state.last_live_bytes = live_bytes_after;
    state.last_pinned_bytes = stats.cache_pinned_bytes;
    state.last_deleted_bytes = stats.removed_cache_bytes;
    state.last_deleted_chunks = stats.removed_cache_chunks as u64;

    if stats.removed_cache_bytes >= config.wal_checkpoint_min_bytes
        && now.saturating_sub(state.last_checkpoint_at_ms)
            >= config.checkpoint_min_interval_secs.saturating_mul(1_000)
    {
        if conn
            .execute_batch("PRAGMA wal_checkpoint(PASSIVE); PRAGMA optimize;")
            .is_ok()
        {
            stats.wal_checkpoint_ran = true;
            state.last_checkpoint_at_ms = now;
        }
    }

    let page_size = sqlite_pragma_u64(conn, "page_size").unwrap_or(0);
    let freelist_count = sqlite_pragma_u64(conn, "freelist_count").unwrap_or(0);
    let reclaimable_bytes = page_size.saturating_mul(freelist_count);
    if reclaimable_bytes >= config.vacuum_min_reclaim_bytes
        && now.saturating_sub(state.last_vacuum_at_ms)
            >= config.vacuum_min_interval_secs.saturating_mul(1_000)
    {
        if conn.execute_batch("VACUUM; PRAGMA optimize;").is_ok() {
            stats.vacuum_ran = true;
            state.last_vacuum_at_ms = now;
        }
    }

    save_desktop_file_chunk_cache_state(conn, &state)?;
    Ok(())
}

fn desktop_file_chunk_cache_live_bytes(conn: &Connection) -> anyhow::Result<u64> {
    let bytes: i64 = conn.query_row(
        "SELECT COALESCE(SUM(
             COALESCE(
               CAST(json_extract(data, '$.size_bytes') AS INTEGER),
               length(COALESCE(json_extract(data, '$.data'), '')),
               0
             )
           ), 0)
         FROM \"ctox_business_os__desktop_file_chunks__v0\"
         WHERE COALESCE(deleted, 0) = 0",
        [],
        |row| row.get(0),
    )?;
    Ok(u64::try_from(bytes).unwrap_or(0))
}

fn desktop_file_chunk_cache_candidates(
    conn: &Connection,
    limit: usize,
) -> anyhow::Result<Vec<DesktopFileChunkCacheCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT f.id,
                f.revision,
                f.data,
                json_extract(f.data, '$.content_generation_id') AS generation_id,
                COUNT(c.rowid) AS chunk_count,
                COALESCE(SUM(
                  COALESCE(
                    CAST(json_extract(c.data, '$.size_bytes') AS INTEGER),
                    length(COALESCE(json_extract(c.data, '$.data'), '')),
                    0
                  )
                ), 0) AS byte_count,
                COALESCE(
                  CAST(json_extract(f.data, '$.content_synced_at_ms') AS INTEGER),
                  MIN(CAST(json_extract(c.data, '$.created_at_ms') AS INTEGER)),
                  CAST(f.lastWriteTime AS INTEGER),
                  0
                ) AS created_at_ms
         FROM \"ctox_business_os__desktop_file_chunks__v0\" AS c
         JOIN \"ctox_business_os__desktop_files__v0\" AS f
           ON f.id = json_extract(c.data, '$.file_id')
          AND json_extract(f.data, '$.content_generation_id') =
              json_extract(c.data, '$.generation_id')
         WHERE COALESCE(c.deleted, 0) = 0
           AND COALESCE(f.deleted, 0) = 0
           AND COALESCE(json_extract(f.data, '$._deleted'), 0) = 0
           AND COALESCE(json_extract(f.data, '$.is_deleted'), 0) = 0
           AND json_extract(f.data, '$.source') = 'ctox-core'
           AND json_extract(f.data, '$.kind') = 'file'
           AND json_extract(f.data, '$.content_state') = 'available'
         GROUP BY f.id, generation_id
         ORDER BY created_at_ms ASC, byte_count DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        let data: String = row.get(2)?;
        let document = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
        let chunk_count: i64 = row.get(4)?;
        let bytes: i64 = row.get(5)?;
        let created_at_ms: i64 = row.get(6)?;
        Ok(DesktopFileChunkCacheCandidate {
            file_id: row.get(0)?,
            revision: row.get(1)?,
            document,
            generation_id: row.get(3)?,
            chunk_count: usize::try_from(chunk_count).unwrap_or(usize::MAX),
            bytes: u64::try_from(bytes).unwrap_or(0),
            created_at_ms: u64::try_from(created_at_ms).unwrap_or(0),
        })
    })?;
    let mut candidates = Vec::new();
    for row in rows {
        let candidate = row?;
        if candidate.document.is_object()
            && !candidate.generation_id.trim().is_empty()
            && candidate.chunk_count > 0
        {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

fn desktop_file_chunk_cache_eviction_metadata(
    root: &Path,
    scan_roots: &[DesktopFileScanRoot],
    document: &Value,
) -> Option<fs::Metadata> {
    let path = document
        .get("local_path")
        .or_else(|| document.get("path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)?;
    ensure_safe_desktop_file_index_path(&path, "desktop file cache eviction").ok()?;
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    if document.get("size_bytes").and_then(Value::as_u64) != Some(metadata.len()) {
        return None;
    }
    let modified_at_ms = metadata_modified_at_ms(&metadata);
    if document
        .get("mtime_ms")
        .and_then(Value::as_u64)
        .map(u128::from)
        != Some(modified_at_ms)
    {
        return None;
    }
    if scan_roots
        .iter()
        .any(|scan_root| path.starts_with(&scan_root.path))
        && should_eager_sync_file(&path, &metadata)
    {
        return None;
    }
    if !path.starts_with(root)
        && document.get("source").and_then(Value::as_str) != Some("ctox-core")
    {
        return None;
    }
    Some(metadata)
}

fn prepare_desktop_file_cache_eviction(
    document: &mut Value,
    revision: &str,
    now: u64,
    metadata: &fs::Metadata,
) {
    let modified_at_ms = metadata_modified_at_ms(metadata);
    if let Some(object) = document.as_object_mut() {
        object.insert("_rev".to_string(), Value::String(revision.to_string()));
        object.insert("_meta".to_string(), json!({ "lwt": now as f64 }));
        object.insert(
            "content_state".to_string(),
            Value::String("lazy".to_string()),
        );
        object.insert("content_generation_id".to_string(), Value::Null);
        object.insert("chunk_count".to_string(), Value::Null);
        object.insert("generation_verified_at_ms".to_string(), Value::Null);
        object.insert("content_synced_at_ms".to_string(), Value::Null);
        object.insert(
            "content_hash".to_string(),
            Value::String(format!("mtime:{modified_at_ms}:size:{}", metadata.len())),
        );
        object.insert(
            "content_hash_scheme".to_string(),
            Value::String(DESKTOP_FILE_CONTENT_HASH_SCHEME.to_string()),
        );
        object.insert("content_evicted_at_ms".to_string(), Value::from(now));
        object.insert(
            "content_eviction_reason".to_string(),
            Value::String("desktop_file_chunk_cache_quota".to_string()),
        );
        object.insert("updated_at_ms".to_string(), Value::from(now));
    }
}

fn ensure_desktop_file_chunk_cache_state_table(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS {DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE} (
                id TEXT PRIMARY KEY,
                updated_at_ms INTEGER NOT NULL,
                value_json TEXT NOT NULL
            )"
        ),
        [],
    )
    .context("ensure desktop file chunk cache state table")?;
    Ok(())
}

fn desktop_file_chunk_cache_state(conn: &Connection) -> anyhow::Result<DesktopFileChunkCacheState> {
    if !sqlite_table_exists(conn, DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE)? {
        return Ok(DesktopFileChunkCacheState::default());
    }
    let state_json = conn
        .query_row(
            &format!("SELECT value_json FROM {DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE} WHERE id = ?1"),
            [DESKTOP_FILE_CHUNK_CACHE_STATE_ID],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(state_json) = state_json else {
        return Ok(DesktopFileChunkCacheState::default());
    };
    Ok(serde_json::from_str(&state_json).unwrap_or_default())
}

fn save_desktop_file_chunk_cache_state(
    conn: &Connection,
    state: &DesktopFileChunkCacheState,
) -> anyhow::Result<()> {
    let now = now_ms() as u64;
    let state_json = serde_json::to_string(state)?;
    conn.execute(
        &format!(
            "INSERT INTO {DESKTOP_FILE_CHUNK_CACHE_STATE_TABLE}
             (id, updated_at_ms, value_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               updated_at_ms = excluded.updated_at_ms,
               value_json = excluded.value_json"
        ),
        params![DESKTOP_FILE_CHUNK_CACHE_STATE_ID, now, state_json],
    )
    .context("save desktop file chunk cache state")?;
    Ok(())
}

fn sqlite_pragma_u64(conn: &Connection, name: &str) -> anyhow::Result<u64> {
    let sql = format!("PRAGMA {name}");
    let value: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(u64::try_from(value).unwrap_or(0))
}

fn unsafe_desktop_file_index_candidates_sql(files_table: &str) -> String {
    const PATH_EXPR: &str =
        "COALESCE(json_extract(data, '$.local_path'), json_extract(data, '$.path'))";
    format!(
        "SELECT id, revision, data FROM {files_table} \
         INDEXED BY ctox_business_os_desktop_files_live_core_idx
         WHERE COALESCE(deleted, 0) = 0
           AND json_extract(data, '$.source') = 'ctox-core'
           AND json_extract(data, '$.kind') = 'file'
           AND COALESCE(json_extract(data, '$.is_deleted'), 0) = 0
           AND (
             ({PATH_EXPR} >= '/tmp/' AND {PATH_EXPR} < '/tmp0')
             OR ({PATH_EXPR} >= '/var/tmp/' AND {PATH_EXPR} < '/var/tmp0')
             OR ({PATH_EXPR} >= '/var/folders/' AND {PATH_EXPR} < '/var/folders0')
             OR ({PATH_EXPR} >= '/private/var/folders/' AND {PATH_EXPR} < '/private/var/folders0')
             OR {PATH_EXPR} GLOB '*/.local/lib/ctox/*'
             OR {PATH_EXPR} GLOB '*/.local/state/ctox/*'
           )
         LIMIT ?1"
    )
}

fn chunk_id_prefix_bounds(key: &str) -> (String, String) {
    // Chunk IDs are "{file_id}_{generation_id}_{idx}". The upper bound uses
    // the next ASCII character after '_' so SQLite can use the primary-key
    // index instead of extracting the owner key from every JSON payload.
    (format!("{key}_"), format!("{key}`"))
}

fn desktop_file_chunk_id_bounds(file_id: &str) -> (String, String) {
    chunk_id_prefix_bounds(file_id)
}

fn desktop_file_index_document_is_unsafe(document: &Value, home: Option<&Path>) -> bool {
    if document.get("kind").and_then(Value::as_str) != Some("file") {
        return false;
    }
    if document.get("source").and_then(Value::as_str) != Some("ctox-core") {
        return false;
    }
    let Some(path) = document
        .get("local_path")
        .or_else(|| document.get("path"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
    else {
        return false;
    };
    is_unsafe_desktop_file_index_path(&path, home)
}

fn is_unsafe_desktop_file_index_path(path: &Path, home: Option<&Path>) -> bool {
    if is_ctox_internal_path_layout(path) {
        return true;
    }
    if let Some(home) = home {
        if is_ctox_internal_desktop_scan_root(path, home) {
            return true;
        }
    }
    path.starts_with("/tmp")
        || path.starts_with("/var/tmp")
        || path.starts_with("/var/folders")
        || path.starts_with("/private/var/folders")
}

fn is_ctox_internal_path_layout(path: &Path) -> bool {
    path_has_component_sequence(path, &[".local", "lib", "ctox"])
        || path_has_component_sequence(path, &[".local", "state", "ctox"])
}

fn ensure_desktop_file_index_query_indexes(conn: &Connection) -> anyhow::Result<()> {
    if !sqlite_table_has_column(conn, "ctox_business_os__desktop_files__v0", "deleted")? {
        return Ok(());
    }
    conn.execute(
        r#"
        CREATE INDEX IF NOT EXISTS ctox_business_os_desktop_files_live_core_idx
        ON "ctox_business_os__desktop_files__v0" (
            json_extract(data, '$.source'),
            json_extract(data, '$.kind'),
            COALESCE(json_extract(data, '$.is_deleted'), 0),
            COALESCE(json_extract(data, '$.local_path'), json_extract(data, '$.path'))
        )
        WHERE COALESCE(deleted, 0) = 0
        "#,
        [],
    )
    .context("ensure desktop_files live ctox-core index")?;
    conn.execute(
        r#"
        CREATE INDEX IF NOT EXISTS ctox_business_os_desktop_files_deleted_unsafe_idx
        ON "ctox_business_os__desktop_files__v0" (
            COALESCE(deleted, 0),
            json_extract(data, '$.tombstone_reason'),
            lastWriteTime,
            id
        )
        WHERE COALESCE(deleted, 0) = 1
          AND json_extract(data, '$.source') = 'ctox-core'
          AND COALESCE(json_extract(data, '$.is_deleted'), 0) = 1
        "#,
        [],
    )
    .context("ensure desktop_files unsafe tombstone cleanup index")?;
    conn.execute(
        r#"
        CREATE INDEX IF NOT EXISTS ctox_business_os_desktop_files_active_generation_idx
        ON "ctox_business_os__desktop_files__v0" (
            json_extract(data, '$.source'),
            json_extract(data, '$.kind'),
            json_extract(data, '$.content_state'),
            json_extract(data, '$.content_generation_id'),
            id
        )
        WHERE COALESCE(deleted, 0) = 0
        "#,
        [],
    )
    .context("ensure desktop_files active generation index")?;
    if sqlite_table_exists(conn, "ctox_business_os__desktop_file_chunks__v0")?
        && sqlite_table_has_column(conn, "ctox_business_os__desktop_file_chunks__v0", "deleted")?
    {
        conn.execute(
            r#"
            CREATE INDEX IF NOT EXISTS ctox_business_os_desktop_file_chunks_deleted_idx
            ON "ctox_business_os__desktop_file_chunks__v0" (
                COALESCE(deleted, 0),
                id
            )
            WHERE COALESCE(deleted, 0) = 1
            "#,
            [],
        )
        .context("ensure desktop_file_chunks deleted index")?;
        conn.execute(
            r#"
            CREATE INDEX IF NOT EXISTS ctox_business_os_desktop_file_chunks_live_owner_idx
            ON "ctox_business_os__desktop_file_chunks__v0" (
                COALESCE(deleted, 0),
                json_extract(data, '$.file_id'),
                json_extract(data, '$.generation_id'),
                CAST(json_extract(data, '$.created_at_ms') AS INTEGER),
                id
            )
            WHERE COALESCE(deleted, 0) = 0
            "#,
            [],
        )
        .context("ensure desktop_file_chunks live owner index")?;
    }
    Ok(())
}

fn path_has_component_sequence(path: &Path, sequence: &[&str]) -> bool {
    if sequence.is_empty() {
        return false;
    }
    let components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    components
        .windows(sequence.len())
        .any(|window| window == sequence)
}

fn prepare_unsafe_desktop_file_tombstone(document: &mut Value, revision: &str, now: f64) {
    let now_u64 = now as u64;
    if let Some(object) = document.as_object_mut() {
        object.insert("_rev".to_string(), Value::String(revision.to_string()));
        object.insert("_deleted".to_string(), Value::Bool(true));
        object.insert("_attachments".to_string(), json!({}));
        object.insert("_meta".to_string(), json!({ "lwt": now }));
        object.insert("is_deleted".to_string(), Value::Bool(true));
        object.insert(
            "content_state".to_string(),
            Value::String("missing".to_string()),
        );
        object.insert("content_generation_id".to_string(), Value::Null);
        object.insert("content_hash".to_string(), Value::String(String::new()));
        object.insert("content_synced_at_ms".to_string(), Value::Null);
        object.insert("deleted_at_ms".to_string(), Value::from(now_u64));
        object.insert("updated_at_ms".to_string(), Value::from(now_u64));
        object.insert(
            "tombstone_reason".to_string(),
            Value::String("unsafe_internal_ctox_path".to_string()),
        );
    }
}

fn maintenance_revision(previous: Option<&str>) -> String {
    let height = previous
        .and_then(|revision| revision.split_once('-').map(|(height, _)| height))
        .and_then(|height| height.parse::<u64>().ok())
        .unwrap_or(0)
        .saturating_add(1);
    format!("{height}-ctox-maintenance")
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

async fn collect_desktop_file_index_scan(
    scan_roots: Vec<DesktopFileScanRoot>,
) -> anyhow::Result<DesktopFileIndexScan> {
    tokio::task::spawn_blocking(move || collect_desktop_file_index_scan_sync(scan_roots))
        .await
        .context("join native desktop file index scan")
}

fn collect_desktop_file_index_scan_sync(
    mut scan_roots: Vec<DesktopFileScanRoot>,
) -> DesktopFileIndexScan {
    normalize_desktop_file_scan_roots(&mut scan_roots);
    let mut candidates = collect_desktop_file_index_candidates(&scan_roots);
    candidates.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.scan_root.path.cmp(&right.scan_root.path))
    });
    let truncated = candidates.len() >= DESKTOP_FILE_SCAN_MAX_FILES;
    let stamp = desktop_file_index_projection_stamp(&scan_roots, &candidates, truncated);
    DesktopFileIndexScan {
        scan_roots,
        candidates,
        stamp,
    }
}

fn desktop_file_index_projection_stamp(
    scan_roots: &[DesktopFileScanRoot],
    candidates: &[DesktopFileIndexCandidate],
    truncated: bool,
) -> DesktopFileIndexProjectionStamp {
    let mut hasher = sha2::Sha256::new();
    for scan_root in scan_roots {
        update_hash_with_string(&mut hasher, &scan_root.path.to_string_lossy());
        update_hash_with_string(&mut hasher, &scan_root.label);
    }
    for candidate in candidates {
        update_hash_with_string(&mut hasher, &candidate.path.to_string_lossy());
        update_hash_with_string(&mut hasher, &candidate.scan_root.path.to_string_lossy());
        update_hash_with_string(&mut hasher, &candidate.scan_root.label);
        let metadata = fs::metadata(&candidate.path);
        match metadata {
            Ok(metadata) if metadata.is_file() => {
                hasher.update([1]);
                hasher.update(metadata.len().to_le_bytes());
                hasher.update(metadata_modified_at_ms(&metadata).to_le_bytes());
                hasher.update([u8::from(should_eager_sync_file(&candidate.path, &metadata))]);
            }
            _ => {
                hasher.update([0]);
            }
        }
    }
    DesktopFileIndexProjectionStamp {
        scan_root_count: scan_roots.len(),
        candidate_count: candidates.len(),
        truncated,
        content_hash: format!("{:x}", hasher.finalize()),
    }
}

fn desktop_file_scan_roots_stamp(scan_roots: &[DesktopFileScanRoot]) -> DesktopFileScanRootsStamp {
    let mut hasher = sha2::Sha256::new();
    for scan_root in scan_roots {
        update_hash_with_string(&mut hasher, &scan_root.path.to_string_lossy());
        update_hash_with_string(&mut hasher, &scan_root.label);
        match fs::metadata(&scan_root.path) {
            Ok(metadata) if metadata.is_dir() => {
                hasher.update([1]);
                hasher.update(metadata.len().to_le_bytes());
                hasher.update(metadata_modified_at_ms(&metadata).to_le_bytes());
            }
            _ => {
                hasher.update([0]);
                continue;
            }
        }

        let mut entries = Vec::new();
        if let Ok(read_dir) = fs::read_dir(&scan_root.path) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                let Some(name) = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
                else {
                    continue;
                };
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                entries.push((
                    name,
                    metadata.is_dir(),
                    metadata.is_file(),
                    metadata.len(),
                    metadata_modified_at_ms(&metadata),
                ));
            }
        }
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        hasher.update(entries.len().to_le_bytes());
        for (name, is_dir, is_file, len, modified_at_ms) in entries {
            update_hash_with_string(&mut hasher, &name);
            hasher.update([u8::from(is_dir), u8::from(is_file)]);
            hasher.update(len.to_le_bytes());
            hasher.update(modified_at_ms.to_le_bytes());
        }
    }
    DesktopFileScanRootsStamp {
        scan_root_count: scan_roots.len(),
        content_hash: format!("{:x}", hasher.finalize()),
    }
}

fn desktop_file_index_should_collect_scan(
    last_scan_roots_stamp: Option<&DesktopFileScanRootsStamp>,
    last_full_scan_at: Option<SystemTime>,
    scan_roots_stamp: &DesktopFileScanRootsStamp,
    dirty_scan_roots: bool,
    now: SystemTime,
) -> bool {
    if dirty_scan_roots {
        return true;
    }
    if last_scan_roots_stamp != Some(scan_roots_stamp) {
        return true;
    }
    let Some(last_full_scan_at) = last_full_scan_at else {
        return true;
    };
    now.duration_since(last_full_scan_at)
        .map(|elapsed| elapsed >= Duration::from_secs(DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS))
        .unwrap_or(true)
}

fn desktop_file_index_sleep_interval(
    has_scan_roots: bool,
    last_maintenance_at: SystemTime,
    last_full_scan_at: Option<SystemTime>,
    now: SystemTime,
) -> Duration {
    if !has_scan_roots {
        let discovery_due = last_full_scan_at
            .map(|last_full_scan_at| {
                duration_until_due(
                    last_full_scan_at,
                    Duration::from_secs(DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS),
                    now,
                )
            })
            .unwrap_or_else(|| Duration::from_secs(DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS));
        let maintenance_due = duration_until_due(
            last_maintenance_at,
            Duration::from_secs(DESKTOP_FILE_INDEX_MAINTENANCE_INTERVAL_SECS),
            now,
        );
        return discovery_due.min(maintenance_due);
    }
    let fallback_due = last_full_scan_at
        .map(|last_full_scan_at| {
            duration_until_due(
                last_full_scan_at,
                Duration::from_secs(DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS),
                now,
            )
        })
        .unwrap_or(Duration::ZERO);
    let maintenance_due = duration_until_due(
        last_maintenance_at,
        Duration::from_secs(DESKTOP_FILE_INDEX_MAINTENANCE_INTERVAL_SECS),
        now,
    );
    fallback_due.min(maintenance_due)
}

fn duration_until_due(last_run_at: SystemTime, interval: Duration, now: SystemTime) -> Duration {
    match now.duration_since(last_run_at) {
        Ok(elapsed) if elapsed < interval => interval - elapsed,
        _ => Duration::ZERO,
    }
}

fn update_hash_with_string(hasher: &mut sha2::Sha256, value: &str) {
    hasher.update(value.len().to_le_bytes());
    hasher.update(value.as_bytes());
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
    // Active harness workspaces are durable queue-owned roots. They must be
    // visible to the background index even before a successful worker turn
    // performs its immediate projection; otherwise a daemon restart or a
    // long-running task can leave the browser with no workspace metadata.
    // Keep discovery bounded and apply the same canonical safe-root gate as
    // every other background scan source.
    let active_statuses = ["pending", "leased", "blocked"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Ok(tasks) = channels::list_queue_tasks(root, &active_statuses, 512) {
        roots.extend(tasks.into_iter().filter_map(|task| {
            let workspace = task.workspace_root?.trim().to_string();
            if workspace.is_empty() {
                return None;
            }
            let path = PathBuf::from(workspace);
            Some((path.clone(), desktop_file_scan_root_label(&path)))
        }));
    }
    let mut roots = roots
        .into_iter()
        .filter_map(|(path, label)| {
            path.canonicalize()
                .ok()
                .map(|path| DesktopFileScanRoot { path, label })
        })
        .filter(|root| is_safe_desktop_file_scan_root(&root.path))
        .collect::<Vec<_>>();
    normalize_desktop_file_scan_roots(&mut roots);
    roots
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
    if is_broad_desktop_file_scan_root(path) {
        return false;
    }
    if is_ctox_internal_path_layout(path) {
        return false;
    }
    true
}

fn ensure_safe_desktop_file_index_path(path: &Path, kind: &str) -> anyhow::Result<()> {
    if is_ctox_internal_path_layout(path) || is_broad_desktop_file_scan_root(path) {
        anyhow::bail!(
            "{kind} is outside the Business OS file-index boundary: {}",
            path.display()
        );
    }
    Ok(())
}

fn is_broad_desktop_file_scan_root(path: &Path) -> bool {
    if path == Path::new("/")
        || path == Path::new("/Users")
        || path == Path::new("/Applications")
        || path == Path::new("/Library")
        || path == Path::new("/System")
        || path == Path::new("/Volumes")
        || path == Path::new("/private")
        || path == Path::new("/var")
        || path == Path::new("/tmp")
        || path == Path::new("/var/tmp")
        || path == Path::new("/var/folders")
        || path == Path::new("/private/var/folders")
    {
        return true;
    }
    let mut components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str());
    matches!(
        (
            components.next(),
            components.next(),
            components.next(),
            components.next()
        ),
        (Some("/"), Some("Users"), Some(_user), None)
    )
}

fn is_ctox_internal_desktop_scan_root(path: &Path, home: &Path) -> bool {
    path.starts_with(home.join(".local/lib/ctox"))
        || path.starts_with(home.join(".local/state/ctox"))
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
    root: &Path,
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
    ensure_desktop_file_index_query_indexes_for_root(root)
        .await
        .context("ensure ctox-core desktop_files query index")?;
    let rows = load_live_ctox_desktop_file_documents(root)
        .await
        .context("load ctox-core desktop_files for missing scan")?;

    let mut marked = 0usize;
    let now = now_ms();
    for row in &rows {
        let mut document = row.clone();
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

async fn load_live_ctox_desktop_file_documents(root: &Path) -> anyhow::Result<Vec<Value>> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || load_live_ctox_desktop_file_documents_sync(&root))
        .await
        .context("join ctox-core desktop_files scan")?
}

async fn ensure_desktop_file_index_query_indexes_for_root(root: &Path) -> anyhow::Result<()> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        ensure_desktop_file_index_query_indexes_for_root_sync(&root)
    })
    .await
    .context("join desktop_files query index ensure")?
}

fn ensure_desktop_file_index_query_indexes_for_root_sync(root: &Path) -> anyhow::Result<()> {
    const FILES_TABLE_NAME: &str = "ctox_business_os__desktop_files__v0";

    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok(());
    }
    let conn = Connection::open(&database_path)
        .with_context(|| format!("open RxDB store {}", database_path.display()))?;
    conn.busy_timeout(Duration::from_secs(10))
        .context("set RxDB index busy timeout")?;
    if !sqlite_table_exists(&conn, FILES_TABLE_NAME)? {
        return Ok(());
    }
    ensure_desktop_file_index_query_indexes(&conn)
}

fn load_live_ctox_desktop_file_documents_sync(root: &Path) -> anyhow::Result<Vec<Value>> {
    const FILES_TABLE: &str = "\"ctox_business_os__desktop_files__v0\"";
    const FILES_TABLE_NAME: &str = "ctox_business_os__desktop_files__v0";

    let database_path = store::rxdb_store_path(root);
    if !database_path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(
        &database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open RxDB store {}", database_path.display()))?;
    conn.busy_timeout(Duration::from_secs(10))
        .context("set RxDB read busy timeout")?;
    if !sqlite_table_exists(&conn, FILES_TABLE_NAME)? {
        return Ok(Vec::new());
    }
    let deleted_predicate = if sqlite_table_has_column(&conn, FILES_TABLE_NAME, "deleted")? {
        "COALESCE(deleted, 0) = 0 AND"
    } else {
        ""
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT data FROM {FILES_TABLE}
         WHERE {deleted_predicate}
           json_extract(data, '$.kind') = 'file'
           AND json_extract(data, '$.source') = 'ctox-core'
           AND COALESCE(json_extract(data, '$.is_deleted'), 0) = 0"
    ))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut documents = Vec::new();
    for row in rows {
        let data = row?;
        let Ok(document) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        documents.push(document);
    }
    Ok(documents)
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
        // The native peer is long-lived and its replication pools hold
        // collection/storage Arcs. Helper paths may open the same named RxDB in
        // process; they must not close the live peer database underneath those
        // pools, or the browser sees SQLITE_CLOSED from an apparently healthy
        // peer.
        ignore_duplicate: true,
        close_duplicates: false,
        event_reduce: true,
        allow_slow_count: true,
    })
    .await
    .map_err(|err| anyhow::anyhow!("open native Business OS RxDB database: {err}"))
}

fn collection_creators_for_root(root: &Path) -> HashMap<String, RxCollectionCreator> {
    let mut creators = collection_creators();
    for (name, creator) in runtime_installed_module_collection_creators(root) {
        creators.entry(name).or_insert(creator);
    }
    creators
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

fn runtime_installed_module_collection_creators(
    root: &Path,
) -> HashMap<String, RxCollectionCreator> {
    let runtime_app_root = resolve_business_os_installed_app_root_for_native_peer(root);
    let mut creators = HashMap::new();
    let static_collections = business_os_schema_contract();
    // installed-modules/ carries API-installed apps (gated on the runtime
    // installed manifest markers); local-modules/ carries operator-placed,
    // git-ignored dev/customer apps — presence in the directory is the opt-in.
    for (dir_name, require_installed_marker) in
        [("installed-modules", true), ("local-modules", false)]
    {
        let modules_root = runtime_app_root.join(dir_name);
        if !modules_root.is_dir() {
            continue;
        }
        let entries = match fs::read_dir(&modules_root) {
            Ok(entries) => entries,
            Err(err) => {
                eprintln!(
                    "[business-os] could not read module schemas from {}: {err:#}",
                    modules_root.display()
                );
                continue;
            }
        };
        for entry in entries.flatten() {
            let module_dir = entry.path();
            if !module_dir.is_dir() {
                continue;
            }
            let schemas = if require_installed_marker {
                runtime_installed_module_collection_schemas(&module_dir)
            } else {
                local_module_collection_schemas(&module_dir)
            };
            for (name, schema) in schemas {
                if static_collections.contains_key(&name) || creators.contains_key(&name) {
                    continue;
                }
                creators.insert(
                    name,
                    RxCollectionCreator {
                        schema,
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                );
            }
        }
    }
    creators
}

fn runtime_installed_module_collection_schemas(module_dir: &Path) -> Vec<(String, RxJsonSchema)> {
    module_dir_collection_schemas(module_dir, true)
}

/// Local modules (`runtime/business-os/local-modules/`, operator-placed and
/// git-ignored) load their collection schemas exactly like installed modules
/// but carry no runtime-installed marker — dropping the directory IS the
/// install. Completes the local-modules discovery from commit 8741c150.
fn local_module_collection_schemas(module_dir: &Path) -> Vec<(String, RxJsonSchema)> {
    module_dir_collection_schemas(module_dir, false)
}

fn module_dir_collection_schemas(
    module_dir: &Path,
    require_installed_marker: bool,
) -> Vec<(String, RxJsonSchema)> {
    let manifest_path = module_dir.join("module.json");
    let schema_path = module_dir.join("collections.schema.json");
    if !manifest_path.is_file() || !schema_path.is_file() {
        return Vec::new();
    }
    let manifest = match read_json_file(&manifest_path) {
        Ok(value) => value,
        Err(err) => {
            eprintln!(
                "[business-os] skipping installed module schema {}: invalid module.json: {err:#}",
                module_dir.display()
            );
            return Vec::new();
        }
    };
    if require_installed_marker && !manifest_value_is_runtime_installed_for_native_peer(&manifest) {
        return Vec::new();
    }
    let declared = manifest
        .get("collections")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    if declared.is_empty() {
        return Vec::new();
    }
    let schema_doc = match read_json_file(&schema_path) {
        Ok(value) => value,
        Err(err) => {
            eprintln!(
                "[business-os] skipping installed module schema {}: invalid collections.schema.json: {err:#}",
                module_dir.display()
            );
            return Vec::new();
        }
    };
    if schema_doc.get("schema_format").and_then(Value::as_str)
        != Some("ctox-business-os-module-collections-v1")
    {
        eprintln!(
            "[business-os] skipping installed module schema {}: unsupported schema_format",
            schema_path.display()
        );
        return Vec::new();
    }
    let Some(collections) = schema_doc.get("collections").and_then(Value::as_object) else {
        return Vec::new();
    };
    collections
        .iter()
        .filter_map(|(name, schema)| {
            if !declared.contains(name) || !is_runtime_module_collection_name(name) {
                return None;
            }
            // A collection entry may be the schema object itself, or a wrapper
            // `{ "schema": {...}, "conflictStrategy": "field-merge" }` matching
            // the schema.js sibling convention (docs/ctox-rxdb.md §8.2). The
            // native peer only needs the schema — conflict strategies are
            // resolved browser-side — so unwrap BEFORE parsing/hashing, which
            // keeps the native schema hash identical to the browser's hash of
            // the unwrapped schema.js schema. Only unwrap when the entry is
            // unambiguously a wrapper (has `schema`, lacks `primaryKey`).
            let schema_value = match schema.get("schema") {
                Some(inner) if inner.is_object() && schema.get("primaryKey").is_none() => {
                    inner.clone()
                }
                _ => schema.clone(),
            };
            match rx_schema_from_runtime_module_schema(name, schema_value) {
                Ok(schema) => Some((name.clone(), schema)),
                Err(err) => {
                    eprintln!(
                        "[business-os] skipping installed module collection `{name}` from {}: {err:#}",
                        schema_path.display()
                    );
                    None
                }
            }
        })
        .collect()
}

fn read_json_file(path: &Path) -> anyhow::Result<Value> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

fn rx_schema_from_runtime_module_schema(
    name: &str,
    mut schema: Value,
) -> anyhow::Result<RxJsonSchema> {
    let object = schema
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("collection schema must be a JSON object"))?;
    object
        .entry("version".to_owned())
        .or_insert_with(|| Value::Number(0.into()));
    object
        .entry("type".to_owned())
        .or_insert_with(|| Value::String("object".to_owned()));
    anyhow::ensure!(
        object.get("primaryKey").and_then(Value::as_str).is_some(),
        "collection `{name}` schema must define primaryKey"
    );
    anyhow::ensure!(
        object
            .get("properties")
            .and_then(Value::as_object)
            .is_some(),
        "collection `{name}` schema must define properties"
    );
    normalize_schema_indexes(&mut schema);
    serde_json::from_value(schema).with_context(|| {
        format!("collection `{name}` schema must match CTOX Sync Engine schema type")
    })
}

fn is_runtime_module_collection_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.len() <= 160
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
}

fn manifest_value_is_runtime_installed_for_native_peer(manifest: &Value) -> bool {
    manifest.get("install_scope").and_then(Value::as_str) == Some("installed")
        || manifest
            .get("entry")
            .and_then(Value::as_str)
            .is_some_and(|entry| entry.starts_with("installed-modules/"))
}

fn resolve_business_os_installed_app_root_for_native_peer(root: &Path) -> PathBuf {
    if root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "runtime")
    {
        return root.join("business-os");
    }
    let runtime = root.join("runtime");
    if runtime.exists() {
        return runtime.join("business-os");
    }
    let direct = root.join("business-os");
    if direct.exists() {
        return direct;
    }
    root.join("business-os")
}

fn native_declarative_migration_operations(spec: &Value) -> anyhow::Result<Vec<Value>> {
    let operations = if let Some(array) = spec.as_array() {
        array.clone()
    } else {
        spec.get("operations")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    for operation in &operations {
        let object = operation
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("declarative migration operation must be an object"))?;
        let op = object.get("op").and_then(Value::as_str).unwrap_or_default();
        match op {
            "set_from_first_truthy" => {
                anyhow::ensure!(
                    object.get("field").and_then(Value::as_str).is_some()
                        && object.get("paths").and_then(Value::as_array).is_some(),
                    "set_from_first_truthy migration needs field and paths"
                );
            }
            "set_boolean" => {
                anyhow::ensure!(
                    object.get("field").and_then(Value::as_str).is_some(),
                    "set_boolean migration needs field"
                );
            }
            other => anyhow::bail!("unsupported declarative migration operation {other}"),
        }
    }
    Ok(operations)
}

fn apply_native_declarative_migration(old_doc: &Value, spec: &Value) -> anyhow::Result<Value> {
    let operations = native_declarative_migration_operations(spec)?;
    let mut migrated = old_doc.as_object().cloned().unwrap_or_default();
    for operation in operations {
        let object = operation
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("declarative migration operation must be an object"))?;
        let field = object
            .get("field")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match object.get("op").and_then(Value::as_str).unwrap_or_default() {
            "set_from_first_truthy" => {
                let paths = object
                    .get("paths")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let next = paths
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(|path| native_declarative_path_value(old_doc, path))
                    .find(|value| native_declarative_value_is_truthy(value))
                    .cloned()
                    .or_else(|| object.get("default").cloned())
                    .unwrap_or(Value::Null);
                migrated.insert(field.to_owned(), next);
            }
            "set_boolean" => {
                let path = object.get("path").and_then(Value::as_str).unwrap_or(field);
                migrated.insert(
                    field.to_owned(),
                    Value::Bool(
                        native_declarative_path_value(old_doc, path)
                            .is_some_and(native_declarative_value_is_truthy),
                    ),
                );
            }
            other => anyhow::bail!("unsupported declarative migration operation {other}"),
        }
    }
    Ok(Value::Object(migrated))
}

fn native_declarative_path_value<'a>(source: &'a Value, path: &str) -> Option<&'a Value> {
    if path.trim().is_empty() {
        return None;
    }
    let mut current = source;
    for segment in path.split('.') {
        current = current.as_object()?.get(segment)?;
    }
    Some(current)
}

fn native_declarative_value_is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|number| number != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
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
        // `knowledge_tables` is owned by the dedicated knowledge-tables
        // projection (`sync_knowledge_tables_with_database`), which embeds
        // parquet rows in each doc payload. Excluding it here keeps that
        // rows-bearing projection the single writer, so the generic
        // business-record projection cannot overwrite it with a row-less doc.
        .filter(|name| {
            // Dedicated singleton/status projections are single-writer too:
            // the generic business-record projector must not create a second
            // `_rev`/`_meta.lwt` stream for semantically identical runtime
            // settings or module catalog snapshots.
            //
            // `desktop_files` is owned by the desktop-file index
            // (scan/materialize/browser masterWrite write the RxDB directly);
            // its store records exist for the MCP/store view only. Projecting
            // them here overwrote a freshly materialized 'available' doc with
            // the store's stale 'lazy' snapshot every interval — the browser
            // file viewer flipped back to an unreadable lazy state ~3s after
            // each ctox.file.materialize (rxdb-soak viewer-restart). Same
            // single-writer rule as `knowledge_tables` below.
            !matches!(
                name.as_str(),
                "browser_frames"
                    | "business_workspace_branding"
                    | "business_module_catalog"
                    | "ctox_runtime_settings"
                    | "desktop_files"
                    | "desktop_file_chunks"
                    | "knowledge_tables"
            )
        })
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
    if !business_os_schema_contract().contains_key(collection) {
        anyhow::bail!("unknown Business OS RxDB collection `{collection}`");
    }
    let required = is_required_native_collection(collection);
    if required && !force {
        anyhow::bail!(
            "refusing to repair required Business OS RxDB collection `{collection}` without --force"
        );
    }
    repair_rxdb_collection_schema_version_drift(root, collection, dry_run, force)
}

fn repair_stale_rxdb_collection_schema_versions(root: &Path) -> anyhow::Result<Value> {
    let database_path = store::rxdb_store_path(root);
    if !database_path.is_file() {
        return Ok(json!({
            "ok": true,
            "code": "ctox_rxdb_stale_schema_versions",
            "action": "repair-stale-schema-versions",
            "repaired": false,
            "repaired_tables": 0,
            "repaired_triggers": 0,
            "collections": []
        }));
    }
    // Opening a connection makes SQLite parse the complete schema. Reusing one
    // connection for every collection avoids repeating that expensive parse
    // hundreds of times during each native-peer start.
    let conn = Connection::open(&database_path).with_context(|| {
        format!(
            "open native Business OS RxDB store {}",
            database_path.display()
        )
    })?;
    let _ = conn.busy_timeout(Duration::from_secs(10));
    let mut results = Vec::new();
    let mut repaired_tables = 0usize;
    let mut repaired_triggers = 0usize;
    for (collection, _) in business_os_collections() {
        let result = repair_rxdb_collection_schema_version_drift_with_connection(
            &database_path,
            &conn,
            &collection,
            false,
            true,
        )?;
        repaired_tables += result
            .get("repaired_tables")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        repaired_triggers += result
            .get("repaired_triggers")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if result
            .get("repaired")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            results.push(result);
        }
    }
    Ok(json!({
        "ok": true,
        "code": "ctox_rxdb_stale_schema_versions",
        "action": "repair-stale-schema-versions",
        "repaired": repaired_tables > 0 || repaired_triggers > 0,
        "repaired_tables": repaired_tables,
        "repaired_triggers": repaired_triggers,
        "collections": results
    }))
}

fn repair_rxdb_collection_schema_version_drift(
    root: &Path,
    collection: &str,
    dry_run: bool,
    force: bool,
) -> anyhow::Result<Value> {
    let database_path = store::rxdb_store_path(root);
    if !database_path.is_file() {
        return Ok(json!({
            "ok": true,
            "code": "ctox_optional_schema_drift",
            "action": "repair-optional-drift",
            "dry_run": dry_run,
            "force": force,
            "collection": collection,
            "database_path": database_path.display().to_string(),
            "expected_version": expected_rxdb_collection_version(collection),
            "active_version": null,
            "expected_table_exists": false,
            "stale_tables": [],
            "repaired": false,
            "repaired_tables": 0,
            "message": "Native Business OS RxDB store is missing; no schema drift repair was needed."
        }));
    }
    let conn = Connection::open(&database_path).with_context(|| {
        format!(
            "open native Business OS RxDB store {}",
            database_path.display()
        )
    })?;
    let _ = conn.busy_timeout(Duration::from_secs(10));
    repair_rxdb_collection_schema_version_drift_with_connection(
        &database_path,
        &conn,
        collection,
        dry_run,
        force,
    )
}

fn repair_rxdb_collection_schema_version_drift_with_connection(
    database_path: &Path,
    conn: &Connection,
    collection: &str,
    dry_run: bool,
    force: bool,
) -> anyhow::Result<Value> {
    let expected_version = expected_rxdb_collection_version(collection);
    let active_version = active_rxdb_collection_version(conn, collection)?;
    let expected_table = rxdb_collection_version_table_name(collection, expected_version);
    let expected_table_exists = sqlite_table_exists(conn, &expected_table)?;
    let stale_tables = stale_rxdb_collection_version_tables(
        conn,
        collection,
        expected_version,
        active_version,
        expected_table_exists,
    )?;
    let stale_triggers = stale_rxdb_collection_version_triggers(
        conn,
        collection,
        expected_version,
        active_version,
        expected_table_exists,
    )?;
    if !dry_run {
        for trigger in &stale_triggers {
            conn.execute(
                &format!(
                    "DROP TRIGGER IF EXISTS {}",
                    quote_sqlite_identifier(&trigger.name)
                ),
                [],
            )
            .with_context(|| format!("drop stale RxDB schema trigger {}", trigger.name))?;
        }
        for table in &stale_tables {
            conn.execute(
                &format!(
                    "DROP TABLE IF EXISTS {}",
                    quote_sqlite_identifier(&table.name)
                ),
                [],
            )
            .with_context(|| format!("drop stale RxDB schema table {}", table.name))?;
        }
    }
    let stale_table_values = stale_tables
        .iter()
        .map(|table| {
            json!({
                "name": table.name,
                "version": table.version,
                "row_count": table.row_count,
                "latest_updated_at_ms": table.latest_updated_at_ms
            })
        })
        .collect::<Vec<_>>();
    let stale_trigger_values = stale_triggers
        .iter()
        .map(|trigger| {
            json!({
                "name": trigger.name,
                "table": trigger.table,
                "stale_versions": trigger.stale_versions
            })
        })
        .collect::<Vec<_>>();
    let repaired = !dry_run && (!stale_tables.is_empty() || !stale_triggers.is_empty());
    let message = if stale_tables.is_empty() && stale_triggers.is_empty() {
        "No stale versioned RxDB table or trigger was present for this collection."
    } else if dry_run {
        "Dry-run only; stale versioned RxDB tables/triggers were detected but not dropped."
    } else {
        "Dropped stale versioned RxDB tables/triggers for this collection."
    };
    Ok(json!({
        "ok": true,
        "code": "ctox_optional_schema_drift",
        "action": "repair-optional-drift",
        "dry_run": dry_run,
        "force": force,
        "collection": collection,
        "database_path": database_path.display().to_string(),
        "expected_version": expected_version,
        "active_version": active_version,
        "expected_table": expected_table,
        "expected_table_exists": expected_table_exists,
        "stale_tables": stale_table_values,
        "stale_triggers": stale_trigger_values,
        "repaired": repaired,
        "repaired_tables": if dry_run { 0 } else { stale_tables.len() },
        "repaired_triggers": if dry_run { 0 } else { stale_triggers.len() },
        "message": message
    }))
}

#[derive(Debug)]
struct StaleRxdbCollectionTable {
    name: String,
    version: i64,
    row_count: i64,
    latest_updated_at_ms: Option<i64>,
}

#[derive(Debug)]
struct StaleRxdbCollectionTrigger {
    name: String,
    table: String,
    stale_versions: Vec<i64>,
}

fn expected_rxdb_collection_version(collection: &str) -> i64 {
    business_os_schema_contract()
        .get(collection)
        .and_then(|schema| schema.get("version"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn active_rxdb_collection_version(
    conn: &Connection,
    collection: &str,
) -> anyhow::Result<Option<i64>> {
    let internal_table = format!("{RXDB_SQLITE_DATABASE_NAME}___rxdb_internal__v0");
    if !sqlite_table_exists(conn, &internal_table)? {
        return Ok(None);
    }
    conn.query_row(
        &format!(
            "SELECT CAST(json_extract(data, '$.data.version') AS INTEGER)
             FROM {}
             WHERE json_extract(data, '$.data.name') = ?1
               AND COALESCE(deleted, 0) = 0
               AND COALESCE(json_extract(data, '$._deleted'), 0) = 0
             ORDER BY CAST(json_extract(data, '$.data.version') AS INTEGER) DESC
             LIMIT 1",
            quote_sqlite_identifier(&internal_table)
        ),
        params![collection],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .with_context(|| format!("read active RxDB schema version for {collection}"))
}

fn stale_rxdb_collection_version_tables(
    conn: &Connection,
    collection: &str,
    expected_version: i64,
    active_version: Option<i64>,
    expected_table_exists: bool,
) -> anyhow::Result<Vec<StaleRxdbCollectionTable>> {
    if active_version != Some(expected_version) || !expected_table_exists {
        return Ok(Vec::new());
    }
    let prefix = rxdb_collection_version_table_prefix(collection);
    let pattern = format!("{prefix}%");
    let mut statement = conn.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name LIKE ?1
         ORDER BY name ASC",
    )?;
    let rows = statement.query_map(params![pattern], |row| row.get::<_, String>(0))?;
    let mut stale = Vec::new();
    for row in rows {
        let table = row?;
        let Some(version) = rxdb_collection_version_from_table_name(&table, &prefix) else {
            continue;
        };
        if version == expected_version {
            continue;
        }
        let row_count = sqlite_table_row_count(conn, &table)?;
        let latest_updated_at_ms = sqlite_table_latest_updated_at_ms(conn, &table)?;
        stale.push(StaleRxdbCollectionTable {
            name: table,
            version,
            row_count,
            latest_updated_at_ms,
        });
    }
    Ok(stale)
}

fn stale_rxdb_collection_version_triggers(
    conn: &Connection,
    collection: &str,
    expected_version: i64,
    active_version: Option<i64>,
    expected_table_exists: bool,
) -> anyhow::Result<Vec<StaleRxdbCollectionTrigger>> {
    if active_version != Some(expected_version) || !expected_table_exists {
        return Ok(Vec::new());
    }
    let prefix = rxdb_collection_version_table_prefix(collection);
    let pattern = format!("%{prefix}%");
    let mut statement = conn.prepare(
        "SELECT name, tbl_name, COALESCE(sql, '')
         FROM sqlite_master
         WHERE type = 'trigger'
           AND (tbl_name LIKE ?1 OR sql LIKE ?1)
         ORDER BY name ASC",
    )?;
    let rows = statement.query_map(params![pattern], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut stale = Vec::new();
    for row in rows {
        let (name, table, sql) = row?;
        let mut versions = rxdb_collection_versions_referenced_by_text(&table, &prefix);
        versions.extend(rxdb_collection_versions_referenced_by_text(&sql, &prefix));
        let stale_versions = versions
            .into_iter()
            .filter(|version| *version != expected_version)
            .collect::<Vec<_>>();
        if stale_versions.is_empty() {
            continue;
        }
        stale.push(StaleRxdbCollectionTrigger {
            name,
            table,
            stale_versions,
        });
    }
    Ok(stale)
}

fn rxdb_collection_versions_referenced_by_text(text: &str, prefix: &str) -> BTreeSet<i64> {
    let mut versions = BTreeSet::new();
    let mut cursor = text;
    while let Some(index) = cursor.find(prefix) {
        let after_prefix = &cursor[index + prefix.len()..];
        let digit_len = after_prefix
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .map(char::len_utf8)
            .sum::<usize>();
        if digit_len > 0 {
            if let Ok(version) = after_prefix[..digit_len].parse::<i64>() {
                versions.insert(version);
            }
            cursor = &after_prefix[digit_len..];
        } else {
            cursor = after_prefix;
        }
    }
    versions
}

fn rxdb_collection_version_table_prefix(collection: &str) -> String {
    format!("{RXDB_SQLITE_DATABASE_NAME}__{collection}__v")
}

fn rxdb_collection_version_table_name(collection: &str, version: i64) -> String {
    format!(
        "{}{version}",
        rxdb_collection_version_table_prefix(collection)
    )
}

fn rxdb_collection_version_from_table_name(table: &str, prefix: &str) -> Option<i64> {
    let suffix = table.strip_prefix(prefix)?;
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

fn sqlite_table_exists(conn: &Connection, table: &str) -> anyhow::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some())
}

fn sqlite_table_has_column(conn: &Connection, table: &str, column: &str) -> anyhow::Result<bool> {
    let quoted_table = table.replace('"', "\"\"");
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info(\"{quoted_table}\")"))
        .with_context(|| format!("inspect SQLite columns for {table}"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn sqlite_trigger_exists(conn: &Connection, trigger: &str) -> anyhow::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
            params![trigger],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some())
}

fn sqlite_table_row_count(conn: &Connection, table: &str) -> anyhow::Result<i64> {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {}", quote_sqlite_identifier(table)),
        [],
        |row| row.get::<_, i64>(0),
    )
    .with_context(|| format!("count rows in {table}"))
}

fn sqlite_table_latest_updated_at_ms(
    conn: &Connection,
    table: &str,
) -> anyhow::Result<Option<i64>> {
    conn.query_row(
        &format!(
            "SELECT MAX(CAST(json_extract(data, '$.updated_at_ms') AS INTEGER)) FROM {}",
            quote_sqlite_identifier(table)
        ),
        [],
        |row| row.get::<_, Option<i64>>(0),
    )
    .with_context(|| format!("read latest updated_at_ms in {table}"))
}

fn quote_sqlite_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
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
    fn browser_capture_prefers_fresh_screenshot_navigation() {
        let screenshot = json!({ "nav": { "url": "https://iana.org/" } });
        let stale_hint = json!({ "url": "https://example.com/" });
        assert_eq!(
            browser_capture_navigation(&screenshot, Some(&stale_hint))["url"],
            "https://iana.org/"
        );
        assert!(!browser_updates_command_watermark("browser.input"));
        assert!(browser_updates_command_watermark("browser.navigate"));
    }

    fn issue_test_capability(root: &Path, user_id: &str, role: &str) -> String {
        store::issue_business_os_capability_token_for_managed_user(
            root,
            user_id,
            user_id,
            role,
            now_ms() as i64,
        )
        .expect("issue Business OS test capability")
        .0
    }

    /// Idle-discipline ratchet (strategy rule: "idle must stay idle", see
    /// docs/ctox-os-framework-strategy.md). Every background loop in this
    /// file must engage one of the sanctioned idle strategies — interval
    /// backoff through a `*_sleep_secs`/`*_sleep_interval` helper, an
    /// event-driven watch, a table-change notification, or an explicit idle
    /// interval constant. A new `async fn *_loop` that polls at a fixed
    /// active-rate interval with none of these fails here. Fix the loop;
    /// extending the marker list is an architecture decision, not a
    /// convenience.
    #[test]
    fn background_loops_use_a_sanctioned_idle_strategy() {
        let source = include_str!("rxdb_peer.rs");
        const IDLE_MARKERS: &[&str] = &[
            "projection_sleep_secs(",
            "sleep_interval(",
            "wait_for_event(",
            "wait_for_table_change",
            "wait_for_business_command_wake(",
            "_IDLE_",
        ];
        let mut checked = Vec::new();
        let mut offset = 0usize;
        while let Some(found) = source[offset..].find("\nasync fn ") {
            let fn_start = offset + found + 1;
            let name_start = fn_start + "async fn ".len();
            let name_end = match source[name_start..].find('(') {
                Some(index) => name_start + index,
                None => break,
            };
            let name = &source[name_start..name_end];
            let next_async = source[name_end..].find("\nasync fn ");
            let next_plain = source[name_end..].find("\nfn ");
            let body_end = match (next_async, next_plain) {
                (Some(a), Some(b)) => name_end + a.min(b),
                (Some(a), None) => name_end + a,
                (None, Some(b)) => name_end + b,
                (None, None) => source.len(),
            };
            let body = &source[name_end..body_end];
            if name.ends_with("_loop") && body.contains("loop {") {
                let sanctioned = IDLE_MARKERS.iter().any(|marker| body.contains(marker));
                assert!(
                    sanctioned,
                    "background loop `{name}` has no sanctioned idle strategy \
                     (expected one of {IDLE_MARKERS:?}); a fixed-interval poll \
                     violates the idle-CPU budget"
                );
                checked.push(name.to_string());
            }
            offset = name_end;
        }
        assert!(
            checked.len() >= 10,
            "idle-strategy scan found only {} background loops ({checked:?}) — \
             the source scan drifted from the file layout; fix the scan, do not \
             delete it",
            checked.len()
        );
    }

    #[test]
    fn business_record_projection_skips_transient_payload_collections() {
        let collections = business_record_projection_collections();
        assert!(!collections.iter().any(|name| name == "browser_frames"));
        assert!(!collections.iter().any(|name| name == "desktop_file_chunks"));
        assert!(!collections
            .iter()
            .any(|name| name == "ctox_runtime_settings"));
        assert!(!collections
            .iter()
            .any(|name| name == "business_module_catalog"));
        // `knowledge_tables` is owned by the dedicated rows-embedding
        // projection (`sync_knowledge_tables_with_database`); the generic
        // business-record projection must not also write it.
        assert!(!collections.iter().any(|name| name == "knowledge_tables"));
        // `desktop_files` is owned by the desktop-file index (scan /
        // materialize / browser masterWrite). Projecting the store snapshot
        // here overwrote freshly materialized 'available' docs with stale
        // 'lazy' ones every interval (rxdb-soak viewer-restart failure) —
        // single-writer rule, same as knowledge_tables.
        assert!(!collections.iter().any(|name| name == "desktop_files"));
        assert!(collections.iter().any(|name| name == "browser_tabs"));
    }

    #[test]
    fn business_record_projection_sleep_backs_off_after_idle_round() {
        assert_eq!(
            business_record_projection_sleep_secs(0, 0),
            BUSINESS_RECORD_PROJECTION_SYNC_INTERVAL_SECS
        );
        assert_eq!(
            business_record_projection_sleep_secs(1, 0),
            BUSINESS_RECORD_PROJECTION_IDLE_SYNC_INTERVAL_SECS
        );
        assert_eq!(
            business_record_projection_sleep_secs(u32::MAX, 0),
            BUSINESS_RECORD_PROJECTION_IDLE_SYNC_INTERVAL_SECS
        );
    }

    #[test]
    fn business_record_projection_errors_use_bounded_backoff() {
        assert_eq!(
            business_record_projection_sleep_secs(0, 1),
            BUSINESS_RECORD_PROJECTION_ERROR_BACKOFF_BASE_SECS
        );
        assert_eq!(business_record_projection_sleep_secs(0, 2), 60);
        assert_eq!(
            business_record_projection_sleep_secs(0, u32::MAX),
            BUSINESS_RECORD_PROJECTION_ERROR_BACKOFF_MAX_SECS
        );
    }

    #[test]
    fn business_record_projection_partial_slices_leave_runtime_headroom() {
        assert_eq!(
            business_record_projection_loop_sleep_secs(0, 0, true),
            BUSINESS_RECORD_PROJECTION_PARTIAL_SYNC_INTERVAL_SECS
        );
        assert_eq!(
            business_record_projection_loop_sleep_secs(0, 0, false),
            BUSINESS_RECORD_PROJECTION_SYNC_INTERVAL_SECS
        );
    }

    #[test]
    fn business_os_projection_sleep_backs_off_after_idle_round() {
        assert_eq!(
            business_os_projection_sleep_secs(RUNTIME_SETTINGS_SYNC_INTERVAL_SECS, 0),
            RUNTIME_SETTINGS_SYNC_INTERVAL_SECS
        );
        assert_eq!(
            business_os_projection_sleep_secs(RUNTIME_SETTINGS_SYNC_INTERVAL_SECS, 1),
            BUSINESS_OS_PROJECTION_IDLE_SYNC_INTERVAL_SECS
        );
        assert_eq!(
            business_os_projection_sleep_secs(RUNTIME_SETTINGS_SYNC_INTERVAL_SECS, u32::MAX),
            BUSINESS_OS_PROJECTION_IDLE_SYNC_INTERVAL_SECS
        );
    }

    #[test]
    fn notes_sync_sleep_backs_off_after_unchanged_round_and_resets_on_change() {
        assert_eq!(
            notes_sync_sleep_interval(0),
            Duration::from_secs(NOTES_SYNC_ACTIVE_INTERVAL_SECS)
        );

        let mut unchanged_ticks = 0;
        notes_sync_update_idle_ticks(&mut unchanged_ticks, false);
        assert_eq!(unchanged_ticks, 1);
        assert_eq!(
            notes_sync_sleep_interval(unchanged_ticks),
            Duration::from_secs(NOTES_SYNC_IDLE_INTERVAL_SECS)
        );

        notes_sync_update_idle_ticks(&mut unchanged_ticks, true);
        assert_eq!(unchanged_ticks, 0);
        assert_eq!(
            notes_sync_sleep_interval(unchanged_ticks),
            Duration::from_secs(NOTES_SYNC_ACTIVE_INTERVAL_SECS)
        );
    }

    #[tokio::test]
    async fn sync_watch_event_filter_drops_read_access_events() -> anyhow::Result<()> {
        let read_event = notify::Event::new(EventKind::Access(AccessKind::Read));
        let open_read_event =
            notify::Event::new(EventKind::Access(AccessKind::Open(AccessMode::Read)));
        let close_read_event =
            notify::Event::new(EventKind::Access(AccessKind::Close(AccessMode::Read)));
        let close_write_event =
            notify::Event::new(EventKind::Access(AccessKind::Close(AccessMode::Write)));
        let create_event = notify::Event::new(EventKind::Create(notify::event::CreateKind::File));

        assert!(!is_sync_relevant_watch_event(&read_event));
        assert!(!is_sync_relevant_watch_event(&open_read_event));
        assert!(!is_sync_relevant_watch_event(&close_read_event));
        assert!(is_sync_relevant_watch_event(&close_write_event));
        assert!(is_sync_relevant_watch_event(&create_event));

        let (tx, mut rx) = mpsc::unbounded_channel();
        forward_sync_relevant_watch_event(&tx, Ok(read_event));
        assert!(
            tokio::time::timeout(Duration::from_millis(25), rx.recv())
                .await
                .is_err(),
            "read/access events must not wake the Business OS sync loop"
        );

        forward_sync_relevant_watch_event(&tx, Ok(close_write_event));
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(25), rx.recv()).await?,
            Some(())
        );
        Ok(())
    }

    #[tokio::test]
    async fn notes_sync_watch_wakes_on_notes_dir_and_markdown_changes() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let notes_dir = root.path().join("runtime/business-os/notes");
        let mut watch = NotesSyncWatch::new(root.path())?;
        assert!(!watch.notes_dir_exists);

        std::fs::create_dir_all(&notes_dir)?;
        let mut saw_create_event = false;
        for _ in 0..10 {
            if watch.wait_for_event(Duration::from_millis(300)).await == WatchEventWait::Event {
                saw_create_event = true;
                break;
            }
        }
        assert!(
            saw_create_event,
            "creating the notes directory must wake the notes sync watcher"
        );

        let mut watch = NotesSyncWatch::new(root.path())?;
        assert!(watch.notes_dir_exists);
        tokio::time::sleep(Duration::from_millis(50)).await;
        watch.drain_pending();

        std::fs::write(notes_dir.join("watch.md"), "# Watch\n")?;
        let mut saw_markdown_event = false;
        for _ in 0..10 {
            if watch.wait_for_event(Duration::from_millis(300)).await == WatchEventWait::Event {
                saw_markdown_event = true;
                break;
            }
        }
        assert!(
            saw_markdown_event,
            "markdown writes must wake the notes sync watcher before fallback polling"
        );
        Ok(())
    }

    #[tokio::test]
    async fn notes_sync_watch_closed_channel_waits_before_closed() -> anyhow::Result<()> {
        let watcher = notify::recommended_watcher(|_: notify::Result<notify::Event>| {})?;
        let (tx, rx) = mpsc::unbounded_channel();
        drop(tx);
        let mut watch = NotesSyncWatch {
            _watcher: watcher,
            rx,
            notes_dir_exists: false,
        };

        let started = Instant::now();
        assert_eq!(
            watch.wait_for_event(Duration::from_millis(25)).await,
            WatchEventWait::Closed
        );
        assert!(
            started.elapsed() >= Duration::from_millis(20),
            "closed notes watcher channels must not return immediately and hot-spin"
        );
        Ok(())
    }

    #[tokio::test]
    async fn desktop_file_watch_closed_channel_waits_before_closed() -> anyhow::Result<()> {
        let watcher = notify::recommended_watcher(|_: notify::Result<notify::Event>| {})?;
        let (tx, rx) = mpsc::unbounded_channel();
        drop(tx);
        let mut watch = DesktopFileIndexWatch {
            _watcher: watcher,
            rx,
        };

        let started = Instant::now();
        assert_eq!(
            watch.wait_for_event(Duration::from_millis(25)).await,
            WatchEventWait::Closed
        );
        assert!(
            started.elapsed() >= Duration::from_millis(20),
            "closed desktop file watcher channels must not return immediately and hot-spin"
        );
        Ok(())
    }

    #[test]
    fn business_command_poll_sleep_backs_off_after_idle_round() {
        assert_eq!(
            business_command_poll_sleep_secs(0),
            BUSINESS_COMMAND_ACTIVE_POLL_SECS
        );
        assert_eq!(
            business_command_poll_sleep_secs(1),
            BUSINESS_COMMAND_ACTIVE_POLL_SECS
        );
        assert_eq!(
            business_command_poll_sleep_secs(u32::MAX),
            BUSINESS_COMMAND_ACTIVE_POLL_SECS
        );
    }

    #[test]
    fn browser_runtime_maintenance_sleep_backs_off_after_idle_round() {
        assert_eq!(
            browser_runtime_maintenance_sleep(0),
            Duration::from_millis(BROWSER_RUNTIME_ACTIVE_MAINTENANCE_INTERVAL_MS)
        );
        assert_eq!(
            browser_runtime_maintenance_sleep(1),
            Duration::from_secs(BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS)
        );
        assert_eq!(
            browser_runtime_maintenance_sleep(u32::MAX),
            Duration::from_secs(BROWSER_RUNTIME_IDLE_MAINTENANCE_INTERVAL_SECS)
        );
    }

    #[test]
    fn business_command_idle_gate_skips_when_no_pending_commands() {
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

            let mut last_source_stamp = None;
            assert!(
                business_commands_source_change(root.path(), &mut last_source_stamp)
                    .await
                    .expect("first command source stamp")
                    .is_none()
            );
            refresh_business_commands_source_stamp(root.path(), &mut last_source_stamp)
                .await
                .expect("refresh first command source stamp");
            assert!(
                business_commands_source_change(root.path(), &mut last_source_stamp)
                    .await
                    .expect("unchanged command source stamp")
                    .is_none()
            );

            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            commands
                .insert(json!({
                    "id": "cmd_idle_gate_pending",
                    "command_id": "cmd_idle_gate_pending",
                    "module": "ctox",
                    "command_type": "business_os.test",
                    "record_id": "",
                    "status": "pending_sync",
                    "inbound_channel": "ctox",
                    "payload": { "title": "Idle gate pending command" },
                    "client_context": {},
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert pending command");

            assert!(
                business_commands_source_change(root.path(), &mut last_source_stamp)
                    .await
                    .expect("changed command source stamp")
                    .is_some()
            );
            refresh_business_commands_source_stamp(root.path(), &mut last_source_stamp)
                .await
                .expect("refresh changed command source stamp");
            assert!(
                business_commands_source_change(root.path(), &mut last_source_stamp)
                    .await
                    .expect("pending command source stamp after refresh")
                    .is_some()
            );

            commands
                .incremental_upsert(json!({
                    "id": "cmd_idle_gate_pending",
                    "command_id": "cmd_idle_gate_pending",
                    "module": "ctox",
                    "command_type": "business_os.test",
                    "record_id": "",
                    "status": "accepted",
                    "inbound_channel": "ctox",
                    "payload": { "title": "Idle gate pending command" },
                    "client_context": {},
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("mark command accepted");
            refresh_business_commands_source_stamp(root.path(), &mut last_source_stamp)
                .await
                .expect("refresh accepted command source stamp");
            assert!(
                business_commands_source_change(root.path(), &mut last_source_stamp)
                    .await
                    .expect("accepted command source stamp")
                    .is_none()
            );
        });
    }

    #[test]
    fn business_command_idle_wait_wakes_on_rxdb_table_change() {
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

            let source_stamp = business_commands_source_stamp(root.path())
                .await
                .expect("command source stamp");
            assert!(
                source_stamp.table.table_name.is_some(),
                "business_commands table should be registered"
            );

            let wait_root = root.path().to_path_buf();
            let wait_stamp = source_stamp.clone();
            let waiter = tokio::spawn(async move {
                wait_for_business_command_wake(
                    &wait_root,
                    Some(&wait_stamp),
                    BUSINESS_COMMAND_IDLE_BACKOFF_AFTER_TICKS,
                )
                .await;
            });
            tokio::time::sleep(Duration::from_millis(25)).await;

            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            commands
                .insert(json!({
                    "id": "cmd_idle_wait_wake",
                    "command_id": "cmd_idle_wait_wake",
                    "module": "ctox",
                    "command_type": "business_os.test",
                    "record_id": "",
                    "status": "pending_sync",
                    "inbound_channel": "ctox",
                    "payload": { "title": "Idle wait wake command" },
                    "client_context": {},
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert pending command");

            tokio::time::timeout(Duration::from_millis(750), waiter)
                .await
                .expect("business command idle wait should wake on table notify")
                .expect("waiter task should not panic");
        });
    }

    #[test]
    fn runtime_installed_module_schemas_extend_native_collection_creators() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let module_dir = temp
            .path()
            .join("runtime/business-os/installed-modules/subscriptions");
        fs::create_dir_all(&module_dir)?;
        fs::write(
            module_dir.join("module.json"),
            serde_json::to_vec_pretty(&json!({
                "id": "subscriptions",
                "entry": "installed-modules/subscriptions/index.html",
                "install_scope": "installed",
                "collections": ["subscriptions_records"]
            }))?,
        )?;
        fs::write(
            module_dir.join("collections.schema.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_format": "ctox-business-os-module-collections-v1",
                "collections": {
                    "subscriptions_records": {
                        "primaryKey": "id",
                        "properties": {
                            "id": { "type": "string", "maxLength": 120 },
                            "title": { "type": "string" },
                            "updated_at_ms": { "type": "number" }
                        },
                        "required": ["id", "title"]
                    }
                }
            }))?,
        )?;

        let creators = collection_creators_for_root(temp.path());
        assert!(creators.contains_key("business_module_catalog"));
        let schema = &creators
            .get("subscriptions_records")
            .context("expected dynamic runtime app collection")?
            .schema;
        assert_eq!(schema.version, 0);
        assert_eq!(schema.primary_key.primary_field(), "id");
        assert_eq!(schema.schema_type, "object");
        Ok(())
    }

    // Backlog OS-C1: a runtime-installed module may declare per-collection
    // options next to the schema (`{ "schema": {...}, "conflictStrategy":
    // "field-merge" }`) — the same sibling convention as schema.js. The
    // native peer must register the UNWRAPPED schema (it never consumes the
    // strategy), so the advertised schema hash matches the browser's hash of
    // the schema.js schema and the collection is not quiesced.
    #[test]
    fn runtime_installed_module_schema_accepts_conflict_strategy_wrapper() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let module_dir = temp
            .path()
            .join("runtime/business-os/installed-modules/subscriptions");
        fs::create_dir_all(&module_dir)?;
        fs::write(
            module_dir.join("module.json"),
            serde_json::to_vec_pretty(&json!({
                "id": "subscriptions",
                "entry": "installed-modules/subscriptions/index.html",
                "install_scope": "installed",
                "collections": ["subscriptions_records"]
            }))?,
        )?;
        fs::write(
            module_dir.join("collections.schema.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_format": "ctox-business-os-module-collections-v1",
                "collections": {
                    "subscriptions_records": {
                        "conflictStrategy": "field-merge",
                        "schema": {
                            "primaryKey": "id",
                            "properties": {
                                "id": { "type": "string", "maxLength": 120 },
                                "title": { "type": "string" }
                            },
                            "required": ["id", "title"]
                        }
                    }
                }
            }))?,
        )?;

        let creators = collection_creators_for_root(temp.path());
        let schema = &creators
            .get("subscriptions_records")
            .context("wrapper-form collection must still register")?
            .schema;
        assert_eq!(schema.primary_key.primary_field(), "id");
        assert_eq!(schema.schema_type, "object");
        // The wrapper must be invisible to the parsed schema: no stray
        // `schema`/`conflictStrategy` members survive into the RxJsonSchema
        // (they would drift the native schema hash away from the browser's).
        assert!(schema.properties.contains_key("title"));
        assert!(!schema.properties.contains_key("schema"));
        assert!(!schema.properties.contains_key("conflictStrategy"));
        Ok(())
    }

    #[test]
    fn runtime_installed_module_schema_fingerprint_changes_with_schema_files() -> anyhow::Result<()>
    {
        let temp = tempfile::tempdir()?;
        let module_dir = temp
            .path()
            .join("runtime/business-os/installed-modules/subscriptions");
        fs::create_dir_all(&module_dir)?;
        fs::write(
            module_dir.join("module.json"),
            serde_json::to_vec_pretty(&json!({
                "id": "subscriptions",
                "entry": "installed-modules/subscriptions/index.html",
                "install_scope": "installed",
                "collections": ["subscriptions_records"]
            }))?,
        )?;
        fs::write(
            module_dir.join("collections.schema.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_format": "ctox-business-os-module-collections-v1",
                "collections": {
                    "subscriptions_records": {
                        "primaryKey": "id",
                        "properties": {
                            "id": { "type": "string", "maxLength": 120 },
                            "title": { "type": "string" }
                        },
                        "required": ["id", "title"]
                    }
                }
            }))?,
        )?;

        let first = runtime_installed_module_schema_fingerprint(temp.path())?;
        assert!(
            !native_peer_runtime_installed_schemas_changed(temp.path(), &first)?,
            "unchanged runtime app schemas must not force a respawn"
        );
        fs::write(
            module_dir.join("collections.schema.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_format": "ctox-business-os-module-collections-v1",
                "collections": {
                    "subscriptions_records": {
                        "primaryKey": "id",
                        "properties": {
                            "id": { "type": "string", "maxLength": 120 },
                            "title": { "type": "string" },
                            "renewal_date": { "type": "string" }
                        },
                        "required": ["id", "title"]
                    }
                }
            }))?,
        )?;
        assert!(
            native_peer_runtime_installed_schemas_changed(temp.path(), &first)?,
            "runtime app schema edits must force a native peer respawn"
        );
        Ok(())
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
        let sqlite_unique_error = rxdb::rx_error::new_rx_error(
            "SQLITE",
            Some(json!({
                "message": "UNIQUE constraint failed: ctox_business_os__business_commands__v0.id"
            })),
        );
        let other_error = rxdb::rx_error::new_rx_error("COL4", Some(json!({})));

        assert!(is_recoverable_projection_write_error(&conflict_error));
        assert!(is_recoverable_projection_write_error(&revision_error));
        assert!(is_recoverable_projection_write_error(&sqlite_unique_error));
        assert!(!is_recoverable_projection_write_error(&other_error));
    }

    #[test]
    fn stale_schema_startup_repair_reuses_an_existing_store() {
        let root = tempfile::tempdir().expect("temp root");
        std::fs::create_dir_all(root.path().join("runtime")).expect("runtime dir");
        let path = store::rxdb_store_path(root.path());
        drop(Connection::open(&path).expect("open empty rxdb sqlite"));

        let result =
            repair_stale_rxdb_collection_schema_versions(root.path()).expect("startup repair");
        assert_eq!(result["code"], "ctox_rxdb_stale_schema_versions");
        assert_eq!(result["repaired"], false);
        assert_eq!(result["repaired_tables"], 0);
        assert_eq!(result["repaired_triggers"], 0);
    }

    #[test]
    fn rxdb_schema_drift_repair_drops_stale_version_table_after_active_meta_upgrade() {
        let root = tempfile::tempdir().expect("temp root");
        std::fs::create_dir_all(root.path().join("runtime")).expect("runtime dir");
        let path = store::rxdb_store_path(root.path());
        let conn = Connection::open(&path).expect("open rxdb sqlite");
        conn.execute(
            "CREATE TABLE ctox_business_os___rxdb_internal__v0 (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .expect("create internal store");
        conn.execute(
            "CREATE TABLE ctox_business_os__business_commands__v0 (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL
            )",
            [],
        )
        .expect("create stale v0 table");
        conn.execute(
            "CREATE TABLE ctox_business_os__business_commands__v1 (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL
            )",
            [],
        )
        .expect("create active v1 table");
        conn.execute_batch(
            "CREATE TRIGGER sync_commands_v1_to_v0_insert
             AFTER INSERT ON ctox_business_os__business_commands__v1
             FOR EACH ROW
             BEGIN
                 INSERT OR REPLACE INTO ctox_business_os__business_commands__v0 (id, data)
                 VALUES (NEW.id, NEW.data);
             END;",
        )
        .expect("create stale v1-to-v0 trigger");
        conn.execute(
            "INSERT INTO ctox_business_os___rxdb_internal__v0 (id, data, deleted)
             VALUES ('collection|business_commands-1', ?1, 0)",
            [json!({
                "id": "collection|business_commands-1",
                "key": "business_commands-1",
                "context": "collection",
                "data": {
                    "name": "business_commands",
                    "version": 1,
                    "schemaHash": "test"
                },
                "_deleted": false
            })
            .to_string()],
        )
        .expect("insert active v1 collection meta");
        conn.execute(
            "INSERT INTO ctox_business_os__business_commands__v0 (id, data)
             VALUES ('cmd_stale', ?1)",
            [json!({"id": "cmd_stale", "updated_at_ms": 100, "status": "accepted"}).to_string()],
        )
        .expect("insert stale row");
        conn.execute(
            "INSERT INTO ctox_business_os__business_commands__v1 (id, data)
             VALUES ('cmd_active', ?1)",
            [json!({"id": "cmd_active", "updated_at_ms": 200, "status": "failed"}).to_string()],
        )
        .expect("insert active row");
        drop(conn);

        let dry_run = repair_optional_rxdb_collection_schema_drift(
            root.path(),
            "business_commands",
            true,
            true,
        )
        .expect("dry-run repair");
        assert_eq!(dry_run["repaired"], false);
        assert_eq!(dry_run["stale_tables"].as_array().unwrap().len(), 1);
        assert_eq!(dry_run["stale_triggers"].as_array().unwrap().len(), 1);
        let conn = Connection::open(&path).expect("reopen rxdb sqlite after dry-run");
        assert!(sqlite_table_exists(&conn, "ctox_business_os__business_commands__v0").unwrap());
        assert!(sqlite_trigger_exists(&conn, "sync_commands_v1_to_v0_insert").unwrap());
        drop(conn);

        let refused = repair_optional_rxdb_collection_schema_drift(
            root.path(),
            "business_commands",
            false,
            false,
        )
        .expect_err("required collection repair must require force");
        assert!(
            refused.to_string().contains("without --force"),
            "unexpected refusal: {refused:#}"
        );

        let applied = repair_optional_rxdb_collection_schema_drift(
            root.path(),
            "business_commands",
            false,
            true,
        )
        .expect("apply repair");
        assert_eq!(applied["repaired"], true);
        assert_eq!(applied["repaired_tables"], 1);
        assert_eq!(applied["repaired_triggers"], 1);

        let conn = Connection::open(&path).expect("reopen rxdb sqlite after apply");
        assert!(!sqlite_table_exists(&conn, "ctox_business_os__business_commands__v0").unwrap());
        assert!(sqlite_table_exists(&conn, "ctox_business_os__business_commands__v1").unwrap());
        assert!(!sqlite_trigger_exists(&conn, "sync_commands_v1_to_v0_insert").unwrap());
        conn.execute(
            "INSERT INTO ctox_business_os__business_commands__v1 (id, data)
             VALUES ('cmd_after_repair', ?1)",
            [
                json!({"id": "cmd_after_repair", "updated_at_ms": 300, "status": "failed"})
                    .to_string(),
            ],
        )
        .expect("active v1 write must not reference removed stale v0 table");
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
        assert_eq!(
            status
                .pointer("/performance/schema")
                .and_then(Value::as_str),
            Some("ctox.native_peer.performance.v1")
        );
        assert_eq!(
            status
                .pointer("/performance/rxdb_sqlite/schema")
                .and_then(Value::as_str),
            Some("ctox.rxdb.sqlite.runtime_counters.v1")
        );
        assert_eq!(
            status
                .pointer("/performance/rxdb_subjects/schema")
                .and_then(Value::as_str),
            Some("ctox.rxdb.subjects.runtime_counters.v1")
        );
        assert!(status
            .pointer("/performance/rxdb_subjects/lagged_items_total")
            .and_then(Value::as_u64)
            .is_some());
        assert!(is_native_peer_running_for_root(root.path()));
    }

    #[test]
    fn native_peer_sync_config_change_detects_room_rotation() {
        let root = tempfile::tempdir().expect("temp root");
        let initial = store::sync_config(root.path()).expect("initial sync config");
        assert!(
            !native_peer_sync_config_changed(
                root.path(),
                &initial.sync_room,
                &initial.signaling_room_password,
                &initial.signaling_urls,
            )
            .expect("unchanged config check"),
            "current config must match itself"
        );

        let rotated =
            store::rotate_sync_room_password(root.path()).expect("rotate sync room password");
        assert_ne!(initial.sync_room, rotated.sync_room);
        assert!(
            native_peer_sync_config_changed(
                root.path(),
                &initial.sync_room,
                &initial.signaling_room_password,
                &initial.signaling_urls,
            )
            .expect("rotated config check"),
            "room rotation must force native peer respawn"
        );
    }

    #[test]
    fn normalized_signaling_urls_ignores_empty_entries() {
        let urls = vec![
            " wss://signaling.ctox.dev ".to_string(),
            "".to_string(),
            "  ".to_string(),
        ];
        assert_eq!(
            normalized_signaling_urls(&urls),
            vec!["wss://signaling.ctox.dev".to_string()]
        );
    }

    #[test]
    fn native_signaling_circuit_opens_and_allows_one_half_open_probe() {
        let mut breaker = NativePeerCircuitBreaker::default();
        assert_eq!(breaker.before_attempt("epoch-a".to_string(), 1_000), None);
        for attempt in 1..=NATIVE_PEER_CIRCUIT_FAILURE_THRESHOLD {
            breaker.record_failure(
                NativePeerSignalingFailure::Retryable,
                format!("signaling unavailable {attempt}"),
                1_000,
            );
        }
        assert_eq!(breaker.state, NativePeerCircuitState::Open);
        assert!(breaker
            .before_attempt("epoch-a".to_string(), 2_000)
            .is_some());
        let probe_at = 1_000 + NATIVE_PEER_CIRCUIT_OPEN_SECS * 1_000;
        assert_eq!(
            breaker.before_attempt("epoch-a".to_string(), probe_at),
            None
        );
        assert_eq!(breaker.state, NativePeerCircuitState::HalfOpen);
        breaker.record_failure(
            NativePeerSignalingFailure::Retryable,
            "signaling still unavailable".to_string(),
            probe_at,
        );
        assert_eq!(breaker.state, NativePeerCircuitState::Open);
        assert_eq!(
            breaker.next_probe_at_ms,
            Some(probe_at + NATIVE_PEER_CIRCUIT_OPEN_SECS * 1_000)
        );
    }

    #[test]
    fn native_signaling_terminal_circuit_requires_config_epoch_change() {
        let mut breaker = NativePeerCircuitBreaker::default();
        assert_eq!(breaker.before_attempt("epoch-a".to_string(), 10), None);
        breaker.record_failure(
            NativePeerSignalingFailure::Permanent,
            "signaling join rejected: protocol mismatch".to_string(),
            10,
        );
        assert!(breaker
            .before_attempt("epoch-a".to_string(), u64::MAX)
            .is_some());
        assert_eq!(breaker.state, NativePeerCircuitState::Open);
        assert_eq!(breaker.before_attempt("epoch-b".to_string(), 20), None);
        assert_eq!(breaker.state, NativePeerCircuitState::Closed);
        assert_eq!(breaker.consecutive_failures, 0);
    }

    #[test]
    fn native_signaling_failure_classification_matches_browser_policy() {
        assert_eq!(
            classify_native_peer_failure("signaling token expired"),
            NativePeerSignalingFailure::Retryable
        );
        assert_eq!(
            classify_native_peer_failure("signaling join rejected: peer revoked"),
            NativePeerSignalingFailure::Permanent
        );
        assert_eq!(
            classify_native_peer_failure("required collection registration failed"),
            NativePeerSignalingFailure::NotSignaling
        );
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
            "rxdb-rs-test-peer",
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
            "rxdb-rs-test-peer"
        );
        assert_eq!(
            parsed
                .query_pairs()
                .find(|(key, _)| key == "native_peer_id")
                .unwrap()
                .1,
            "rxdb-rs-test-peer"
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

    #[test]
    fn native_declarative_migration_matches_browser_operations() {
        let old_doc = json!({
            "id": "cmd-1",
            "module": "creator",
            "nested": {
                "enabled": "yes"
            },
            "already": "kept"
        });
        let spec = json!({
            "operations": [
                {
                    "op": "set_from_first_truthy",
                    "field": "inbound_channel",
                    "paths": ["inbound_channel", "module"],
                    "default": ""
                },
                {
                    "op": "set_boolean",
                    "field": "nested_enabled",
                    "path": "nested.enabled"
                }
            ]
        });

        let migrated = apply_native_declarative_migration(&old_doc, &spec)
            .expect("native declarative migration applies");

        assert_eq!(
            migrated.get("inbound_channel").and_then(Value::as_str),
            Some("creator")
        );
        assert_eq!(
            migrated.get("nested_enabled").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            migrated.get("already").and_then(Value::as_str),
            Some("kept")
        );
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

    #[tokio::test]
    async fn hot_business_os_schema_indexes_have_sqlite_query_plan_guards() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = root.path().join("hot-indexes.sqlite3");
        let database = open_test_database(database_path.clone())
            .await
            .expect("open test database");
        database
            .add_collections(HashMap::from([
                (
                    "business_commands".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("business_commands", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "ctox_queue_tasks".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("ctox_queue_tasks", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "desktop_file_chunks".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("desktop_file_chunks", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "document_blob_chunks".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("document_blob_chunks", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "spreadsheet_blob_chunks".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("spreadsheet_blob_chunks", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "browser_frames".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("browser_frames", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "browser_input_events".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("browser_input_events", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
            ]))
            .await
            .expect("add hot collections");
        let commands = database
            .collection("business_commands")
            .expect("business_commands collection");
        let queue_tasks = database
            .collection("ctox_queue_tasks")
            .expect("ctox_queue_tasks collection");
        let chunks = database
            .collection("desktop_file_chunks")
            .expect("desktop_file_chunks collection");
        let document_chunks = database
            .collection("document_blob_chunks")
            .expect("document_blob_chunks collection");
        let spreadsheet_chunks = database
            .collection("spreadsheet_blob_chunks")
            .expect("spreadsheet_blob_chunks collection");
        let browser_frames = database
            .collection("browser_frames")
            .expect("browser_frames collection");
        let browser_input_events = database
            .collection("browser_input_events")
            .expect("browser_input_events collection");
        for idx in 0..200 {
            let command_status = if idx == 0 {
                "pending_sync"
            } else {
                "completed"
            };
            let task_status = if idx == 0 { "queued" } else { "done" };
            commands
                .incremental_upsert(json!({
                    "id": format!("cmd_doc_{idx}"),
                    "command_id": format!("cmd_{idx}"),
                    "module": "ctox",
                    "command_type": "queue",
                    "record_id": format!("record_{idx}"),
                    "status": command_status,
                    "updated_at_ms": idx,
                }))
                .await
                .expect("insert business command");
            queue_tasks
                .incremental_upsert(json!({
                    "id": format!("task_doc_{idx}"),
                    "command_id": format!("cmd_{idx}"),
                    "command_type": "queue",
                    "title": format!("Task {idx}"),
                    "status": task_status,
                    "module": "ctox",
                    "updated_at_ms": idx,
                }))
                .await
                .expect("insert queue task");
            chunks
                .incremental_upsert(json!({
                    "id": format!("file_1_gen_1_{idx}"),
                    "file_id": "file_1",
                    "generation_id": "gen_1",
                    "idx": idx,
                    "total": 200,
                    "encoding": "base64",
                    "data": "",
                    "created_at_ms": idx,
                }))
                .await
                .expect("insert desktop file chunk");
            document_chunks
                .incremental_upsert(json!({
                    "id": format!("blob_1_{idx}"),
                    "blob_id": "blob_1",
                    "document_id": "document_1",
                    "version_id": "version_1",
                    "idx": idx,
                    "total": 200,
                    "mime_type": "application/octet-stream",
                    "encoding": "base64",
                    "data": "",
                    "created_at_ms": idx,
                }))
                .await
                .expect("insert document blob chunk");
            spreadsheet_chunks
                .incremental_upsert(json!({
                    "id": format!("sheet_blob_1_{idx}"),
                    "blob_id": "sheet_blob_1",
                    "spreadsheet_id": "spreadsheet_1",
                    "version_id": "version_1",
                    "idx": idx,
                    "total": 200,
                    "mime_type": "application/octet-stream",
                    "encoding": "base64",
                    "data": "",
                    "created_at_ms": idx,
                }))
                .await
                .expect("insert spreadsheet blob chunk");
            browser_frames
                .incremental_upsert(json!({
                    "id": format!("frame_{idx}"),
                    "session_id": "session_1",
                    "tab_id": "tab_1",
                    "seq": idx,
                    "mime_type": "image/png",
                    "encoding": "base64",
                    "data": "",
                    "width": 10,
                    "height": 10,
                    "captured_at_ms": idx,
                    "expires_at_ms": idx,
                    "updated_at_ms": idx,
                }))
                .await
                .expect("insert browser frame");
            browser_input_events
                .incremental_upsert(json!({
                    "id": format!("input_{idx}"),
                    "session_id": "session_1",
                    "tab_id": "tab_1",
                    "seq": idx,
                    "type": "click",
                    "status": if idx == 0 { "pending" } else { "consumed" },
                    "created_at_ms": idx,
                    "updated_at_ms": idx,
                }))
                .await
                .expect("insert browser input event");
        }
        let conn = Connection::open(&database_path).expect("open sqlite");
        conn.execute_batch("ANALYZE")
            .expect("analyze hot index test db");
        let business_commands_table = rxdb_test_table_name(&conn, "business_commands", 1);
        let ctox_queue_tasks_table = rxdb_test_table_name(&conn, "ctox_queue_tasks", 0);
        let desktop_file_chunks_table = rxdb_test_table_name(&conn, "desktop_file_chunks", 0);
        let document_blob_chunks_table = rxdb_test_table_name(&conn, "document_blob_chunks", 0);
        let spreadsheet_blob_chunks_table =
            rxdb_test_table_name(&conn, "spreadsheet_blob_chunks", 0);
        let browser_frames_table = rxdb_test_table_name(&conn, "browser_frames", 0);
        let browser_input_events_table = rxdb_test_table_name(&conn, "browser_input_events", 0);

        assert_plan_uses_index(
            &conn,
            &format!(
                r#"
            SELECT data FROM {}
            WHERE deleted = 0
              AND json_extract(data, '$.status') = ?
            LIMIT 10
            "#,
                quote_sqlite_identifier(&business_commands_table)
            ),
            &[&"pending_sync"],
            &format!("{business_commands_table}_json__deleted__status"),
        );
        assert_plan_uses_index(
            &conn,
            &format!(
                r#"
            SELECT data FROM {}
            WHERE deleted = 0
              AND json_extract(data, '$.command_id') = ?
            LIMIT 1
            "#,
                quote_sqlite_identifier(&business_commands_table)
            ),
            &[&"cmd_1"],
            &format!("{business_commands_table}_json__deleted__command_id"),
        );
        assert_plan_uses_index(
            &conn,
            &format!(
                r#"
            SELECT data FROM {}
            WHERE deleted = 0
              AND json_extract(data, '$.status') = ?
            LIMIT 10
            "#,
                quote_sqlite_identifier(&ctox_queue_tasks_table)
            ),
            &[&"queued"],
            &format!("{ctox_queue_tasks_table}_json__deleted__status"),
        );
        assert_plan_uses_index(
            &conn,
            &format!(
                r#"
            SELECT data FROM {}
            WHERE deleted = 0
              AND json_extract(data, '$.command_id') = ?
            LIMIT 1
            "#,
                quote_sqlite_identifier(&ctox_queue_tasks_table)
            ),
            &[&"cmd_1"],
            &format!("{ctox_queue_tasks_table}_json__deleted__command_id"),
        );
        assert_plan_uses_index(
            &conn,
            &format!(
                r#"
            SELECT data FROM {}
            WHERE deleted = 0
              AND json_extract(data, '$.file_id') = ?
              AND json_extract(data, '$.generation_id') = ?
              AND json_extract(data, '$.idx') >= ?
              AND json_extract(data, '$.idx') < ?
            ORDER BY json_extract(data, '$.idx') ASC
            "#,
                quote_sqlite_identifier(&desktop_file_chunks_table)
            ),
            &[&"file_1", &"gen_1", &0_i64, &100_i64],
            &format!(
                "{desktop_file_chunks_table}_json__deleted__file_id__generation_id__idx__id_idx"
            ),
        );
        let (document_lower, document_upper) = chunk_id_prefix_bounds("blob_1");
        assert_plan_uses_primary_key_range_without_temp_sort(
            &conn,
            &format!(
                r#"
            SELECT data FROM {}
            WHERE id >= ?1
              AND id < ?2
              AND deleted = 0
            ORDER BY id
            LIMIT ?3
            "#,
                quote_sqlite_identifier(&document_blob_chunks_table)
            ),
            &[
                &document_lower as &dyn rusqlite::ToSql,
                &document_upper as &dyn rusqlite::ToSql,
                &100_i64 as &dyn rusqlite::ToSql,
            ],
        );
        let (spreadsheet_lower, spreadsheet_upper) = chunk_id_prefix_bounds("sheet_blob_1");
        assert_plan_uses_primary_key_range_without_temp_sort(
            &conn,
            &format!(
                r#"
            SELECT data FROM {}
            WHERE id >= ?1
              AND id < ?2
              AND deleted = 0
            ORDER BY id
            LIMIT ?3
            "#,
                quote_sqlite_identifier(&spreadsheet_blob_chunks_table)
            ),
            &[
                &spreadsheet_lower as &dyn rusqlite::ToSql,
                &spreadsheet_upper as &dyn rusqlite::ToSql,
                &100_i64 as &dyn rusqlite::ToSql,
            ],
        );
        assert_plan_uses_index(
            &conn,
            &format!(
                r#"
            SELECT data FROM {}
            WHERE deleted = 0
              AND json_extract(data, '$.expires_at_ms') < ?
            LIMIT 256
            "#,
                quote_sqlite_identifier(&browser_frames_table)
            ),
            &[&1_i64],
            &format!("{browser_frames_table}_json__deleted__expires_at_ms"),
        );
        assert_plan_uses_index(
            &conn,
            &format!(
                r#"
            SELECT data FROM {}
            WHERE deleted = 0
              AND json_extract(data, '$.session_id') = ?
              AND json_extract(data, '$.status') = ?
            ORDER BY json_extract(data, '$.seq') ASC
            LIMIT 64
            "#,
                quote_sqlite_identifier(&browser_input_events_table)
            ),
            &[&"session_1", &"pending"],
            &format!("{browser_input_events_table}_json__deleted__session_id__status__seq"),
        );
    }

    #[tokio::test]
    async fn browser_runtime_gc_redacts_frames_and_retires_old_input_events() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = root.path().join("browser-runtime-gc.sqlite3");
        let database = open_test_database(database_path.clone())
            .await
            .expect("open test database");
        database
            .add_collections(HashMap::from([
                (
                    "browser_frames".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("browser_frames", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "browser_input_events".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("browser_input_events", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
            ]))
            .await
            .expect("add browser runtime collections");
        let frames = database
            .collection("browser_frames")
            .expect("browser_frames collection");
        frames
            .incremental_upsert(json!({
                "id": "expired_frame",
                "session_id": "session_1",
                "tab_id": "tab_1",
                "seq": 1,
                "mime_type": "image/png",
                "encoding": "base64",
                "data": "AAAA",
                "width": 10,
                "height": 10,
                "captured_at_ms": 1,
                "expires_at_ms": 1,
                "updated_at_ms": 1,
                "size_bytes": 3,
                "frame_hash": "hash",
            }))
            .await
            .expect("insert expired frame");
        let events = database
            .collection("browser_input_events")
            .expect("browser_input_events collection");
        for (id, status, created_at_ms) in [
            ("old_consumed", "consumed", 1_u64),
            ("old_failed", "failed", 1_u64),
            ("old_pending", "pending", 1_u64),
            (
                "recent_consumed",
                "consumed",
                now_ms() as u64 - (BROWSER_INPUT_EVENT_RETENTION_SECS * 500),
            ),
        ] {
            events
                .incremental_upsert(json!({
                    "id": id,
                    "session_id": "session_1",
                    "tab_id": "tab_1",
                    "seq": 1,
                    "type": "click",
                    "status": status,
                    "created_at_ms": created_at_ms,
                    "updated_at_ms": created_at_ms,
                }))
                .await
                .expect("insert browser input event");
        }

        assert_eq!(gc_expired_browser_frames(&database).await.unwrap(), 1);
        assert_eq!(
            gc_consumed_browser_input_events(&database).await.unwrap(),
            2
        );

        let conn = Connection::open(&database_path).expect("open sqlite");
        let frame_table = rxdb_test_table_name(&conn, "browser_frames", 0);
        let frame: (i64, String, i64, String) = conn
            .query_row(
                &format!(
                    "SELECT deleted, json_extract(data, '$.data'), CAST(json_extract(data, '$.size_bytes') AS INTEGER), json_extract(data, '$.encoding') FROM {} WHERE id = ?1",
                    quote_sqlite_identifier(&frame_table)
                ),
                params!["expired_frame"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("expired frame row");
        assert_eq!(frame, (1, String::new(), 0, "redacted".to_string()));

        let input_table = rxdb_test_table_name(&conn, "browser_input_events", 0);
        let deleted_state = |id: &str| -> i64 {
            conn.query_row(
                &format!(
                    "SELECT deleted FROM {} WHERE id = ?1",
                    quote_sqlite_identifier(&input_table)
                ),
                params![id],
                |row| row.get(0),
            )
            .expect("input event row")
        };
        assert_eq!(deleted_state("old_consumed"), 1);
        assert_eq!(deleted_state("old_failed"), 1);
        assert_eq!(deleted_state("old_pending"), 0);
        assert_eq!(deleted_state("recent_consumed"), 0);
    }

    #[tokio::test]
    async fn browser_session_recovery_uses_indexed_active_query_without_fallback() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = root.path().join("browser-session-recovery.sqlite3");
        let database = open_test_database(database_path.clone())
            .await
            .expect("open test database");
        database
            .add_collections(HashMap::from([(
                "browser_sessions".to_string(),
                RxCollectionCreator {
                    schema: business_os_schema("browser_sessions", "id"),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .expect("add browser_sessions collection");
        let sessions = database
            .collection("browser_sessions")
            .expect("browser_sessions collection");
        for (id, status, runtime_status) in [
            ("active_stale", "active", "active"),
            ("stopped", "stopped", "stopped"),
            ("already_disconnected", "disconnected", "disconnected"),
            ("requested", "requested", "pending_command"),
            ("synthetic", "synthetic", "not_started"),
        ] {
            sessions
                .incremental_upsert(json!({
                    "id": id,
                    "owner_user_id": "ctox",
                    "controller_user_id": "ctox",
                    "status": status,
                    "runtime_status": runtime_status,
                    "current_tab_id": "tab_1",
                    "current_url": "https://example.com",
                    "title": id,
                    "viewport_w": 1280,
                    "viewport_h": 720,
                    "device_scale_factor": 1,
                    "frame_rate_target": 0,
                    "active_frame_id": "",
                    "last_frame_seq": 0,
                    "last_input_seq": 0,
                    "pending_input_count": 0,
                    "error": "",
                    "payload": {},
                    "created_at_ms": 1,
                    "updated_at_ms": 1,
                }))
                .await
                .expect("insert browser session");
        }

        let fallback_calls_before =
            rxdb::storage::sqlite::instance::sqlite_runtime_counters_snapshot()
                .get("query_fallback_calls")
                .and_then(Value::as_u64)
                .unwrap_or(0);
        recover_stale_browser_sessions(&database)
            .await
            .expect("recover stale browser sessions");
        let fallback_calls_after =
            rxdb::storage::sqlite::instance::sqlite_runtime_counters_snapshot()
                .get("query_fallback_calls")
                .and_then(Value::as_u64)
                .unwrap_or(0);
        assert_eq!(
            fallback_calls_after, fallback_calls_before,
            "browser session recovery must not use the unsupported Mango fallback"
        );

        let conn = Connection::open(&database_path).expect("open sqlite");
        let session_table = rxdb_test_table_name(&conn, "browser_sessions", 0);
        let status_for = |id: &str| -> String {
            conn.query_row(
                &format!(
                    "SELECT json_extract(data, '$.status') FROM {} WHERE id = ?1",
                    quote_sqlite_identifier(&session_table)
                ),
                params![id],
                |row| row.get(0),
            )
            .expect("browser session status")
        };

        assert_eq!(status_for("active_stale"), "disconnected");
        assert_eq!(status_for("stopped"), "stopped");
        assert_eq!(status_for("already_disconnected"), "disconnected");
        assert_eq!(status_for("requested"), "requested");
        assert_eq!(status_for("synthetic"), "synthetic");
    }

    fn rxdb_test_table_name(conn: &Connection, collection: &str, version: i64) -> String {
        conn.query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE ?1 ORDER BY name",
            params![format!("%__{collection}__v{version}")],
            |row| row.get::<_, String>(0),
        )
        .expect("RxDB collection table exists")
    }

    fn quote_sqlite_identifier(identifier: &str) -> String {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }

    fn assert_plan_uses_index(
        conn: &Connection,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
        expected_index: &str,
    ) {
        let plan = sqlite_query_plan(conn, sql, params);
        assert!(
            plan.contains(expected_index),
            "expected SQLite plan to use {expected_index}, got:\n{plan}\nindexes:\n{}",
            sqlite_index_debug_list(conn)
        );
    }

    fn assert_plan_uses_primary_key_range_without_temp_sort(
        conn: &Connection,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
    ) {
        let plan = sqlite_query_plan(conn, sql, params);
        let normalized = plan.to_ascii_uppercase();
        assert!(
            normalized.contains("SEARCH "),
            "expected SQLite plan to search by primary-key range, got:\n{plan}\nindexes:\n{}",
            sqlite_index_debug_list(conn)
        );
        assert!(
            !normalized.starts_with("SCAN ")
                && !normalized.contains("\nSCAN ")
                && !normalized.contains(" SCAN "),
            "expected primary-key demand chunk query to avoid table scans, got:\n{plan}\nindexes:\n{}",
            sqlite_index_debug_list(conn)
        );
        assert!(
            !normalized.contains("USE TEMP B-TREE"),
            "expected primary-key demand chunk query to avoid temp sort, got:\n{plan}\nindexes:\n{}",
            sqlite_index_debug_list(conn)
        );
        assert!(
            normalized.contains("USING INDEX")
                || normalized.contains("USING COVERING INDEX")
                || normalized.contains("PRIMARY KEY")
                || normalized.contains("AUTOINDEX"),
            "expected SQLite plan to use an index for the primary-key range, got:\n{plan}\nindexes:\n{}",
            sqlite_index_debug_list(conn)
        );
    }

    fn sqlite_query_plan(conn: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> String {
        let mut statement = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .expect("prepare explain query plan");
        statement
            .query_map(params, |row| row.get::<_, String>(3))
            .expect("query explain plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect explain plan")
            .join("\n")
    }

    fn sqlite_index_debug_list(conn: &Connection) -> String {
        let mut statement = conn
            .prepare(
                "SELECT name, COALESCE(sql, '') FROM sqlite_master \
                 WHERE type = 'index' ORDER BY name",
            )
            .expect("prepare index debug list");
        statement
            .query_map([], |row| {
                Ok(format!(
                    "{}: {}",
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?
                ))
            })
            .expect("query index debug list")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect index debug list")
            .join("\n")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn demand_file_source_streams_decoded_chunks_in_idx_order() {
        // Phase 4: a registered file stream source must read the chunk docs by
        // the collection's key field, order by `idx`, base64-decode each `data`,
        // and emit the raw bytes in order. This is what makes `rxdb.file.fetch`
        // serve bytes instead of FILE_NOT_FOUND.
        let root = tempfile::tempdir().expect("temp root");
        let database_path = store::rxdb_store_path(root.path());
        fs::create_dir_all(database_path.parent().expect("rxdb parent")).expect("runtime dir");
        let database =
            open_test_database_with_name(database_path, RXDB_SQLITE_DATABASE_NAME.to_string())
                .await
                .expect("open db");
        database
            .add_collections(HashMap::from([
                (
                    "desktop_files".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("desktop_files", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "desktop_file_chunks".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("desktop_file_chunks", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
            ]))
            .await
            .expect("add desktop file collections");
        let files = database
            .collection("desktop_files")
            .expect("desktop_files collection");
        let mut payload = vec![b'a'; DESKTOP_FILE_CHUNK_DECODED_SIZE as usize];
        payload.extend_from_slice(b"world!");
        files
            .incremental_upsert(json!({
                "id": "f1",
                "name": "file.txt",
                "kind": "file",
                "size_bytes": payload.len() as u64,
                "content_state": "available",
                "content_generation_id": "gen-active",
                "created_at_ms": 1,
                "updated_at_ms": 1,
            }))
            .await
            .expect("upsert file");
        let chunks = database
            .collection("desktop_file_chunks")
            .expect("desktop_file_chunks collection");
        // Insert production-shaped chunks out of order to prove the source
        // sorts by `idx` while range fetches can still load only the touched
        // chunk ids.
        let enc = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let encoded = enc(&payload);
        let chunk_payloads = if encoded.is_empty() {
            vec![""]
        } else {
            encoded
                .as_bytes()
                .chunks(DESKTOP_FILE_CHUNK_SIZE)
                .map(|chunk| std::str::from_utf8(chunk).unwrap())
                .collect::<Vec<_>>()
        };
        for (idx, data) in chunk_payloads.iter().enumerate().rev() {
            chunks
                .incremental_upsert(json!({
                    "id": format!("f1_gen-active_{idx}"),
                    "file_id": "f1",
                    "generation_id": "gen-active",
                    "idx": idx,
                    "total": chunk_payloads.len(),
                    "encoding": "base64",
                    "data": data,
                    "created_at_ms": 1,
                }))
                .await
                .expect("upsert chunk");
        }

        let mut collected: Vec<u8> = Vec::new();
        stream_demand_file_chunks(
            root.path(),
            "desktop_file_chunks",
            "file_id",
            "f1",
            None,
            &mut |bytes| {
                collected.extend_from_slice(bytes);
                Ok(true)
            },
        )
        .expect("stream chunks");
        assert_eq!(collected, payload);

        // A byte range clips the stream across chunk boundaries.
        DEMAND_FILE_FETCH_METRICS.reset();
        let mut ranged: Vec<u8> = Vec::new();
        stream_demand_file_chunks(
            root.path(),
            "desktop_file_chunks",
            "file_id",
            "f1",
            Some(&FileRange {
                offset: DESKTOP_FILE_CHUNK_DECODED_SIZE,
                length: 5,
            }),
            &mut |bytes| {
                ranged.extend_from_slice(bytes);
                Ok(true)
            },
        )
        .expect("stream ranged");
        assert_eq!(String::from_utf8(ranged).unwrap(), "world");
        let metrics = DEMAND_FILE_FETCH_METRICS.snapshot();
        assert_eq!(
            metrics.pointer("/ranged_requests").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            metrics.pointer("/rows_loaded").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            metrics.pointer("/chunks_decoded").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            metrics.pointer("/bytes_requested").and_then(Value::as_u64),
            Some(5)
        );
        assert_eq!(
            metrics.pointer("/bytes_emitted").and_then(Value::as_u64),
            Some(5)
        );
        assert!(
            metrics
                .pointer("/bytes_decoded")
                .and_then(Value::as_u64)
                .is_some_and(|bytes| bytes < DESKTOP_FILE_CHUNK_DECODED_SIZE),
            "range fetch must decode only the touched tail chunk: {metrics}"
        );

        // Unknown file → no bytes emitted (dispatcher maps that to FILE_NOT_FOUND).
        let mut none: Vec<u8> = Vec::new();
        stream_demand_file_chunks(
            root.path(),
            "desktop_file_chunks",
            "file_id",
            "missing",
            None,
            &mut |bytes| {
                none.extend_from_slice(bytes);
                Ok(true)
            },
        )
        .expect("stream missing");
        assert!(none.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn demand_file_source_streams_active_desktop_file_generation() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = store::rxdb_store_path(root.path());
        fs::create_dir_all(database_path.parent().expect("rxdb parent")).expect("runtime dir");
        let database =
            open_test_database_with_name(database_path, RXDB_SQLITE_DATABASE_NAME.to_string())
                .await
                .expect("open db");
        database
            .add_collections(HashMap::from([
                (
                    "desktop_files".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("desktop_files", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "desktop_file_chunks".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("desktop_file_chunks", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
            ]))
            .await
            .expect("add desktop file collections");
        let files = database
            .collection("desktop_files")
            .expect("desktop_files collection");
        files
            .incremental_upsert(json!({
                "id": "f1",
                "name": "file.txt",
                "kind": "file",
                "content_state": "available",
                "content_generation_id": "gen-active",
                "created_at_ms": 1,
                "updated_at_ms": 1,
            }))
            .await
            .expect("upsert file");
        let chunks = database
            .collection("desktop_file_chunks")
            .expect("desktop_file_chunks collection");
        let enc = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        for (generation_id, payload) in [("gen-old", &b"old"[..]), ("gen-active", &b"active"[..])] {
            chunks
                .incremental_upsert(json!({
                    "id": format!("f1_{generation_id}_0"),
                    "file_id": "f1",
                    "generation_id": generation_id,
                    "idx": 0,
                    "total": 1,
                    "encoding": "base64",
                    "data": enc(payload),
                    "created_at_ms": 1,
                }))
                .await
                .expect("upsert chunk");
        }

        let mut collected: Vec<u8> = Vec::new();
        stream_demand_file_chunks(
            root.path(),
            "desktop_file_chunks",
            "file_id",
            "f1",
            None,
            &mut |bytes| {
                collected.extend_from_slice(bytes);
                Ok(true)
            },
        )
        .expect("stream active generation");
        assert_eq!(String::from_utf8(collected).unwrap(), "active");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn demand_file_source_streams_blob_chunks_by_primary_key_prefix() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = store::rxdb_store_path(root.path());
        fs::create_dir_all(database_path.parent().expect("rxdb parent")).expect("runtime dir");
        let database =
            open_test_database_with_name(database_path, RXDB_SQLITE_DATABASE_NAME.to_string())
                .await
                .expect("open db");
        database
            .add_collections(HashMap::from([
                (
                    "document_blob_chunks".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("document_blob_chunks", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
                (
                    "spreadsheet_blob_chunks".to_string(),
                    RxCollectionCreator {
                        schema: business_os_schema("spreadsheet_blob_chunks", "id"),
                        conflict_handler: None,
                        options: HashMap::new(),
                    },
                ),
            ]))
            .await
            .expect("add blob chunk collections");
        let enc = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let document_chunks = database
            .collection("document_blob_chunks")
            .expect("document_blob_chunks collection");
        for (id, blob_id, idx, data) in [
            ("blob_1_1", "blob_1", 1_u64, enc(b"world")),
            ("blob_1_extra_0", "blob_1_extra", 0_u64, enc(b"wrong")),
            ("blob_1_0", "blob_1", 0_u64, enc(b"hello ")),
        ] {
            document_chunks
                .incremental_upsert(json!({
                    "id": id,
                    "blob_id": blob_id,
                    "document_id": "document_1",
                    "version_id": "version_1",
                    "idx": idx,
                    "total": 2,
                    "mime_type": "application/octet-stream",
                    "encoding": "base64",
                    "data": data,
                    "created_at_ms": idx,
                }))
                .await
                .expect("insert document blob chunk");
        }
        let spreadsheet_chunks = database
            .collection("spreadsheet_blob_chunks")
            .expect("spreadsheet_blob_chunks collection");
        for (id, blob_id, idx, data) in [
            ("sheet_blob_1_1", "sheet_blob_1", 1_u64, enc(b"data")),
            (
                "sheet_blob_1_shadow_0",
                "sheet_blob_1_shadow",
                0_u64,
                enc(b"wrong"),
            ),
            ("sheet_blob_1_0", "sheet_blob_1", 0_u64, enc(b"sheet ")),
        ] {
            spreadsheet_chunks
                .incremental_upsert(json!({
                    "id": id,
                    "blob_id": blob_id,
                    "spreadsheet_id": "spreadsheet_1",
                    "version_id": "version_1",
                    "idx": idx,
                    "total": 2,
                    "mime_type": "application/octet-stream",
                    "encoding": "base64",
                    "data": data,
                    "created_at_ms": idx,
                }))
                .await
                .expect("insert spreadsheet blob chunk");
        }

        let mut document_bytes = Vec::new();
        stream_demand_file_chunks(
            root.path(),
            "document_blob_chunks",
            "blob_id",
            "blob_1",
            None,
            &mut |bytes| {
                document_bytes.extend_from_slice(bytes);
                Ok(true)
            },
        )
        .expect("stream document blob chunks");
        assert_eq!(String::from_utf8(document_bytes).unwrap(), "hello world");

        let mut spreadsheet_bytes = Vec::new();
        stream_demand_file_chunks(
            root.path(),
            "spreadsheet_blob_chunks",
            "blob_id",
            "sheet_blob_1",
            None,
            &mut |bytes| {
                spreadsheet_bytes.extend_from_slice(bytes);
                Ok(true)
            },
        )
        .expect("stream spreadsheet blob chunks");
        assert_eq!(String::from_utf8(spreadsheet_bytes).unwrap(), "sheet data");
    }

    async fn open_test_database(database_path: PathBuf) -> anyhow::Result<Arc<RxDatabase>> {
        let test_id = TEST_RXDB_DATABASE_COUNTER.fetch_add(1, Ordering::Relaxed);
        open_test_database_with_name(database_path, format!("ctox-business-os-test-{test_id}"))
            .await
    }

    async fn open_test_database_with_name(
        database_path: PathBuf,
        name: String,
    ) -> anyhow::Result<Arc<RxDatabase>> {
        let storage = get_rx_storage_sqlite(RxStorageSqliteSettings { database_path });
        create_rx_database(RxDatabaseCreator {
            name,
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
    fn open_database_does_not_close_existing_business_os_instance() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = store::rxdb_store_path(root.path());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let first = open_database(database_path.clone())
                .await
                .expect("open first database");
            first
                .add_collections(collection_creators())
                .await
                .expect("register first collections");
            let first_catalog = first
                .collection("business_module_catalog")
                .expect("first catalog collection");

            let second = open_database(database_path)
                .await
                .expect("open duplicate database");

            assert!(
                !first.closed(),
                "opening a helper database must not close the live peer database"
            );
            assert!(
                !first_catalog.closed(),
                "opening a helper database must not close live peer collections"
            );

            second.close().await.expect("close second database");
            first.close().await.expect("close first database");
        });
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
        assert_eq!(
            file.get("virtual_path").and_then(Value::as_str),
            Some("/CTOX/artifact.md")
        );
        assert_eq!(
            file.get("path").and_then(Value::as_str),
            Some(
                file_path
                    .canonicalize()
                    .expect("canonical desktop file path")
                    .to_string_lossy()
                    .as_ref()
            )
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
    fn sync_workspace_csv_publishes_files_and_spreadsheet_records() {
        let root = tempfile::tempdir().expect("temp root");
        let workspace = root.path().join("csv-export-workspace");
        fs::create_dir_all(&workspace).expect("create CSV workspace");
        let file_path = workspace.join("measurements.csv");
        fs::write(
            &file_path,
            "propeller;diameter_in;pitch_in;rpm;force_n;torque_nm\n9x5;9;5;12000;18,4;0,42\n",
        )
        .expect("write CSV export");

        let indexed = sync_desktop_files_from_workspace_root(root.path(), &workspace)
            .expect("sync CSV workspace");
        assert_eq!(indexed, 1);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let file_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE json_extract(data, '$.name') = 'measurements.csv'",
                [],
                |row| row.get(0),
            )
            .expect("CSV desktop file");
        let file: Value = serde_json::from_str(&file_json).expect("CSV desktop file json");
        assert_eq!(
            file.get("virtual_path").and_then(Value::as_str),
            Some("/CTOX/csv-export-workspace/measurements.csv")
        );

        let spreadsheet_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__spreadsheets__v0 WHERE json_extract(data, '$.filename') = 'measurements.csv'",
                [],
                |row| row.get(0),
            )
            .expect("CSV Spreadsheet record");
        let spreadsheet: Value =
            serde_json::from_str(&spreadsheet_json).expect("CSV Spreadsheet json");
        assert_eq!(
            spreadsheet.get("row_count").and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            spreadsheet.get("col_count").and_then(Value::as_u64),
            Some(6)
        );
        assert_eq!(
            spreadsheet
                .pointer("/linked_records/0/path")
                .and_then(Value::as_str),
            Some("/CTOX/csv-export-workspace/measurements.csv")
        );

        let version_id = spreadsheet
            .get("current_version_id")
            .and_then(Value::as_str)
            .expect("current CSV Spreadsheet version");
        let version_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__spreadsheet_versions__v0 WHERE id = ?1",
                [version_id],
                |row| row.get(0),
            )
            .expect("CSV Spreadsheet version");
        let version: Value =
            serde_json::from_str(&version_json).expect("CSV Spreadsheet version json");
        assert_eq!(
            version
                .pointer("/model_json/data/1/4")
                .and_then(Value::as_str),
            Some("18,4")
        );
        assert_eq!(
            version
                .pointer("/model_json/data/1/5")
                .and_then(Value::as_str),
            Some("0,42")
        );

        let chunk_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__spreadsheet_blob_chunks__v0 WHERE json_extract(data, '$.version_id') = ?1",
                [version_id],
                |row| row.get(0),
            )
            .expect("CSV Spreadsheet chunks");
        assert!(chunk_count >= 1);
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
    fn active_desktop_file_demand_fetch_falls_back_to_file_id_chunks() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("fallback.pdf");
        let payload = vec![b'p'; DESKTOP_FILE_CHUNK_SIZE + 117];
        fs::write(&file_path, &payload).expect("write fallback artifact");

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
        let chunks = read_desktop_file_chunks(&conn, &file_id, false);
        assert!(
            chunks.len() > 1,
            "test file should create multiple chunks for fallback coverage"
        );
        for chunk in &chunks {
            let old_id = chunk.get("id").and_then(Value::as_str).expect("chunk id");
            let idx = chunk.get("idx").and_then(Value::as_u64).expect("chunk idx");
            let legacy_id = format!("{file_id}-legacy-{idx}");
            conn.execute(
                "UPDATE ctox_business_os__desktop_file_chunks__v0 \
                 SET id = ?1, data = json_set(data, '$.id', ?1) \
                 WHERE id = ?2",
                params![legacy_id, old_id],
            )
            .expect("rewrite chunk id to legacy form");
        }
        drop(conn);

        let mut fallback_stats = DemandFileFetchRequestStats::new(None);
        let (fallback_chunks, fallback_base_offset) = active_desktop_file_chunk_rows_from_sqlite(
            root.path(),
            &file_id,
            None,
            &mut fallback_stats,
        )
        .expect("load fallback chunks");
        assert_eq!(fallback_base_offset, 0);
        let decoded = reconstruct_desktop_file_chunks(
            &fallback_chunks,
            &file_id,
            generation_id,
            content_hash,
        )
        .expect("fallback chunks reconstruct");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn active_desktop_file_demand_fetch_dedupes_mixed_chunk_rows() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("mixed.pdf");
        let payload = vec![b'm'; DESKTOP_FILE_CHUNK_DECODED_SIZE as usize + 513];
        fs::write(&file_path, &payload).expect("write mixed artifact");

        sync_desktop_file_from_path(root.path(), &file_path).expect("sync desktop file");

        let canonical = file_path.canonicalize().expect("canonical file");
        let file_id = desktop_file_id(&canonical);
        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let file = read_desktop_file_row(&conn, &file_id);
        let generation_id = file
            .get("content_generation_id")
            .and_then(Value::as_str)
            .expect("active generation id");
        let chunks = read_desktop_file_chunks(&conn, &file_id, false);
        assert!(chunks.len() > 1, "test file should have multiple chunks");
        let padded_total = chunks
            .iter()
            .filter_map(|chunk| chunk.get("total").and_then(Value::as_u64))
            .max()
            .expect("chunk total")
            + 1;
        for chunk in &chunks {
            let idx = chunk.get("idx").and_then(Value::as_u64).expect("idx");
            let mut duplicate = chunk.clone();
            let duplicate_id = format!("{file_id}_{generation_id}_padded_{idx}");
            duplicate["id"] = Value::String(duplicate_id.clone());
            duplicate["total"] = Value::from(padded_total);
            conn.execute(
                "INSERT INTO ctox_business_os__desktop_file_chunks__v0 \
                 (id, revision, deleted, lastWriteTime, data) VALUES (?1, ?2, 0, ?3, ?4)",
                params![
                    duplicate_id,
                    format!("test-rev-{idx}"),
                    1_f64,
                    duplicate.to_string()
                ],
            )
            .expect("insert padded duplicate");
        }
        drop(conn);

        let mut collected = Vec::new();
        stream_demand_file_chunks(
            root.path(),
            "desktop_file_chunks",
            "file_id",
            &file_id,
            None,
            &mut |bytes| {
                collected.extend_from_slice(bytes);
                Ok(true)
            },
        )
        .expect("stream deduped chunks");
        assert_eq!(collected, payload);
    }

    #[test]
    fn active_desktop_file_demand_fetch_uses_equivalent_hash_source() {
        let root = tempfile::tempdir().expect("temp root");
        let source_path = root.path().join("complete.pdf");
        let target_path = root.path().join("incomplete.pdf");
        let payload = vec![b'h'; DESKTOP_FILE_CHUNK_DECODED_SIZE as usize + 891];
        fs::write(&source_path, &payload).expect("write complete artifact");
        fs::write(&target_path, &payload).expect("write incomplete artifact");

        sync_desktop_file_from_path(root.path(), &source_path).expect("sync source file");
        sync_desktop_file_from_path(root.path(), &target_path).expect("sync target file");

        let target_id = desktop_file_id(&target_path.canonicalize().expect("canonical target"));
        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        conn.execute(
            "DELETE FROM ctox_business_os__desktop_file_chunks__v0
             WHERE json_extract(data, '$.file_id') = ?1
               AND CAST(json_extract(data, '$.idx') AS INTEGER) > 0",
            params![target_id],
        )
        .expect("truncate target chunks");
        drop(conn);

        let mut collected = Vec::new();
        stream_demand_file_chunks(
            root.path(),
            "desktop_file_chunks",
            "file_id",
            &target_id,
            None,
            &mut |bytes| {
                collected.extend_from_slice(bytes);
                Ok(true)
            },
        )
        .expect("stream equivalent chunks");
        assert_eq!(collected, payload);
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
    fn desktop_file_chunk_cleanup_uses_primary_key_range_plan() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("range-plan.txt");
        fs::write(&file_path, vec![b'a'; DESKTOP_FILE_CHUNK_SIZE + 32])
            .expect("write desktop file");

        sync_desktop_file_from_path(root.path(), &file_path).expect("sync desktop file");
        let canonical = file_path.canonicalize().expect("canonical file");
        let file_id = desktop_file_id(&canonical);
        let mut conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        {
            let tx = conn.transaction().expect("begin irrelevant chunk seed tx");
            for idx in 0..5_000 {
                let irrelevant_id = format!("irrelevant_{idx}_gen_0_0");
                let irrelevant_file_id = format!("irrelevant_{idx}");
                let data = json!({
                    "id": irrelevant_id,
                    "file_id": irrelevant_file_id,
                    "generation_id": "gen_0",
                    "idx": 0,
                    "total": 1,
                    "encoding": "base64",
                    "data": "",
                    "created_at_ms": idx,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO ctox_business_os__desktop_file_chunks__v0 \
                     (id, revision, deleted, lastWriteTime, data) \
                     VALUES (?1, ?2, 0, ?3, ?4)",
                    params![irrelevant_id, "1-seed", idx as f64, data],
                )
                .expect("insert irrelevant chunk row");
            }
            tx.commit().expect("commit irrelevant chunk seed tx");
        }
        conn.execute_batch("ANALYZE")
            .expect("analyze desktop chunk db");

        let table = quote_sqlite_identifier("ctox_business_os__desktop_file_chunks__v0");
        let (chunk_id_lower, chunk_id_upper) = desktop_file_chunk_id_bounds(&file_id);
        let limit = DESKTOP_FILE_CHUNK_CLEANUP_SCAN_LIMIT as i64;
        let sql = format!(
            "SELECT data FROM {table}
             WHERE id >= ?1
               AND id < ?2
               AND COALESCE(deleted, 0) = 0
             LIMIT ?3"
        );
        let plan = sqlite_query_plan(&conn, &sql, &[&chunk_id_lower, &chunk_id_upper, &limit]);
        assert!(
            plan.contains("SEARCH") && plan.contains("id>?"),
            "desktop chunk cleanup must use the primary-key range, got:\n{plan}"
        );
        assert!(
            !plan.contains("SCAN "),
            "desktop chunk cleanup must not scan the chunk table, got:\n{plan}"
        );

        let chunks =
            desktop_file_chunk_rows_for_file_id(root.path(), &file_id).expect("load chunks");
        assert!(!chunks.is_empty(), "chunk range query should return rows");
        assert!(chunks.iter().all(|chunk| {
            chunk.get("file_id").and_then(Value::as_str) == Some(file_id.as_str())
        }));
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
    fn sync_channel_state_idle_gate_skips_unchanged_projection() {
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
                "jami:idle",
                "jami",
                "idle@example.test",
                "jami",
                "{}",
                "2026-05-20T08:00:00Z",
            ],
        )
        .expect("insert communication account");
        drop(conn);

        let mut last_projection_stamp = None;
        let first = sync_channel_state_if_changed(root.path(), &mut last_projection_stamp)
            .expect("first changed sync");
        assert!(first >= BUSINESS_OS_CHANNEL_IDS.len() + 1);

        let second = sync_channel_state_if_changed(root.path(), &mut last_projection_stamp)
            .expect("unchanged idle sync");
        assert_eq!(second, 0);

        let artifacts =
            crate::communication::runtime::artifacts_dir_for_business_os(root.path(), "jami");
        fs::create_dir_all(&artifacts).expect("create pairing artifacts");
        fs::write(
            artifacts.join("pairing-status.json"),
            r#"{"status":"qr_ready","qr_payload":"idle-pair"}"#,
        )
        .expect("write pairing status");

        let third = sync_channel_state_if_changed(root.path(), &mut last_projection_stamp)
            .expect("changed artifact sync");
        assert!(third >= 1);
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
                profile: None,
                accept_recovery_responsibility: false,
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
    fn sync_business_users_idle_gate_skips_unchanged_projection() {
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
                profile: None,
                accept_recovery_responsibility: false,
            },
        )
        .expect("upsert business user");

        let mut last_projection_stamp = None;
        let first = sync_business_users_if_changed(root.path(), &mut last_projection_stamp)
            .expect("first business users sync");
        assert!(first >= 2);

        let second = sync_business_users_if_changed(root.path(), &mut last_projection_stamp)
            .expect("unchanged business users sync");
        assert_eq!(second, 0);

        store::upsert_user(
            root.path(),
            &admin,
            store::BusinessOsUserMutation {
                id: "alice".to_string(),
                display_name: "Alice Updated".to_string(),
                role: "founder".to_string(),
                active: true,
                profile: None,
                accept_recovery_responsibility: false,
            },
        )
        .expect("update business user");

        let third = sync_business_users_if_changed(root.path(), &mut last_projection_stamp)
            .expect("changed business users sync");
        assert!(third >= 1);

        let fourth = sync_business_users_if_changed(root.path(), &mut last_projection_stamp)
            .expect("unchanged business users resync");
        assert_eq!(fourth, 0);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let user_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__business_users__v0 WHERE id = 'alice'",
                [],
                |row| row.get(0),
            )
            .expect("projected user row");
        let user: Value = serde_json::from_str(&user_json).expect("user json");
        assert_eq!(
            user.get("display_name").and_then(Value::as_str),
            Some("Alice Updated")
        );
    }

    #[test]
    fn sync_runtime_settings_projects_status_document() {
        let root = tempfile::tempdir().expect("temp root");

        let synced = sync_runtime_settings(root.path()).expect("sync runtime settings");
        assert_eq!(synced, 1);
        let resynced = sync_runtime_settings(root.path()).expect("resync runtime settings");
        assert_eq!(resynced, 0);

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
    fn sync_runtime_settings_idle_gate_skips_unchanged_projection() {
        let root = tempfile::tempdir().expect("temp root");
        let mut last_projection_stamp = None;

        let first = sync_runtime_settings_if_changed(root.path(), &mut last_projection_stamp)
            .expect("first runtime settings sync");
        assert_eq!(first, 1);

        let second = sync_runtime_settings_if_changed(root.path(), &mut last_projection_stamp)
            .expect("unchanged runtime settings sync");
        assert_eq!(second, 0);

        crate::inference::runtime_env::set_runtime_env_value(
            root.path(),
            "CTOX_CHAT_TURN_TIMEOUT_SECS",
            "777",
        )
        .expect("persist runtime setting");
        let third = sync_runtime_settings_if_changed(root.path(), &mut last_projection_stamp)
            .expect("changed runtime settings sync");
        assert_eq!(third, 1);

        let fourth = sync_runtime_settings_if_changed(root.path(), &mut last_projection_stamp)
            .expect("runtime settings sync after changed projection");
        assert_eq!(fourth, 0);

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
            settings
                .pointer("/runtime/max_run_secs")
                .and_then(Value::as_u64),
            Some(777)
        );
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

        let synced = sync_module_catalog(root.path()).expect("sync module catalog");
        assert_eq!(synced, 1);

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

        let resynced = sync_module_catalog(root.path()).expect("resync unchanged module catalog");
        assert_eq!(resynced, 0);
        let catalog_json_again: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__business_module_catalog__v0 WHERE id = 'module-catalog'",
                [],
                |row| row.get(0),
            )
            .expect("projected module catalog row after resync");
        assert_eq!(catalog_json, catalog_json_again);
    }

    #[test]
    fn sync_module_catalog_idle_gate_skips_unchanged_projection() {
        let root = tempfile::tempdir().expect("temp root");
        let app_root = root.path().join("src/apps/business-os");
        fs::create_dir_all(app_root.join("modules/ctox")).expect("create ctox module");
        fs::write(app_root.join("index.html"), "<!doctype html>").expect("write app index");
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","collections":["ctox_queue_tasks"]}"#,
        )
        .expect("write ctox manifest");

        let mut last_projection_stamp = None;
        let first = sync_module_catalog_if_changed(root.path(), &mut last_projection_stamp)
            .expect("first module catalog sync");
        assert_eq!(first, 1);

        let second = sync_module_catalog_if_changed(root.path(), &mut last_projection_stamp)
            .expect("unchanged module catalog sync");
        assert_eq!(second, 0);

        fs::write(app_root.join("modules/ctox/icon.svg"), "<svg></svg>").expect("write ctox icon");
        let third = sync_module_catalog_if_changed(root.path(), &mut last_projection_stamp)
            .expect("changed module catalog source sync");
        assert_eq!(third, 1);

        let fourth = sync_module_catalog_if_changed(root.path(), &mut last_projection_stamp)
            .expect("unchanged module catalog resync");
        assert_eq!(fourth, 0);

        crate::inference::runtime_env::set_runtime_env_value(
            root.path(),
            "CTOX_BUSINESS_OS_MODULE_ALLOWLIST",
            "ctox",
        )
        .expect("persist module allowlist");
        let fifth = sync_module_catalog_if_changed(root.path(), &mut last_projection_stamp)
            .expect("changed module catalog allowlist sync");
        assert_eq!(fifth, 1);

        let sixth = sync_module_catalog_if_changed(root.path(), &mut last_projection_stamp)
            .expect("unchanged module catalog sync after allowlist");
        assert_eq!(sixth, 0);
    }

    #[test]
    fn sync_knowledge_tables_tombstones_stale_once_then_noops() {
        let root = tempfile::tempdir().expect("temp root");
        assert_eq!(
            sync_knowledge_tables(root.path()).expect("initial empty knowledge sync"),
            0
        );

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let stale = json!({
            "id": "stale-knowledge-table",
            "kind": "table",
            "title": "Stale Knowledge Table",
            "updated_at_ms": 1,
            "_deleted": false,
            "_meta": { "lwt": 1.0 },
            "_rev": "1-test",
            "_attachments": {}
        });
        conn.execute(
            "INSERT INTO ctox_business_os__knowledge_tables__v0 \
             (id, revision, deleted, lastWriteTime, data) VALUES (?1, ?2, 0, ?3, ?4)",
            params![
                "stale-knowledge-table",
                "1-test",
                1.0_f64,
                serde_json::to_string(&stale).expect("stale knowledge json")
            ],
        )
        .expect("seed stale knowledge table");
        drop(conn);

        let synced = sync_knowledge_tables(root.path()).expect("tombstone stale knowledge table");
        assert_eq!(synced, 1);
        let resynced = sync_knowledge_tables(root.path()).expect("resync stale knowledge table");
        assert_eq!(resynced, 0);

        let conn =
            Connection::open(store::rxdb_store_path(root.path())).expect("reopen rxdb sqlite");
        let deleted: i64 = conn
            .query_row(
                "SELECT deleted FROM ctox_business_os__knowledge_tables__v0 WHERE id = ?1",
                ["stale-knowledge-table"],
                |row| row.get(0),
            )
            .expect("stale knowledge tombstone row");
        assert_eq!(deleted, 1);
    }

    #[test]
    fn sync_knowledge_tables_idle_gate_skips_unchanged_source() {
        let root = tempfile::tempdir().expect("temp root");
        crate::knowledge::knowledge_tables_projection_source_stamp(root.path())
            .expect("bootstrap knowledge schema");
        let conn = Connection::open(crate::paths::core_db(root.path())).expect("open core db");
        conn.execute(
            "INSERT INTO knowledge_data_tables (
                 table_id, domain, table_key, source_system, title, description,
                 parquet_path, schema_hash, row_count, bytes, tags_json, archived_at,
                 created_at, updated_at
             ) VALUES ('kdt-idle-gate', 'idle', 'gate', 'agent',
                       'Idle Gate Table', 'Knowledge source gate test',
                       '/stale.parquet', '', 0, 0, '{}', NULL,
                       '2026-06-25T00:00:00+00:00',
                       '2026-06-25T00:00:00+00:00')",
            [],
        )
        .expect("insert knowledge catalog row");
        drop(conn);

        let mut last_source_stamp = None;
        let first = sync_knowledge_tables_if_changed(root.path(), &mut last_source_stamp)
            .expect("first knowledge tables sync");
        assert_eq!(first, 1);

        let unchanged_stamp = last_source_stamp.clone();
        let second = sync_knowledge_tables_if_changed(root.path(), &mut last_source_stamp)
            .expect("unchanged knowledge tables sync");
        assert_eq!(second, 0);
        assert_eq!(last_source_stamp, unchanged_stamp);

        let conn = Connection::open(crate::paths::core_db(root.path())).expect("open core db");
        conn.execute(
            "UPDATE knowledge_data_tables
                SET title = 'Idle Gate Table Updated',
                    updated_at = '2026-06-25T00:00:01+00:00'
              WHERE table_id = 'kdt-idle-gate'",
            [],
        )
        .expect("update knowledge catalog row");
        drop(conn);

        let third = sync_knowledge_tables_if_changed(root.path(), &mut last_source_stamp)
            .expect("changed knowledge tables sync");
        assert_eq!(third, 1);

        let fourth = sync_knowledge_tables_if_changed(root.path(), &mut last_source_stamp)
            .expect("unchanged knowledge tables resync");
        assert_eq!(fourth, 0);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let table_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__knowledge_tables__v0 WHERE id = 'table:kdt-idle-gate'",
                [],
                |row| row.get(0),
            )
            .expect("knowledge table projection row");
        let table: Value = serde_json::from_str(&table_json).expect("knowledge table json");
        assert_eq!(
            table.get("title").and_then(Value::as_str),
            Some("Idle Gate Table Updated")
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
    fn sync_business_record_projections_materializes_procedural_knowledge() {
        let root = tempfile::tempdir().expect("temp root");
        crate::mission::tickets::create_or_update_main_skill(
            root.path(),
            "projection.main.v1",
            "Projection main skill",
            "research",
            "resolve",
            None,
            None,
            vec!["load evidence".to_string()],
            vec!["persist knowledge".to_string()],
            vec!["projection.skillbook.v1".to_string()],
            vec!["projection.runbook.v1".to_string()],
        )
        .expect("create main skill");
        crate::mission::tickets::create_or_update_skillbook(
            root.path(),
            "projection.skillbook.v1",
            "Projection skillbook",
            "v1",
            "Project source-backed knowledge into Business OS.",
            "Fail closed when evidence is missing.",
            "Return cited facts only.",
            vec!["Cite every factual claim.".to_string()],
            vec!["load".to_string(), "verify".to_string()],
            vec!["research".to_string()],
            vec!["projection.runbook.v1".to_string()],
        )
        .expect("create skillbook");
        crate::mission::tickets::create_or_update_runbook(
            root.path(),
            "projection.runbook.v1",
            "projection.skillbook.v1",
            "Projection runbook",
            "v1",
            "active",
            "research",
            vec!["VERIFY".to_string()],
        )
        .expect("create runbook");

        let synced = sync_business_record_projections(root.path())
            .expect("sync procedural knowledge projections");
        assert!(synced >= 3);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let skillbook_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__knowledge_items__v0 WHERE id = 'skillbook:projection.skillbook.v1'",
                [],
                |row| row.get(0),
            )
            .expect("projected skillbook row");
        let skillbook: Value =
            serde_json::from_str(&skillbook_json).expect("projected skillbook json");
        assert_eq!(
            skillbook.get("title").and_then(Value::as_str),
            Some("Projection skillbook")
        );
        assert_eq!(
            skillbook.get("kind").and_then(Value::as_str),
            Some("skillbook")
        );

        let runbook_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__knowledge_runbooks__v0 WHERE id = 'runbook:projection.runbook.v1'",
                [],
                |row| row.get(0),
            )
            .expect("projected runbook row");
        let runbook: Value = serde_json::from_str(&runbook_json).expect("projected runbook json");
        assert_eq!(
            runbook.get("title").and_then(Value::as_str),
            Some("Projection runbook")
        );
        assert_eq!(
            runbook.get("skillbook_id").and_then(Value::as_str),
            Some("projection.skillbook.v1")
        );
    }

    #[test]
    fn business_record_projection_stamp_tracks_procedural_knowledge_database() {
        let root = tempfile::tempdir().expect("temp root");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let before = runtime
            .block_on(business_record_projection_source_stamp(root.path()))
            .expect("initial projection stamp");

        crate::mission::tickets::create_or_update_main_skill(
            root.path(),
            "stamp.main.v1",
            "Stamp main skill",
            "research",
            "resolve",
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .expect("create main skill");

        let after = runtime
            .block_on(business_record_projection_source_stamp(root.path()))
            .expect("updated projection stamp");
        assert_ne!(before.knowledge, after.knowledge);
    }

    #[test]
    fn bulk_business_record_projection_writes_multiple_batches_idempotently() {
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
            let documents = database
                .collection("documents")
                .expect("documents collection");
            let projected = (0..(BUSINESS_RECORD_PROJECTION_WRITE_BATCH_SIZE * 2 + 1))
                .map(|index| {
                    json!({
                        "id": format!("bulk_projection_{index:04}"),
                        "title": format!("Bulk projected document {index}"),
                        "filename": format!("bulk-{index}.txt"),
                        "mime_type": "text/plain",
                        "status": "imported",
                        "current_version_id": "",
                        "index_text": format!("Bulk projected document body {index}"),
                        "is_deleted": false,
                        "created_at_ms": 1_000 + index as i64,
                        "updated_at_ms": 2_000 + index as i64
                    })
                })
                .collect::<Vec<_>>();

            let first = bulk_upsert_business_record_projection_documents(
                &documents,
                "documents",
                projected.clone(),
            )
            .await
            .expect("bulk project documents");
            assert_eq!(first, projected.len());

            let second = bulk_upsert_business_record_projection_documents(
                &documents,
                "documents",
                projected,
            )
            .await
            .expect("repeat bulk project documents");
            assert_eq!(second, 0, "unchanged projections must not be rewritten");

            let count = documents
                .find(Some(MangoQuery {
                    limit: Some(1_000),
                    ..Default::default()
                }))
                .expect("documents query")
                .exec(false)
                .await
                .expect("projected documents");
            assert_eq!(count.as_array().map(Vec::len), Some(501));
        });
    }

    #[test]
    fn business_record_projection_drains_more_than_one_page() {
        let root = tempfile::tempdir().expect("temp root");
        let conn = store::open_store(root.path()).expect("open business store");
        let record_count = BUSINESS_RECORD_PROJECTION_PAGE_SIZE * 2 + 1;
        for index in 0..record_count {
            store::upsert_business_record(
                &conn,
                "documents",
                &format!("paged_projection_{index:04}"),
                10_000,
                json!({
                    "id": format!("paged_projection_{index:04}"),
                    "title": format!("Paged projected document {index}"),
                    "filename": format!("paged-{index}.txt"),
                    "mime_type": "text/plain",
                    "status": "imported",
                    "current_version_id": "",
                    "index_text": format!("Paged projected document body {index}"),
                    "is_deleted": false,
                    "created_at_ms": 9_000 + index as i64,
                    "updated_at_ms": 10_000
                }),
            )
            .expect("insert paged document business record");
        }
        drop(conn);

        let synced = sync_business_record_projections(root.path())
            .expect("sync all business record projection pages");
        assert!(synced >= record_count);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let projected_count: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__documents__v0 WHERE id LIKE 'paged_projection_%'",
                [],
                |row| row.get(0),
            )
            .expect("count paged document projections");
        assert_eq!(projected_count, record_count);
    }

    #[test]
    fn sync_business_record_projections_idle_gate_skips_unchanged_source() {
        let root = tempfile::tempdir().expect("temp root");
        let conn = store::open_store(root.path()).expect("open business store");
        store::upsert_business_record(
            &conn,
            "documents",
            "doc_projection_idle_gate",
            1_000,
            json!({
                "id": "doc_projection_idle_gate",
                "title": "Projected idle gate document",
                "filename": "idle-gate.docx",
                "mime_type": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "status": "imported",
                "current_version_id": "doc_projection_idle_gate_v1",
                "index_text": "Projected idle gate document body",
                "is_deleted": false,
                "created_at_ms": 900,
                "updated_at_ms": 1_000
            }),
        )
        .expect("insert document business record");
        drop(conn);

        let mut since_by_collection = HashMap::new();
        let mut queue_chat_repair_stamp = None;
        let mut last_source_stamp = None;
        let first = sync_business_record_projections_if_changed(
            root.path(),
            &mut since_by_collection,
            &mut queue_chat_repair_stamp,
            &mut last_source_stamp,
        )
        .expect("first business record projection sync");
        assert!(first >= 1);

        let unchanged_stamp = last_source_stamp.clone();
        let second = sync_business_record_projections_if_changed(
            root.path(),
            &mut since_by_collection,
            &mut queue_chat_repair_stamp,
            &mut last_source_stamp,
        )
        .expect("unchanged business record projection sync");
        assert_eq!(second, 0);
        assert_eq!(last_source_stamp, unchanged_stamp);

        let conn = store::open_store(root.path()).expect("reopen business store");
        store::upsert_business_record(
            &conn,
            "documents",
            "doc_projection_idle_gate",
            2_000,
            json!({
                "id": "doc_projection_idle_gate",
                "title": "Projected idle gate document updated",
                "filename": "idle-gate.docx",
                "mime_type": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "status": "imported",
                "current_version_id": "doc_projection_idle_gate_v1",
                "index_text": "Projected idle gate document body",
                "is_deleted": false,
                "created_at_ms": 900,
                "updated_at_ms": 2_000
            }),
        )
        .expect("update document business record");
        drop(conn);

        let third = sync_business_record_projections_if_changed(
            root.path(),
            &mut since_by_collection,
            &mut queue_chat_repair_stamp,
            &mut last_source_stamp,
        )
        .expect("changed business record projection sync");
        assert!(third >= 1);

        let fourth = sync_business_record_projections_if_changed(
            root.path(),
            &mut since_by_collection,
            &mut queue_chat_repair_stamp,
            &mut last_source_stamp,
        )
        .expect("unchanged business record projection resync");
        assert_eq!(fourth, 0);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let document_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__documents__v0 WHERE id = 'doc_projection_idle_gate'",
                [],
                |row| row.get(0),
            )
            .expect("projected idle gate document row");
        let document: Value = serde_json::from_str(&document_json).expect("document json");
        assert_eq!(
            document.get("title").and_then(Value::as_str),
            Some("Projected idle gate document updated")
        );
        assert_eq!(
            document.get("updated_at_ms").and_then(Value::as_i64),
            Some(2_000)
        );
    }

    #[test]
    fn sync_business_record_projections_updates_existing_support_conversation_fields() {
        let root = tempfile::tempdir().expect("temp root");
        let conn = store::open_store(root.path()).expect("open business store");
        store::upsert_business_record(
            &conn,
            "support_conversations",
            "conv_projection_update",
            1_000,
            json!({
                "id": "conv_projection_update",
                "is_deleted": false,
                "created_at_ms": 900,
                "updated_at_ms": 1_000,
                "inbox_id": "",
                "primary_thread_key": "mail:projection-update",
                "status": "open",
                "priority": "high",
                "assignee_id": "",
                "team_id": "",
                "customer_account_id": "",
                "customer_contact_id": "",
                "ticket_case_id": "",
                "last_message_key": "",
                "last_activity_at_ms": 1_000,
                "waiting_since_ms": 0,
                "snoozed_until_ms": 0,
                "unread_count": 0,
                "label_ids": [],
                "custom_attributes": {},
                "search_text": "Projection update support conversation"
            }),
        )
        .expect("insert support conversation business record");
        drop(conn);

        assert!(sync_business_record_projections(root.path()).expect("initial sync") >= 1);

        let conn = store::open_store(root.path()).expect("reopen business store");
        store::upsert_business_record(
            &conn,
            "support_conversations",
            "conv_projection_update",
            2_000,
            json!({
                "id": "conv_projection_update",
                "is_deleted": false,
                "created_at_ms": 900,
                "updated_at_ms": 2_000,
                "inbox_id": "",
                "primary_thread_key": "mail:projection-update",
                "status": "waiting",
                "priority": "low",
                "assignee_id": "local-dev",
                "team_id": "",
                "customer_account_id": "",
                "customer_contact_id": "",
                "ticket_case_id": "",
                "last_message_key": "",
                "last_activity_at_ms": 2_000,
                "waiting_since_ms": 0,
                "snoozed_until_ms": 0,
                "unread_count": 0,
                "label_ids": [],
                "custom_attributes": {},
                "search_text": "Projection update support conversation"
            }),
        )
        .expect("update support conversation business record");
        drop(conn);

        assert!(sync_business_record_projections(root.path()).expect("second sync") >= 1);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let conversation_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__support_conversations__v0 WHERE id = 'conv_projection_update'",
                [],
                |row| row.get(0),
            )
            .expect("projected support conversation row");
        let conversation: Value =
            serde_json::from_str(&conversation_json).expect("support conversation json");
        assert_eq!(
            conversation.get("status").and_then(Value::as_str),
            Some("waiting")
        );
        assert_eq!(
            conversation.get("priority").and_then(Value::as_str),
            Some("low")
        );
        assert_eq!(
            conversation.get("assignee_id").and_then(Value::as_str),
            Some("local-dev")
        );
        assert_eq!(
            conversation.get("updated_at_ms").and_then(Value::as_i64),
            Some(2_000)
        );
    }

    #[test]
    fn sync_business_record_projections_repairs_legacy_document_versions() {
        let root = tempfile::tempdir().expect("temp root");
        let conn = store::open_store(root.path()).expect("open business store");
        store::upsert_business_record(
            &conn,
            "document_versions",
            "doc_legacy_v1",
            1_001,
            json!({
                "id": "doc_legacy_v1",
                "document_id": "doc_legacy",
                "version": 1,
                "source_kind": "imported_docx",
                "blob_id": "doc_blob_legacy",
                "model_json": {},
                "created_at_ms": 901,
                "updated_at_ms": 1_001
            }),
        )
        .expect("insert legacy document version business record");
        drop(conn);

        let synced = sync_business_record_projections(root.path())
            .expect("sync business record projections");
        assert!(synced >= 1);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let version_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__document_versions__v0 WHERE id = 'doc_legacy_v1'",
                [],
                |row| row.get(0),
            )
            .expect("projected legacy document version row");
        let version: Value = serde_json::from_str(&version_json).expect("document version json");
        assert_eq!(
            version
                .get("diagnostics")
                .and_then(Value::as_array)
                .map(Vec::is_empty),
            Some(true)
        );
    }

    #[test]
    fn projection_upsert_repairs_legacy_matching_requirements() {
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
            let requirements = database
                .collection("matching_requirements")
                .expect("matching_requirements collection");

            upsert_business_record_projection_document(
                &requirements,
                "matching_requirements",
                json!({
                    "id": "src_byteforge_tech",
                    "kind": "source",
                    "title": "ByteForge Technologies AG",
                    "status": "active",
                    "data": {
                        "id": "src_byteforge_tech",
                        "title": "ByteForge Technologies AG"
                    },
                    "_meta": {
                        "lwt": 1_780_632_672_277.48_f64
                    },
                    "_rev": "3-legacy"
                }),
            )
            .await
            .expect("upsert legacy matching requirement projection");

            let requirement = requirements
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "src_byteforge_tech" } })),
                    ..Default::default()
                }))
                .expect("matching requirement query")
                .exec(false)
                .await
                .expect("projected legacy matching requirement row");
            assert_eq!(
                requirement.get("updated_at_ms").and_then(Value::as_i64),
                Some(1_780_632_672_277)
            );
        });
    }

    #[test]
    fn projection_upsert_coerces_legacy_shift_timestamps() {
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
            let shifts = database
                .collection("planning_shifts")
                .expect("planning_shifts collection");

            upsert_business_record_projection_document(
                &shifts,
                "planning_shifts",
                json!({
                    "id": "shift_gen_1",
                    "employee_id": "emp_clara",
                    "project_id": "project_gen_2",
                    "start_time": "2026-05-25T16:10:11.908Z",
                    "end_time": "2026-05-26T00:10:11.908Z",
                    "shift_type": "standard_workday",
                    "is_deleted": false,
                    "_meta": {
                        "lwt": 1_780_635_648_436.48_f64
                    },
                    "_rev": "2-legacy"
                }),
            )
            .await
            .expect("upsert legacy planning shift projection");

            let shift = shifts
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "shift_gen_1" } })),
                    ..Default::default()
                }))
                .expect("planning shift query")
                .exec(false)
                .await
                .expect("projected legacy planning shift row");
            assert_eq!(
                shift.get("start_time").and_then(Value::as_i64),
                Some(1_779_725_411_908)
            );
            assert_eq!(
                shift.get("end_time").and_then(Value::as_i64),
                Some(1_779_754_211_908)
            );
            assert_eq!(shift.get("status").and_then(Value::as_str), Some(""));
            assert_eq!(
                shift.get("updated_at_ms").and_then(Value::as_i64),
                Some(1_780_635_648_436)
            );
        });
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
    fn sync_business_record_projections_accepts_schema_light_tombstones() {
        let root = tempfile::tempdir().expect("temp root");
        let conn = store::open_store(root.path()).expect("open business store");
        store::upsert_business_record(
            &conn,
            "notes",
            "note_projection_tombstone",
            1_000,
            json!({
                "id": "note_projection_tombstone",
                "title": "Projected note",
                "content": "temporary",
                "updated_at_ms": 1_000
            }),
        )
        .expect("insert note business record");
        drop(conn);

        assert!(sync_business_record_projections(root.path()).expect("initial sync") >= 1);

        let conn = store::open_store(root.path()).expect("reopen business store");
        conn.execute(
            "UPDATE business_records
             SET deleted = 1, updated_at_ms = ?3, payload_json = ?4
             WHERE collection = ?1 AND record_id = ?2",
            params![
                "notes",
                "note_projection_tombstone",
                2_000_i64,
                serde_json::to_string(&json!({
                    "id": "note_projection_tombstone",
                    "_deleted": true,
                    "updated_at_ms": 2_000
                }))
                .expect("serialize tombstone")
            ],
        )
        .expect("write schema-light tombstone business record");
        drop(conn);

        assert!(sync_business_record_projections(root.path()).expect("tombstone sync") >= 1);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
        let notes_table: String = conn
            .query_row(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name LIKE 'ctox_business_os__notes__v%'
                 ORDER BY name DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("notes rxdb table");
        let tombstone_json: String = conn
            .query_row(
                &format!("SELECT data FROM {notes_table} WHERE id = 'note_projection_tombstone'"),
                [],
                |row| row.get(0),
            )
            .expect("projected note tombstone row");
        let tombstone: Value = serde_json::from_str(&tombstone_json).expect("tombstone json");
        assert_eq!(
            tombstone.get("_deleted").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            tombstone.get("title").and_then(Value::as_str),
            Some("Projected note")
        );
        assert_eq!(
            tombstone.get("updated_at_ms").and_then(Value::as_i64),
            Some(2_000)
        );
    }

    #[test]
    fn reconcile_ctox_queue_task_projections_completes_stale_completed_commands() {
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
                    "id": "cmd_queue_completed",
                    "command_id": "cmd_queue_completed",
                    "module": "ctox",
                    "command_type": "ctox.test.completed",
                    "record_id": "cmd_queue_completed",
                    "status": "completed",
                    "inbound_channel": "ctox",
                    "payload": { "ok": true },
                    "client_context": {},
                    "updated_at_ms": 2_000
                }))
                .await
                .expect("insert completed command");

            let queue = database
                .collection("ctox_queue_tasks")
                .expect("ctox_queue_tasks collection");
            queue
                .insert(json!({
                    "id": "task_queue_completed",
                    "command_id": "cmd_queue_completed",
                    "title": "completed task",
                    "status": "queued",
                    "route_status": "queued",
                    "task_status": "queued",
                    "module": "ctox",
                    "source_module": "ctox",
                    "inbound_channel": "ctox",
                    "updated_at_ms": 1_000
                }))
                .await
                .expect("insert stale queue projection");

            assert_eq!(
                reconcile_ctox_queue_task_projections(root.path(), &database)
                    .await
                    .expect("reconcile queue projections"),
                1
            );

            let repaired = queue
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "task_queue_completed" } })),
                    ..Default::default()
                }))
                .expect("queue task query")
                .exec(false)
                .await
                .expect("queue task document");
            assert_eq!(
                repaired.get("status").and_then(Value::as_str),
                Some("completed")
            );
            assert_eq!(
                repaired.get("route_status").and_then(Value::as_str),
                Some("handled")
            );

            let conn = store::open_store(root.path()).expect("open business store");
            let payload_json: String = conn
                .query_row(
                    "SELECT payload_json
                     FROM business_records
                     WHERE collection = 'ctox_queue_tasks' AND record_id = 'task_queue_completed'",
                    [],
                    |row| row.get(0),
                )
                .expect("queue task business record");
            let payload: Value = serde_json::from_str(&payload_json).expect("queue payload");
            assert_eq!(
                payload.get("status").and_then(Value::as_str),
                Some("completed")
            );
            assert_eq!(
                payload.get("route_status").and_then(Value::as_str),
                Some("handled")
            );
        });
    }

    #[test]
    fn reconcile_ctox_queue_task_projections_filters_to_active_queue_statuses() {
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
                    "id": "cmd_queue_active_after_terminals",
                    "command_id": "cmd_queue_active_after_terminals",
                    "module": "ctox",
                    "command_type": "ctox.test.completed",
                    "record_id": "cmd_queue_active_after_terminals",
                    "status": "completed",
                    "inbound_channel": "ctox",
                    "payload": { "ok": true },
                    "client_context": {},
                    "updated_at_ms": 5_000
                }))
                .await
                .expect("insert completed command");

            let queue = database
                .collection("ctox_queue_tasks")
                .expect("ctox_queue_tasks collection");
            let mut rxdb_conn =
                Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
            let queue_table = rxdb_test_table_name(&rxdb_conn, "ctox_queue_tasks", 0);
            let queue_table_sql = quote_sqlite_identifier(&queue_table);
            let insert_sql = format!(
                "INSERT INTO {queue_table_sql} (id, revision, deleted, lastWriteTime, data)
                 VALUES (?1, ?2, 0, ?3, ?4)"
            );
            let tx = rxdb_conn.transaction().expect("seed terminal queue tx");
            for index in 0..600 {
                let id = format!("task_terminal_{index:03}");
                let payload = json!({
                    "id": id.clone(),
                    "command_id": format!("cmd_terminal_{index:03}"),
                    "title": "terminal filler",
                    "status": "completed",
                    "route_status": "handled",
                    "task_status": "completed",
                    "module": "ctox",
                    "source_module": "ctox",
                    "inbound_channel": "ctox",
                    "updated_at_ms": index,
                    "_deleted": false,
                    "_rev": "1-terminal",
                    "_meta": { "lwt": index as f64 }
                });
                tx.execute(
                    &insert_sql,
                    params![
                        id,
                        "1-terminal",
                        index as f64,
                        serde_json::to_string(&payload).expect("serialize terminal queue payload")
                    ],
                )
                .expect("insert terminal filler queue projection");
            }
            tx.commit().expect("commit terminal queue seed");
            queue
                .insert(json!({
                    "id": "task_active_after_terminals",
                    "command_id": "cmd_queue_active_after_terminals",
                    "title": "active after terminals",
                    "status": "queued",
                    "route_status": "queued",
                    "task_status": "queued",
                    "module": "ctox",
                    "source_module": "ctox",
                    "inbound_channel": "ctox",
                    "updated_at_ms": 6_000
                }))
                .await
                .expect("insert active queue projection");

            assert_eq!(
                reconcile_ctox_queue_task_projections(root.path(), &database)
                    .await
                    .expect("reconcile queue projections"),
                1,
                "active queue docs must be selected even when terminal docs fill the first page"
            );

            let repaired = queue
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "task_active_after_terminals" } })),
                    ..Default::default()
                }))
                .expect("queue task query")
                .exec(false)
                .await
                .expect("queue task document");
            assert_eq!(
                repaired.get("status").and_then(Value::as_str),
                Some("completed")
            );
            assert_eq!(
                repaired.get("route_status").and_then(Value::as_str),
                Some("handled")
            );
        });
    }

    #[test]
    fn reconcile_ctox_queue_task_projections_does_not_run_global_queue_repair() {
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
                    "id": "cmd_queue_local_completed",
                    "command_id": "cmd_queue_local_completed",
                    "module": "ctox",
                    "command_type": "ctox.test.completed",
                    "record_id": "cmd_queue_local_completed",
                    "status": "completed",
                    "inbound_channel": "ctox",
                    "payload": { "ok": true },
                    "client_context": {},
                    "updated_at_ms": 2_000
                }))
                .await
                .expect("insert completed command");

            let queue = database
                .collection("ctox_queue_tasks")
                .expect("ctox_queue_tasks collection");
            queue
                .insert(json!({
                    "id": "task_queue_local_completed",
                    "command_id": "cmd_queue_local_completed",
                    "title": "completed task",
                    "status": "queued",
                    "route_status": "queued",
                    "task_status": "queued",
                    "module": "ctox",
                    "source_module": "ctox",
                    "inbound_channel": "ctox",
                    "updated_at_ms": 1_000
                }))
                .await
                .expect("insert stale queue projection");

            store::push_collection_records(
                root.path(),
                json!({
                    "collection": "ctox_queue_tasks",
                    "documents": [
                        {
                            "id": "task_unrelated_old_orphan",
                            "command_id": "cmd_unrelated_missing",
                            "title": "unrelated old orphan",
                            "status": "queued",
                            "route_status": "queued",
                            "task_status": "queued",
                            "module": "ctox",
                            "source_module": "ctox",
                            "inbound_channel": "ctox",
                            "updated_at_ms": 1
                        }
                    ]
                }),
            )
            .expect("seed unrelated stale queue business record");

            assert_eq!(
                reconcile_ctox_queue_task_projections(root.path(), &database)
                    .await
                    .expect("reconcile queue projections"),
                1
            );

            let conn = store::open_store(root.path()).expect("open business store");
            let local_payload_json: String = conn
                .query_row(
                    "SELECT payload_json
                     FROM business_records
                     WHERE collection = 'ctox_queue_tasks' AND record_id = 'task_queue_local_completed'",
                    [],
                    |row| row.get(0),
                )
                .expect("local queue task business record");
            let local_payload: Value =
                serde_json::from_str(&local_payload_json).expect("local queue payload");
            assert_eq!(
                local_payload.get("status").and_then(Value::as_str),
                Some("completed")
            );

            let unrelated_payload_json: String = conn
                .query_row(
                    "SELECT payload_json
                     FROM business_records
                     WHERE collection = 'ctox_queue_tasks' AND record_id = 'task_unrelated_old_orphan'",
                    [],
                    |row| row.get(0),
                )
                .expect("unrelated queue task business record");
            let unrelated_payload: Value =
                serde_json::from_str(&unrelated_payload_json).expect("unrelated queue payload");
            assert_eq!(
                unrelated_payload.get("status").and_then(Value::as_str),
                Some("queued"),
                "local queue reconciliation must not run global orphan repair"
            );
            assert!(
                unrelated_payload.get("repair_note").is_none(),
                "global queue repair would have marked unrelated records"
            );
            assert!(
                unrelated_payload.get("error").is_none(),
                "global queue repair would have failed the unrelated orphan"
            );
        });
    }

    #[test]
    fn queue_chat_repair_idle_gate_skips_unchanged_sources() {
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

            let mut last_projection_stamp =
                Some(queue_chat_repair_projection_stamp(root.path()).expect("initial stamp"));
            let unchanged_stamp = last_projection_stamp.clone();
            assert_eq!(
                reconcile_queue_chat_tracking_projections_if_changed(
                    root.path(),
                    &database,
                    &mut last_projection_stamp
                )
                .await
                .expect("idle queue/chat repair gate"),
                0
            );
            assert_eq!(last_projection_stamp, unchanged_stamp);

            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            commands
                .insert(json!({
                    "id": "cmd_queue_idle_gate",
                    "command_id": "cmd_queue_idle_gate",
                    "module": "ctox",
                    "command_type": "ctox.test.idle_gate",
                    "record_id": "cmd_queue_idle_gate",
                    "status": "completed",
                    "inbound_channel": "ctox",
                    "payload": { "ok": true },
                    "client_context": {},
                    "updated_at_ms": 2_000
                }))
                .await
                .expect("insert completed command");

            let queue = database
                .collection("ctox_queue_tasks")
                .expect("ctox_queue_tasks collection");
            queue
                .insert(json!({
                    "id": "task_queue_idle_gate",
                    "command_id": "cmd_queue_idle_gate",
                    "title": "idle gate task",
                    "status": "queued",
                    "route_status": "queued",
                    "task_status": "queued",
                    "module": "ctox",
                    "source_module": "ctox",
                    "inbound_channel": "ctox",
                    "updated_at_ms": 1_000
                }))
                .await
                .expect("insert stale queue projection");

            assert_eq!(
                reconcile_queue_chat_tracking_projections_if_changed(
                    root.path(),
                    &database,
                    &mut last_projection_stamp
                )
                .await
                .expect("changed queue/chat repair gate"),
                1
            );
            let repaired = queue
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "task_queue_idle_gate" } })),
                    ..Default::default()
                }))
                .expect("queue task query")
                .exec(false)
                .await
                .expect("queue task document");
            assert_eq!(
                repaired.get("status").and_then(Value::as_str),
                Some("completed")
            );

            assert_eq!(
                reconcile_queue_chat_tracking_projections_if_changed(
                    root.path(),
                    &database,
                    &mut last_projection_stamp
                )
                .await
                .expect("unchanged queue/chat repair gate"),
                0
            );
        });
    }

    #[test]
    fn reconcile_ctox_queue_task_projections_fails_orphaned_accepted_commands() {
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
            let stale_at_ms = (now_ms() as i64).saturating_sub(20 * 60 * 1_000);
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            commands
                .insert(json!({
                    "id": "cmd_queue_orphaned",
                    "command_id": "cmd_queue_orphaned",
                    "module": "ctox",
                    "command_type": "ctox.test.orphaned",
                    "record_id": "cmd_queue_orphaned",
                    "status": "accepted",
                    "inbound_channel": "ctox",
                    "payload": { "ok": true },
                    "client_context": {},
                    "updated_at_ms": stale_at_ms
                }))
                .await
                .expect("insert accepted command");

            let queue = database
                .collection("ctox_queue_tasks")
                .expect("ctox_queue_tasks collection");
            queue
                .insert(json!({
                    "id": "task_queue_orphaned",
                    "command_id": "cmd_queue_orphaned",
                    "title": "orphaned task",
                    "status": "queued",
                    "route_status": "queued",
                    "task_status": "queued",
                    "module": "ctox",
                    "source_module": "ctox",
                    "inbound_channel": "ctox",
                    "updated_at_ms": stale_at_ms
                }))
                .await
                .expect("insert orphaned queue projection");

            assert_eq!(
                reconcile_ctox_queue_task_projections(root.path(), &database)
                    .await
                    .expect("reconcile queue projections"),
                1
            );

            let repaired = queue
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "task_queue_orphaned" } })),
                    ..Default::default()
                }))
                .expect("queue task query")
                .exec(false)
                .await
                .expect("queue task document");
            assert_eq!(
                repaired.get("status").and_then(Value::as_str),
                Some("failed")
            );
            assert_eq!(
                repaired.get("route_status").and_then(Value::as_str),
                Some("failed")
            );
            assert!(repaired.get("error").and_then(Value::as_str).is_some());

            let conn = store::open_store(root.path()).expect("open business store");
            let command_payload_json: String = conn
                .query_row(
                    "SELECT payload_json
                     FROM business_records
                     WHERE collection = 'business_commands' AND record_id = 'cmd_queue_orphaned'",
                    [],
                    |row| row.get(0),
                )
                .expect("business command repair record");
            let command_payload: Value =
                serde_json::from_str(&command_payload_json).expect("command payload");
            assert_eq!(
                command_payload.get("status").and_then(Value::as_str),
                Some("failed")
            );
            assert!(command_payload
                .get("error")
                .and_then(Value::as_str)
                .is_some());
        });
    }

    #[test]
    fn reconcile_business_chat_tracking_projections_fails_orphaned_messages() {
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
            let stale_at_ms = (now_ms() as i64).saturating_sub(20 * 60 * 1_000);
            let chats = database
                .collection("business_chats")
                .expect("business_chats collection");
            chats
                .insert(json!({
                    "id": "chat_orphaned",
                    "title": "stale chat",
                    "open": true,
                    "tracking_active": true,
                    "tracking_status": "queued",
                    "tracking_id": "task_missing",
                    "tracking_command_id": "cmd_missing",
                    "tracking_task_id": "task_missing",
                    "tracking_message_id": "status_cmd_missing",
                    "messages": [
                        {
                            "id": "status_cmd_missing",
                            "role": "ctox",
                            "text": "Task angelegt und in der CTOX Queue.",
                            "commandId": "cmd_missing",
                            "taskId": "task_missing",
                            "status": "queued",
                            "createdAt": stale_at_ms
                        }
                    ],
                    "updated_at_ms": stale_at_ms
                }))
                .await
                .expect("insert stale chat projection");

            assert_eq!(
                reconcile_business_chat_tracking_projections(&database)
                    .await
                    .expect("reconcile chat projections"),
                1
            );

            let repaired = chats
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "chat_orphaned" } })),
                    ..Default::default()
                }))
                .expect("chat query")
                .exec(false)
                .await
                .expect("chat document");
            let message = repaired
                .get("messages")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .expect("chat status message");
            assert_eq!(
                message.get("status").and_then(Value::as_str),
                Some("failed")
            );
            assert_eq!(
                message.get("trackable").and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(
                repaired.get("tracking_active").and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(
                repaired.get("tracking_status").and_then(Value::as_str),
                Some("failed")
            );
            assert!(message
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .contains("kein passender Command"));
        });
    }

    #[test]
    fn reconcile_business_chat_tracking_projections_filters_to_active_tracking() {
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
                    "id": "cmd_chat_active_after_inactive",
                    "command_id": "cmd_chat_active_after_inactive",
                    "module": "ctox",
                    "command_type": "business_os.chat.task",
                    "record_id": "chat_active_after_inactive",
                    "status": "completed",
                    "inbound_channel": "ctox",
                    "payload": { "ok": true },
                    "client_context": {},
                    "updated_at_ms": 5_000
                }))
                .await
                .expect("insert completed chat command");

            let chats = database
                .collection("business_chats")
                .expect("business_chats collection");
            let mut rxdb_conn =
                Connection::open(store::rxdb_store_path(root.path())).expect("open rxdb sqlite");
            let chat_table = rxdb_test_table_name(&rxdb_conn, "business_chats", 0);
            let chat_table_sql = quote_sqlite_identifier(&chat_table);
            let insert_sql = format!(
                "INSERT INTO {chat_table_sql} (id, revision, deleted, lastWriteTime, data)
                 VALUES (?1, ?2, 0, ?3, ?4)"
            );
            let tx = rxdb_conn.transaction().expect("seed inactive chats tx");
            for index in 0..600 {
                let id = format!("chat_inactive_{index:03}");
                let payload = json!({
                    "id": id.clone(),
                    "title": "inactive filler",
                    "open": false,
                    "tracking_active": false,
                    "tracking_status": "completed",
                    "tracking_id": format!("task_inactive_{index:03}"),
                    "tracking_task_id": format!("task_inactive_{index:03}"),
                    "messages": [],
                    "updated_at_ms": index,
                    "_deleted": false,
                    "_rev": "1-inactive",
                    "_meta": { "lwt": index as f64 }
                });
                tx.execute(
                    &insert_sql,
                    params![
                        id,
                        "1-inactive",
                        index as f64,
                        serde_json::to_string(&payload).expect("serialize inactive chat payload")
                    ],
                )
                .expect("insert inactive filler chat");
            }
            tx.commit().expect("commit inactive chats seed");

            chats
                .insert(json!({
                    "id": "chat_active_after_inactive",
                    "title": "active after inactive fillers",
                    "open": true,
                    "tracking_active": true,
                    "tracking_status": "queued",
                    "tracking_id": "cmd_chat_active_after_inactive",
                    "tracking_command_id": "cmd_chat_active_after_inactive",
                    "tracking_task_id": "",
                    "tracking_message_id": "status_cmd_chat_active_after_inactive",
                    "messages": [
                        {
                            "id": "status_cmd_chat_active_after_inactive",
                            "role": "ctox",
                            "text": "Task angelegt und in der CTOX Queue.",
                            "commandId": "cmd_chat_active_after_inactive",
                            "taskId": "",
                            "status": "queued",
                            "createdAt": 6_000
                        }
                    ],
                    "updated_at_ms": 6_000
                }))
                .await
                .expect("insert active chat projection");

            assert_eq!(
                reconcile_business_chat_tracking_projections(&database)
                    .await
                    .expect("reconcile chat projections"),
                1,
                "active chat docs must be selected even when inactive docs fill the first page"
            );

            let repaired = chats
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "chat_active_after_inactive" } })),
                    ..Default::default()
                }))
                .expect("chat query")
                .exec(false)
                .await
                .expect("chat document");
            let message = repaired
                .get("messages")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .expect("chat status message");
            assert_eq!(
                message.get("status").and_then(Value::as_str),
                Some("completed")
            );
            assert_eq!(
                repaired.get("tracking_active").and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(
                repaired.get("tracking_status").and_then(Value::as_str),
                Some("completed")
            );
        });
    }

    #[test]
    fn reconcile_business_chat_tracking_projections_batches_active_document_lookups() {
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
            let queue = database
                .collection("ctox_queue_tasks")
                .expect("ctox_queue_tasks collection");
            let mut messages = Vec::new();
            for index in 0..40 {
                let command_id = format!("cmd_chat_batch_{index:03}");
                let task_id = format!("task_chat_batch_{index:03}");
                commands
                    .insert(json!({
                        "id": command_id.clone(),
                        "command_id": command_id.clone(),
                        "module": "ctox",
                        "command_type": "business_os.chat.task",
                        "record_id": "chat_batch",
                        "status": "running",
                        "task_id": task_id.clone(),
                        "task_status": "running",
                        "inbound_channel": "ctox",
                        "payload": { "ok": true },
                        "client_context": {},
                        "updated_at_ms": index
                    }))
                    .await
                    .expect("insert batch chat command");
                queue
                    .insert(json!({
                        "id": task_id.clone(),
                        "command_id": command_id.clone(),
                        "title": "batch chat task",
                        "status": "completed",
                        "route_status": "handled",
                        "task_status": "completed",
                        "module": "ctox",
                        "source_module": "ctox",
                        "inbound_channel": "ctox",
                        "updated_at_ms": index
                    }))
                    .await
                    .expect("insert batch chat queue task");
                messages.push(json!({
                    "id": format!("status_cmd_chat_batch_{index:03}"),
                    "role": "ctox",
                    "text": "Task angelegt und in der CTOX Queue.",
                    "commandId": command_id,
                    "taskId": task_id,
                    "status": "queued",
                    "createdAt": index
                }));
            }

            let chats = database
                .collection("business_chats")
                .expect("business_chats collection");
            chats
                .insert(json!({
                    "id": "chat_batch",
                    "title": "batch active chat",
                    "open": true,
                    "tracking_active": true,
                    "tracking_status": "queued",
                    "tracking_id": "task_chat_batch_039",
                    "tracking_command_id": "cmd_chat_batch_039",
                    "tracking_task_id": "task_chat_batch_039",
                    "tracking_message_id": "status_cmd_chat_batch_039",
                    "messages": messages,
                    "updated_at_ms": 10_000
                }))
                .await
                .expect("insert batch chat projection");

            CHAT_TRACKING_BATCH_DOCUMENT_LOOKUPS.store(0, Ordering::Relaxed);
            assert_eq!(
                reconcile_business_chat_tracking_projections(&database)
                    .await
                    .expect("reconcile chat projections"),
                1
            );
            assert_eq!(
                CHAT_TRACKING_BATCH_DOCUMENT_LOOKUPS.load(Ordering::Relaxed),
                2,
                "active Chat tracking repair must batch command and task lookups"
            );

            let repaired = chats
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "chat_batch" } })),
                    ..Default::default()
                }))
                .expect("chat query")
                .exec(false)
                .await
                .expect("chat document");
            let repaired_messages = repaired
                .get("messages")
                .and_then(Value::as_array)
                .expect("repaired chat messages");
            assert_eq!(repaired_messages.len(), 40);
            assert!(repaired_messages.iter().all(|message| {
                message.get("status").and_then(Value::as_str) == Some("completed")
            }));
            assert_eq!(
                repaired.get("tracking_active").and_then(Value::as_bool),
                Some(false)
            );
            assert_eq!(
                repaired.get("tracking_status").and_then(Value::as_str),
                Some("completed")
            );
        });
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
    fn desktop_file_index_idle_gate_skips_unchanged_scan_roots() {
        let root = tempfile::tempdir().expect("temp root");
        let imports_dir = root.path().join("runtime/business-os-imports");
        fs::create_dir_all(&imports_dir).expect("create imports dir");
        let file_path = imports_dir.join("handoff.md");
        fs::write(&file_path, b"# Handoff\n\ninitial\n").expect("write import file");

        let mut last_projection_stamp = None;
        let first = sync_desktop_file_index_if_changed(root.path(), &mut last_projection_stamp)
            .expect("first desktop file index sync");
        assert_eq!(first, 1);

        let second = sync_desktop_file_index_if_changed(root.path(), &mut last_projection_stamp)
            .expect("unchanged desktop file index sync");
        assert_eq!(
            second, 0,
            "unchanged scan roots must not enter the RxDB write path"
        );

        fs::write(
            &file_path,
            b"# Handoff\n\nchanged content with a different size\n",
        )
        .expect("update import file");
        let third = sync_desktop_file_index_if_changed(root.path(), &mut last_projection_stamp)
            .expect("changed desktop file index sync");
        assert_eq!(third, 1);

        let fourth = sync_desktop_file_index_if_changed(root.path(), &mut last_projection_stamp)
            .expect("stable desktop file index sync after change");
        assert_eq!(
            fourth, 0,
            "the source stamp must settle after the changed file was synced"
        );

        let file_id = desktop_file_id(&file_path.canonicalize().expect("canonical file"));
        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let file = read_desktop_file_row(&conn, &file_id);
        assert_eq!(
            file.get("size_bytes").and_then(Value::as_u64),
            Some(b"# Handoff\n\nchanged content with a different size\n".len() as u64)
        );
    }

    #[test]
    fn desktop_file_background_scan_gate_skips_recursive_scan_until_dirty_or_fallback() {
        let root = tempfile::tempdir().expect("temp root");
        let imports_dir = root.path().join("runtime/business-os-imports");
        fs::create_dir_all(&imports_dir).expect("create imports dir");
        let file_path = imports_dir.join("handoff.md");
        fs::write(&file_path, b"# Handoff\n\ninitial\n").expect("write import file");

        let scan_roots = desktop_file_scan_roots(root.path());
        let first_stamp = desktop_file_scan_roots_stamp(&scan_roots);
        let first_scan_at = UNIX_EPOCH + Duration::from_secs(1_000);
        assert!(
            desktop_file_index_should_collect_scan(None, None, &first_stamp, true, first_scan_at),
            "first background round must collect a full scan"
        );
        assert!(
            !desktop_file_index_should_collect_scan(
                Some(&first_stamp),
                Some(first_scan_at),
                &first_stamp,
                false,
                first_scan_at + Duration::from_secs(DESKTOP_FILE_SCAN_INTERVAL_SECS)
            ),
            "unchanged roots must not recurse every desktop scan interval"
        );
        assert!(
            desktop_file_index_should_collect_scan(
                Some(&first_stamp),
                Some(first_scan_at),
                &first_stamp,
                true,
                first_scan_at + Duration::from_secs(DESKTOP_FILE_SCAN_INTERVAL_SECS)
            ),
            "watcher-dirty roots must collect a full scan without waiting for fallback"
        );

        fs::write(
            &file_path,
            b"# Handoff\n\nchanged content with a different size\n",
        )
        .expect("update import file");
        let changed_stamp = desktop_file_scan_roots_stamp(&scan_roots);
        assert_ne!(
            first_stamp, changed_stamp,
            "direct scan-root file metadata changes must dirty the cheap root stamp"
        );
        assert!(
            desktop_file_index_should_collect_scan(
                Some(&first_stamp),
                Some(first_scan_at),
                &changed_stamp,
                false,
                first_scan_at + Duration::from_secs(DESKTOP_FILE_SCAN_INTERVAL_SECS)
            ),
            "dirty roots must collect a full scan without waiting for fallback"
        );
        assert!(
            desktop_file_index_should_collect_scan(
                Some(&changed_stamp),
                Some(first_scan_at),
                &changed_stamp,
                false,
                first_scan_at + Duration::from_secs(DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS)
            ),
            "unchanged roots still get a slow fallback scan for missed nested events"
        );
    }

    #[test]
    fn desktop_file_scan_roots_include_bounded_active_queue_workspaces() {
        let root = tempfile::tempdir().expect("temp root");
        let workspace = root.path().join("workspaces/active-command");
        fs::create_dir_all(&workspace).expect("create workspace");
        channels::create_queue_task(
            root.path(),
            channels::QueueTaskCreateRequest {
                title: "Index active workspace".to_string(),
                prompt: "Keep the active workspace visible.".to_string(),
                thread_key: "workspace/index-test".to_string(),
                workspace_root: Some(workspace.to_string_lossy().into_owned()),
                priority: "normal".to_string(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )
        .expect("create queue task");

        let canonical = workspace.canonicalize().expect("canonical workspace");
        let roots = desktop_file_scan_roots(root.path());
        assert!(
            roots.iter().any(|entry| entry.path == canonical),
            "active queue workspace must be part of the bounded background scan roots"
        );
    }

    #[test]
    fn desktop_file_background_sleep_uses_slow_fallback_after_successful_scan() {
        let first_scan_at = UNIX_EPOCH + Duration::from_secs(1_000);
        let now = first_scan_at + Duration::from_secs(DESKTOP_FILE_SCAN_INTERVAL_SECS);
        assert_eq!(
            desktop_file_index_sleep_interval(true, first_scan_at, Some(first_scan_at), now),
            Duration::from_secs(
                DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS - DESKTOP_FILE_SCAN_INTERVAL_SECS
            ),
            "without a watcher, stable roots should still use the slow fallback after a full scan"
        );
        assert_eq!(
            desktop_file_index_sleep_interval(true, first_scan_at, Some(first_scan_at), now),
            Duration::from_secs(
                DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS - DESKTOP_FILE_SCAN_INTERVAL_SECS
            ),
            "with a watcher, unchanged roots should sleep until the slow fallback scan"
        );
        assert_eq!(
            desktop_file_index_sleep_interval(
                true,
                first_scan_at,
                Some(first_scan_at),
                first_scan_at + Duration::from_secs(DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS)
            ),
            Duration::ZERO,
            "the fallback scan is due immediately at the fallback boundary"
        );
        assert_eq!(
            desktop_file_index_sleep_interval(false, first_scan_at, Some(first_scan_at), now),
            Duration::from_secs(
                DESKTOP_FILE_SCAN_FALLBACK_INTERVAL_SECS - DESKTOP_FILE_SCAN_INTERVAL_SECS
            ),
            "with no scan roots, the background loop must not poll every desktop scan interval"
        );
    }

    #[test]
    fn sync_desktop_files_from_workspace_root_indexes_agent_workspace() {
        let root = tempfile::tempdir().expect("temp root");
        let workspace = root.path().join("agent-workspace");
        let nested = workspace.join("reports");
        fs::create_dir_all(&nested).expect("create workspace dirs");
        let file_path = nested.join("brief.md");
        let pdf_path = nested.join("report.pdf");
        let binary_path = nested.join("simulation.ctoxdata");
        fs::write(&file_path, b"# Brief\n\nvisible from Business OS\n").expect("write file");
        fs::write(&pdf_path, b"%PDF-1.7\nworkspace report\n").expect("write PDF");
        fs::write(&binary_path, b"opaque workspace payload").expect("write binary file");

        let indexed = sync_desktop_files_from_workspace_root(root.path(), &workspace)
            .expect("sync workspace root");
        assert_eq!(indexed, 3);

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

        for (path, expected_state) in [(&pdf_path, "available"), (&binary_path, "lazy")] {
            let id = desktop_file_id(&path.canonicalize().expect("canonical output file"));
            let json: String = conn
                .query_row(
                    "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .expect("generic workspace file row");
            let record: Value = serde_json::from_str(&json).expect("generic file json");
            assert_eq!(
                record.get("content_state").and_then(Value::as_str),
                Some(expected_state)
            );
            assert!(
                record
                    .get("virtual_path")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.starts_with("/CTOX/agent-workspace/reports/")),
                "every workspace file has a user-visible CTOX path"
            );
        }
    }

    #[tokio::test]
    async fn desktop_file_chunk_completion_uses_primary_chunk_ids() {
        let root = tempfile::tempdir().expect("temp root");
        let database = open_test_database(root.path().join("chunk-complete.sqlite3"))
            .await
            .expect("open db");
        database
            .add_collections(HashMap::from([(
                "desktop_file_chunks".to_string(),
                RxCollectionCreator {
                    schema: business_os_schema("desktop_file_chunks", "id"),
                    conflict_handler: None,
                    options: HashMap::new(),
                },
            )]))
            .await
            .expect("add desktop_file_chunks");
        let chunks = database
            .collection("desktop_file_chunks")
            .expect("desktop_file_chunks collection");
        chunks
            .incremental_upsert(json!({
                "id": "file_a_gen_a_0",
                "file_id": "file_a",
                "generation_id": "gen_a",
                "content_hash": "hash",
                "content_hash_scheme": DESKTOP_FILE_CONTENT_HASH_SCHEME,
                "idx": 0,
                "total": 1,
                "encoding": "base64",
                "data": "YQ==",
                "chunk_hash": "chunk_hash",
                "chunk_hash_scheme": DESKTOP_FILE_CHUNK_HASH_SCHEME,
                "size_bytes": 4,
                "created_at_ms": 1,
            }))
            .await
            .expect("insert chunk");

        assert!(
            desktop_file_chunk_generation_is_complete(&database, "file_a", "gen_a", 1).await,
            "the deterministic primary-key chunk set is complete"
        );
        assert!(
            !desktop_file_chunk_generation_is_complete(&database, "file_a", "missing_gen", 1).await,
            "a missing deterministic primary-key chunk set is incomplete"
        );
    }

    #[test]
    fn desktop_file_scan_rejects_ctox_internal_roots() {
        let home = PathBuf::from("/tmp/ctox-home");
        assert!(is_ctox_internal_desktop_scan_root(
            &home.join(".local/lib/ctox/current"),
            &home
        ));
        assert!(is_ctox_internal_desktop_scan_root(
            &home.join(".local/state/ctox/backups/update"),
            &home
        ));
        assert!(!is_ctox_internal_desktop_scan_root(
            &home.join("workspace/project"),
            &home
        ));
    }

    #[test]
    fn desktop_file_scan_rejects_broad_roots() {
        assert!(!is_safe_desktop_file_scan_root(Path::new("/")));
        if Path::new("/Users").is_dir() {
            assert!(!is_safe_desktop_file_scan_root(Path::new("/Users")));
        }
        assert!(!is_broad_desktop_file_scan_root(Path::new(
            "/Users/example/project"
        )));
        assert!(is_broad_desktop_file_scan_root(Path::new("/Users/example")));
        assert!(is_broad_desktop_file_scan_root(Path::new("/var")));
        assert!(is_broad_desktop_file_scan_root(Path::new("/var/folders")));
    }

    #[test]
    fn sync_desktop_file_from_path_rejects_internal_ctox_file() {
        let root = tempfile::tempdir().expect("temp root");
        let home = tempfile::tempdir().expect("temp home");
        let internal_dir = home.path().join(".local/lib/ctox/current/logs");
        fs::create_dir_all(&internal_dir).expect("create internal dir");
        let internal_file = internal_dir.join("ctox-real.log");
        fs::write(&internal_file, b"internal file content").expect("write internal file");

        let err = sync_desktop_file_from_path(root.path(), &internal_file)
            .expect_err("internal CTOX files must not enter the desktop file index");
        assert!(
            format!("{err:#}").contains("outside the Business OS file-index boundary"),
            "unexpected error: {err:#}"
        );
        assert!(
            !store::rxdb_store_path(root.path()).exists(),
            "rejected file sync must not create the RxDB file index"
        );
    }

    #[test]
    fn background_desktop_scan_uses_only_static_business_os_roots() {
        let root = tempfile::tempdir().expect("temp root");
        for rel in [
            "runtime/business-os/notes",
            "runtime/business-os/documents/generated",
            "runtime/business-os-imports",
        ] {
            fs::create_dir_all(root.path().join(rel)).expect("create static scan root");
        }

        let roots = desktop_file_scan_roots(root.path());
        let labels: HashSet<&str> = roots.iter().map(|root| root.label.as_str()).collect();
        assert_eq!(roots.len(), 3);
        assert!(labels.contains("Notes"));
        assert!(labels.contains("Generated Documents"));
        assert!(labels.contains("Imports"));
        assert!(
            !roots
                .iter()
                .any(|root| root.path.to_string_lossy().contains(".local/lib/ctox")),
            "background scan must not index CTOX release/install roots"
        );
    }

    #[test]
    fn desktop_file_index_maintenance_removes_internal_file_chunks() {
        let root = tempfile::tempdir().expect("temp root");
        let home = tempfile::tempdir().expect("temp home");
        let internal_dir = home
            .path()
            .join(".local/lib/ctox/releases/test-release/bin");
        fs::create_dir_all(&internal_dir).expect("create internal dir");
        let internal_file = internal_dir.join("ctox-real.log");
        fs::write(&internal_file, b"internal file content").expect("write internal file");

        let safe_file = root.path().join("safe.log");
        fs::write(&safe_file, b"legacy indexed content").expect("write safe seed file");
        sync_desktop_file_from_path(root.path(), &safe_file).expect("sync safe seed file");
        let file_id = desktop_file_id(&safe_file.canonicalize().expect("canonical safe file"));
        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let file_json: String = conn
            .query_row(
                "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [file_id.as_str()],
                |row| row.get(0),
            )
            .expect("file row before maintenance");
        let mut file_doc: Value = serde_json::from_str(&file_json).expect("file doc json");
        let internal_path = internal_file
            .canonicalize()
            .expect("canonical internal file")
            .display()
            .to_string();
        if let Some(object) = file_doc.as_object_mut() {
            object.insert("path".to_string(), Value::String(internal_path.clone()));
            object.insert("local_path".to_string(), Value::String(internal_path));
        }
        conn.execute(
            "UPDATE ctox_business_os__desktop_files__v0 SET data = ?2 WHERE id = ?1",
            params![
                file_id,
                serde_json::to_string(&file_doc).expect("serialize file doc")
            ],
        )
        .expect("rewrite legacy unsafe file path");
        let before_chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 WHERE id LIKE ?1",
                params![format!("{file_id}_%")],
                |row| row.get(0),
            )
            .expect("chunk count before maintenance");
        assert!(before_chunks > 0, "test setup must create an eager chunk");
        drop(conn);

        let stats = compact_desktop_file_index_store_sync(root.path(), Some(home.path()))
            .expect("compact desktop index");
        assert_eq!(stats.tombstoned_unsafe_files, 1);
        assert_eq!(stats.removed_unsafe_chunks, before_chunks as usize);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("reopen sqlite");
        let file_row = read_desktop_file_row(&conn, &file_id);
        assert_eq!(
            file_row.get("_deleted").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            file_row.get("is_deleted").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            file_row.get("tombstone_reason").and_then(Value::as_str),
            Some("unsafe_internal_ctox_path")
        );
        let after_chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 WHERE id LIKE ?1",
                params![format!("{file_id}_%")],
                |row| row.get(0),
            )
            .expect("chunk count after maintenance");
        assert_eq!(after_chunks, 0);
    }

    #[test]
    fn desktop_file_chunk_cache_quota_evicts_active_rematerializable_file() {
        let root = safe_business_os_tempdir("chunk-cache-evict");
        let file_path = root.path().join("materialized-cache.txt");
        fs::write(&file_path, vec![b'a'; DESKTOP_FILE_CHUNK_SIZE + 128]).expect("write cache file");
        sync_desktop_file_from_path(root.path(), &file_path).expect("sync cache file");
        let canonical = file_path.canonicalize().expect("canonical cache file");
        let file_id = desktop_file_id(&canonical);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
        let before_chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 \
                 WHERE id LIKE ?1 AND deleted = 0",
                params![format!("{file_id}_%")],
                |row| row.get(0),
            )
            .expect("chunk count before cache eviction");
        assert!(before_chunks > 0);
        drop(conn);

        let stats = compact_desktop_file_index_store_sync_with_config(
            root.path(),
            None,
            DesktopFileChunkCacheConfig {
                max_live_bytes: 1,
                target_live_bytes: 0,
                active_min_age_secs: 0,
                max_files_per_pass: 10,
                max_chunks_per_pass: 100,
                checkpoint_min_interval_secs: u64::MAX / 1_000,
                wal_checkpoint_min_bytes: u64::MAX,
                vacuum_min_interval_secs: u64::MAX / 1_000,
                vacuum_min_reclaim_bytes: u64::MAX,
            },
        )
        .expect("compact cache");
        assert_eq!(stats.evicted_cache_files, 1, "{stats:?}");
        assert_eq!(
            stats.removed_cache_chunks, before_chunks as usize,
            "{stats:?}"
        );
        assert_eq!(stats.cache_live_bytes_after, 0, "{stats:?}");

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("reopen sqlite");
        let file = read_desktop_file_row(&conn, &file_id);
        assert_eq!(
            file.get("content_state").and_then(Value::as_str),
            Some("lazy")
        );
        assert_eq!(file.get("content_generation_id"), Some(&Value::Null));
        assert_eq!(file.get("chunk_count"), Some(&Value::Null));
        assert_eq!(
            file.get("content_eviction_reason").and_then(Value::as_str),
            Some("desktop_file_chunk_cache_quota")
        );
        let after_chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 \
                 WHERE id LIKE ?1",
                params![format!("{file_id}_%")],
                |row| row.get(0),
            )
            .expect("chunk count after cache eviction");
        assert_eq!(after_chunks, 0);
    }

    #[test]
    fn desktop_file_chunk_cache_quota_keeps_eager_scan_root_file_pinned() {
        let root = safe_business_os_tempdir("chunk-cache-pinned");
        let import_dir = root.path().join("runtime/business-os-imports");
        fs::create_dir_all(&import_dir).expect("create import scan root");
        let file_path = import_dir.join("small-eager.txt");
        fs::write(&file_path, b"small eager scan-root file").expect("write eager file");
        sync_desktop_file_from_path(root.path(), &file_path).expect("sync eager file");
        let canonical = file_path.canonicalize().expect("canonical eager file");
        let file_id = desktop_file_id(&canonical);

        let stats = compact_desktop_file_index_store_sync_with_config(
            root.path(),
            None,
            DesktopFileChunkCacheConfig {
                max_live_bytes: 1,
                target_live_bytes: 0,
                active_min_age_secs: 0,
                max_files_per_pass: 10,
                max_chunks_per_pass: 100,
                checkpoint_min_interval_secs: u64::MAX / 1_000,
                wal_checkpoint_min_bytes: u64::MAX,
                vacuum_min_interval_secs: u64::MAX / 1_000,
                vacuum_min_reclaim_bytes: u64::MAX,
            },
        )
        .expect("compact cache");
        assert_eq!(stats.evicted_cache_files, 0, "{stats:?}");
        assert_eq!(stats.removed_cache_chunks, 0, "{stats:?}");
        assert!(stats.cache_over_quota_pinned_bytes > 0, "{stats:?}");

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("reopen sqlite");
        let file = read_desktop_file_row(&conn, &file_id);
        assert_eq!(
            file.get("content_state").and_then(Value::as_str),
            Some("available")
        );
        assert!(file
            .get("content_generation_id")
            .and_then(Value::as_str)
            .is_some_and(|generation| !generation.is_empty()));
        let chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 \
                 WHERE id LIKE ?1 AND deleted = 0",
                params![format!("{file_id}_%")],
                |row| row.get(0),
            )
            .expect("pinned chunk count");
        assert!(chunks > 0);
    }

    #[test]
    fn desktop_file_index_maintenance_purges_old_unsafe_file_tombstones() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = store::rxdb_store_path(root.path());
        fs::create_dir_all(database_path.parent().expect("rxdb parent")).expect("create rxdb dir");
        let conn = Connection::open(&database_path).expect("open rxdb sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE ctox_business_os__desktop_files__v0 (
                id TEXT NOT NULL PRIMARY KEY UNIQUE,
                revision TEXT,
                deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
                lastWriteTime REAL NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE ctox_business_os__desktop_file_chunks__v0 (
                id TEXT NOT NULL PRIMARY KEY UNIQUE,
                revision TEXT,
                deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
                lastWriteTime REAL NOT NULL,
                data TEXT NOT NULL
            );
            "#,
        )
        .expect("create desktop index tables");
        let old_tombstone = json!({
            "id": "old-unsafe-file",
            "_rev": "2-ctox-maintenance",
            "_deleted": true,
            "kind": "file",
            "source": "ctox-core",
            "is_deleted": true,
            "local_path": "/Users/test/.local/lib/ctox/releases/old/file.md",
            "tombstone_reason": "unsafe_internal_ctox_path",
            "deleted_at_ms": 1,
        });
        let fresh_tombstone = json!({
            "id": "fresh-unsafe-file",
            "_rev": "2-ctox-maintenance",
            "_deleted": true,
            "kind": "file",
            "source": "ctox-core",
            "is_deleted": true,
            "local_path": "/Users/test/.local/lib/ctox/releases/fresh/file.md",
            "tombstone_reason": "unsafe_internal_ctox_path",
            "deleted_at_ms": now_ms() as u64,
        });
        let old_missing_tombstone = json!({
            "id": "old-missing-file",
            "_rev": "2-ctox-maintenance",
            "_deleted": true,
            "kind": "file",
            "source": "ctox-core",
            "is_deleted": true,
            "local_path": "/Users/test/Documents/missing.md",
            "tombstone_reason": "missing_from_scan",
            "deleted_at_ms": 1,
        });
        let old_browser_tombstone = json!({
            "id": "old-browser-file",
            "_rev": "2-browser",
            "_deleted": true,
            "kind": "file",
            "source": "browser-upload",
            "is_deleted": true,
            "local_path": "/Users/test/.local/lib/ctox/releases/browser/file.md",
            "tombstone_reason": "unsafe_internal_ctox_path",
            "deleted_at_ms": 1,
        });
        let live_unsafe_looking_row = json!({
            "id": "live-unsafe-looking-file",
            "_rev": "1-live",
            "kind": "file",
            "source": "ctox-core",
            "is_deleted": false,
            "local_path": "/Users/test/.local/lib/ctox/releases/live/file.md",
        });
        for (id, deleted, last_write_time, document) in [
            ("old-unsafe-file", 1, 1.0_f64, old_tombstone),
            ("fresh-unsafe-file", 1, f64::MAX, fresh_tombstone),
            ("old-missing-file", 1, 1.0_f64, old_missing_tombstone),
            ("old-browser-file", 1, 1.0_f64, old_browser_tombstone),
            (
                "live-unsafe-looking-file",
                0,
                1.0_f64,
                live_unsafe_looking_row,
            ),
        ] {
            conn.execute(
                "INSERT INTO ctox_business_os__desktop_files__v0 \
                 (id, revision, deleted, lastWriteTime, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    document
                        .get("_rev")
                        .and_then(Value::as_str)
                        .unwrap_or("1-seed"),
                    deleted,
                    last_write_time,
                    serde_json::to_string(&document).expect("desktop file json")
                ],
            )
            .expect("insert desktop file row");
        }
        drop(conn);

        let stats = compact_desktop_file_index_store_sync(root.path(), None)
            .expect("compact desktop index");
        assert_eq!(stats.removed_unsafe_file_tombstones, 1);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("reopen sqlite");
        let row_exists = |id: &str| -> i64 {
            conn.query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .expect("desktop file row count")
        };
        let old_exists = row_exists("old-unsafe-file");
        let fresh_exists = row_exists("fresh-unsafe-file");
        let missing_exists = row_exists("old-missing-file");
        let browser_exists = row_exists("old-browser-file");
        let live_exists = row_exists("live-unsafe-looking-file");
        assert_eq!(old_exists, 0);
        assert_eq!(fresh_exists, 1);
        assert_eq!(missing_exists, 1);
        assert_eq!(browser_exists, 1);
        assert_eq!(live_exists, 1);
    }

    #[test]
    fn desktop_file_index_maintenance_uses_bounded_unsafe_candidate_plan() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = store::rxdb_store_path(root.path());
        fs::create_dir_all(database_path.parent().expect("rxdb parent")).expect("create rxdb dir");
        let mut conn = Connection::open(&database_path).expect("open rxdb sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE ctox_business_os__desktop_files__v0 (
                id TEXT NOT NULL PRIMARY KEY UNIQUE,
                revision TEXT,
                deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
                lastWriteTime REAL NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE ctox_business_os__desktop_file_chunks__v0 (
                id TEXT NOT NULL PRIMARY KEY UNIQUE,
                revision TEXT,
                deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
                lastWriteTime REAL NOT NULL,
                data TEXT NOT NULL
            );
            "#,
        )
        .expect("create desktop index tables");
        {
            let tx = conn.transaction().expect("begin seed tx");
            for idx in 0..1_500 {
                let id = format!("safe-core-{idx}");
                let path = format!("/Users/test/Documents/safe-{idx}.md");
                let document = json!({
                    "id": id,
                    "_rev": "1-seed",
                    "kind": "file",
                    "source": "ctox-core",
                    "is_deleted": false,
                    "local_path": path,
                });
                tx.execute(
                    "INSERT INTO ctox_business_os__desktop_files__v0 \
                     (id, revision, deleted, lastWriteTime, data) VALUES (?1, ?2, 0, ?3, ?4)",
                    params![
                        document.get("id").and_then(Value::as_str).unwrap(),
                        "1-seed",
                        idx as f64,
                        serde_json::to_string(&document).expect("safe core json")
                    ],
                )
                .expect("insert safe core file");
            }
            for idx in 0..1_500 {
                let id = format!("upload-temp-{idx}");
                let path = format!("/tmp/upload-{idx}.md");
                let document = json!({
                    "id": id,
                    "_rev": "1-seed",
                    "kind": "file",
                    "source": "upload",
                    "is_deleted": false,
                    "local_path": path,
                });
                tx.execute(
                    "INSERT INTO ctox_business_os__desktop_files__v0 \
                     (id, revision, deleted, lastWriteTime, data) VALUES (?1, ?2, 0, ?3, ?4)",
                    params![
                        document.get("id").and_then(Value::as_str).unwrap(),
                        "1-seed",
                        idx as f64,
                        serde_json::to_string(&document).expect("upload json")
                    ],
                )
                .expect("insert upload file");
            }
            let unsafe_file = json!({
                "id": "unsafe-temp-file",
                "_rev": "1-seed",
                "kind": "file",
                "source": "ctox-core",
                "is_deleted": false,
                "local_path": "/tmp/ctox-leaked-index/file.md",
                "content_generation_id": "gen-1",
            });
            tx.execute(
                "INSERT INTO ctox_business_os__desktop_files__v0 \
                 (id, revision, deleted, lastWriteTime, data) VALUES (?1, ?2, 0, ?3, ?4)",
                params![
                    "unsafe-temp-file",
                    "1-seed",
                    9_999_f64,
                    serde_json::to_string(&unsafe_file).expect("unsafe file json")
                ],
            )
            .expect("insert unsafe file");
            let unsafe_chunk = json!({
                "id": "unsafe-temp-file_gen-1_0",
                "file_id": "unsafe-temp-file",
                "generation_id": "gen-1",
                "idx": 0,
                "total": 1,
                "encoding": "base64",
                "data": "",
            });
            tx.execute(
                "INSERT INTO ctox_business_os__desktop_file_chunks__v0 \
                 (id, revision, deleted, lastWriteTime, data) VALUES (?1, ?2, 0, ?3, ?4)",
                params![
                    "unsafe-temp-file_gen-1_0",
                    "1-seed",
                    9_999_f64,
                    serde_json::to_string(&unsafe_chunk).expect("unsafe chunk json")
                ],
            )
            .expect("insert unsafe chunk");
            tx.commit().expect("commit seed tx");
        }

        ensure_desktop_file_index_query_indexes(&conn).expect("ensure query indexes");
        conn.execute_batch("ANALYZE")
            .expect("analyze desktop index seed");
        let limit = DESKTOP_FILE_INDEX_MAINTENANCE_FILE_LIMIT as i64;
        let sql =
            unsafe_desktop_file_index_candidates_sql("\"ctox_business_os__desktop_files__v0\"");
        let plan = sqlite_query_plan(&conn, &sql, &[&limit]);
        assert!(
            plan.contains("ctox_business_os_desktop_files_live_core_idx"),
            "unsafe maintenance candidate query must use the live core index, got:\n{plan}"
        );
        assert!(
            !plan.contains("SCAN ctox_business_os__desktop_files__v0"),
            "unsafe maintenance candidate query must not full-scan desktop_files, got:\n{plan}"
        );
        drop(conn);

        let stats = compact_desktop_file_index_store_sync(root.path(), None)
            .expect("compact desktop index");
        assert_eq!(stats.tombstoned_unsafe_files, 1);
        assert_eq!(stats.removed_unsafe_chunks, 1);

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("reopen sqlite");
        let unsafe_row = read_desktop_file_row(&conn, "unsafe-temp-file");
        assert_eq!(
            unsafe_row.get("_deleted").and_then(Value::as_bool),
            Some(true)
        );
        let safe_deleted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_files__v0 \
                 WHERE id LIKE 'safe-core-%' AND deleted = 1",
                [],
                |row| row.get(0),
            )
            .expect("safe deleted count");
        assert_eq!(safe_deleted, 0);
        let upload_deleted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_files__v0 \
                 WHERE id LIKE 'upload-temp-%' AND deleted = 1",
                [],
                |row| row.get(0),
            )
            .expect("upload deleted count");
        assert_eq!(upload_deleted, 0);
        let unsafe_chunks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ctox_business_os__desktop_file_chunks__v0 \
                 WHERE id LIKE 'unsafe-temp-file_%'",
                [],
                |row| row.get(0),
            )
            .expect("unsafe chunk count");
        assert_eq!(unsafe_chunks, 0);
    }

    #[test]
    fn live_ctox_desktop_file_loader_filters_non_core_and_deleted_rows() {
        let root = tempfile::tempdir().expect("temp root");
        let database_path = store::rxdb_store_path(root.path());
        fs::create_dir_all(database_path.parent().expect("rxdb parent")).expect("create rxdb dir");
        let conn = Connection::open(&database_path).expect("open rxdb sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE ctox_business_os__desktop_files__v0 (
                id TEXT NOT NULL PRIMARY KEY UNIQUE,
                revision TEXT,
                deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
                lastWriteTime REAL NOT NULL,
                data TEXT NOT NULL
            );
            "#,
        )
        .expect("create desktop_files table");
        for (id, deleted, document) in [
            (
                "core-live",
                0,
                json!({
                    "id": "core-live",
                    "kind": "file",
                    "source": "ctox-core",
                    "is_deleted": false,
                    "local_path": "/Users/test/runtime/output/live.md"
                }),
            ),
            (
                "upload-live",
                0,
                json!({
                    "id": "upload-live",
                    "kind": "file",
                    "source": "upload",
                    "is_deleted": false,
                    "local_path": "/Users/test/Downloads/upload.md"
                }),
            ),
            (
                "core-deleted-json",
                0,
                json!({
                    "id": "core-deleted-json",
                    "kind": "file",
                    "source": "ctox-core",
                    "is_deleted": true,
                    "local_path": "/Users/test/runtime/output/deleted.md"
                }),
            ),
            (
                "core-deleted-column",
                1,
                json!({
                    "id": "core-deleted-column",
                    "kind": "file",
                    "source": "ctox-core",
                    "is_deleted": false,
                    "local_path": "/Users/test/runtime/output/deleted-column.md"
                }),
            ),
        ] {
            conn.execute(
                "INSERT INTO ctox_business_os__desktop_files__v0 \
                 (id, revision, deleted, lastWriteTime, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    "1-test",
                    deleted,
                    1.0_f64,
                    serde_json::to_string(&document).expect("desktop file json")
                ],
            )
            .expect("insert desktop file row");
        }
        drop(conn);

        ensure_desktop_file_index_query_indexes_for_root_sync(root.path()).expect("ensure index");
        let conn = Connection::open(&database_path).expect("reopen rxdb sqlite");
        let index_names = conn
            .prepare("PRAGMA index_list('ctox_business_os__desktop_files__v0')")
            .expect("prepare index list")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query index list")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect index list");
        assert!(index_names
            .iter()
            .any(|name| name == "ctox_business_os_desktop_files_live_core_idx"));
        drop(conn);

        let documents = load_live_ctox_desktop_file_documents_sync(root.path())
            .expect("load ctox-core desktop files");
        let ids = documents
            .iter()
            .filter_map(|doc| doc.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["core-live"]);
    }

    /// REGRESSION (data-plane churn): rescanning an UNCHANGED workspace must
    /// be a complete no-op. The 15s desktop-file index scan used to mint a
    /// fresh timestamped generation id for every file on every pass, rewrite
    /// all its chunks and tombstone the previous generation — an endless
    /// insert/tombstone churn that browser-side replication (batchSize 2 for
    /// desktop_file_chunks) could never catch up with (rxdb-soak churn mode
    /// failure). A content change must still rotate the generation.
    #[test]
    fn rescan_of_unchanged_workspace_is_a_no_op() {
        let root = tempfile::tempdir().expect("temp root");
        let workspace = root.path().join("agent-workspace");
        let nested = workspace.join("reports");
        fs::create_dir_all(&nested).expect("create workspace dirs");
        let file_path = nested.join("brief.md");
        fs::write(&file_path, b"# Brief\n\nstable content\n").expect("write file");

        sync_desktop_files_from_workspace_root(root.path(), &workspace).expect("first scan");

        let file_id = desktop_file_id(&file_path.canonicalize().expect("canonical file"));
        let first_file;
        let first_chunks;
        let first_desktop_files;
        {
            let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
            first_file = read_desktop_file_row(&conn, &file_id);
            first_chunks = read_desktop_file_chunks(&conn, &file_id, true);
            first_desktop_files = read_desktop_file_rows(&conn);
        }
        assert!(!first_chunks.is_empty(), "first scan produced chunks");

        DESKTOP_FILE_CHUNK_COMPLETENESS_CHECKS.store(0, Ordering::Relaxed);
        sync_desktop_files_from_workspace_root(root.path(), &workspace).expect("rescan");
        assert_eq!(
            DESKTOP_FILE_CHUNK_COMPLETENESS_CHECKS.load(Ordering::Relaxed),
            0,
            "verified unchanged file rescan must not re-check every chunk id"
        );

        let second_file;
        let second_chunks;
        let second_desktop_files;
        {
            let conn =
                Connection::open(store::rxdb_store_path(root.path())).expect("reopen sqlite");
            second_file = read_desktop_file_row(&conn, &file_id);
            second_chunks = read_desktop_file_chunks(&conn, &file_id, true);
            second_desktop_files = read_desktop_file_rows(&conn);
        }
        // Byte-identical documents (including _rev/_meta) prove the rescan
        // wrote NOTHING — no new generation, no chunk rotation, no
        // tombstones, no replication traffic.
        assert_eq!(
            first_file, second_file,
            "rescan of an unchanged file must not rewrite the file doc"
        );
        assert_eq!(
            first_chunks, second_chunks,
            "rescan of an unchanged file must not rotate its chunks"
        );
        assert_eq!(
            first_desktop_files, second_desktop_files,
            "rescan of an unchanged workspace must not rewrite folder docs"
        );

        // A real content change (different size) must still mint a new
        // generation whose chunks fully replace the old ones.
        fs::write(
            &file_path,
            b"# Brief\n\nupdated content, longer than before\n",
        )
        .expect("update file");
        sync_desktop_files_from_workspace_root(root.path(), &workspace).expect("third scan");

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("reopen sqlite");
        let third_file = read_desktop_file_row(&conn, &file_id);
        assert_ne!(
            second_file.get("content_generation_id"),
            third_file.get("content_generation_id"),
            "content change must mint a new generation"
        );
        let third_generation = third_file
            .get("content_generation_id")
            .and_then(Value::as_str)
            .expect("third generation id");
        let live_chunks = read_desktop_file_chunks(&conn, &file_id, false);
        let new_generation_chunks = live_chunks
            .iter()
            .filter(|chunk| {
                chunk.get("generation_id").and_then(Value::as_str) == Some(third_generation)
            })
            .count();
        assert!(
            new_generation_chunks > 0,
            "updated content produced chunks of the new generation"
        );
        // prune intentionally retains up to DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS
        // generations; what must never happen is unbounded rotation.
        let live_generations: HashSet<&str> = live_chunks
            .iter()
            .filter_map(|chunk| chunk.get("generation_id").and_then(Value::as_str))
            .collect();
        assert!(
            live_generations.len() <= DESKTOP_FILE_CHUNK_RETAIN_GENERATIONS,
            "live generations stay bounded (got {})",
            live_generations.len()
        );
    }

    /// REGRESSION (sticky materialization): a large file is indexed lazily by
    /// size policy; after an explicit ctox.file.materialize the periodic scan
    /// used to DEMOTE the doc back to lazy (empty content_generation_id),
    /// stranding the replicated chunks and reverting the browser file viewer
    /// to an unreadable state ~15s after every materialize (rxdb-soak
    /// workspace-large-file-viewer-restart). A rescan must keep the
    /// materialized doc byte-identical.
    #[test]
    fn materialized_large_file_survives_lazy_rescan() {
        let root = tempfile::tempdir().expect("temp root");
        let file_path = root.path().join("large.txt");
        fs::write(
            &file_path,
            vec![b'x'; DESKTOP_FILE_EAGER_LIMIT_BYTES as usize + 1],
        )
        .expect("write large artifact");
        let canonical = file_path.canonicalize().expect("canonical file");
        let file_id = desktop_file_id(&canonical);

        sync_desktop_file_from_path(root.path(), &file_path).expect("lazy index");
        materialize_desktop_file_from_path(root.path(), &file_path).expect("materialize");

        let file_before;
        let chunks_before;
        {
            let conn = Connection::open(store::rxdb_store_path(root.path())).expect("open sqlite");
            file_before = read_desktop_file_row(&conn, &file_id);
            chunks_before = read_desktop_file_chunks(&conn, &file_id, true);
        }
        assert_eq!(
            file_before.get("content_state").and_then(Value::as_str),
            Some("available"),
            "materialize produced an available doc"
        );
        assert!(
            file_before
                .get("content_generation_id")
                .and_then(Value::as_str)
                .is_some_and(|generation| !generation.is_empty()),
            "materialize stamped a generation"
        );
        assert!(!chunks_before.is_empty(), "materialize produced chunks");

        // The periodic index scan revisits the file with its size policy
        // (lazy). It must keep the materialized state, not demote it.
        DESKTOP_FILE_CHUNK_COMPLETENESS_CHECKS.store(0, Ordering::Relaxed);
        sync_desktop_file_from_path(root.path(), &file_path).expect("rescan");
        assert_eq!(
            DESKTOP_FILE_CHUNK_COMPLETENESS_CHECKS.load(Ordering::Relaxed),
            0,
            "verified materialized file rescan must not re-check every chunk id"
        );

        let conn = Connection::open(store::rxdb_store_path(root.path())).expect("reopen sqlite");
        let file_after = read_desktop_file_row(&conn, &file_id);
        let chunks_after = read_desktop_file_chunks(&conn, &file_id, true);
        assert_eq!(
            file_after.get("content_state").and_then(Value::as_str),
            Some("available"),
            "rescan must not demote a materialized file back to lazy"
        );
        assert_eq!(
            file_before, file_after,
            "rescan of an unchanged materialized file must be a no-op"
        );
        assert_eq!(
            chunks_before, chunks_after,
            "rescan must not rotate or strand materialized chunks"
        );
    }

    fn safe_business_os_tempdir(prefix: &str) -> tempfile::TempDir {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp/business-os-rxdb-peer");
        fs::create_dir_all(&base).expect("create safe test temp root");
        tempfile::Builder::new()
            .prefix(prefix)
            .tempdir_in(base)
            .expect("safe business os tempdir")
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

    fn read_desktop_file_rows(conn: &Connection) -> Vec<(String, String)> {
        let mut rows = conn
            .prepare("SELECT id, data FROM ctox_business_os__desktop_files__v0 ORDER BY id")
            .expect("desktop files query");
        rows.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("desktop file rows")
        .map(|row| row.expect("desktop file row"))
        .collect()
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
        let resynced = sync_ticket_state(root.path()).expect("resync ticket projections");
        assert_eq!(resynced, 0);

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
    fn sync_ticket_state_idle_gate_skips_unchanged_source() {
        let root = tempfile::tempdir().expect("temp root");
        crate::mission::ticket_local_native::create_local_ticket(
            root.path(),
            "Business OS ticket idle gate",
            "Ticket source should only project after changes.",
            Some("open"),
            Some("normal"),
        )
        .expect("create local ticket");
        tickets::sync_ticket_system(root.path(), "local").expect("sync local tickets");

        let mut last_source_stamp = None;
        let first = sync_ticket_state_if_changed(root.path(), &mut last_source_stamp)
            .expect("first ticket source sync");
        assert!(first >= 2, "expected item and event projection");

        let unchanged_stamp = last_source_stamp.clone();
        let second = sync_ticket_state_if_changed(root.path(), &mut last_source_stamp)
            .expect("unchanged ticket source sync");
        assert_eq!(second, 0);
        assert_eq!(last_source_stamp, unchanged_stamp);

        crate::mission::ticket_local_native::create_local_ticket(
            root.path(),
            "Business OS ticket idle gate change",
            "A second ticket should refresh the projection.",
            Some("open"),
            Some("normal"),
        )
        .expect("create changed local ticket");
        tickets::sync_ticket_system(root.path(), "local").expect("sync changed local tickets");

        let third = sync_ticket_state_if_changed(root.path(), &mut last_source_stamp)
            .expect("changed ticket source sync");
        assert!(third >= 2, "expected changed ticket projection");

        let fourth = sync_ticket_state_if_changed(root.path(), &mut last_source_stamp)
            .expect("unchanged ticket source resync");
        assert_eq!(fourth, 0);
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
            let (capability_token, _) = store::issue_business_os_capability_token_for_managed_user(
                root.path(),
                "tester",
                "Tester",
                "owner",
                now_ms() as i64,
            )
            .expect("issue test capability token");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            let pending_command = json!({
                "id": "cmd_native_consumer",
                "command_id": "cmd_native_consumer",
                "contract_version": 2,
                "idempotency_key": "cmd_native_consumer",
                "payload_hash": "sha256:native-consumer-fixture",
                "module": "ctox",
                "command_type": "business_os.test",
                "record_id": "",
                "status": "pending_sync",
                "replication_phase": "pushed",
                "projection_version": 0,
                "attempt": 0,
                "created_at_ms": now_ms() as u64,
                "inbound_channel": "ctox",
                "payload": { "title": "Native consumer test", "instruction": "test only" },
                "client_context": {
                    "actor": { "id": "tester", "display_name": "Tester" },
                    "capability_token": capability_token,
                },
                "updated_at_ms": now_ms() as u64
            });
            commands
                .insert(pending_command.clone())
                .await
                .expect("insert pending command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
            assert_eq!(
                accepted.get("replication_phase").and_then(Value::as_str),
                Some("native_observed")
            );
            assert_eq!(
                accepted.get("execution_mode").and_then(Value::as_str),
                Some("queue")
            );
            assert_eq!(
                accepted.get("execution_phase").and_then(Value::as_str),
                Some("queued")
            );
            assert_eq!(
                accepted.get("terminal_status").and_then(Value::as_str),
                Some("none")
            );
            assert_eq!(
                accepted.get("payload_hash").and_then(Value::as_str),
                Some("sha256:native-consumer-fixture")
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
            assert_eq!(
                duplicate.get("payload_hash").and_then(Value::as_str),
                Some("sha256:native-consumer-fixture")
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
    fn native_lifecycle_v2_separates_control_target_from_execution_task() {
        let mut document = json!({
            "id": "cmd-branding-v2",
            "command_id": "cmd-branding-v2",
            "contract_version": 2,
            "idempotency_key": "cmd-branding-v2",
            "payload_hash": "sha256:branding-fixture",
            "module": "ctox",
            "command_type": "ctox.business_os.branding.update",
            "record_id": "",
            "payload": {"title": "CTOX"},
            "client_context": {},
            "created_at_ms": 1,
            "replication_phase": "pushed",
            "projection_version": 0,
            "attempt": 0
        });
        let accepted = json!({
            "command_id": "cmd-branding-v2",
            "status": "completed",
            "execution_mode": "control",
            "execution_task_id": "",
            "target_task_id": "",
            "target_record_id": "workspace-branding",
            "task_id": "workspace-branding",
            "task_status": "completed"
        });

        enrich_native_command_lifecycle(&mut document, &accepted)
            .expect("enrich control lifecycle");

        assert_eq!(
            document.get("execution_mode").and_then(Value::as_str),
            Some("control")
        );
        assert_eq!(
            document.get("execution_task_id").and_then(Value::as_str),
            Some("")
        );
        assert_eq!(
            document.get("target_record_id").and_then(Value::as_str),
            Some("workspace-branding")
        );
        assert_eq!(
            document.get("execution_phase").and_then(Value::as_str),
            Some("terminal")
        );
        assert_eq!(
            document.get("terminal_status").and_then(Value::as_str),
            Some("completed")
        );
    }

    #[test]
    fn native_peer_replay_preserves_completed_chat_reply_fields() {
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
            let (capability_token, _) = store::issue_business_os_capability_token_for_managed_user(
                root.path(),
                "tester",
                "Tester",
                "owner",
                now_ms() as i64,
            )
            .expect("issue test capability token");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            let pending_command = json!({
                "id": "cmd_native_chat_reply_replay",
                "command_id": "cmd_native_chat_reply_replay",
                "module": "ctox",
                "command_type": "business_os.chat.task",
                "record_id": "ctox",
                "status": "pending_sync",
                "inbound_channel": "business_os_chat",
                "payload": {
                    "title": "CTOX",
                    "chat_id": "chat_native_reply_replay",
                    "message_id": "chatmsg_native_reply_replay",
                    "instruction": "zeige eine sichtbare Antwort",
                    "prompt": "zeige eine sichtbare Antwort"
                },
                "client_context": {
                    "source": "business-os-chat",
                    "module": "ctox",
                    "owner_user_id": "tester",
                    "actor": { "id": "tester", "display_name": "Tester" },
                    "capability_token": capability_token,
                },
                "updated_at_ms": now_ms() as u64
            });
            commands
                .insert(pending_command.clone())
                .await
                .expect("insert pending chat command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
                .await
                .expect("consume pending chat command");
            let accepted = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_native_chat_reply_replay" } })),
                    ..Default::default()
                }))
                .expect("accepted query")
                .exec(false)
                .await
                .expect("accepted document");
            let task_id = accepted
                .get("task_id")
                .and_then(Value::as_str)
                .expect("task_id")
                .to_string();
            assert!(!task_id.is_empty());

            channels::lease_queue_task(root.path(), &task_id, "ctox-service")
                .expect("lease chat command queue task");
            let reply_text = "Sichtbare CTOX Antwort bleibt erhalten.";
            channels::transition_business_command_for_task(
                root.path(),
                &task_id,
                "leased",
                None,
                None,
                None,
                "test worker leased chat command",
            )
            .expect("lease chat command");
            channels::transition_business_command_for_task(
                root.path(),
                &task_id,
                "running",
                None,
                None,
                None,
                "test worker started chat command",
            )
            .expect("start chat command");
            channels::persist_business_command_worker_result(root.path(), &task_id, reply_text)
                .expect("persist typed chat result");
            channels::record_business_command_review(
                root.path(),
                &task_id,
                "passed",
                "passed",
                &json!({"review": "PASS", "validation": "PASS"}),
            )
            .expect("persist chat review and validation");
            let completed = store::complete_business_command_from_queue_reply(
                root.path(),
                &task_id,
                reply_text,
            )
            .expect("complete business chat command")
            .expect("business chat command completion result");
            assert_eq!(
                completed.get("response").and_then(Value::as_str),
                Some(reply_text)
            );
            assert_eq!(
                completed
                    .pointer("/result/outbound_text")
                    .and_then(Value::as_str),
                Some(reply_text)
            );

            let mut replayed_pending = pending_command.clone();
            if let Some(obj) = replayed_pending.as_object_mut() {
                obj.insert(
                    "status".to_string(),
                    Value::String("pending_sync".to_string()),
                );
                obj.remove("task_id");
                obj.remove("task_status");
                obj.remove("result");
                obj.remove("outbound_text");
                obj.remove("response");
                obj.remove("answer");
                obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
            }
            commands
                .incremental_upsert(replayed_pending.clone())
                .await
                .expect("replay stale pending chat command");

            accept_pending_business_command(root.path(), &database, replayed_pending)
                .await
                .expect("accept replayed pending chat command");
            let replayed = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_native_chat_reply_replay" } })),
                    ..Default::default()
                }))
                .expect("replayed query")
                .exec(false)
                .await
                .expect("replayed document");
            assert_eq!(
                replayed.get("status").and_then(Value::as_str),
                Some("completed")
            );
            assert_eq!(
                replayed.get("task_status").and_then(Value::as_str),
                Some("completed")
            );
            assert_eq!(
                replayed.get("outbound_text").and_then(Value::as_str),
                Some(reply_text)
            );
            assert_eq!(
                replayed.get("response").and_then(Value::as_str),
                Some(reply_text)
            );
            assert_eq!(
                replayed.get("answer").and_then(Value::as_str),
                Some(reply_text)
            );
            assert_eq!(
                replayed
                    .pointer("/result/outbound_text")
                    .and_then(Value::as_str),
                Some(reply_text)
            );

            let chats = database
                .collection("business_chats")
                .expect("business_chats collection");
            let chat = chats
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "chat_native_reply_replay" } })),
                    ..Default::default()
                }))
                .expect("chat query")
                .exec(false)
                .await
                .expect("chat document");
            let messages = chat
                .get("messages")
                .and_then(Value::as_array)
                .expect("chat messages");
            assert!(
                messages
                    .iter()
                    .any(|message| message.get("text").and_then(Value::as_str) == Some(reply_text)),
                "missing visible chat reply in {messages:?}"
            );
        });
    }

    #[test]
    fn native_peer_consumes_pending_business_command_written_directly_to_sqlite() {
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
            let (capability_token, _) = store::issue_business_os_capability_token_for_managed_user(
                root.path(),
                "tester",
                "Tester",
                "owner",
                now_ms() as i64,
            )
            .expect("issue test capability token");
            let command = json!({
                "id": "cmd_native_consumer_sqlite",
                "command_id": "cmd_native_consumer_sqlite",
                "module": "ctox",
                "command_type": "business_os.test",
                "record_id": "",
                "status": "pending_sync",
                "inbound_channel": "ctox",
                "payload": { "title": "Native consumer SQLite test", "instruction": "test only" },
                "client_context": {
                    "actor": { "id": "tester", "display_name": "Tester" },
                    "capability_token": capability_token,
                },
                "updated_at_ms": now_ms() as u64
            });

            {
                let path = store::rxdb_store_path(root.path());
                let conn = Connection::open(&path).expect("open rxdb sqlite directly");
                let table = latest_rxdb_collection_table(&conn, "business_commands")
                    .expect("lookup business_commands table")
                    .expect("business_commands table");
                conn.execute(
                    &format!(
                        "INSERT INTO {} (id, revision, deleted, lastWriteTime, data) VALUES (?1, ?2, 0, ?3, ?4)",
                        sqlite_quote_identifier(&table)
                    ),
                    params![
                        "cmd_native_consumer_sqlite",
                        "1-sqlite",
                        now_ms() as f64,
                        command.to_string()
                    ],
                )
                .expect("insert pending command directly into sqlite");
            }

            let pending = pending_business_command_documents_sync(root.path(), 25)
                .expect("load direct sqlite pending command");
            assert!(
                pending
                    .iter()
                    .any(|doc| doc.get("id").and_then(Value::as_str)
                        == Some("cmd_native_consumer_sqlite")),
                "direct sqlite command not found: {pending:?}"
            );

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
                .await
                .expect("consume direct sqlite pending command");

            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            let accepted = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_native_consumer_sqlite" } })),
                    ..Default::default()
                }))
                .expect("accepted query")
                .exec(false)
                .await
                .expect("accepted document");
            assert_eq!(
                accepted.get("status").and_then(Value::as_str),
                Some("accepted"),
                "accepted={accepted}"
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
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": task_id } })),
                    ..Default::default()
                }))
                .expect("task query")
                .exec(false)
                .await
                .expect("task document");
            assert_eq!(
                task_doc.get("command_id").and_then(Value::as_str),
                Some("cmd_native_consumer_sqlite"),
                "task_doc={task_doc}"
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
            let capability_token = issue_test_capability(root.path(), "tester", "admin");
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
                        },
                        "capability_token": capability_token
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert pending ticket command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
    fn native_peer_consumes_support_note_command_and_projects_support_records() {
        let root = tempfile::tempdir().expect("temp root");
        {
            let conn = store::open_store(root.path()).expect("open business store");
            conn.execute(
                "INSERT INTO business_users
                    (user_id, display_name, role, active, created_at_ms, updated_at_ms)
                 VALUES ('tester', 'Tester', 'admin', 1, ?1, ?1)",
                params![now_ms() as i64],
            )
            .expect("seed admin user");
        }
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
            let capability_token = issue_test_capability(root.path(), "tester", "admin");
            let commands = database
                .collection("business_commands")
                .expect("business_commands collection");
            commands
                .insert(json!({
                    "id": "cmd_support_note_create",
                    "command_id": "cmd_support_note_create",
                    "module": "support",
                    "command_type": "support.note.create",
                    "record_id": "conv-support-1",
                    "status": "pending_sync",
                    "inbound_channel": "support",
                    "payload": {
                        "conversation_id": "conv-support-1",
                        "thread_key": "mail:thread-support-1",
                        "body": "Customer asked for a printengine update.",
                        "visibility": "internal"
                    },
                    "client_context": {
                        "actor": {
                            "id": "tester",
                            "display_name": "Tester",
                            "role": "admin",
                            "is_admin": true
                        },
                        "capability_token": capability_token
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert pending support command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
                .await
                .expect("consume pending support command");

            let accepted = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_support_note_create" } })),
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

            let notes = database
                .collection("support_notes")
                .expect("support_notes collection");
            let note_docs = notes
                .find(None)
                .expect("support notes query")
                .exec(false)
                .await
                .expect("support note documents");
            let note_docs = note_docs.as_array().expect("support note documents array");
            assert!(
                note_docs.iter().any(|doc| {
                    doc.get("conversation_id").and_then(Value::as_str) == Some("conv-support-1")
                        && doc.get("body").and_then(Value::as_str)
                            == Some("Customer asked for a printengine update.")
                }),
                "missing projected support note: {note_docs:?}"
            );

            let conversations = database
                .collection("support_conversations")
                .expect("support_conversations collection");
            let conversation_docs = conversations
                .find(None)
                .expect("support conversation query")
                .exec(false)
                .await
                .expect("support conversation documents");
            let conversation_docs = conversation_docs
                .as_array()
                .expect("support conversation documents array");
            assert!(
                conversation_docs.iter().any(|doc| {
                    doc.get("id").and_then(Value::as_str) == Some("conv-support-1")
                        && doc.get("status").and_then(Value::as_str) == Some("open")
                }),
                "missing projected support conversation: {conversation_docs:?}"
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
            let capability_token = issue_test_capability(root.path(), "tester", "admin");
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
                            },
                            "capability_token": capability_token.clone()
                        },
                        "updated_at_ms": now_ms() as u64
                    }))
                    .await
                    .expect("insert invalid ticket command");
            }

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
            let capability_token = issue_test_capability(root.path(), "chef", "chef");
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
                    "client_context": {
                        "actor": { "id": "chef", "display_name": "Chef" },
                        "capability_token": capability_token.clone()
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert seed command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
                        },
                        "capability_token": capability_token
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert task update command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
            let capability_token = issue_test_capability(root.path(), "business-user", "user");
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
                        "capability_token": capability_token,
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert report command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
            let capability_token = issue_test_capability(root.path(), "chef", "chef");
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
                    "client_context": {
                        "actor": { "id": "chef", "display_name": "Chef" },
                        "capability_token": capability_token.clone(),
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert source load command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
                        "capability_token": capability_token,
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert source save command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
        let admin_token = issue_test_capability(root.path(), "admin", "admin");
        let module_root = root.path().join("src/apps/business-os/modules/gov-demo");
        std::fs::create_dir_all(&module_root).expect("create module root");
        std::fs::write(root.path().join("src/apps/business-os/index.html"), "")
            .expect("write app index");
        let validator = root
            .path()
            .join("src/apps/business-os/scripts/validate-app-module.mjs");
        std::fs::create_dir_all(validator.parent().expect("validator parent"))
            .expect("create validator directory");
        std::fs::write(&validator, "process.exit(0);\n").expect("write validator stub");
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
                    "id": "cmd_admin_owner_upsert_denied",
                    "command_id": "cmd_admin_owner_upsert_denied",
                    "module": "ctox",
                    "command_type": "ctox.business_os.user.upsert",
                    "record_id": "chef",
                    "status": "pending_sync",
                    "inbound_channel": "ctox",
                    "payload": {
                        "id": "chef",
                        "display_name": "Chef",
                        "role": "chef",
                        "active": true
                    },
                    "client_context": {
                        "capability_token": admin_token,
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
                .expect("insert denied owner-upsert command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
                .await
                .expect("consume denied owner-upsert command");

            let denied_owner_command = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_admin_owner_upsert_denied" } })),
                    ..Default::default()
                }))
                .expect("denied owner-upsert command query")
                .exec(false)
                .await
                .expect("denied owner-upsert command document");
            assert_eq!(
                denied_owner_command.get("status").and_then(Value::as_str),
                Some("failed"),
                "denied_owner_command={denied_owner_command}"
            );
            assert_eq!(
                denied_owner_command
                    .pointer("/result/policy_decision/permission")
                    .and_then(Value::as_str),
                Some("workspace.manage"),
                "denied_owner_command={denied_owner_command}"
            );

            let owner_conn = store::open_store(root.path()).expect("open business store");
            let owner_now = now_ms() as i64;
            owner_conn
                .execute(
                    "INSERT INTO business_users
                        (user_id, display_name, role, active, created_at_ms, updated_at_ms)
                     VALUES ('workspace-owner', 'Workspace Owner', 'chef', 1, ?1, ?1)",
                    rusqlite::params![owner_now],
                )
                .expect("seed workspace owner");
            drop(owner_conn);
            let workspace_owner_token =
                issue_test_capability(root.path(), "workspace-owner", "chef");

            commands
                .insert(json!({
                    "id": "cmd_user_upsert",
                    "command_id": "cmd_user_upsert",
                    "module": "ctox",
                    "command_type": "ctox.business_os.user.upsert",
                    "record_id": "chef",
                    "status": "pending_sync",
                    "inbound_channel": "ctox",
                    "payload": {
                        "id": "chef",
                        "display_name": "Chef",
                        "role": "chef",
                        "active": true
                    },
                    "client_context": {
                        "capability_token": workspace_owner_token,
                        "actor": {
                            "id": "workspace-owner",
                            "display_name": "Workspace Owner",
                            "is_admin": true
                        },
                        "source": "test"
                    },
                    "updated_at_ms": now_ms() as u64
                }))
                .await
                .expect("insert owner user command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
                .await
                .expect("consume owner user command");

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
            let has_chef = user_command
                .get("result")
                .and_then(|result| result.get("users"))
                .and_then(Value::as_array)
                .map(|users| {
                    users.iter().any(|user| {
                        user.get("id").and_then(Value::as_str) == Some("chef")
                            && user.get("role").and_then(Value::as_str) == Some("chef")
                    })
                })
                .unwrap_or(false);
            assert!(has_chef, "user_command={user_command}");
            let chef_token = issue_test_capability(root.path(), "chef", "chef");

            commands
                .insert(json!({
                    "id": "cmd_module_owner_upsert",
                    "command_id": "cmd_module_owner_upsert",
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
                        "capability_token": chef_token.clone(),
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
                .expect("insert module owner command");

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
                .await
                .expect("consume module owner command");

            let owner_command = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_module_owner_upsert" } })),
                    ..Default::default()
                }))
                .expect("module owner command query")
                .exec(false)
                .await
                .expect("module owner command document");
            assert_eq!(
                owner_command.get("status").and_then(Value::as_str),
                Some("completed"),
                "owner_command={owner_command}"
            );
            let has_module_owner = owner_command
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
            assert!(has_module_owner, "owner_command={owner_command}");

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
                        "context": "256k",
                        "max_run_secs": 120
                    },
                    "client_context": {
                        "capability_token": chef_token.clone(),
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

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
                        "capability_token": chef_token.clone(),
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

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
                        "capability_token": chef_token.clone(),
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

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
                        "capability_token": chef_token.clone(),
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
            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
                .await
                .expect("consume module save command");
            let failed_module_save = commands
                .find_one(Some(MangoQuery {
                    selector: Some(json!({ "id": { "$eq": "cmd_gov_module_save" } })),
                    ..Default::default()
                }))
                .expect("module save command query")
                .exec(false)
                .await
                .expect("module save command document");
            assert_eq!(
                failed_module_save.get("status").and_then(Value::as_str),
                Some("failed"),
                "failed_module_save={failed_module_save}"
            );
            assert!(
                failed_module_save.to_string().contains("does not exist"),
                "failed_module_save={failed_module_save}"
            );
            assert!(
                !root
                    .path()
                    .join("runtime/business-os/installed-modules/new-module/module.json")
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
                        "capability_token": chef_token.clone(),
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
            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
                .await
                .expect("consume template install command");
            let installed_template_manifest = std::fs::read_to_string(
                root.path()
                    .join("runtime/business-os/installed-modules/installed-from-template/module.json"),
            )
            .expect("read installed template manifest");
            assert!(installed_template_manifest.contains("Installed From Template"));

            commands
                .insert(json!({
                    "id": "cmd_gov_module_delete",
                    "command_id": "cmd_gov_module_delete",
                    "module": "ctox",
                    "command_type": "ctox.module.delete",
                    "record_id": "installed-from-template",
                    "status": "pending_sync",
                    "inbound_channel": "installed-from-template",
                    "payload": { "module_id": "installed-from-template" },
                    "client_context": {
                        "capability_token": chef_token.clone(),
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
            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
                .await
                .expect("consume module delete command");
            assert!(
                !root
                    .path()
                    .join("runtime/business-os/installed-modules/installed-from-template")
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
                            "capability_token": chef_token.clone(),
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

                consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
                        "capability_token": chef_token,
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

            consume_pending_business_commands(root.path(), &database, &mut HashMap::new())
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
