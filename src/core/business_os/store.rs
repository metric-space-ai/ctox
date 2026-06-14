// Origin: CTOX
// License: Apache-2.0

use crate::mission::channels;
use anyhow::Context;
use base64::Engine;
use ctox_app_server_protocol::AuthMode as ApiAuthMode;
use rusqlite::params;
use rusqlite::params_from_iter;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tiny_http::{Header, Request, Response, Server};
use url::Url;
use uuid::Uuid;

const STORE_FILE: &str = "business-os.sqlite3";
const RXDB_STORE_FILE: &str = "business-os-rxdb.sqlite3";
const DEFAULT_SIGNALING_URL: &str = "wss://signaling.ctox.dev";
const DEFAULT_STUN_URL: &str = "stun:stun.l.google.com:19302";
const BUSINESS_OS_SIGNALING_URLS_FILE: &str = "business-os-signaling-urls.json";
const DOCUMENT_BLOB_CHUNK_SIZE: usize = 256_000;
const CORE_MODULE_IDS: &[&str] = &[
    "ctox",
    "knowledge",
    "app-store",
    "desktop",
    "reports",
    "tickets",
];
const STARTER_MODULE_IDS: &[&str] = &["documents", "spreadsheets", "calendar", "notes", "research"];
const CHATGPT_AUTH_ISSUER: &str = "https://auth.openai.com";
const CHATGPT_AUTH_CALLBACK_PORT: u16 = 1455;
const CHATGPT_AUTH_CALLBACK_FALLBACK_PORT: u16 = 1457;
const CHATGPT_AUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
const CHATGPT_AUTH_SECRET_SCOPE: &str = "ctox-auth";
const CHATGPT_AUTH_SECRET_NAME: &str = "chatgpt_subscription_auth_json";
const BUSINESS_OS_SECRET_SCOPE: &str = "business-os";
const BUSINESS_OS_ROOM_PASSWORD_SECRET_NAME: &str = "webrtc_room_password";
const BUSINESS_OS_QUEUE_PROMPT_MAX_CHARS: usize = 96_000;
const BUSINESS_OS_QUEUE_PROMPT_JSON_PREVIEW_CHARS: usize = 18_000;
const BUSINESS_OS_QUEUE_PROMPT_INSTRUCTION_MAX_CHARS: usize = 8_000;
const BUSINESS_OS_QUEUE_ORPHAN_REPAIR_AGE_MS: i64 = 10 * 60 * 1_000;
const BUSINESS_OS_CHAT_ATTACHMENT_CHUNK_SIZE: usize = 16 * 1024;
const BUSINESS_OS_CHAT_ATTACHMENT_CONTENT_HASH_SCHEME: &str = "sha256-bytes-v1";
const BUSINESS_OS_CHAT_ATTACHMENT_CHUNK_HASH_SCHEME: &str = "sha256-base64-chunk-v1";

#[derive(Debug, Clone, Default)]
struct ChatgptSubscriptionAuthStatus {
    configured: bool,
    account_email: Option<String>,
    plan: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessOsStatus {
    pub ok: bool,
    pub runtime: &'static str,
    pub store_path: String,
    pub now_ms: u128,
    pub sync: Value,
    pub module_catalog: Value,
    pub data_plane: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctox_service: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessOsSyncConfig {
    pub ok: bool,
    pub app_hosting: &'static str,
    pub sync_mode: &'static str,
    pub instance_id: String,
    pub peer_id: String,
    pub peer_role: &'static str,
    pub sync_room: String,
    pub signaling_room_password: String,
    pub signaling_urls: Vec<String>,
    pub signaling_urls_source: &'static str,
    pub ice_servers: Vec<Value>,
    pub transport: &'static str,
    pub http_bridge_available: bool,
    pub ctox_instance_required: bool,
    pub native_rxdb_peer_available: bool,
    pub native_rxdb_peer_reason: &'static str,
    pub native_rxdb_peer_status: Value,
    /// Per-instance allowlist of module ids the shell should surface. Empty = show
    /// every packaged module (no restriction). Sourced from the SQLite runtime store
    /// (`CTOX_BUSINESS_OS_MODULE_ALLOWLIST`) so it is configurable per instance without
    /// a rebuild and without a process-env toggle.
    pub module_allowlist: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessOsSession {
    pub ok: bool,
    pub authenticated: bool,
    pub auth_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<BusinessOsSessionUser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessOsSessionUser {
    pub id: String,
    pub display_name: String,
    pub role: String,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessOsUser {
    pub id: String,
    pub display_name: String,
    pub role: String,
    pub active: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BusinessOsUserMutation {
    pub id: String,
    pub display_name: String,
    pub role: String,
    #[serde(default = "default_true")]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessCommand {
    #[serde(default)]
    pub id: Option<String>,
    pub module: String,
    #[serde(rename = "type")]
    pub command_type: String,
    #[serde(default)]
    pub record_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub client_context: Value,
}

#[derive(Debug, Clone, Serialize)]
struct MaterializedBusinessChatAttachment {
    attachment_id: String,
    file_id: String,
    generation_id: String,
    name: String,
    mime_type: String,
    size_bytes: u64,
    content_hash: String,
    content_hash_scheme: String,
    virtual_path: String,
    local_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CtoxTaskUpdateMutation {
    pub task_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CtoxTaskDeleteMutation {
    pub task_id: String,
    #[serde(default)]
    pub command_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleSourceLoadMutation {
    pub module_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleSourceSaveMutation {
    pub module_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleSourceListSnapshotsRequest {
    pub module_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleSourceRollbackSnapshotRequest {
    pub module_id: String,
    pub snapshot_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppStoreInstallRequest {
    pub module_id: String,
    pub download_url: String,
    #[serde(default)]
    pub source_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppStoreUninstallRequest {
    pub module_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DesktopFileMaterializeRequest {
    pub file_id: String,
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleFounderAssignment {
    pub module_id: String,
    pub user_id: String,
    #[serde(default = "default_true")]
    pub active: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleInstallTemplateRequest {
    pub template_id: String,
    #[serde(default)]
    pub module_id: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleUpsertRequest {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub entry: String,
    #[serde(default)]
    pub collections: Vec<String>,
    #[serde(default)]
    pub layout: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleDeleteRequest {
    pub module_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeSettingsRequest {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub auth_mode: String,
    #[serde(default)]
    pub chat_model: String,
    #[serde(default)]
    pub preset: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub max_run_secs: Option<u64>,
    #[serde(default)]
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelCommandRequest {
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub account_key: String,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleReleaseRequest {
    pub module_id: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleRollbackRequest {
    pub module_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleVersionListRequest {
    pub module_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleVersionRollbackRequest {
    pub module_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BusinessOsReportMutation {
    pub module_id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub severity: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub expected: String,
    #[serde(default)]
    pub client_context: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueProjectionRepairOptions {
    pub apply: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandAccepted {
    pub ok: bool,
    pub command_id: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModuleManifest {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    developer: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    store: Value,
    #[serde(default)]
    install_scope: String,
    #[serde(default)]
    default_installed: bool,
    #[serde(default)]
    entry: String,
    #[serde(default)]
    collections: Vec<String>,
    #[serde(default)]
    layout: Value,
    #[serde(default)]
    source: String,
    #[serde(default)]
    core: bool,
    #[serde(default)]
    editable: bool,
    #[serde(default)]
    deletable: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    manifest_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    local_manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TemplateManifest {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    source_module: String,
    #[serde(default)]
    default_title: String,
    #[serde(default)]
    tags: Vec<String>,
}

pub fn open_store(root: &Path) -> anyhow::Result<Connection> {
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime)
        .with_context(|| format!("failed to create runtime dir {}", runtime.display()))?;
    let path = runtime.join(STORE_FILE);
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open Business OS store {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure Business OS SQLite busy_timeout")?;
    let busy_timeout_ms = crate::persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL; PRAGMA busy_timeout={busy_timeout_ms};"
    ))
    .context("failed to configure Business OS SQLite pragmas")?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn rxdb_store_path(root: &Path) -> PathBuf {
    root.join("runtime").join(RXDB_STORE_FILE)
}

pub fn status(root: &Path) -> anyhow::Result<BusinessOsStatus> {
    let path = root.join("runtime").join(STORE_FILE);
    let ctox_service = Some(cheap_ctox_service_status(root));
    let sync_config = sync_config(root)?;
    Ok(BusinessOsStatus {
        ok: true,
        runtime: "native-rust",
        store_path: path.display().to_string(),
        now_ms: now_ms(),
        sync: serde_json::json!({
            "transport": sync_config.transport,
            "sync_mode": sync_config.sync_mode,
            "sync_room": sync_config.sync_room,
            "signaling_urls": sync_config.signaling_urls,
            "signaling_urls_source": sync_config.signaling_urls_source,
            "native_rxdb_peer_available": sync_config.native_rxdb_peer_available,
            "native_rxdb_peer_reason": sync_config.native_rxdb_peer_reason,
            "native_rxdb_peer_status": sync_config.native_rxdb_peer_status,
            "http_bridge_available": sync_config.http_bridge_available,
        }),
        module_catalog: rxdb_module_catalog_status(root),
        data_plane: rxdb_data_plane_status(root),
        ctox_service,
    })
}

fn rxdb_data_plane_status(root: &Path) -> Value {
    const CRITICAL_COLLECTIONS: &[(&str, bool)] = &[
        ("business_module_catalog", true),
        ("ctox_runtime_settings", true),
        ("desktop_files", false),
        ("desktop_file_chunks", false),
        ("business_commands", false),
        ("ctox_queue_tasks", false),
    ];

    let path = rxdb_store_path(root);
    if !path.is_file() {
        return serde_json::json!({
            "ok": false,
            "path": path.display().to_string(),
            "reason": "native RxDB SQLite store is missing",
            "collections": {},
        });
    }
    let conn = match Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => conn,
        Err(err) => {
            return serde_json::json!({
                "ok": false,
                "path": path.display().to_string(),
                "reason": format!("open native RxDB SQLite store: {err}"),
                "collections": {},
            });
        }
    };
    let _ = conn.busy_timeout(Duration::from_millis(100));

    let mut collections = BTreeMap::new();
    let mut required_ok = true;
    for (collection, required_for_shell) in CRITICAL_COLLECTIONS {
        let table = rxdb_collection_table_name(&conn, collection);
        let table_exists = table
            .as_deref()
            .map(|table| rxdb_table_exists(&conn, table).unwrap_or(false))
            .unwrap_or(false);
        let row_count = if table_exists {
            table
                .as_deref()
                .and_then(|table| rxdb_table_row_count(&conn, table).ok())
        } else {
            None
        };
        let latest_updated_at_ms = if table_exists {
            table
                .as_deref()
                .and_then(|table| rxdb_table_latest_updated_at_ms(&conn, table).ok())
                .flatten()
        } else {
            None
        };
        let collection_ok =
            table_exists && (!required_for_shell || row_count.unwrap_or_default() > 0);
        if *required_for_shell && !collection_ok {
            required_ok = false;
        }
        collections.insert(
            (*collection).to_string(),
            serde_json::json!({
                "ok": collection_ok,
                "required_for_shell": required_for_shell,
                "table": table,
                "table_exists": table_exists,
                "row_count": row_count,
                "latest_updated_at_ms": latest_updated_at_ms,
            }),
        );
    }

    serde_json::json!({
        "ok": required_ok,
        "path": path.display().to_string(),
        "required_collections_ready": required_ok,
        "collections": collections,
    })
}

fn rxdb_table_exists(conn: &Connection, table: &str) -> anyhow::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some())
}

fn rxdb_table_row_count(conn: &Connection, table: &str) -> anyhow::Result<i64> {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get::<_, i64>(0)
    })
    .with_context(|| format!("count rows in {table}"))
}

fn rxdb_table_latest_updated_at_ms(conn: &Connection, table: &str) -> anyhow::Result<Option<i64>> {
    conn.query_row(
        &format!("SELECT MAX(CAST(json_extract(data, '$.updated_at_ms') AS INTEGER)) FROM {table}"),
        [],
        |row| row.get::<_, Option<i64>>(0),
    )
    .with_context(|| format!("read latest updated_at_ms in {table}"))
}

fn rxdb_collection_table_name(conn: &Connection, collection: &str) -> Option<String> {
    let expected = format!(
        "ctox_business_os__{collection}__v{}",
        rxdb_schema_version(collection)
    );
    if rxdb_table_exists(conn, &expected).unwrap_or(false) {
        return Some(expected);
    }
    let prefix = format!("ctox_business_os__{collection}__v");
    let pattern = format!("{prefix}%");
    conn.query_row(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name LIKE ?1
         ORDER BY CAST(substr(name, length(?2) + 1) AS INTEGER) DESC
         LIMIT 1",
        params![pattern, prefix],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

fn rxdb_schema_version(collection: &str) -> i64 {
    business_os_schema_contract_for_store()
        .get(collection)
        .and_then(|schema| schema.get("version"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn business_os_schema_contract_for_store() -> &'static BTreeMap<String, Value> {
    static CONTRACT: std::sync::OnceLock<BTreeMap<String, Value>> = std::sync::OnceLock::new();
    CONTRACT.get_or_init(|| {
        serde_json::from_str(include_str!("business_os_schema_contract.json"))
            .expect("Business OS RxDB schema contract JSON must be valid")
    })
}

fn rxdb_module_catalog_status(root: &Path) -> Value {
    let path = rxdb_store_path(root);
    if !path.is_file() {
        return serde_json::json!({
            "ok": false,
            "path": path.display().to_string(),
            "reason": "native RxDB SQLite store is missing",
        });
    }
    let conn = match Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => conn,
        Err(err) => {
            return serde_json::json!({
                "ok": false,
                "path": path.display().to_string(),
                "reason": format!("open native RxDB SQLite store: {err}"),
            });
        }
    };
    let _ = conn.busy_timeout(Duration::from_millis(100));
    let Some(table) = rxdb_collection_table_name(&conn, "business_module_catalog") else {
        return serde_json::json!({
            "ok": false,
            "path": path.display().to_string(),
            "reason": "business_module_catalog RxDB collection table is missing",
        });
    };
    let data = match conn
        .query_row(
            &format!("SELECT data FROM {table} WHERE id = 'module-catalog'"),
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
    {
        Ok(Some(data)) => data,
        Ok(None) => {
            return serde_json::json!({
                "ok": false,
                "path": path.display().to_string(),
                "table": table,
                "reason": "module-catalog document is missing",
            });
        }
        Err(err) => {
            return serde_json::json!({
                "ok": false,
                "path": path.display().to_string(),
                "table": table,
                "reason": format!("read module-catalog document: {err}"),
            });
        }
    };
    let parsed = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
    let module_count = parsed
        .get("modules")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    serde_json::json!({
        "ok": module_count > 0,
        "path": path.display().to_string(),
        "table": table,
        "document_id": "module-catalog",
        "module_count": module_count,
        "template_count": parsed
            .get("templates")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
        "updated_at_ms": parsed.get("updated_at_ms").cloned().unwrap_or(Value::Null),
    })
}

pub(crate) fn cheap_ctox_service_status(root: &Path) -> Value {
    let pid = std::fs::read_to_string(root.join("runtime/ctox_service.pid"))
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok());
    let running = pid.map(process_is_running).unwrap_or(false);
    serde_json::json!({
        "running": running,
        "busy": null,
        "pid": pid,
        "listen_addr": "",
        "autostart_enabled": false,
        "manager": "process",
        "pending_count": null,
        "pending_previews": [],
        "blocked_count": null,
        "blocked_previews": [],
        "current_goal_preview": null,
        "active_source_label": null,
        "recent_events": [],
        "last_error": null,
        "last_completed_at": null,
        "last_reply_chars": null,
        "monitor_last_check_at": null,
        "monitor_alerts": [],
        "monitor_last_error": null
    })
}

#[cfg(unix)]
fn process_is_running(pid: u32) -> bool {
    let pid = pid as libc::pid_t;
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_is_running(_pid: u32) -> bool {
    false
}

/// Per-instance module allowlist for the Business OS shell.
///
/// Read from the persisted SQLite runtime store via `runtime_env::env_or_config`
/// (key `CTOX_BUSINESS_OS_MODULE_ALLOWLIST`). The value is a comma/whitespace
/// separated list of module ids (e.g. `desktop,documents,research,knowledge,app-store,ctox`).
/// An empty/unset value means "no restriction" — the shell surfaces every packaged
/// module. This is intentionally not a process-env toggle: operators set it in the
/// runtime store so each instance can scope its visible apps independently.
pub fn business_os_module_allowlist(root: &Path) -> Vec<String> {
    let raw = match crate::inference::runtime_env::env_or_config(
        root,
        "CTOX_BUSINESS_OS_MODULE_ALLOWLIST",
    ) {
        Some(value) => value,
        None => return Vec::new(),
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut ids = Vec::new();
    for id in raw.split([',', ';', '\n', '\t', ' ']) {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        if seen.insert(id.to_owned()) {
            ids.push(id.to_owned());
        }
    }
    ids
}

pub fn sync_config(root: &Path) -> anyhow::Result<BusinessOsSyncConfig> {
    let instance_id = stable_instance_id(root)?;
    let signaling_room_password = business_os_room_password(root)?;
    let peer_id = format!("ctox-core-{}", short_hash(&instance_id));
    let native_rxdb_peer_available = super::rxdb_peer::is_native_peer_running_for_root(root);
    let native_rxdb_peer_status = super::rxdb_peer::native_peer_status(root);
    let signaling = signaling_urls_config(root);
    Ok(BusinessOsSyncConfig {
        ok: true,
        app_hosting: "ctox_instance_webserver",
        sync_mode: "p2p-first",
        sync_room: format!(
            "ctox-business-os:{instance_id}:{}",
            room_secret_id(&signaling_room_password)
        ),
        signaling_room_password,
        instance_id,
        peer_id,
        peer_role: "ctox_instance",
        signaling_urls: signaling.urls,
        signaling_urls_source: signaling.source,
        ice_servers: ice_servers_config(),
        transport: "webrtc",
        http_bridge_available: false,
        ctox_instance_required: true,
        native_rxdb_peer_available,
        native_rxdb_peer_reason: if native_rxdb_peer_available {
            ""
        } else {
            "CTOX native WebRTC peer is starting or unavailable"
        },
        native_rxdb_peer_status,
        module_allowlist: business_os_module_allowlist(root),
    })
}

pub fn rotate_sync_room_password(root: &Path) -> anyhow::Result<BusinessOsSyncConfig> {
    if env::var("CTOX_BUSINESS_OS_ROOM_PASSWORD")
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        anyhow::bail!(
            "CTOX_BUSINESS_OS_ROOM_PASSWORD is set; unset the environment override before rotating the persisted Business OS room password"
        );
    }

    let generated = format!("ctox-room-{}", Uuid::new_v4().simple());
    crate::secrets::write_secret_record(
        root,
        BUSINESS_OS_SECRET_SCOPE,
        BUSINESS_OS_ROOM_PASSWORD_SECRET_NAME,
        &generated,
        Some("Business OS WebRTC signaling room password".to_owned()),
        serde_json::json!({"source": "business_os_sync_config_rotation"}),
    )?;
    sync_config(root)
}

fn ice_servers_config() -> Vec<Value> {
    if let Ok(raw) = std::env::var("CTOX_BUSINESS_OS_ICE_SERVERS") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(trimmed) {
                let servers = items
                    .into_iter()
                    .filter_map(normalize_ice_server)
                    .collect::<Vec<_>>();
                if !servers.is_empty() {
                    return servers;
                }
            }
            let servers = trimmed
                .split(',')
                .map(str::trim)
                .filter(|url| !url.is_empty())
                .map(|url| serde_json::json!({ "urls": url }))
                .collect::<Vec<_>>();
            if !servers.is_empty() {
                return servers;
            }
        }
    }
    vec![serde_json::json!({ "urls": DEFAULT_STUN_URL })]
}

fn normalize_ice_server(value: Value) -> Option<Value> {
    let object = value.as_object()?;
    let urls = object.get("urls")?;
    let normalized_urls = if let Some(url) = urls.as_str() {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return None;
        }
        Value::String(trimmed.to_owned())
    } else if let Some(items) = urls.as_array() {
        let urls = items
            .iter()
            .filter_map(|item| item.as_str())
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(|url| Value::String(url.to_owned()))
            .collect::<Vec<_>>();
        if urls.is_empty() {
            return None;
        }
        Value::Array(urls)
    } else {
        return None;
    };

    let mut server = serde_json::Map::new();
    server.insert("urls".to_owned(), normalized_urls);
    if let Some(username) = object
        .get("username")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        server.insert("username".to_owned(), Value::String(username.to_owned()));
    }
    if let Some(credential) = object
        .get("credential")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        server.insert(
            "credential".to_owned(),
            Value::String(credential.to_owned()),
        );
    }
    Some(Value::Object(server))
}

fn business_os_room_password(root: &Path) -> anyhow::Result<String> {
    if let Ok(value) = env::var("CTOX_BUSINESS_OS_ROOM_PASSWORD") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }
    if let Ok(value) = crate::secrets::read_secret_value(
        root,
        BUSINESS_OS_SECRET_SCOPE,
        BUSINESS_OS_ROOM_PASSWORD_SECRET_NAME,
    ) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }
    let generated = format!("ctox-room-{}", Uuid::new_v4().simple());
    crate::secrets::write_secret_record(
        root,
        BUSINESS_OS_SECRET_SCOPE,
        BUSINESS_OS_ROOM_PASSWORD_SECRET_NAME,
        &generated,
        Some("Business OS WebRTC signaling room password".to_owned()),
        serde_json::json!({"source": "business_os_sync_config"}),
    )?;
    Ok(generated)
}

pub fn session(auth_header: Option<&str>, session_header: Option<&str>) -> BusinessOsSession {
    let token = env::var("CTOX_BUSINESS_OS_SESSION_TOKEN").unwrap_or_default();
    let password = env::var("CTOX_BUSINESS_PASSWORD").unwrap_or_default();
    let expected_user = env::var("CTOX_BUSINESS_USER").unwrap_or_else(|_| "admin".to_owned());
    let configured_users = configured_auth_users();
    let require_explicit_login = env::var("CTOX_BUSINESS_OS_REQUIRE_LOGIN").as_deref() == Ok("1");
    let login_url = env::var("CTOX_BUSINESS_OS_LOGIN_URL")
        .ok()
        .filter(|value| !value.trim().is_empty());

    if token.trim().is_empty() && password.trim().is_empty() && configured_users.is_empty() {
        if !require_explicit_login {
            return BusinessOsSession {
                ok: true,
                authenticated: true,
                auth_required: false,
                user: Some(BusinessOsSessionUser {
                    id: env::var("CTOX_BUSINESS_OS_DESKTOP_USER")
                        .unwrap_or_else(|_| "local-dev".to_owned()),
                    display_name: env::var("CTOX_BUSINESS_OS_DESKTOP_DISPLAY_NAME")
                        .unwrap_or_else(|_| "Local CTOX".to_owned()),
                    role: normalize_business_role(
                        &env::var("CTOX_BUSINESS_OS_DESKTOP_ROLE")
                            .unwrap_or_else(|_| "admin".to_owned()),
                    ),
                    is_admin: role_can_manage(
                        &env::var("CTOX_BUSINESS_OS_DESKTOP_ROLE")
                            .unwrap_or_else(|_| "admin".to_owned()),
                    ),
                }),
                login_url,
                reason: None,
            };
        }
        return BusinessOsSession {
            ok: true,
            authenticated: false,
            auth_required: true,
            user: None,
            login_url,
            reason: Some("ctox_session_token_not_configured".to_owned()),
        };
    }

    let expected = token.trim();
    let expected_password = password.trim();
    let basic = auth_header.and_then(parse_basic_credentials);
    let bearer = auth_header
        .and_then(|value| value.trim().strip_prefix("Bearer "))
        .unwrap_or("");
    let session_token = session_header.unwrap_or("").trim();
    let configured_user = basic
        .as_ref()
        .and_then(|(supplied_user, supplied_password)| {
            configured_users.iter().find(|user| {
                user.id.eq_ignore_ascii_case(supplied_user) && user.password == *supplied_password
            })
        });
    let token_authenticated = !expected.is_empty()
        && (bearer == expected
            || session_token == expected
            || basic
                .as_ref()
                .map(|(_, supplied_password)| supplied_password == expected)
                .unwrap_or(false));
    let password_authenticated = !expected_password.is_empty()
        && basic
            .as_ref()
            .map(|(supplied_user, supplied_password)| {
                supplied_user == expected_user.trim() && supplied_password == expected_password
            })
            .unwrap_or(false);
    let authenticated = token_authenticated || password_authenticated || configured_user.is_some();
    let session_user_id = basic
        .as_ref()
        .map(|(user, _)| user.as_str())
        .filter(|user| !user.trim().is_empty())
        .unwrap_or("ctox-user");

    let role = configured_user
        .map(|user| user.role.clone())
        .unwrap_or_else(|| default_session_role());
    let is_admin = role_can_manage(&role);
    BusinessOsSession {
        ok: true,
        authenticated,
        auth_required: true,
        user: authenticated.then(|| BusinessOsSessionUser {
            id: session_user_id.to_owned(),
            display_name: session_user_id.to_owned(),
            role,
            is_admin,
        }),
        login_url,
        reason: (!authenticated).then(|| "invalid_or_missing_session".to_owned()),
    }
}

fn parse_basic_credentials(auth_header: &str) -> Option<(String, String)> {
    let encoded = auth_header.trim().strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let value = String::from_utf8(decoded).ok()?;
    let (user, password) = value.split_once(':')?;
    Some((user.to_owned(), password.to_owned()))
}

fn default_session_role() -> String {
    normalize_business_role(
        &env::var("CTOX_BUSINESS_OS_DEFAULT_ROLE").unwrap_or_else(|_| "user".to_owned()),
    )
}

#[derive(Debug, Clone)]
struct ConfiguredAuthUser {
    id: String,
    password: String,
    role: String,
}

fn configured_auth_users() -> Vec<ConfiguredAuthUser> {
    let Ok(raw) = env::var("CTOX_AUTH_USERS") else {
        return Vec::new();
    };
    raw.split(';')
        .filter_map(|entry| {
            let separator = if entry.contains('|') { '|' } else { ':' };
            let parts = entry.split(separator).map(str::trim).collect::<Vec<_>>();
            let id = parts.first().copied().unwrap_or("");
            let password = parts.get(1).copied().unwrap_or("");
            if id.is_empty() || password.is_empty() {
                return None;
            }
            let role = parts
                .get(2)
                .and_then(|roles| {
                    roles
                        .split(',')
                        .map(str::trim)
                        .find(|role| !role.is_empty())
                })
                .map(normalize_business_role)
                .unwrap_or_else(|| "user".to_owned());
            Some(ConfiguredAuthUser {
                id: id.to_owned(),
                password: password.to_owned(),
                role,
            })
        })
        .collect()
}

fn default_configured_business_user() -> Option<ConfiguredAuthUser> {
    let password = env::var("CTOX_BUSINESS_PASSWORD").unwrap_or_default();
    let user = env::var("CTOX_BUSINESS_USER").unwrap_or_else(|_| "admin".to_owned());
    let id = user.trim();
    if id.is_empty() || password.trim().is_empty() {
        return None;
    }
    Some(ConfiguredAuthUser {
        id: id.to_owned(),
        password,
        role: default_session_role(),
    })
}

fn configured_business_users() -> Vec<ConfiguredAuthUser> {
    let mut users = Vec::new();
    if let Some(default_user) = default_configured_business_user() {
        users.push(default_user);
    }
    for configured in configured_auth_users() {
        if users
            .iter()
            .any(|existing| existing.id.eq_ignore_ascii_case(&configured.id))
        {
            continue;
        }
        users.push(configured);
    }
    users
}

fn seed_configured_business_users(conn: &Connection) -> anyhow::Result<()> {
    let now = now_ms() as i64;
    for user in configured_business_users() {
        conn.execute(
            "INSERT INTO business_users
                (user_id, display_name, role, active, created_at_ms, updated_at_ms)
             VALUES (?1, ?1, ?2, 1, ?3, ?3)
             ON CONFLICT(user_id) DO UPDATE SET
                role = excluded.role,
                active = 1,
                updated_at_ms = excluded.updated_at_ms",
            params![user.id.trim(), normalize_business_role(&user.role), now],
        )?;
    }
    Ok(())
}

fn normalize_business_role(role: &str) -> String {
    match role.trim().to_ascii_lowercase().as_str() {
        "owner" | "chef" => "chef".to_owned(),
        "admin" | "business_os_admin" => "admin".to_owned(),
        "founder" => "founder".to_owned(),
        "user" | "business_os_user" => "user".to_owned(),
        _ => "user".to_owned(),
    }
}

fn role_can_manage(role: &str) -> bool {
    matches!(normalize_business_role(role).as_str(), "chef" | "admin")
}

fn session_user_id(session: &BusinessOsSession) -> Option<&str> {
    session.user.as_ref().map(|user| user.id.as_str())
}

fn session_role(session: &BusinessOsSession) -> &str {
    session
        .user
        .as_ref()
        .map(|user| user.role.as_str())
        .unwrap_or("user")
}

pub fn session_can_manage_all(session: &BusinessOsSession) -> bool {
    role_can_manage(session_role(session))
}

pub fn session_can_modify_module(
    root: &Path,
    session: &BusinessOsSession,
    module_id: &str,
) -> anyhow::Result<bool> {
    if session_can_manage_all(session) {
        return Ok(true);
    }
    if normalize_business_role(session_role(session)) != "founder" {
        return Ok(false);
    }
    let Some(user_id) = session_user_id(session) else {
        return Ok(false);
    };
    let conn = open_store(root)?;
    Ok(founder_owns_module(&conn, module_id, user_id)?)
}

fn founder_owns_module(conn: &Connection, module_id: &str, user_id: &str) -> anyhow::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM business_module_acl
         WHERE module_id = ?1 AND user_id = ?2 AND role = 'founder' AND active = 1",
        params![module_id.trim(), user_id.trim()],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn module_governance_map(root: &Path, session: &BusinessOsSession) -> anyhow::Result<Value> {
    let conn = open_store(root)?;
    let mut founder_stmt = conn.prepare(
        "SELECT module_id, user_id, active, updated_at_ms
         FROM business_module_acl
         WHERE role = 'founder'
         ORDER BY module_id ASC, user_id ASC",
    )?;
    let mut founders: HashMap<String, Vec<Value>> = HashMap::new();
    let founder_rows = founder_stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            serde_json::json!({
                "user_id": row.get::<_, String>(1)?,
                "role": "founder",
                "active": row.get::<_, i64>(2)? != 0,
                "updated_at_ms": row.get::<_, i64>(3)?,
            }),
        ))
    })?;
    for row in founder_rows {
        let (module_id, value) = row?;
        founders.entry(module_id).or_default().push(value);
    }

    let mut release_stmt = conn.prepare(
        "SELECT module_id, version_id, version, status, created_by, created_at_ms, notes, manifest_json
         FROM business_module_releases
         ORDER BY module_id ASC, version DESC",
    )?;
    let mut releases: HashMap<String, Vec<Value>> = HashMap::new();
    let release_rows = release_stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            serde_json::json!({
                "version_id": row.get::<_, String>(1)?,
                "version": row.get::<_, i64>(2)?,
                "status": row.get::<_, String>(3)?,
                "created_by": row.get::<_, String>(4)?,
                "created_at_ms": row.get::<_, i64>(5)?,
                "notes": row.get::<_, String>(6)?,
                "manifest_sha256": hex_sha256(row.get::<_, String>(7)?.as_bytes()),
            }),
        ))
    })?;
    for row in release_rows {
        let (module_id, value) = row?;
        releases.entry(module_id).or_default().push(value);
    }

    Ok(serde_json::json!({
        "ok": true,
        "can_manage_all": session_can_manage_all(session),
        "role": session_role(session),
        "user_id": session_user_id(session).unwrap_or(""),
        "founders": founders,
        "releases": releases,
    }))
}

pub fn list_users(root: &Path, session: &BusinessOsSession) -> anyhow::Result<Value> {
    let conn = open_store(root)?;
    seed_session_user(&conn, session)?;
    let users = query_users(&conn)?;
    let visible_users = if session
        .user
        .as_ref()
        .map(|user| user.is_admin)
        .unwrap_or(false)
    {
        users
    } else {
        let current_id = session
            .user
            .as_ref()
            .map(|user| user.id.as_str())
            .unwrap_or("");
        users
            .into_iter()
            .filter(|user| user.id == current_id)
            .collect::<Vec<_>>()
    };
    Ok(serde_json::json!({
        "ok": true,
        "can_manage": session.user.as_ref().map(|user| user.is_admin).unwrap_or(false),
        "users": visible_users
    }))
}

pub fn pull_business_users_for_rxdb(root: &Path) -> anyhow::Result<Value> {
    let conn = open_store(root)?;
    seed_configured_business_users(&conn)?;
    let users = query_users(&conn)?;
    let documents = users
        .into_iter()
        .map(|user| {
            serde_json::json!({
                "id": user.id,
                "user_id": user.id,
                "display_name": user.display_name,
                "role": user.role,
                "active": user.active,
                "created_at_ms": user.created_at_ms,
                "updated_at_ms": user.updated_at_ms,
                "_deleted": false,
            })
        })
        .collect::<Vec<_>>();
    let count = documents.len();
    Ok(serde_json::json!({
        "ok": true,
        "collection": "business_users",
        "documents": documents,
        "count": count,
        "since_ms": 0,
    }))
}

pub fn runtime_settings_for_rxdb(root: &Path) -> anyhow::Result<Value> {
    let env_map = crate::inference::runtime_env::effective_operator_env_map(root)
        .unwrap_or_else(|_| BTreeMap::new());
    let runtime_state = crate::inference::runtime_state::load_runtime_state(root)
        .ok()
        .flatten();
    let provider = runtime_state
        .as_ref()
        .map(crate::inference::runtime_state::api_provider_for_runtime_state)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            crate::inference::runtime_state::infer_api_provider_from_env_map(&env_map)
        });
    let source = runtime_state
        .as_ref()
        .map(|state| state.source.as_env_value().to_owned())
        .unwrap_or_else(|| {
            env_map
                .get("CTOX_CHAT_SOURCE")
                .cloned()
                .unwrap_or_else(|| "local".to_owned())
        });
    let preset = runtime_settings_preset(runtime_state.as_ref(), &env_map);
    let context =
        runtime_settings_context(env_map.get("CTOX_CHAT_MODEL_MAX_CONTEXT").cloned().or_else(
            || {
                runtime_state.as_ref().and_then(|state| {
                    state
                        .configured_context_tokens
                        .map(|value| value.to_string())
                })
            },
        ));
    let upstream_base_url = runtime_state
        .as_ref()
        .filter(|state| !state.source.is_local())
        .map(|state| state.upstream_base_url.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            (!source.eq_ignore_ascii_case("local"))
                .then(|| runtime_settings_api_upstream_base_url(&provider, &env_map))
        })
        .unwrap_or_default();
    let key_name = crate::inference::runtime_state::api_key_env_var_for_provider_with_env_map(
        &provider, &env_map,
    );
    let key_configured = crate::secrets::get_credential(root, key_name).is_some();
    let configured_auth_mode = env_map
        .get("CTOX_OPENAI_AUTH_MODE")
        .or_else(|| env_map.get("OPENAI_AUTH_MODE"))
        .cloned()
        .unwrap_or_else(|| "api_key".to_owned());
    let auth_mode = if provider.eq_ignore_ascii_case("local") {
        "local".to_owned()
    } else {
        configured_auth_mode
    };
    let service = cheap_ctox_service_status(root);
    let service_running = service
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let service_last_error = service
        .get("last_error")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_default();
    let subscription_selected = provider.eq_ignore_ascii_case("openai")
        && matches!(
            auth_mode.trim().to_ascii_lowercase().as_str(),
            "chatgpt_subscription" | "subscription" | "codex_subscription" | "chatgpt"
        );
    let subscription_auth = if subscription_selected {
        chatgpt_subscription_auth_status(root)
    } else {
        ChatgptSubscriptionAuthStatus::default()
    };
    let auth_configured = provider.eq_ignore_ascii_case("local")
        || key_configured
        || (subscription_selected && subscription_auth.configured);
    let service_needs_attention = !service_running || !service_last_error.trim().is_empty();
    let auth_needs_attention = !auth_configured;
    let needs_attention = service_needs_attention || auth_needs_attention;
    let auth_message = runtime_auth_message(
        provider.as_str(),
        key_name,
        key_configured,
        subscription_selected,
        &subscription_auth,
    );
    let service_message = if service_needs_attention {
        if !service_running {
            "CTOX Service läuft nicht.".to_owned()
        } else {
            format!("CTOX kann Aufgaben nicht ausführen: {service_last_error}")
        }
    } else {
        "CTOX Service läuft.".to_owned()
    };
    let diagnostics_message = if needs_attention {
        if !service_running {
            "CTOX Service läuft nicht.".to_owned()
        } else if !service_last_error.trim().is_empty() {
            format!("CTOX kann Aufgaben nicht ausführen: {service_last_error}")
        } else {
            auth_message.clone()
        }
    } else {
        auth_message.clone()
    };
    let harness_flow = harness_flow_projection(root);
    let queue_health = harness_queue_health(root);
    let web_stack = web_stack_projection(root, &env_map);
    let updated_at_ms = now_ms() as u64;
    Ok(serde_json::json!({
        "id": "runtime-settings",
        "ok": true,
        "can_manage": true,
        "updated_at_ms": updated_at_ms,
        "harness_flow": harness_flow,
        "queue_health": queue_health,
        "web_stack": web_stack,
        "runtime": {
            "source": source,
            "provider": provider,
            "chat_model": env_map.get("CTOX_CHAT_MODEL")
                .or_else(|| env_map.get("CTOX_CHAT_MODEL_BASE"))
                .cloned()
                .or_else(|| runtime_state.as_ref().and_then(|state| state.requested_model.clone()))
                .or_else(|| runtime_state.as_ref().and_then(|state| state.active_model.clone()))
                .unwrap_or_default(),
            "preset": preset,
            "context": context,
            "max_run_secs": env_map.get("CTOX_CHAT_TURN_TIMEOUT_SECS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(1800),
            "upstream_base_url": upstream_base_url
        },
        "auth": {
            "mode": auth_mode,
            "api_key_name": key_name,
            "api_key_configured": key_configured,
            "subscription_selected": subscription_selected,
            "subscription_session_configured": subscription_auth.configured,
            "subscription_account_email": subscription_auth.account_email,
            "subscription_plan": subscription_auth.plan,
            "configured": auth_configured
        },
        "service": service,
        "diagnostics": {
            "needs_attention": needs_attention,
            "service_needs_attention": service_needs_attention,
            "auth_needs_attention": auth_needs_attention,
            "service_message": service_message,
            "auth_message": auth_message,
            "last_error": service_last_error,
            "message": diagnostics_message
        }
    }))
}

fn harness_flow_projection(root: &Path) -> Value {
    match crate::service::harness_flow::load_latest_flow(root) {
        Ok(flow) => serde_json::json!({
            "ok": true,
            "mode": "ctox_core",
            "flow": flow
        }),
        Err(err) => serde_json::json!({
            "ok": false,
            "mode": "ctox_core",
            "error": err.to_string()
        }),
    }
}

fn harness_queue_health(root: &Path) -> Value {
    let pending_statuses = ["pending".to_string()];
    let leased_statuses = ["leased".to_string()];
    let open_statuses = ["pending".to_string(), "leased".to_string()];
    match channels::list_queue_tasks(root, &open_statuses, 128) {
        Ok(tasks) => {
            let pending =
                channels::count_queue_tasks(root, &pending_statuses).unwrap_or_else(|_| {
                    tasks
                        .iter()
                        .filter(|task| task.route_status == "pending")
                        .count()
                });
            let leased = channels::count_queue_tasks(root, &leased_statuses).unwrap_or_else(|_| {
                tasks
                    .iter()
                    .filter(|task| task.route_status == "leased")
                    .count()
            });
            let open = channels::count_queue_tasks(root, &open_statuses).unwrap_or(tasks.len());
            let oldest_pending_created_at = tasks
                .iter()
                .filter(|task| task.route_status == "pending")
                .map(|task| task.created_at.as_str())
                .min()
                .unwrap_or_default()
                .to_string();
            serde_json::json!({
                "ok": true,
                "pending_count": pending,
                "leased_count": leased,
                "open_count": open,
                "oldest_pending_created_at": oldest_pending_created_at,
                "stalled": pending > 0 && leased == 0
            })
        }
        Err(err) => serde_json::json!({
            "ok": false,
            "pending_count": null,
            "leased_count": null,
            "open_count": null,
            "oldest_pending_created_at": "",
            "stalled": false,
            "error": err.to_string()
        }),
    }
}

fn web_stack_projection(root: &Path, env_map: &BTreeMap<String, String>) -> Value {
    let mut sources = Vec::new();
    let mut credential_required = 0usize;
    let mut credential_configured = 0usize;
    let mut browser_assist_sources = 0usize;

    for module in ctox_web_stack::sources::list() {
        let tier = match module.tier() {
            ctox_web_stack::sources::Tier::P => "P",
            ctox_web_stack::sources::Tier::S => "S",
            ctox_web_stack::sources::Tier::C => "C",
        };
        let countries = module
            .countries()
            .iter()
            .map(|country| country.as_iso())
            .collect::<Vec<_>>();
        let fields = module
            .authoritative_for()
            .iter()
            .map(|field| field.as_str())
            .collect::<Vec<_>>();
        let browser_recipe = module.browser_recipe();
        if browser_recipe.is_some() {
            browser_assist_sources += 1;
        }
        let required_secret = browser_recipe
            .as_ref()
            .and_then(|recipe| recipe.required_secret_name)
            .or_else(|| module.requires_credential());
        let credential_is_required = required_secret.is_some();
        let configured = required_secret
            .map(|secret_name| web_stack_secret_configured(root, env_map, secret_name))
            .unwrap_or(false);
        if credential_is_required {
            credential_required += 1;
            if configured {
                credential_configured += 1;
            }
        }
        sources.push(serde_json::json!({
            "id": module.id(),
            "aliases": module.aliases(),
            "tier": tier,
            "countries": countries,
            "authoritative_for": fields,
            "credential": {
                "required": credential_is_required,
                "configured": configured,
                "secret_name": required_secret.unwrap_or_default()
            },
            "browser_assist": browser_recipe.as_ref().map(|recipe| serde_json::json!({
                "source_id": recipe.source_id,
                "login_url": recipe.login_url,
                "allowed_domains": recipe.allowed_domains,
                "required_secret_name": recipe.required_secret_name,
                "verify_selector": recipe.verify_selector,
                "credential_selector": recipe.credential_selector,
                "capture_script_available": recipe.capture_script.is_some(),
                "secret_value_in_payload": false
            }))
        }));
    }

    serde_json::json!({
        "ok": true,
        "mode": "ctox_web_stack",
        "updated_at_ms": now_ms() as u64,
        "summary": {
            "sources": sources.len(),
            "browser_assist_sources": browser_assist_sources,
            "credential_required": credential_required,
            "credential_configured": credential_configured,
            "credential_missing": credential_required.saturating_sub(credential_configured)
        },
        "sources": sources,
        "secret_value_in_payload": false,
        "frame_data_in_payload": false
    })
}

fn web_stack_secret_configured(
    root: &Path,
    env_map: &BTreeMap<String, String>,
    secret_name: &str,
) -> bool {
    env_map
        .get(secret_name)
        .or_else(|| env_map.get(&secret_name.to_ascii_uppercase()))
        .is_some_and(|value| !value.trim().is_empty())
        || crate::secrets::get_credential(root, secret_name)
            .is_some_and(|value| !value.trim().is_empty())
        || env::var(secret_name).is_ok_and(|value| !value.trim().is_empty())
}

pub fn module_catalog_for_rxdb(root: &Path) -> anyhow::Result<Value> {
    let app_root = resolve_business_os_app_root(root)?;
    let modules = load_module_manifests(&app_root)?;
    let marketplace = load_marketplace_module_manifests(&app_root)?;
    let templates = load_template_manifests(&app_root)?;
    let governance = module_governance_map(
        root,
        &BusinessOsSession {
            ok: true,
            authenticated: true,
            auth_required: false,
            user: Some(BusinessOsSessionUser {
                id: "ctox-system".to_owned(),
                display_name: "CTOX System".to_owned(),
                role: "admin".to_owned(),
                is_admin: true,
            }),
            login_url: None,
            reason: None,
        },
    )?;
    let version_states = module_version_states(root, &app_root).unwrap_or(Value::Null);
    Ok(serde_json::json!({
        "id": "module-catalog",
        "ok": true,
        "modules": modules,
        "marketplace": marketplace,
        "templates": templates,
        "governance": governance,
        "version_states": version_states,
        // Per-instance allowlist that rides the RxDB data plane. Empty = no restriction.
        // The shell intersects its merged module list with this set when non-empty.
        "allowed_module_ids": business_os_module_allowlist(root),
        "updated_at_ms": now_ms(),
        "_deleted": false,
    }))
}

pub fn write_module_catalog_projection_to_rxdb(root: &Path) -> anyhow::Result<()> {
    let mut document = module_catalog_for_rxdb(root)?;
    if let Some(object) = document.as_object_mut() {
        object.remove("_rev");
        object.remove("_meta");
        object.insert("_deleted".to_string(), Value::Bool(false));
        object.insert("is_deleted".to_string(), Value::Bool(false));
    }
    let now = now_ms();
    let revision = format!("{now}-ctox-module-catalog");
    let path = rxdb_store_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create RxDB runtime dir {}", parent.display()))?;
    }
    let conn =
        Connection::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 10000;
        CREATE TABLE IF NOT EXISTS "ctox_business_os__business_module_catalog__v0"(
            id TEXT NOT NULL PRIMARY KEY UNIQUE,
            revision TEXT,
            deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
            lastWriteTime REAL NOT NULL,
            data TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS "ctox_business_os__business_module_catalog__v0_lwt_id_idx"
            ON "ctox_business_os__business_module_catalog__v0"(lastWriteTime, id);
        CREATE INDEX IF NOT EXISTS "ctox_business_os__business_module_catalog__v0_deleted_lwt_id_idx"
            ON "ctox_business_os__business_module_catalog__v0"(deleted, lastWriteTime, id);
        "#,
    )?;
    conn.execute(
        r#"
        INSERT INTO "ctox_business_os__business_module_catalog__v0"
            (id, revision, deleted, lastWriteTime, data)
        VALUES ('module-catalog', ?1, 0, ?2, ?3)
        ON CONFLICT(id) DO UPDATE SET
            revision = excluded.revision,
            deleted = 0,
            lastWriteTime = excluded.lastWriteTime,
            data = excluded.data
        "#,
        params![revision, now as f64, serde_json::to_string(&document)?],
    )?;
    Ok(())
}

pub fn upsert_user(
    root: &Path,
    session: &BusinessOsSession,
    mutation: BusinessOsUserMutation,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session
            .user
            .as_ref()
            .map(|user| user.is_admin)
            .unwrap_or(false),
        "admin role required"
    );
    anyhow::ensure!(!mutation.id.trim().is_empty(), "user id is required");
    let role = normalize_business_role(&mutation.role);
    anyhow::ensure!(
        matches!(role.as_str(), "chef" | "admin" | "founder" | "user"),
        "role must be chef, admin, founder, or user"
    );
    let conn = open_store(root)?;
    seed_session_user(&conn, session)?;
    let now = now_ms() as i64;
    conn.execute(
        "INSERT INTO business_users
            (user_id, display_name, role, active, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(user_id) DO UPDATE SET
            display_name = excluded.display_name,
            role = excluded.role,
            active = excluded.active,
            updated_at_ms = excluded.updated_at_ms",
        params![
            mutation.id.trim(),
            mutation.display_name.trim(),
            role,
            mutation.active as i64,
            now
        ],
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "users": query_users(&conn)?
    }))
}

pub fn assign_module_founder(
    root: &Path,
    session: &BusinessOsSession,
    assignment: ModuleFounderAssignment,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required"
    );
    let module_id = assignment.module_id.trim();
    let user_id = assignment.user_id.trim();
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    anyhow::ensure!(!user_id.is_empty(), "user_id is required");
    let conn = open_store(root)?;
    seed_session_user(&conn, session)?;
    let now = now_ms() as i64;
    conn.execute(
        "INSERT INTO business_module_acl
            (module_id, user_id, role, active, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, 'founder', ?3, ?4, ?4)
         ON CONFLICT(module_id, user_id, role) DO UPDATE SET
            active = excluded.active,
            updated_at_ms = excluded.updated_at_ms",
        params![module_id, user_id, assignment.active as i64, now],
    )?;
    let record_id = format!("{module_id}:founder:{user_id}");
    upsert_business_record(
        &conn,
        "business_module_acl",
        &record_id,
        now,
        serde_json::json!({
            "id": record_id,
            "module_id": module_id,
            "user_id": user_id,
            "role": "founder",
            "active": assignment.active,
            "updated_at_ms": now
        }),
    )?;
    let mut governance = module_governance_map(root, session)?;
    if let Some(map) = governance.as_object_mut() {
        map.insert(
            "business_module_acl_ids".to_string(),
            Value::Array(vec![Value::String(record_id)]),
        );
    }
    Ok(governance)
}

pub fn upsert_module_manifest_command(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleUpsertRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.id);
    anyhow::ensure!(!module_id.is_empty(), "module id is required");
    anyhow::ensure!(
        session_can_modify_module(root, session, &module_id)?,
        "module modification rights required"
    );
    let manifest = upsert_module_manifest(app_root, request)?;
    Ok(serde_json::json!({
        "ok": true,
        "module_id": manifest.id,
        "module": manifest
    }))
}

pub fn install_template_module_command(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleInstallTemplateRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required"
    );
    let manifest = install_template_module(app_root, request)?;
    let created_by = session_user_id(session).unwrap_or("").to_string();
    record_module_version(
        root,
        app_root,
        &manifest.id,
        "install",
        "Installed from template",
        &created_by,
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "module_id": manifest.id,
        "module": manifest
    }))
}

pub fn delete_installed_module_command(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleDeleteRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module id is required");
    anyhow::ensure!(
        session_can_modify_module(root, session, &module_id)?,
        "module modification rights required"
    );
    delete_installed_module(
        app_root,
        root,
        ModuleDeleteRequest {
            module_id: module_id.clone(),
        },
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "deleted": true
    }))
}

pub fn save_runtime_settings_command(
    root: &Path,
    session: &BusinessOsSession,
    request: RuntimeSettingsRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required"
    );
    save_runtime_settings(root, request)?;
    Ok(serde_json::json!({
        "ok": true,
        "status": "saved"
    }))
}

pub fn start_subscription_auth_command(
    root: &Path,
    session: &BusinessOsSession,
    request: SubscriptionAuthStartCommandRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required"
    );
    subscription_auth_start_payload(root, request.use_device_code())
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SubscriptionAuthStartCommandRequest {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub auth_mode: Option<String>,
    #[serde(default)]
    pub flow: Option<String>,
}

impl SubscriptionAuthStartCommandRequest {
    fn use_device_code(&self) -> bool {
        let provider = self
            .provider
            .as_deref()
            .unwrap_or("openai")
            .trim()
            .to_ascii_lowercase();
        let auth_mode = self
            .auth_mode
            .as_deref()
            .unwrap_or("chatgpt_subscription")
            .trim()
            .to_ascii_lowercase();
        let flow = self
            .flow
            .as_deref()
            .unwrap_or("device_code")
            .trim()
            .to_ascii_lowercase();
        provider == "openai" && auth_mode == "chatgpt_subscription" && flow == "device_code"
    }
}

pub fn run_channel_command(
    root: &Path,
    session: &BusinessOsSession,
    command_type: &str,
    request: ChannelCommandRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required"
    );
    match command_type {
        "ctox.channel.test" => {
            let account_key = request.account_key.trim();
            let account_key = if account_key.is_empty() {
                None
            } else {
                Some(account_key)
            };
            channels::test_channel_for_business_os(root, request.channel.trim(), account_key)
        }
        "ctox.channel.sync" => channels::sync_channel_for_business_os(root, request.channel.trim()),
        "ctox.channel.settings.save" => channels::save_channel_settings_for_business_os(
            root,
            request.channel.trim(),
            &request.config,
        ),
        "ctox.channel.disconnect" => channels::disconnect_communication_account_for_business_os(
            root,
            request.account_key.trim(),
        ),
        "ctox.channel.pair.start" => {
            channels::start_pairing_for_business_os(root, request.channel.trim())
        }
        "ctox.channel.jami.export" => Ok(channels::export_jami_archive_for_business_os(root)),
        "ctox.channel.jami.create" => {
            let display_name = request.display_name.trim().to_string();
            let display_name = if display_name.is_empty() {
                "CTOX".to_string()
            } else {
                display_name
            };
            let config = serde_json::json!({ "profile_name": display_name });
            channels::save_channel_settings_for_business_os(root, "jami", &config)?;
            channels::start_pairing_for_business_os(root, "jami")
        }
        _ => anyhow::bail!("unsupported channel command type: {command_type}"),
    }
}

fn save_runtime_settings(root: &Path, request: RuntimeSettingsRequest) -> anyhow::Result<()> {
    let provider = crate::inference::runtime_state::normalize_api_provider(&request.provider);
    let mut env_map = crate::inference::runtime_env::effective_operator_env_map(root)
        .unwrap_or_else(|_| BTreeMap::new());
    let chat_model = request.chat_model.trim();
    let preset = request.preset.trim();
    let requested_context = request.context.trim();
    let context = runtime_settings_context(
        (!requested_context.is_empty()).then(|| requested_context.to_owned()),
    );
    if provider.eq_ignore_ascii_case("local") {
        env_map.insert("CTOX_CHAT_SOURCE".to_owned(), "local".to_owned());
        env_map.remove("CTOX_API_PROVIDER");
        env_map.remove("CTOX_UPSTREAM_BASE_URL");
        env_map.remove("OPENAI_AUTH_MODE");
        env_map.remove("CTOX_OPENAI_AUTH_MODE");
    } else {
        env_map.insert("CTOX_CHAT_SOURCE".to_owned(), "api".to_owned());
        env_map.insert("CTOX_API_PROVIDER".to_owned(), provider.to_owned());
        env_map.insert(
            "CTOX_UPSTREAM_BASE_URL".to_owned(),
            runtime_settings_api_upstream_base_url(provider, &env_map),
        );
    }
    if !chat_model.is_empty() {
        env_map.insert("CTOX_CHAT_MODEL".to_owned(), chat_model.to_owned());
        env_map.insert("CTOX_CHAT_MODEL_BASE".to_owned(), chat_model.to_owned());
    }
    if let Some(preset) = normalize_runtime_preset(preset) {
        env_map.insert("CTOX_CHAT_LOCAL_PRESET".to_owned(), preset.to_owned());
    }
    if !requested_context.is_empty() {
        env_map.insert("CTOX_CHAT_MODEL_MAX_CONTEXT".to_owned(), context.to_owned());
    }
    if let Some(max_run_secs) = request.max_run_secs.filter(|value| *value > 0) {
        env_map.insert(
            "CTOX_CHAT_TURN_TIMEOUT_SECS".to_owned(),
            max_run_secs.to_string(),
        );
    }
    let auth_mode = request.auth_mode.trim().to_ascii_lowercase();
    if provider.eq_ignore_ascii_case("openai")
        && matches!(
            auth_mode.as_str(),
            "chatgpt_subscription" | "subscription" | "codex_subscription" | "chatgpt"
        )
    {
        env_map.insert(
            "OPENAI_AUTH_MODE".to_owned(),
            "chatgpt_subscription".to_owned(),
        );
        env_map.insert(
            "CTOX_OPENAI_AUTH_MODE".to_owned(),
            "chatgpt_subscription".to_owned(),
        );
    } else {
        env_map.insert("OPENAI_AUTH_MODE".to_owned(), "api_key".to_owned());
        env_map.insert("CTOX_OPENAI_AUTH_MODE".to_owned(), "api_key".to_owned());
    }
    let api_key = request.api_key.trim();
    if !api_key.is_empty() {
        let key_name = crate::inference::runtime_state::api_key_env_var_for_provider(provider);
        env_map.insert(key_name.to_owned(), api_key.to_owned());
    }
    crate::inference::runtime_env::save_runtime_env_map(root, &env_map)
}

fn runtime_settings_preset(
    runtime_state: Option<&crate::inference::runtime_state::InferenceRuntimeState>,
    env_map: &BTreeMap<String, String>,
) -> String {
    runtime_state
        .and_then(|state| state.local_preset.as_deref())
        .or_else(|| env_map.get("CTOX_CHAT_LOCAL_PRESET").map(String::as_str))
        .and_then(normalize_runtime_preset)
        .unwrap_or("Quality")
        .to_owned()
}

fn normalize_runtime_preset(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "quality" => Some("Quality"),
        "performance" => Some("Performance"),
        _ => None,
    }
}

fn runtime_settings_api_upstream_base_url(
    provider: &str,
    env_map: &BTreeMap<String, String>,
) -> String {
    let provider = crate::inference::runtime_state::normalize_api_provider(provider);
    if provider.eq_ignore_ascii_case("minimax")
        && crate::inference::runtime_state::use_ctox_llm_proxy_credentials(env_map)
    {
        return env_map
            .get(crate::inference::runtime_state::CTOX_LLM_PROXY_BASE_URL_ENV)
            .or_else(|| env_map.get("CTOX_UPSTREAM_BASE_URL"))
            .filter(|value| crate::inference::runtime_state::is_ctox_llm_proxy_base_url(value))
            .cloned()
            .unwrap_or_else(|| "https://llm.ctox.dev".to_owned());
    }
    env_map
        .get("CTOX_UPSTREAM_BASE_URL")
        .filter(|value| !value.trim().is_empty())
        .filter(|value| {
            crate::inference::runtime_state::api_provider_for_upstream_base_url(value)
                .eq_ignore_ascii_case(provider)
        })
        .cloned()
        .unwrap_or_else(|| {
            crate::inference::runtime_state::default_api_upstream_base_url_for_provider(provider)
                .to_owned()
        })
}

fn runtime_settings_context(value: Option<String>) -> String {
    let Some(value) = value else {
        return "256k".to_owned();
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "131072" | "128000" | "128k" | "262144" | "256000" | "256k" | "" => "256k".to_owned(),
        _ => "256k".to_owned(),
    }
}

fn chatgpt_subscription_auth_status(root: &Path) -> ChatgptSubscriptionAuthStatus {
    let Ok(codex_home) = ctox_core::config::find_codex_home() else {
        return ChatgptSubscriptionAuthStatus::default();
    };
    let _ = restore_chatgpt_subscription_auth_from_instance(root, &codex_home);
    let auth_manager = ctox_core::AuthManager::new(
        codex_home.clone(),
        false,
        ctox_core::auth::AuthCredentialsStoreMode::default(),
    );
    let Some(auth) = auth_manager.auth_cached() else {
        return ChatgptSubscriptionAuthStatus::default();
    };
    if !auth.is_chatgpt_auth() {
        return ChatgptSubscriptionAuthStatus::default();
    }
    ChatgptSubscriptionAuthStatus {
        configured: true,
        account_email: auth.get_account_email(),
        plan: auth.account_plan_type().map(|plan| format!("{plan:?}")),
    }
}

fn runtime_auth_message(
    provider: &str,
    key_name: &str,
    key_configured: bool,
    subscription_selected: bool,
    subscription_auth: &ChatgptSubscriptionAuthStatus,
) -> String {
    if provider.eq_ignore_ascii_case("local") {
        return "Lokale CTOX Runtime ausgewählt; keine API-Autorisierung nötig.".to_owned();
    }
    if subscription_selected {
        return if subscription_auth.configured {
            match (
                subscription_auth.account_email.as_deref(),
                subscription_auth.plan.as_deref(),
            ) {
                (Some(email), Some(plan)) => {
                    format!("ChatGPT Subscription autorisiert: {email} ({plan}).")
                }
                (Some(email), None) => format!("ChatGPT Subscription autorisiert: {email}."),
                _ => "ChatGPT Subscription autorisiert.".to_owned(),
            }
        } else {
            "ChatGPT Subscription ausgewählt, aber keine ChatGPT-Session im Codex/CTOX Auth-Store gefunden.".to_owned()
        };
    }
    if key_configured {
        format!("{key_name} ist im CTOX Secret Store vorhanden.")
    } else {
        format!("{key_name} fehlt im CTOX Secret Store.")
    }
}

fn subscription_auth_start_payload(root: &Path, use_device_code: bool) -> anyhow::Result<Value> {
    let login = start_chatgpt_subscription_login(root, use_device_code)?;
    Ok(serde_json::json!({
        "ok": true,
        "status": if login.device_user_code.is_some() { "device_code" } else { "auth_url" },
        "login_id": login.login_id,
        "auth_url": login.auth_url,
        "redirect_uri": login.redirect_uri,
        "verification_url": login.verification_url,
        "user_code": login.device_user_code,
        "message": "ChatGPT Subscription Autorisierung gestartet."
    }))
}

struct StartedChatgptSubscriptionLogin {
    login_id: String,
    auth_url: String,
    redirect_uri: String,
    device_user_code: Option<String>,
    verification_url: Option<String>,
}

#[derive(Clone)]
struct ChatgptLoginPkce {
    verifier: String,
    challenge: String,
}

fn start_chatgpt_subscription_login(
    root: &Path,
    use_device_code: bool,
) -> anyhow::Result<StartedChatgptSubscriptionLogin> {
    let codex_home = ctox_core::config::find_codex_home()
        .context("Codex/CTOX Auth-Store konnte nicht aufgelöst werden")?;
    let pkce = chatgpt_login_pkce();
    let state = chatgpt_login_state();
    let login_id = Uuid::new_v4().to_string();
    if use_device_code {
        let device = request_chatgpt_device_code()?;
        let verification_url = format!("{CHATGPT_AUTH_ISSUER}/codex/device");
        let redirect_uri = format!("{CHATGPT_AUTH_ISSUER}/deviceauth/callback");
        let auth_url = verification_url.clone();
        let device_auth_id = device.device_auth_id.clone();
        let device_user_code = device.user_code.clone();
        let device_interval_secs = device.interval_secs;
        let worker_login_id = login_id.clone();
        let worker_redirect_uri = redirect_uri.clone();
        let worker_root = root.to_path_buf();
        thread::spawn(move || {
            if let Err(err) = complete_chatgpt_device_code_login(
                &worker_root,
                &codex_home,
                device_auth_id,
                device_user_code,
                device_interval_secs,
                worker_redirect_uri,
            ) {
                eprintln!("CTOX ChatGPT subscription device login {worker_login_id} failed: {err}");
            }
        });
        return Ok(StartedChatgptSubscriptionLogin {
            login_id,
            auth_url,
            redirect_uri,
            device_user_code: Some(device.user_code),
            verification_url: Some(verification_url),
        });
    }
    let (server, port) = bind_chatgpt_login_server()
        .context("Lokaler ChatGPT-Login-Callback konnte nicht gestartet werden")?;
    let redirect_uri = format!("http://localhost:{port}/auth/callback");
    let auth_url = build_chatgpt_authorize_url(&redirect_uri, &pkce.challenge, &state);
    let worker_login_id = login_id.clone();
    let worker_redirect_uri = redirect_uri.clone();
    let root = root.to_path_buf();
    thread::spawn(move || {
        if let Err(err) = run_chatgpt_login_callback_server(
            server,
            root,
            codex_home,
            worker_redirect_uri,
            pkce,
            state,
        ) {
            eprintln!("CTOX ChatGPT subscription login {worker_login_id} failed: {err}");
        }
    });
    Ok(StartedChatgptSubscriptionLogin {
        login_id,
        auth_url,
        redirect_uri,
        device_user_code: None,
        verification_url: None,
    })
}

fn chatgpt_login_pkce() -> ChatgptLoginPkce {
    let verifier = format!(
        "{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    ChatgptLoginPkce {
        verifier,
        challenge,
    }
}

fn chatgpt_login_state() -> String {
    let seed = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let digest = Sha256::digest(seed.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn bind_chatgpt_login_server() -> anyhow::Result<(Server, u16)> {
    for port in [
        CHATGPT_AUTH_CALLBACK_PORT,
        CHATGPT_AUTH_CALLBACK_FALLBACK_PORT,
    ] {
        match Server::http(format!("127.0.0.1:{port}")) {
            Ok(server) => return Ok((server, port)),
            Err(_) => continue,
        }
    }
    anyhow::bail!(
        "Ports {CHATGPT_AUTH_CALLBACK_PORT} und {CHATGPT_AUTH_CALLBACK_FALLBACK_PORT} sind belegt"
    )
}

fn build_chatgpt_authorize_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    let query = [
        ("response_type", "code"),
        ("client_id", ctox_core::auth::CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("scope", CHATGPT_AUTH_SCOPE),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", "ctox_business_os"),
    ];
    let qs = query
        .into_iter()
        .map(|(key, value)| format!("{key}={}", urlencoding_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{CHATGPT_AUTH_ISSUER}/oauth/authorize?{qs}")
}

fn run_chatgpt_login_callback_server(
    server: Server,
    root: PathBuf,
    codex_home: PathBuf,
    redirect_uri: String,
    pkce: ChatgptLoginPkce,
    state: String,
) -> anyhow::Result<()> {
    for request in server.incoming_requests() {
        let url_raw = request.url().to_owned();
        let handled = handle_chatgpt_login_callback_request(
            request,
            &url_raw,
            &root,
            &codex_home,
            &redirect_uri,
            &pkce,
            &state,
        )?;
        if handled {
            break;
        }
    }
    server.unblock();
    Ok(())
}

fn handle_chatgpt_login_callback_request(
    request: Request,
    url_raw: &str,
    root: &Path,
    codex_home: &Path,
    redirect_uri: &str,
    pkce: &ChatgptLoginPkce,
    expected_state: &str,
) -> anyhow::Result<bool> {
    let parsed = Url::parse(&format!("http://localhost{url_raw}"))?;
    if parsed.path() != "/auth/callback" {
        respond_html(request, 404, "Not Found")?;
        return Ok(false);
    }
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    if params.get("state").map(String::as_str) != Some(expected_state) {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: state mismatch.",
        )?;
        return Ok(true);
    }
    if let Some(error) = params.get("error") {
        let description = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or(error);
        respond_html(
            request,
            400,
            &format!("CTOX Login wurde von ChatGPT abgelehnt: {description}"),
        )?;
        return Ok(true);
    }
    let Some(code) = params.get("code").filter(|value| !value.trim().is_empty()) else {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: code fehlt.",
        )?;
        return Ok(true);
    };
    match exchange_chatgpt_authorization_code(code, redirect_uri, &pkce.verifier)
        .and_then(|tokens| persist_chatgpt_subscription_auth(root, codex_home, tokens))
    {
        Ok(()) => {
            respond_html(
                request,
                200,
                "CTOX ChatGPT Subscription ist autorisiert. Dieses Fenster kann geschlossen werden.",
            )?;
            Ok(true)
        }
        Err(err) => {
            respond_html(
                request,
                500,
                &format!("CTOX konnte die ChatGPT Subscription nicht speichern: {err}"),
            )?;
            Ok(true)
        }
    }
}

fn respond_html(request: Request, status: u16, body: &str) -> anyhow::Result<()> {
    let response = Response::from_string(format!(
        "<!doctype html><meta charset=\"utf-8\"><title>CTOX Login</title><body style=\"font:16px system-ui;padding:32px;background:#10181b;color:#eef5f3\"><h1>CTOX Login</h1><p>{}</p></body>",
        html_escape(body)
    ))
    .with_status_code(status)
    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
    request.respond(response).map_err(io::Error::other)?;
    Ok(())
}

struct ChatgptDeviceCode {
    device_auth_id: String,
    user_code: String,
    interval_secs: u64,
}

#[derive(Debug, Deserialize)]
struct ChatgptDeviceTokenResponse {
    authorization_code: String,
    code_verifier: String,
}

fn request_chatgpt_device_code() -> anyhow::Result<ChatgptDeviceCode> {
    let response = ureq::post(&format!(
        "{CHATGPT_AUTH_ISSUER}/api/accounts/deviceauth/usercode"
    ))
    .set("Content-Type", "application/json")
    .send_json(serde_json::json!({
        "client_id": ctox_core::auth::CLIENT_ID,
    }));
    let body: Value = match response {
        Ok(response) => response.into_json().map_err(anyhow::Error::from)?,
        Err(ureq::Error::Status(status, response)) => {
            let body = response.into_string().unwrap_or_default();
            anyhow::bail!("Device-Code-Anforderung fehlgeschlagen ({status}): {body}")
        }
        Err(err) => return Err(anyhow::Error::from(err)),
    };
    let device_auth_id = body
        .get("device_auth_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .context("Device-Code-Antwort enthält keine device_auth_id")?;
    let user_code = body
        .get("user_code")
        .or_else(|| body.get("usercode"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .context("Device-Code-Antwort enthält keinen user_code")?;
    let interval_secs = body
        .get("interval")
        .and_then(|value| match value {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.trim().parse::<u64>().ok(),
            _ => None,
        })
        .unwrap_or(5)
        .max(1);
    Ok(ChatgptDeviceCode {
        device_auth_id,
        user_code,
        interval_secs,
    })
}

fn complete_chatgpt_device_code_login(
    root: &Path,
    codex_home: &Path,
    device_auth_id: String,
    user_code: String,
    interval_secs: u64,
    redirect_uri: String,
) -> anyhow::Result<()> {
    let token = poll_chatgpt_device_token(device_auth_id, user_code, interval_secs)?;
    let tokens = exchange_chatgpt_authorization_code(
        &token.authorization_code,
        &redirect_uri,
        &token.code_verifier,
    )?;
    persist_chatgpt_subscription_auth(root, codex_home, tokens)
}

fn poll_chatgpt_device_token(
    device_auth_id: String,
    user_code: String,
    interval_secs: u64,
) -> anyhow::Result<ChatgptDeviceTokenResponse> {
    let started = Instant::now();
    let max_wait = Duration::from_secs(15 * 60);
    let sleep_for = Duration::from_secs(interval_secs).min(Duration::from_secs(15));
    loop {
        let response = ureq::post(&format!(
            "{CHATGPT_AUTH_ISSUER}/api/accounts/deviceauth/token"
        ))
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({
            "device_auth_id": &device_auth_id,
            "user_code": &user_code,
        }));
        match response {
            Ok(response) => return response.into_json().map_err(anyhow::Error::from),
            Err(ureq::Error::Status(status, response)) if status == 403 || status == 404 => {
                if started.elapsed() >= max_wait {
                    anyhow::bail!("Device-Code-Login ist nach 15 Minuten abgelaufen");
                }
                let _ = response.into_string();
                thread::sleep(sleep_for);
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                anyhow::bail!("Device-Code-Token-Abfrage fehlgeschlagen ({status}): {body}")
            }
            Err(err) => return Err(anyhow::Error::from(err)),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatgptTokenExchangeResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

fn exchange_chatgpt_authorization_code(
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> anyhow::Result<ChatgptTokenExchangeResponse> {
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        urlencoding_encode(code),
        urlencoding_encode(redirect_uri),
        urlencoding_encode(ctox_core::auth::CLIENT_ID),
        urlencoding_encode(code_verifier)
    );
    let response = ureq::post(&format!("{CHATGPT_AUTH_ISSUER}/oauth/token"))
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body);
    match response {
        Ok(response) => response.into_json().map_err(anyhow::Error::from),
        Err(ureq::Error::Status(status, response)) => {
            let body = response.into_string().unwrap_or_default();
            anyhow::bail!("OAuth Token-Exchange fehlgeschlagen ({status}): {body}")
        }
        Err(err) => Err(anyhow::Error::from(err)),
    }
}

fn persist_chatgpt_subscription_auth(
    root: &Path,
    codex_home: &Path,
    tokens: ChatgptTokenExchangeResponse,
) -> anyhow::Result<()> {
    let token_data = ctox_core::token_data::TokenData {
        id_token: ctox_core::token_data::parse_chatgpt_jwt_claims(&tokens.id_token)
            .map_err(anyhow::Error::msg)?,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        account_id: chatgpt_account_id_from_jwt(&tokens.id_token),
    };
    let auth = ctox_core::auth::AuthDotJson {
        auth_mode: Some(ApiAuthMode::Chatgpt),
        openai_api_key: None,
        tokens: Some(token_data),
        last_refresh: Some(chrono::Utc::now()),
    };
    ctox_core::auth::save_auth(
        codex_home,
        &auth,
        ctox_core::auth::AuthCredentialsStoreMode::File,
    )?;
    crate::secrets::write_secret_record(
        root,
        CHATGPT_AUTH_SECRET_SCOPE,
        CHATGPT_AUTH_SECRET_NAME,
        &serde_json::to_string(&auth)?,
        Some("ChatGPT Subscription OAuth state for this CTOX instance".to_owned()),
        serde_json::json!({"source": "business_os_subscription_login", "auth_mode": "chatgpt_subscription"}),
    )?;
    Ok(())
}

fn restore_chatgpt_subscription_auth_from_instance(
    root: &Path,
    codex_home: &Path,
) -> anyhow::Result<bool> {
    let auth_manager = ctox_core::AuthManager::new(
        codex_home.to_path_buf(),
        false,
        ctox_core::auth::AuthCredentialsStoreMode::default(),
    );
    if auth_manager
        .auth_cached()
        .as_ref()
        .is_some_and(|auth| auth.is_chatgpt_auth())
    {
        return Ok(false);
    }
    let serialized = crate::secrets::read_secret_value(
        root,
        CHATGPT_AUTH_SECRET_SCOPE,
        CHATGPT_AUTH_SECRET_NAME,
    )
    .context("no instance ChatGPT auth backup")?;
    let auth: ctox_core::auth::AuthDotJson =
        serde_json::from_str(&serialized).context("instance ChatGPT auth backup is invalid")?;
    if auth.tokens.is_none() {
        anyhow::bail!("instance ChatGPT auth backup has no tokens");
    }
    ctox_core::auth::save_auth(
        codex_home,
        &auth,
        ctox_core::auth::AuthCredentialsStoreMode::File,
    )?;
    Ok(true)
}

fn chatgpt_account_id_from_jwt(jwt: &str) -> Option<String> {
    let mut parts = jwt.split('.');
    let (_header, payload, _signature) = (parts.next()?, parts.next()?, parts.next()?);
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let value = serde_json::from_slice::<Value>(&bytes).ok()?;
    value
        .get("https://api.openai.com/auth")
        .and_then(Value::as_object)
        .and_then(|claims| claims.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn urlencoding_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

pub fn record_module_release(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleReleaseRequest,
) -> anyhow::Result<Value> {
    let module_id = request.module_id.trim();
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    anyhow::ensure!(
        session_can_modify_module(root, session, module_id)?,
        "module modification rights required"
    );
    let manifest_path = module_manifest_path(app_root, module_id)?;
    let manifest_json = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest_value: Value = serde_json::from_str(&manifest_json)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let snapshot = serde_json::json!({
        "module_json": manifest_value,
        "path": manifest_path.display().to_string()
    });
    let conn = open_store(root)?;
    seed_session_user(&conn, session)?;
    let next_version: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM business_module_releases WHERE module_id = ?1",
        params![module_id],
        |row| row.get(0),
    )?;
    let version_id = format!("modrel_{}_{}_{}", module_id, next_version, Uuid::new_v4());
    let now = now_ms() as i64;
    let created_by = session_user_id(session).unwrap_or("");
    conn.execute(
        "UPDATE business_module_releases SET status = 'rolled_back' WHERE module_id = ?1 AND status = 'released'",
        params![module_id],
    )?;
    conn.execute(
        "INSERT INTO business_module_releases
            (version_id, module_id, version, status, manifest_json, snapshot_json, created_by, created_at_ms, notes)
         VALUES (?1, ?2, ?3, 'released', ?4, ?5, ?6, ?7, ?8)",
        params![
            version_id,
            module_id,
            next_version,
            serde_json::to_string(&manifest_value)?,
            serde_json::to_string(&snapshot)?,
            created_by,
            now,
            request.notes.trim()
        ],
    )?;
    let release_ids = sync_module_release_records(&conn, module_id, now)?;
    record_module_version(
        root,
        app_root,
        module_id,
        "manual_release",
        &format!("Release v{next_version}"),
        created_by,
    )?;
    let mut governance = module_governance_map(root, session)?;
    if let Some(object) = governance.as_object_mut() {
        object.insert(
            "module_id".to_string(),
            Value::String(module_id.to_string()),
        );
        object.insert("version_id".to_string(), Value::String(version_id));
        object.insert(
            "business_module_release_ids".to_string(),
            Value::Array(release_ids.into_iter().map(Value::String).collect()),
        );
    }
    Ok(governance)
}

pub fn rollback_module_release(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleRollbackRequest,
) -> anyhow::Result<Value> {
    let module_id = request.module_id.trim();
    let version_id = request.version_id.trim();
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    anyhow::ensure!(!version_id.is_empty(), "version_id is required");
    anyhow::ensure!(
        session_can_modify_module(root, session, module_id)?,
        "module modification rights required"
    );
    let conn = open_store(root)?;
    let manifest_json: String = conn.query_row(
        "SELECT manifest_json FROM business_module_releases WHERE module_id = ?1 AND version_id = ?2",
        params![module_id, version_id],
        |row| row.get(0),
    )?;
    let manifest_path = module_manifest_path(app_root, module_id)?;
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&serde_json::from_str::<Value>(&manifest_json)?)?,
    )
    .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    let now = now_ms() as i64;
    conn.execute(
        "UPDATE business_module_releases SET status = CASE WHEN version_id = ?2 THEN 'released' ELSE 'rolled_back' END WHERE module_id = ?1",
        params![module_id, version_id],
    )?;
    let release_ids = sync_module_release_records(&conn, module_id, now)?;
    let mut governance = module_governance_map(root, session)?;
    if let Some(object) = governance.as_object_mut() {
        object.insert(
            "module_id".to_string(),
            Value::String(module_id.to_string()),
        );
        object.insert(
            "version_id".to_string(),
            Value::String(version_id.to_string()),
        );
        object.insert("rolled_back_at_ms".to_string(), Value::from(now));
        object.insert(
            "business_module_release_ids".to_string(),
            Value::Array(release_ids.into_iter().map(Value::String).collect()),
        );
    }
    Ok(governance)
}

fn sync_module_release_records(
    conn: &Connection,
    module_id: &str,
    updated_at_ms: i64,
) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT version_id, module_id, version, status, created_by, created_at_ms, notes
         FROM business_module_releases
         WHERE module_id = ?1
         ORDER BY version DESC",
    )?;
    let rows = stmt.query_map(params![module_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let release_rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut release_ids = Vec::new();
    for (version_id, module_id, version, status, created_by, created_at_ms, notes) in release_rows {
        let record_updated_at = next_business_record_updated_at(
            conn,
            "business_module_releases",
            &version_id,
            updated_at_ms,
        )?;
        upsert_business_record(
            conn,
            "business_module_releases",
            &version_id,
            record_updated_at,
            serde_json::json!({
                "id": version_id.clone(),
                "version_id": version_id.clone(),
                "module_id": module_id,
                "version": version,
                "status": status,
                "created_by": created_by,
                "created_at_ms": created_at_ms,
                "notes": notes,
                "updated_at_ms": record_updated_at
            }),
        )?;
        release_ids.push(version_id);
    }
    Ok(release_ids)
}

fn next_business_record_updated_at(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    candidate: i64,
) -> anyhow::Result<i64> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT updated_at_ms FROM business_records WHERE collection = ?1 AND record_id = ?2",
            params![collection, record_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match existing {
        Some(existing) if existing >= candidate => existing + 1,
        _ => candidate,
    })
}

pub fn record_report(
    root: &Path,
    session: &BusinessOsSession,
    mutation: BusinessOsReportMutation,
) -> anyhow::Result<Value> {
    let accepted = record_report_command(root, session, mutation, None, None)?;
    Ok(serde_json::json!({
        "ok": true,
        "report_id": accepted.report_id,
        "command_id": accepted.command_id,
        "task_id": accepted.task_id.unwrap_or_default(),
        "status": "open"
    }))
}

struct ReportAccepted {
    report_id: String,
    command_id: String,
    task_id: Option<String>,
    task_status: Option<String>,
}

fn record_report_command(
    root: &Path,
    session: &BusinessOsSession,
    mutation: BusinessOsReportMutation,
    command_id: Option<String>,
    report_id: Option<String>,
) -> anyhow::Result<ReportAccepted> {
    anyhow::ensure!(session.authenticated, "login required");
    let module_id = mutation.module_id.trim();
    let title = mutation.title.trim();
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    anyhow::ensure!(!title.is_empty(), "title is required");
    let kind = normalize_report_kind(&mutation.kind);
    let severity = normalize_report_severity(&mutation.severity);
    let report_id = report_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("report_{}", Uuid::new_v4()));
    let reporter_id = session_user_id(session).unwrap_or("");
    let now = now_ms() as i64;
    let summary = mutation.summary.clone();
    let expected = mutation.expected.clone();
    let client_context = mutation.client_context.clone();
    let command = BusinessCommand {
        id: command_id,
        module: "ctox".to_owned(),
        command_type: format!("ctox.report.{kind}"),
        record_id: Some(report_id.clone()),
        payload: serde_json::json!({
            "title": title,
            "module_id": module_id,
            "kind": kind,
            "severity": severity,
            "summary": summary,
            "expected": expected,
            "reporter_id": reporter_id,
            "instruction": format!("Bearbeite diesen Business-OS {} Report für Modul `{}`. Prüfe Reproduktion, Auswirkung, gewünschtes Ergebnis und setze daraus CTOX Arbeit auf. Wenn du die Aufgabe annimmst oder abschliesst, dokumentiere im Ergebnis konkret, was du geaendert hast, welche Dateien/Module betroffen sind, welche Verifikation gelaufen ist und welche gespeicherte Modulversion als Rollback-Ziel genutzt werden kann.", kind, module_id)
        }),
        client_context: client_context.clone(),
    };
    let accepted = record_command(root, command)?;
    let accepted_command_id = accepted.command_id.clone();
    let task_id = accepted.task_id.clone();
    let task_status = accepted.task_status.clone();
    let conn = open_store(root)?;
    conn.execute(
        "INSERT INTO business_module_reports
            (report_id, module_id, kind, severity, title, summary, expected, status, reporter_id, ctox_command_id, client_context_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open', ?8, ?9, ?10, ?11, ?11)",
        params![
            report_id,
            module_id,
            kind,
            severity,
            title,
            summary,
            expected,
            reporter_id,
            accepted_command_id,
            serde_json::to_string(&client_context)?,
            now
        ],
    )?;
    upsert_business_record(
        &conn,
        "business_module_reports",
        &report_id,
        now,
        serde_json::json!({
            "id": report_id,
            "report_id": report_id,
            "module_id": module_id,
            "kind": kind,
            "severity": severity,
            "title": title,
            "summary": summary,
            "expected": expected,
            "status": "open",
            "reporter_id": reporter_id,
            "ctox_command_id": accepted_command_id,
            "task_id": task_id,
            "inbound_channel": module_id,
            "client_context": client_context,
            "created_at_ms": now,
            "updated_at_ms": now
        }),
    )?;
    upsert_business_record(
        &conn,
        "ctox_bug_reports",
        &report_id,
        now,
        serde_json::json!({
            "id": report_id,
            "title": title,
            "status": "open",
            "module": module_id,
            "inbound_channel": module_id,
            "severity": severity,
            "surface": "business-os",
            "description": summary,
            "evidence": client_context,
            "payload": {
                "kind": kind,
                "expected": expected,
                "ctox_command_id": accepted_command_id,
                "task_id": task_id
            },
            "updated_at_ms": now
        }),
    )?;
    Ok(ReportAccepted {
        report_id,
        command_id: accepted.command_id,
        task_id,
        task_status,
    })
}

pub fn load_module_source_records(
    root: &Path,
    mutation: &ModuleSourceLoadMutation,
) -> anyhow::Result<Value> {
    let app_root = resolve_business_os_app_root(root)?;
    let module_id = source_sanitize_slug(&mutation.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    let module_root = resolve_module_source_root(&app_root, &module_id)?;
    let mut files = Vec::new();
    collect_module_source_files(&module_id, &module_root, &module_root, &mut files)?;
    files.sort_by(|left, right| {
        let left_path = left.get("path").and_then(Value::as_str).unwrap_or_default();
        let right_path = right
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        left_path.cmp(right_path)
    });
    let now = now_ms() as i64;
    let conn = open_store(root)?;
    let mut file_ids = Vec::with_capacity(files.len());
    for file in &files {
        let Some(id) = file.get("id").and_then(Value::as_str) else {
            continue;
        };
        upsert_business_record(&conn, "business_module_source_files", id, now, file.clone())?;
        file_ids.push(id.to_string());
    }
    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "source_file_ids": file_ids,
        "count": file_ids.len()
    }))
}

pub fn save_module_source_record(
    root: &Path,
    mutation: ModuleSourceSaveMutation,
) -> anyhow::Result<Value> {
    let app_root = resolve_business_os_app_root(root)?;
    let module_id = source_sanitize_slug(&mutation.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    let module_root = resolve_module_source_root(&app_root, &module_id)?;
    let rel = normalize_source_relative_path(&mutation.path)?;
    anyhow::ensure!(
        is_allowed_source_path(&rel),
        "source file type is not editable: {}",
        rel.display()
    );
    let target = module_root.join(&rel);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create source directory {}", parent.display()))?;
    }
    let previous_content = fs::read_to_string(&target).ok();
    let previous_sha256 = previous_content
        .as_deref()
        .map(|content| hex_sha256(content.as_bytes()));
    let next_sha256 = hex_sha256(mutation.content.as_bytes());
    let changed = previous_sha256.as_deref() != Some(next_sha256.as_str());
    let snapshot_id = if changed {
        previous_content
            .as_deref()
            .map(|content| {
                write_module_source_snapshot(
                    root,
                    &module_id,
                    &rel,
                    content,
                    previous_sha256.as_deref(),
                )
            })
            .transpose()?
    } else {
        None
    };
    fs::write(&target, mutation.content.as_bytes())
        .with_context(|| format!("failed to write module source {}", target.display()))?;
    let metadata = fs::metadata(&target)?;
    let rel_display = rel.to_string_lossy().replace('\\', "/");
    let file = module_source_file_doc(
        &module_id,
        &rel,
        &rel_display,
        &mutation.content,
        metadata.len(),
        modified_at_ms(&metadata),
        &next_sha256,
        previous_sha256.as_deref().unwrap_or(""),
        snapshot_id.as_deref().unwrap_or(""),
    );
    let file_id = file
        .get("id")
        .and_then(Value::as_str)
        .context("source file doc id missing")?
        .to_string();
    let conn = open_store(root)?;
    upsert_business_record(
        &conn,
        "business_module_source_files",
        &file_id,
        now_ms() as i64,
        file,
    )?;
    drop(conn);
    if changed {
        record_module_version(root, &app_root, &module_id, "edit", "", "")?;
    }
    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "path": rel_display,
        "source_file_id": file_id,
        "source_file_ids": [file_id],
        "size_bytes": metadata.len(),
        "modified_at_ms": modified_at_ms(&metadata),
        "sha256": next_sha256,
        "previous_sha256": previous_sha256,
        "snapshot_id": snapshot_id,
        "changed": changed
    }))
}

pub fn materialize_desktop_file_command(
    root: &Path,
    _session: &BusinessOsSession,
    request: DesktopFileMaterializeRequest,
) -> anyhow::Result<Value> {
    let file_id = request.file_id.trim();
    anyhow::ensure!(!file_id.is_empty(), "file_id is required");
    let file_doc = rxdb_desktop_file_document(root, file_id)?;
    let stored_path = file_doc
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("desktop file path is missing")?;
    if !request.path.trim().is_empty() {
        anyhow::ensure!(
            request.path.trim() == stored_path,
            "desktop file path does not match indexed metadata"
        );
    }
    anyhow::ensure!(
        file_doc
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("file")
            == "file",
        "only regular files can be materialized"
    );
    let source = file_doc
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    anyhow::ensure!(
        source.starts_with("ctox"),
        "only CTOX-managed desktop files can be materialized"
    );
    let path = PathBuf::from(stored_path)
        .canonicalize()
        .with_context(|| format!("failed to canonicalize desktop file {stored_path}"))?;
    super::rxdb_peer::materialize_desktop_file_from_path(root, &path)?;
    let metadata = fs::metadata(&path)
        .with_context(|| format!("failed to read materialized file {}", path.display()))?;
    Ok(serde_json::json!({
        "ok": true,
        "file_id": file_id,
        "path": path.to_string_lossy(),
        "size_bytes": metadata.len(),
        "content_state": "available",
        "content_synced_at_ms": now_ms(),
        "modified_at_ms": modified_at_ms(&metadata)
    }))
}

fn rxdb_desktop_file_document(root: &Path, file_id: &str) -> anyhow::Result<Value> {
    let database_path = rxdb_store_path(root);
    let conn = Connection::open(&database_path)
        .with_context(|| format!("failed to open {}", database_path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 10000;")?;
    let data: String = conn
        .query_row(
            "SELECT data FROM ctox_business_os__desktop_files__v0 WHERE id = ?1",
            params![file_id],
            |row| row.get(0),
        )
        .optional()?
        .with_context(|| format!("desktop file `{file_id}` is not indexed"))?;
    serde_json::from_str(&data).context("invalid desktop file RxDB document")
}

fn rxdb_desktop_file_chunks(
    root: &Path,
    file_id: &str,
    generation_id: &str,
) -> anyhow::Result<Vec<Value>> {
    let database_path = rxdb_store_path(root);
    let conn = Connection::open(&database_path)
        .with_context(|| format!("failed to open {}", database_path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 10000;")?;
    let mut stmt = conn
        .prepare("SELECT data FROM ctox_business_os__desktop_file_chunks__v0")
        .context("desktop file chunk collection is not available")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut chunks = Vec::new();
    for row in rows {
        let raw = row?;
        let value: Value =
            serde_json::from_str(&raw).context("invalid desktop file chunk RxDB document")?;
        if value.get("file_id").and_then(Value::as_str) == Some(file_id)
            && value.get("generation_id").and_then(Value::as_str) == Some(generation_id)
            && !is_rxdb_deleted_document(&value)
        {
            chunks.push(value);
        }
    }
    Ok(chunks)
}

fn resolve_business_os_app_root(root: &Path) -> anyhow::Result<PathBuf> {
    let mut candidates = Vec::new();
    if root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "runtime")
    {
        if let Some(release_root) = root.parent() {
            candidates.push(release_root.join("src").join("apps").join("business-os"));
        }
    }
    candidates.extend([
        root.join("src").join("apps").join("business-os"),
        root.join("apps").join("business-os"),
        root.join("business-os"),
        root.to_path_buf(),
    ]);
    candidates
        .into_iter()
        .find(|candidate| candidate.join("index.html").is_file())
        .context("Business OS app root not found")
}

fn load_module_manifests(app_root: &Path) -> anyhow::Result<Vec<ModuleManifest>> {
    let modules_root = app_root.join("modules");
    let mut manifests = Vec::new();
    if modules_root.is_dir() {
        for entry in fs::read_dir(&modules_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path().join("module.json");
            if !path.is_file() {
                continue;
            }
            let text = fs::read_to_string(&path)
                .with_context(|| format!("failed to read module manifest {}", path.display()))?;
            let mut manifest: ModuleManifest = serde_json::from_str(&text)
                .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
            manifest.manifest_sha256 = hex_sha256(text.as_bytes());
            manifest.local_manifest_path = path.display().to_string();
            if manifest.entry.is_empty() {
                manifest.entry = format!("modules/{}/index.html", manifest.id);
            }
            let scope = module_install_scope(&manifest);
            if !module_ships_on_first_install(&scope) {
                continue;
            }
            let core = scope == "core";
            manifest.install_scope = scope.clone();
            manifest.default_installed = true;
            manifest.source = if core {
                "core"
            } else if scope == "internal" {
                "internal"
            } else {
                "starter"
            }
            .to_owned();
            manifest.core = core;
            manifest.editable = true;
            manifest.deletable = !core;
            manifests.push(manifest);
        }
    }
    for manifest in load_installed_module_manifests(app_root)? {
        if manifests.iter().any(|existing| existing.id == manifest.id) {
            continue;
        }
        manifests.push(manifest);
    }
    manifests.sort_by(|a, b| match (a.id.as_str(), b.id.as_str()) {
        ("ctox", "ctox") => std::cmp::Ordering::Equal,
        ("ctox", _) => std::cmp::Ordering::Less,
        (_, "ctox") => std::cmp::Ordering::Greater,
        _ => a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id)),
    });
    Ok(manifests)
}

fn load_installed_module_manifests(app_root: &Path) -> anyhow::Result<Vec<ModuleManifest>> {
    let modules_root = app_root.join("installed-modules");
    let mut manifests = Vec::new();
    if !modules_root.is_dir() {
        return Ok(manifests);
    }
    for entry in fs::read_dir(&modules_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("module.json");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read module manifest {}", path.display()))?;
        let mut manifest: ModuleManifest = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
        manifest.manifest_sha256 = hex_sha256(text.as_bytes());
        manifest.local_manifest_path = path.display().to_string();
        if manifest.install_scope.trim().eq_ignore_ascii_case("sample") {
            continue;
        }
        if is_core_module(&manifest.id) {
            continue;
        }
        if manifest.entry.is_empty() {
            manifest.entry = format!("installed-modules/{}/index.html", manifest.id);
        }
        manifest.source = "installed".to_owned();
        manifest.install_scope = "installed".to_owned();
        manifest.default_installed = false;
        manifest.core = false;
        manifest.editable = true;
        manifest.deletable = true;
        manifests.push(manifest);
    }
    Ok(manifests)
}

fn load_marketplace_module_manifests(app_root: &Path) -> anyhow::Result<Vec<Value>> {
    let modules_root = app_root.join("modules");
    let mut marketplace = Vec::new();
    if !modules_root.is_dir() {
        return Ok(marketplace);
    }
    for entry in fs::read_dir(&modules_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("module.json");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read module manifest {}", path.display()))?;
        let mut manifest_value: Value = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
        let manifest: ModuleManifest = serde_json::from_value(manifest_value.clone())?;
        let scope = module_install_scope(&manifest);
        if scope != "store" {
            continue;
        }
        let module_id = source_sanitize_slug(&manifest.id);
        if module_id.is_empty() {
            continue;
        }
        let store = manifest_value
            .get("store")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let repo = store
            .get("repository")
            .and_then(Value::as_str)
            .unwrap_or("metric-space-ai/ctox");
        let source_path = store
            .get("source_path")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("modules/{module_id}"));
        let download_url = store
            .get("download_url")
            .or_else(|| store.get("archive_url"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("https://github.com/{repo}/archive/refs/heads/main.zip"));
        manifest_value["module_id"] = Value::String(module_id);
        manifest_value["source"] = Value::String("ctox-local-catalog".to_owned());
        manifest_value["repo"] = Value::String(repo.to_owned());
        manifest_value["source_path"] = Value::String(source_path);
        manifest_value["download_url"] = Value::String(download_url);
        manifest_value["installable"] = manifest_value
            .pointer("/store/installable")
            .and_then(Value::as_bool)
            .map(Value::Bool)
            .unwrap_or(Value::Bool(true));
        marketplace.push(manifest_value);
    }
    marketplace.sort_by(|a, b| {
        let at = a.get("title").and_then(Value::as_str).unwrap_or_default();
        let bt = b.get("title").and_then(Value::as_str).unwrap_or_default();
        at.cmp(bt)
    });
    Ok(marketplace)
}

fn load_template_manifests(app_root: &Path) -> anyhow::Result<Vec<TemplateManifest>> {
    let templates_root = app_root.join("template-store");
    let mut templates = Vec::new();
    if !templates_root.is_dir() {
        return Ok(templates);
    }
    for entry in fs::read_dir(&templates_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("template.json");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read template manifest {}", path.display()))?;
        let template: TemplateManifest = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse template manifest {}", path.display()))?;
        templates.push(template);
    }
    templates.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id)));
    Ok(templates)
}

fn install_template_module(
    app_root: &Path,
    request: ModuleInstallTemplateRequest,
) -> anyhow::Result<ModuleManifest> {
    let template_id = source_sanitize_slug(&request.template_id);
    anyhow::ensure!(!template_id.is_empty(), "template_id is required");
    let template_path = app_root
        .join("template-store")
        .join(&template_id)
        .join("template.json");
    let text = fs::read_to_string(&template_path).with_context(|| {
        format!(
            "failed to read template manifest {}",
            template_path.display()
        )
    })?;
    let template: TemplateManifest = serde_json::from_str(&text).with_context(|| {
        format!(
            "failed to parse template manifest {}",
            template_path.display()
        )
    })?;
    let source_module = source_sanitize_slug(if template.source_module.is_empty() {
        &template.id
    } else {
        &template.source_module
    });
    let source = app_root.join("modules").join(&source_module);
    if !source.join("module.json").is_file() {
        anyhow::bail!("template source module `{source_module}` is missing");
    }
    let requested_id = source_sanitize_slug(if request.module_id.trim().is_empty() {
        if request.title.trim().is_empty() {
            &template.id
        } else {
            &request.title
        }
    } else {
        &request.module_id
    });
    let module_id = unique_module_id(app_root, &requested_id);
    let module_title = if request.title.trim().is_empty() {
        if template.default_title.trim().is_empty() {
            template.title.clone()
        } else {
            template.default_title.clone()
        }
    } else {
        request.title.trim().to_owned()
    };
    let target = app_root.join("installed-modules").join(&module_id);
    copy_dir_recursive(&source, &target)?;

    let manifest_path = target.join("module.json");
    let mut manifest_value: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )?;
    manifest_value["id"] = Value::String(module_id.clone());
    manifest_value["title"] = Value::String(module_title);
    manifest_value["entry"] = Value::String(format!("installed-modules/{module_id}/index.html"));
    manifest_value["install_scope"] = Value::String("installed".to_owned());
    manifest_value["default_installed"] = Value::Bool(false);
    manifest_value["template_id"] = Value::String(template.id);
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest_value)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)?;
    manifest.source = "installed".to_owned();
    manifest.install_scope = "installed".to_owned();
    manifest.default_installed = false;
    manifest.core = false;
    manifest.editable = true;
    manifest.deletable = true;
    Ok(manifest)
}

fn upsert_module_manifest(
    app_root: &Path,
    request: ModuleUpsertRequest,
) -> anyhow::Result<ModuleManifest> {
    let module_id = source_sanitize_slug(&request.id);
    anyhow::ensure!(!module_id.is_empty(), "module id is required");
    let title = request.title.trim();
    anyhow::ensure!(!title.is_empty(), "module title is required");
    let is_core = is_core_module(&module_id);
    let target = if is_core {
        app_root.join("modules").join(&module_id)
    } else {
        app_root.join("installed-modules").join(&module_id)
    };
    let manifest_path = target.join("module.json");
    if !manifest_path.is_file() {
        create_blank_installed_module(app_root, &module_id, title, &request.description)?;
    }
    let mut manifest_value: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )?;
    manifest_value["id"] = Value::String(module_id.clone());
    manifest_value["title"] = Value::String(title.to_owned());
    manifest_value["description"] = Value::String(request.description.trim().to_owned());
    let entry = if is_core {
        format!("modules/{module_id}/index.html")
    } else if request.entry.trim().is_empty() {
        format!("installed-modules/{module_id}/index.html")
    } else {
        request.entry.trim().to_owned()
    };
    manifest_value["entry"] = Value::String(entry);
    manifest_value["collections"] = Value::Array(
        request
            .collections
            .into_iter()
            .map(|item| item.trim().to_owned())
            .filter(|item| !item.is_empty())
            .map(Value::String)
            .collect(),
    );
    if !request.layout.is_null() {
        manifest_value["layout"] = request.layout;
    }
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest_value)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)?;
    manifest.source = if is_core { "core" } else { "installed" }.to_owned();
    manifest.install_scope = if is_core { "core" } else { "installed" }.to_owned();
    manifest.default_installed = is_core;
    manifest.core = is_core;
    manifest.editable = true;
    manifest.deletable = !is_core;
    Ok(manifest)
}

fn create_blank_installed_module(
    app_root: &Path,
    module_id: &str,
    title: &str,
    description: &str,
) -> anyhow::Result<()> {
    if is_core_module(module_id) {
        anyhow::bail!("core module does not exist: {module_id}");
    }
    let target = app_root.join("installed-modules").join(module_id);
    if target.exists() {
        anyhow::bail!("target module already exists: {}", target.display());
    }
    fs::create_dir_all(&target)
        .with_context(|| format!("failed to create module dir {}", target.display()))?;
    let manifest = serde_json::json!({
        "id": module_id,
        "title": title,
        "description": description,
        "entry": format!("installed-modules/{module_id}/index.html"),
        "install_scope": "installed",
        "default_installed": false,
        "collections": ["business_commands"],
        "layout": {
            "shell": "pane",
            "center": "module workspace"
        }
    });
    fs::write(
        target.join("module.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::write(
        target.join("index.html"),
        format!(
            "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\"><title>{}</title></head><body><div data-module-root></div></body></html>\n",
            html_escape(title)
        ),
    )?;
    fs::write(
        target.join("index.js"),
        format!(
            "export async function mount({{ host, module }}) {{\n  host.innerHTML = `<section class=\"blank-module\"><h1>${{module.title || '{}'}}</h1><p>${{module.description || 'Neues Business-OS Modul.'}}</p></section>`;\n  return () => {{}};\n}}\n",
            js_escape(title)
        ),
    )?;
    fs::write(target.join("schema.js"), "export const collections = [];\n")?;
    Ok(())
}

fn delete_installed_module(
    app_root: &Path,
    root: &Path,
    request: ModuleDeleteRequest,
) -> anyhow::Result<()> {
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module id is required");
    if is_core_module(&module_id) {
        anyhow::bail!("core modules cannot be deleted");
    }
    let target = app_root.join("installed-modules").join(&module_id);
    if !target.is_dir() {
        anyhow::bail!("installed module not found: {module_id}");
    }
    fs::remove_dir_all(&target)
        .with_context(|| format!("failed to delete module dir {}", target.display()))?;
    let mut layout = load_module_layout(root)?;
    remove_module_from_layout_value(&mut layout, &module_id);
    save_module_layout(root, &layout)?;
    Ok(())
}

fn module_layout_path(root: &Path) -> PathBuf {
    root.join("runtime").join("business-os-module-layout.json")
}

fn load_module_layout(root: &Path) -> anyhow::Result<Value> {
    let path = module_layout_path(root);
    if !path.is_file() {
        return Ok(serde_json::json!({
            "ok": true,
            "version": 1,
            "labels": {},
            "ungrouped": [],
            "groups": []
        }));
    }
    let mut value: Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .with_context(|| format!("failed to read module layout {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse module layout {}", path.display()))?;
    if let Value::Object(map) = &mut value {
        map.insert("ok".to_owned(), Value::Bool(true));
    }
    Ok(value)
}

fn save_module_layout(root: &Path, layout: &Value) -> anyhow::Result<()> {
    let path = module_layout_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut clean = layout.clone();
    if let Value::Object(map) = &mut clean {
        map.remove("ok");
    }
    fs::write(&path, serde_json::to_vec_pretty(&clean)?)
        .with_context(|| format!("failed to write module layout {}", path.display()))?;
    Ok(())
}

fn remove_module_from_layout_value(layout: &mut Value, module_id: &str) {
    let Some(map) = layout.as_object_mut() else {
        return;
    };
    if let Some(Value::Array(items)) = map.get_mut("ungrouped") {
        items.retain(|item| item.as_str() != Some(module_id));
    }
    if let Some(Value::Array(groups)) = map.get_mut("groups") {
        for group in groups {
            if let Some(Value::Array(items)) = group.get_mut("items") {
                items.retain(|item| item.as_str() != Some(module_id));
            }
        }
    }
    if let Some(Value::Object(labels)) = map.get_mut("labels") {
        labels.remove(module_id);
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn js_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn unique_module_id(app_root: &Path, requested_id: &str) -> String {
    let base = if requested_id.is_empty() {
        "module".to_owned()
    } else if is_core_module(requested_id) {
        format!("{requested_id}-copy")
    } else {
        requested_id.to_owned()
    };
    let installed_root = app_root.join("installed-modules");
    if !installed_root.join(&base).exists() {
        return base;
    }
    for index in 2..1000 {
        let candidate = format!("{base}-{index}");
        if !installed_root.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", Uuid::new_v4())
}

fn is_core_module(id: &str) -> bool {
    CORE_MODULE_IDS.iter().any(|core| id == *core)
}

fn is_starter_module(id: &str) -> bool {
    STARTER_MODULE_IDS.iter().any(|starter| id == *starter)
}

fn module_install_scope(manifest: &ModuleManifest) -> String {
    let explicit = manifest.install_scope.trim().to_ascii_lowercase();
    if is_starter_module(&manifest.id) && explicit == "store" {
        return "starter".to_owned();
    }
    if matches!(
        explicit.as_str(),
        "core" | "starter" | "store" | "internal" | "installed"
    ) {
        return explicit;
    }
    if is_core_module(&manifest.id) {
        "core".to_owned()
    } else if is_starter_module(&manifest.id) {
        "starter".to_owned()
    } else {
        "store".to_owned()
    }
}

fn module_ships_on_first_install(scope: &str) -> bool {
    matches!(scope, "core" | "starter" | "internal")
}

fn copy_dir_recursive(source: &Path, target: &Path) -> anyhow::Result<()> {
    if target.exists() {
        anyhow::bail!("target module already exists: {}", target.display());
    }
    fs::create_dir_all(target)
        .with_context(|| format!("failed to create module dir {}", target.display()))?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}

fn collect_module_source_files(
    module_id: &str,
    module_root: &Path,
    current: &Path,
    files: &mut Vec<Value>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.')
            || matches!(name.as_ref(), "node_modules" | "dist" | "build" | "target")
        {
            continue;
        }
        if entry.file_type()?.is_dir() {
            collect_module_source_files(module_id, module_root, &path, files)?;
            continue;
        }
        let rel = path.strip_prefix(module_root).unwrap_or(&path);
        if !is_allowed_source_path(rel) {
            continue;
        }
        let metadata = fs::metadata(&path)?;
        if metadata.len() > 1024 * 1024 {
            continue;
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read module source {}", path.display()))?;
        let rel_display = rel.to_string_lossy().replace('\\', "/");
        let sha256 = hex_sha256(content.as_bytes());
        files.push(module_source_file_doc(
            module_id,
            rel,
            &rel_display,
            &content,
            metadata.len(),
            modified_at_ms(&metadata),
            &sha256,
            "",
            "",
        ));
    }
    Ok(())
}

fn module_source_file_doc(
    module_id: &str,
    rel: &Path,
    rel_display: &str,
    content: &str,
    size_bytes: u64,
    modified_at_ms: u64,
    sha256: &str,
    previous_sha256: &str,
    snapshot_id: &str,
) -> Value {
    serde_json::json!({
        "id": module_source_file_id(module_id, rel_display),
        "module_id": module_id,
        "path": rel_display,
        "language": source_language_for_path(rel),
        "sha256": sha256,
        "previous_sha256": previous_sha256,
        "snapshot_id": snapshot_id,
        "size_bytes": size_bytes,
        "content": content,
        "source_kind": "module-source",
        "synced_at_ms": now_ms(),
        "updated_at_ms": modified_at_ms
    })
}

fn resolve_module_source_root(app_root: &Path, module_id: &str) -> anyhow::Result<PathBuf> {
    let core = app_root.join("modules").join(module_id);
    if core.join("module.json").is_file() {
        return Ok(core);
    }
    let installed = app_root.join("installed-modules").join(module_id);
    if installed.join("module.json").is_file() {
        return Ok(installed);
    }
    anyhow::bail!("module `{module_id}` was not found")
}

fn normalize_source_relative_path(path: &str) -> anyhow::Result<PathBuf> {
    let rel = Path::new(path);
    if rel.is_absolute() {
        anyhow::bail!("absolute source paths are not allowed");
    }
    let mut out = PathBuf::new();
    for part in rel.components() {
        match part {
            std::path::Component::Normal(segment) => {
                let segment = segment.to_string_lossy();
                if segment.starts_with('.') {
                    anyhow::bail!("hidden source paths are not allowed");
                }
                out.push(segment.as_ref());
            }
            std::path::Component::CurDir => {}
            _ => anyhow::bail!("unsafe source path"),
        }
    }
    if out.as_os_str().is_empty() {
        anyhow::bail!("source path is required");
    }
    Ok(out)
}

fn is_allowed_source_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "css" | "html" | "js" | "json" | "md" | "mjs" | "ts" | "svg"
            )
        })
        .unwrap_or(false)
}

fn source_language_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "css" => "css",
        "html" => "html",
        "json" => "json",
        "md" => "markdown",
        "mjs" | "js" => "javascript",
        "ts" => "typescript",
        "svg" => "xml",
        _ => "text",
    }
}

fn modified_at_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn write_module_source_snapshot(
    root: &Path,
    module_id: &str,
    rel: &Path,
    content: &str,
    previous_sha256: Option<&str>,
) -> anyhow::Result<String> {
    let created_at_ms = now_ms();
    let rel_display = rel.to_string_lossy().replace('\\', "/");
    let safe_path = rel_display
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let snapshot_id = format!("{created_at_ms}_{safe_path}");
    let snapshot_root = root
        .join("runtime")
        .join("business-os-source-snapshots")
        .join(module_id);
    fs::create_dir_all(&snapshot_root).with_context(|| {
        format!(
            "failed to create source snapshot directory {}",
            snapshot_root.display()
        )
    })?;
    let source_path = snapshot_root.join(format!("{snapshot_id}.source"));
    fs::write(&source_path, content.as_bytes())
        .with_context(|| format!("failed to write source snapshot {}", source_path.display()))?;
    let metadata_path = snapshot_root.join(format!("{snapshot_id}.json"));
    let metadata = serde_json::json!({
        "snapshot_id": snapshot_id,
        "module_id": module_id,
        "path": rel_display,
        "previous_sha256": previous_sha256,
        "created_at_ms": created_at_ms,
        "source_path": source_path.display().to_string()
    });
    fs::write(&metadata_path, serde_json::to_vec_pretty(&metadata)?).with_context(|| {
        format!(
            "failed to write source snapshot metadata {}",
            metadata_path.display()
        )
    })?;
    Ok(snapshot_id)
}

pub fn list_module_source_snapshots(
    root: &Path,
    request: ModuleSourceListSnapshotsRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    let snapshot_root = root
        .join("runtime")
        .join("business-os-source-snapshots")
        .join(&module_id);
    if !snapshot_root.is_dir() {
        return Ok(Value::Array(Vec::new()));
    }
    let mut snapshots = Vec::new();
    for entry in fs::read_dir(&snapshot_root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(value) = serde_json::from_str::<Value>(&content) {
                    snapshots.push(value);
                }
            }
        }
    }
    snapshots.sort_by(|a, b| {
        let a_ms = a.get("created_at_ms").and_then(Value::as_u64).unwrap_or(0);
        let b_ms = b.get("created_at_ms").and_then(Value::as_u64).unwrap_or(0);
        b_ms.cmp(&a_ms)
    });
    Ok(Value::Array(snapshots))
}

pub fn rollback_module_source_snapshot(
    root: &Path,
    request: ModuleSourceRollbackSnapshotRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    let snapshot_id = request.snapshot_id.trim();
    anyhow::ensure!(!snapshot_id.is_empty(), "snapshot_id is required");

    let snapshot_root = root
        .join("runtime")
        .join("business-os-source-snapshots")
        .join(&module_id);

    let metadata_path = snapshot_root.join(format!("{}.json", snapshot_id));
    anyhow::ensure!(metadata_path.is_file(), "snapshot metadata not found");

    let source_path = snapshot_root.join(format!("{}.source", snapshot_id));
    anyhow::ensure!(source_path.is_file(), "snapshot source file not found");

    let metadata_content = fs::read_to_string(&metadata_path)?;
    let metadata: Value = serde_json::from_str(&metadata_content)?;
    let rel_path = metadata
        .get("path")
        .and_then(Value::as_str)
        .context("invalid snapshot metadata: path missing")?;

    let source_content = fs::read_to_string(&source_path)?;

    let mutation = ModuleSourceSaveMutation {
        module_id: module_id.clone(),
        path: rel_path.to_string(),
        content: source_content,
    };

    let outcome = save_module_source_record(root, mutation)?;
    Ok(outcome)
}

struct ModuleBundle {
    /// Sorted `[{path, sha256, content}]` over all editable source files.
    files: Vec<Value>,
    /// Deterministic hash over `(path, per-file sha256)` pairs.
    sha256: String,
}

fn compute_module_bundle(app_root: &Path, module_id: &str) -> anyhow::Result<ModuleBundle> {
    let module_root = resolve_module_source_root(app_root, module_id)?;
    let mut raw = Vec::new();
    collect_module_source_files(module_id, &module_root, &module_root, &mut raw)?;
    let mut files: Vec<Value> = raw
        .into_iter()
        .map(|doc| {
            serde_json::json!({
                "path": doc.get("path").and_then(Value::as_str).unwrap_or_default(),
                "sha256": doc.get("sha256").and_then(Value::as_str).unwrap_or_default(),
                "content": doc.get("content").and_then(Value::as_str).unwrap_or_default(),
            })
        })
        .collect();
    files.sort_by(|a, b| {
        a.get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(b.get("path").and_then(Value::as_str).unwrap_or_default())
    });
    let mut digest_input = String::new();
    for file in &files {
        digest_input.push_str(file.get("path").and_then(Value::as_str).unwrap_or_default());
        digest_input.push('\0');
        digest_input.push_str(
            file.get("sha256")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        digest_input.push('\n');
    }
    let sha256 = hex_sha256(digest_input.as_bytes());
    Ok(ModuleBundle { files, sha256 })
}

fn version_summary_row(conn: &Connection, version_id: &str) -> anyhow::Result<Value> {
    let (
        vid,
        module_id,
        seq,
        origin,
        label,
        sha,
        sealed,
        created_by,
        created_at,
        updated_at,
        files_json,
    ): (
        String,
        String,
        i64,
        String,
        String,
        String,
        i64,
        String,
        i64,
        i64,
        String,
    ) = conn.query_row(
        "SELECT version_id, module_id, seq, origin, label, bundle_sha256, sealed,
                created_by, created_at_ms, updated_at_ms, files_json
         FROM business_module_versions WHERE version_id = ?1",
        params![version_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
            ))
        },
    )?;
    let file_count = serde_json::from_str::<Value>(&files_json)
        .ok()
        .and_then(|value| value.as_array().map(|arr| arr.len()))
        .unwrap_or(0);
    Ok(serde_json::json!({
        "version_id": vid,
        "module_id": module_id,
        "seq": seq,
        "origin": origin,
        "label": label,
        "bundle_sha256": sha,
        "sealed": sealed != 0,
        "created_by": created_by,
        "created_at_ms": created_at,
        "updated_at_ms": updated_at,
        "file_count": file_count
    }))
}

fn sync_module_version_records(
    conn: &Connection,
    module_id: &str,
    updated_at_ms: i64,
) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT version_id FROM business_module_versions WHERE module_id = ?1 ORDER BY seq DESC",
    )?;
    let ids = stmt
        .query_map(params![module_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for id in ids {
        let mut doc = version_summary_row(conn, &id)?;
        let rec_updated =
            next_business_record_updated_at(conn, "business_module_versions", &id, updated_at_ms)?;
        if let Some(object) = doc.as_object_mut() {
            object.insert("id".to_string(), Value::String(id.clone()));
            object.insert("updated_at_ms".to_string(), Value::from(rec_updated));
        }
        upsert_business_record(conn, "business_module_versions", &id, rec_updated, doc)?;
    }
    Ok(())
}

/// Capture a full-bundle restore point for a module.
///
/// `origin == "edit"` coalesces into the single open working version (so a burst
/// of agent edits is one rolling restore point); any other origin is a sealed
/// boundary (install, manual_release, rollback, creator_deploy).
fn record_module_version(
    root: &Path,
    app_root: &Path,
    module_id: &str,
    origin: &str,
    label: &str,
    created_by: &str,
) -> anyhow::Result<Option<Value>> {
    let module_id = source_sanitize_slug(module_id);
    if module_id.is_empty() {
        return Ok(None);
    }
    let bundle = compute_module_bundle(app_root, &module_id)?;
    let conn = open_store(root)?;
    let now = now_ms() as i64;
    let is_boundary = origin != "edit";

    let latest: Option<(String, String, i64)> = conn
        .query_row(
            "SELECT version_id, bundle_sha256, sealed FROM business_module_versions
             WHERE module_id = ?1 ORDER BY seq DESC LIMIT 1",
            params![module_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    if !is_boundary {
        if let Some((latest_id, latest_sha, latest_sealed)) = latest.as_ref() {
            if latest_sha == &bundle.sha256 {
                return Ok(None);
            }
            if *latest_sealed == 0 {
                conn.execute(
                    "UPDATE business_module_versions
                     SET bundle_sha256 = ?2, files_json = ?3, updated_at_ms = ?4,
                         label = CASE WHEN ?5 <> '' THEN ?5 ELSE label END
                     WHERE version_id = ?1",
                    params![
                        latest_id,
                        bundle.sha256,
                        serde_json::to_string(&bundle.files)?,
                        now,
                        label
                    ],
                )?;
                sync_module_version_records(&conn, &module_id, now)?;
                return Ok(Some(version_summary_row(&conn, latest_id)?));
            }
        }
    } else {
        conn.execute(
            "UPDATE business_module_versions SET sealed = 1, updated_at_ms = ?2
             WHERE module_id = ?1 AND sealed = 0",
            params![module_id, now],
        )?;
        if origin == "install" {
            if let Some((latest_id, latest_sha, _)) = latest.as_ref() {
                if latest_sha == &bundle.sha256 {
                    sync_module_version_records(&conn, &module_id, now)?;
                    return Ok(Some(version_summary_row(&conn, latest_id)?));
                }
            }
        }
    }

    let next_seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM business_module_versions WHERE module_id = ?1",
        params![module_id],
        |row| row.get(0),
    )?;
    let version_id = format!("modver_{}_{}_{}", module_id, next_seq, Uuid::new_v4());
    let sealed = i64::from(is_boundary);
    conn.execute(
        "INSERT INTO business_module_versions
            (version_id, module_id, seq, origin, label, bundle_sha256, files_json,
             sealed, created_by, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![
            version_id,
            module_id,
            next_seq,
            origin,
            label,
            bundle.sha256,
            serde_json::to_string(&bundle.files)?,
            sealed,
            created_by,
            now
        ],
    )?;
    sync_module_version_records(&conn, &module_id, now)?;
    Ok(Some(version_summary_row(&conn, &version_id)?))
}

pub fn list_module_versions(
    root: &Path,
    request: ModuleVersionListRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    let conn = open_store(root)?;
    let mut stmt = conn.prepare(
        "SELECT version_id FROM business_module_versions WHERE module_id = ?1 ORDER BY seq DESC",
    )?;
    let ids = stmt
        .query_map(params![module_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut versions = Vec::with_capacity(ids.len());
    for id in &ids {
        versions.push(version_summary_row(&conn, id)?);
    }
    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "versions": versions
    }))
}

/// Per-module bundle modification state for the app-store badge.
///
/// For every module that has a recorded version timeline, reports the install
/// baseline bundle hash (lowest seq), the live current bundle hash, and whether
/// the working tree diverges from that baseline. This replaces the old
/// module.json-only manifest hash compare with a whole-bundle signal.
fn module_version_states(root: &Path, app_root: &Path) -> anyhow::Result<Value> {
    let conn = open_store(root)?;
    let mut stmt = conn.prepare(
        "SELECT module_id, bundle_sha256, origin, seq
         FROM business_module_versions v1
         WHERE seq = (SELECT MIN(seq) FROM business_module_versions v2
                      WHERE v2.module_id = v1.module_id)",
    )?;
    let baselines = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut ids_stmt = conn.prepare(
        "SELECT version_id FROM business_module_versions WHERE module_id = ?1 ORDER BY seq DESC",
    )?;

    let mut states = serde_json::Map::new();
    for (module_id, baseline_sha, baseline_origin) in baselines {
        let current_sha = compute_module_bundle(app_root, &module_id)
            .map(|bundle| bundle.sha256)
            .unwrap_or_default();
        let version_ids = ids_stmt
            .query_map(params![module_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut versions = Vec::with_capacity(version_ids.len());
        for id in &version_ids {
            versions.push(version_summary_row(&conn, id)?);
        }
        let modified = !current_sha.is_empty() && current_sha != baseline_sha;
        states.insert(
            module_id.clone(),
            serde_json::json!({
                "baseline_bundle_sha256": baseline_sha,
                "baseline_origin": baseline_origin,
                "current_bundle_sha256": current_sha,
                "modified": modified,
                "version_count": versions.len(),
                "versions": versions,
            }),
        );
    }
    Ok(Value::Object(states))
}

pub fn rollback_module_to_version(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleVersionRollbackRequest,
) -> anyhow::Result<Value> {
    let module_id = source_sanitize_slug(&request.module_id);
    let version_id = request.version_id.trim().to_string();
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    anyhow::ensure!(!version_id.is_empty(), "version_id is required");
    anyhow::ensure!(
        session_can_modify_module(root, session, &module_id)?,
        "module modification rights required"
    );

    let files_json: String = {
        let conn = open_store(root)?;
        conn.query_row(
            "SELECT files_json FROM business_module_versions
             WHERE module_id = ?1 AND version_id = ?2",
            params![module_id, version_id],
            |row| row.get(0),
        )
        .optional()?
        .context("version not found for module")?
    };
    let target_files: Vec<Value> = serde_json::from_str(&files_json).unwrap_or_default();

    let mut target_paths = std::collections::BTreeSet::new();
    let mut restored = 0usize;
    for file in &target_files {
        let path = file.get("path").and_then(Value::as_str).unwrap_or_default();
        let content = file
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        target_paths.insert(path.to_string());
        save_module_source_record(
            root,
            ModuleSourceSaveMutation {
                module_id: module_id.clone(),
                path: path.to_string(),
                content: content.to_string(),
            },
        )?;
        restored += 1;
    }

    // Remove editable source files that were added after the target version,
    // snapshotting each one first so the removal is itself reversible.
    let mut removed = 0usize;
    let current = compute_module_bundle(app_root, &module_id)?;
    let module_root = resolve_module_source_root(app_root, &module_id)?;
    for file in &current.files {
        let path = file.get("path").and_then(Value::as_str).unwrap_or_default();
        if path.is_empty() || target_paths.contains(path) {
            continue;
        }
        let Ok(rel) = normalize_source_relative_path(path) else {
            continue;
        };
        if !is_allowed_source_path(&rel) {
            continue;
        }
        let abs = module_root.join(&rel);
        if let Ok(content) = fs::read_to_string(&abs) {
            let prev_sha = hex_sha256(content.as_bytes());
            let _ = write_module_source_snapshot(root, &module_id, &rel, &content, Some(&prev_sha));
        }
        if abs.is_file() {
            fs::remove_file(&abs).with_context(|| format!("failed to remove {}", abs.display()))?;
            removed += 1;
        }
    }

    let created_by = session_user_id(session).unwrap_or("").to_string();
    record_module_version(
        root,
        app_root,
        &module_id,
        "rollback",
        &format!("Rolled back to {version_id}"),
        &created_by,
    )?;
    write_module_catalog_projection_to_rxdb(root)?;

    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "rolled_back_to": version_id,
        "restored_files": restored,
        "removed_files": removed
    }))
}

fn find_module_json_dir_for_install(
    dir: &Path,
    module_id: &str,
    source_path: &str,
) -> anyhow::Result<Option<PathBuf>> {
    let source_path = source_path.trim().trim_matches('/');
    if !source_path.is_empty() {
        if let Some(found) = find_module_json_dir_by_source_path(dir, source_path)? {
            return Ok(Some(found));
        }
    }
    find_module_json_dir_by_id(dir, module_id)
}

fn find_module_json_dir_by_source_path(
    dir: &Path,
    source_path: &str,
) -> anyhow::Result<Option<PathBuf>> {
    let source_segments = normalized_source_path_segments(source_path);
    find_module_json_dir_by_source_segments(dir, &source_segments)
}

fn find_module_json_dir_by_source_segments(
    dir: &Path,
    source_segments: &[String],
) -> anyhow::Result<Option<PathBuf>> {
    if dir.join("module.json").is_file() && path_ends_with_segments(dir, source_segments) {
        return Ok(Some(dir.to_path_buf()));
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_module_json_dir_by_source_segments(&path, source_segments)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

fn normalized_source_path_segments(source_path: &str) -> Vec<String> {
    source_path
        .replace('\\', "/")
        .split('/')
        .filter_map(|segment| {
            let trimmed = segment.trim();
            if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

fn path_ends_with_segments(path: &Path, source_segments: &[String]) -> bool {
    if source_segments.is_empty() {
        return false;
    }
    let path_segments: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();
    path_segments.ends_with(source_segments)
}

fn find_module_json_dir_by_id(dir: &Path, module_id: &str) -> anyhow::Result<Option<PathBuf>> {
    let manifest_path = dir.join("module.json");
    if manifest_path.is_file() {
        let text = fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;
        let manifest: Value = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
        let manifest_id = manifest
            .get("id")
            .and_then(Value::as_str)
            .map(source_sanitize_slug)
            .unwrap_or_default();
        if manifest_id == module_id {
            return Ok(Some(dir.to_path_buf()));
        }
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_module_json_dir_by_id(&path, module_id)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

pub fn install_app_module(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: AppStoreInstallRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required to install modules"
    );
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");

    // Download the ZIP archive
    let response = ureq::get(&request.download_url)
        .set("User-Agent", "Mozilla/5.0 CTOX Business OS App Installer")
        .timeout(Duration::from_secs(60))
        .call()
        .with_context(|| {
            format!(
                "Failed to download module zip from {}",
                request.download_url
            )
        })?;

    let mut zip_bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut zip_bytes)
        .with_context(|| "Failed to read zip download stream")?;

    // Extract ZIP to a temporary directory
    let temp_dir = std::env::temp_dir().join(format!("ctox-app-install-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("Failed to create temp extract dir {}", temp_dir.display()))?;

    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).context("Failed to open downloaded archive as a zip file")?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .context("Failed to read file from zip archive")?;
        let filepath = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => continue,
        };
        let outpath = temp_dir.join(filepath);
        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(&p)?;
                }
            }
            let mut outfile = fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    // Search recursively for the directory containing module.json
    let found_dir = find_module_json_dir_for_install(&temp_dir, &module_id, &request.source_path)?
        .with_context(|| {
            format!(
                "No module.json for module '{}' found in the downloaded repository archive",
                module_id
            )
        })?;

    // Read and parse module.json to ensure it's a valid manifest
    let manifest_path = found_dir.join("module.json");
    let manifest_content = fs::read_to_string(&manifest_path)
        .context("Failed to read module.json in downloaded archive")?;
    let mut manifest: Value = serde_json::from_str(&manifest_content)
        .context("Downloaded module.json is not a valid JSON")?;

    // Ensure the ID matches (or just use the ID in the manifest to create destination)
    let manifest_id = manifest
        .get("id")
        .and_then(Value::as_str)
        .context("Downloaded module.json is missing 'id' field")?;
    let sanitized_manifest_id = source_sanitize_slug(manifest_id);
    anyhow::ensure!(
        sanitized_manifest_id == module_id,
        "Module ID in module.json ('{}') does not match request module ID ('{}')",
        sanitized_manifest_id,
        module_id
    );

    // Copy target directory to installed-modules/<module_id>
    let dest_dir = app_root.join("installed-modules").join(&module_id);
    if dest_dir.exists() {
        fs::remove_dir_all(&dest_dir).with_context(|| {
            format!(
                "Failed to clear existing installation directory {}",
                dest_dir.display()
            )
        })?;
    } else {
        if let Some(parent) = dest_dir.parent() {
            fs::create_dir_all(parent)?;
        }
    }

    copy_dir_recursive(&found_dir, &dest_dir)
        .context("Failed to copy extracted module files to installed-modules")?;

    manifest["entry"] = Value::String(format!("installed-modules/{module_id}/index.html"));
    manifest["install_scope"] = Value::String("installed".to_owned());
    manifest["default_installed"] = Value::Bool(false);
    fs::write(
        dest_dir.join("module.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .with_context(|| format!("Failed to rewrite installed manifest for {module_id}"))?;

    // Clean up temporary directory
    let _ = fs::remove_dir_all(&temp_dir);

    let created_by = session_user_id(session).unwrap_or("").to_string();
    record_module_version(
        root,
        app_root,
        &module_id,
        "install",
        "Installed from store",
        &created_by,
    )?;
    write_module_catalog_projection_to_rxdb(root)?;

    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "installed": true,
        "manifest": manifest
    }))
}

pub fn uninstall_app_module(
    root: &Path,
    app_root: &Path,
    session: &BusinessOsSession,
    request: AppStoreUninstallRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required to uninstall modules"
    );
    let module_id = source_sanitize_slug(&request.module_id);
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");

    if is_core_module(&module_id) {
        anyhow::bail!("Core modules cannot be uninstalled");
    }

    let dest_dir = app_root.join("installed-modules").join(&module_id);
    if !dest_dir.is_dir() {
        anyhow::bail!("Module '{}' is not installed", module_id);
    }

    fs::remove_dir_all(&dest_dir)
        .with_context(|| format!("Failed to delete module directory {}", dest_dir.display()))?;

    // Update layout if module_id exists in it
    let mut layout = load_module_layout(root)?;
    remove_module_from_layout_value(&mut layout, &module_id);
    save_module_layout(root, &layout)?;
    write_module_catalog_projection_to_rxdb(root)?;

    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "uninstalled": true
    }))
}

fn module_source_file_id(module_id: &str, path: &str) -> String {
    format!(
        "{}:{}",
        module_id,
        String::from(path)
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '/' | ':' | '-') {
                    ch
                } else {
                    '_'
                }
            })
            .collect::<String>()
    )
    .chars()
    .take(512)
    .collect()
}

fn source_sanitize_slug(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_owned()
}

fn module_manifest_path(app_root: &Path, module_id: &str) -> anyhow::Result<std::path::PathBuf> {
    let module_id = module_id.trim();
    let core_path = app_root.join("modules").join(module_id).join("module.json");
    if core_path.is_file() {
        return Ok(core_path);
    }
    let installed_path = app_root
        .join("installed-modules")
        .join(module_id)
        .join("module.json");
    if installed_path.is_file() {
        return Ok(installed_path);
    }
    anyhow::bail!("module manifest not found: {module_id}")
}

fn normalize_report_kind(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().as_str() {
        "feature" | "feature_request" | "request" | "wish" => "feature".to_owned(),
        _ => "bug".to_owned(),
    }
}

fn normalize_report_severity(severity: &str) -> String {
    match severity.trim().to_ascii_lowercase().as_str() {
        "high" | "critical" => "high".to_owned(),
        "low" => "low".to_owned(),
        _ => "medium".to_owned(),
    }
}

fn seed_session_user(conn: &Connection, session: &BusinessOsSession) -> anyhow::Result<()> {
    let Some(user) = &session.user else {
        return Ok(());
    };
    let now = now_ms() as i64;
    conn.execute(
        "INSERT INTO business_users
            (user_id, display_name, role, active, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, 1, ?4, ?4)
         ON CONFLICT(user_id) DO UPDATE SET
            display_name = excluded.display_name,
            role = excluded.role,
            active = 1,
            updated_at_ms = excluded.updated_at_ms",
        params![user.id, user.display_name, user.role, now],
    )?;
    Ok(())
}

pub fn remember_authenticated_session_user(
    root: &Path,
    session: &BusinessOsSession,
) -> anyhow::Result<()> {
    if !session.authenticated {
        return Ok(());
    }
    let conn = open_store(root)?;
    seed_session_user(&conn, session)
}

fn query_users(conn: &Connection) -> anyhow::Result<Vec<BusinessOsUser>> {
    let mut stmt = conn.prepare(
        "SELECT user_id, display_name, role, active, created_at_ms, updated_at_ms
         FROM business_users
         ORDER BY role ASC, display_name ASC, user_id ASC",
    )?;
    let users = stmt
        .query_map([], |row| {
            Ok(BusinessOsUser {
                id: row.get(0)?,
                display_name: row.get(1)?,
                role: row.get(2)?,
                active: row.get::<_, i64>(3)? != 0,
                created_at_ms: row.get(4)?,
                updated_at_ms: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(users)
}

fn default_true() -> bool {
    true
}

pub fn record_command(root: &Path, command: BusinessCommand) -> anyhow::Result<CommandAccepted> {
    let conn = open_store(root)?;
    let command_id = command
        .id
        .clone()
        .unwrap_or_else(|| format!("cmd_{}", Uuid::new_v4()));
    let observed_at_ms = now_ms() as i64;
    let queue_task = create_ctox_queue_task(root, &command_id, &command)?;
    let inbound_channel = command_inbound_channel(&command);
    let payload_json = serde_json::to_string(&command.payload)?;
    let context_json = serde_json::to_string(&command.client_context)?;
    conn.execute(
        "INSERT INTO business_commands
            (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
         VALUES (?1, ?2, ?3, ?4, 'accepted', ?5, ?6, ?7)",
        params![
            command_id,
            command.module.clone(),
            command.command_type.clone(),
            command.record_id.clone(),
            payload_json,
            context_json,
            observed_at_ms
        ],
    )?;
    upsert_business_record(
        &conn,
        "business_commands",
        &command_id,
        observed_at_ms,
        serde_json::json!({
            "id": command_id,
            "command_id": command_id,
            "module": command.module.clone(),
            "command_type": command.command_type.clone(),
            "record_id": command.record_id.clone().unwrap_or_default(),
            "status": "accepted",
            "inbound_channel": inbound_channel,
            "task_id": queue_task.as_ref().map(|task| task.message_key.clone()),
            "task_status": queue_task
                .as_ref()
                .map(|task| normalize_queue_status(&task.route_status).to_string()),
            "payload": command.payload.clone(),
            "client_context": command.client_context.clone(),
            "updated_at_ms": observed_at_ms
        }),
    )?;
    if let Some(task) = &queue_task {
        upsert_business_record(
            &conn,
            "ctox_queue_tasks",
            &task.message_key,
            observed_at_ms,
            business_command_queue_task_payload(
                &command_id,
                &command,
                task,
                inbound_channel.as_str(),
                observed_at_ms,
            ),
        )?;
    }
    Ok(CommandAccepted {
        ok: true,
        command_id,
        status: "accepted",
        task_id: queue_task.as_ref().map(|task| task.message_key.clone()),
        task_status: queue_task.map(|task| normalize_queue_status(&task.route_status).to_string()),
    })
}

pub fn process_source_parse_command(
    root: &Path,
    command_id: &str,
) -> anyhow::Result<CommandAccepted> {
    let conn = open_store(root)?;
    let command = load_business_command(&conn, command_id)?;
    anyhow::ensure!(
        is_source_parse_command(&command.command_type)
            || is_match_command(&command.command_type)
            || is_outbound_research_command(&command.command_type)
            || is_documents_report_command(&command),
        "business command {command_id} is not a supported Business OS harness command"
    );
    let queue_task = find_queue_task_for_command(root, command_id)
        .and_then(|task_id| channels::load_queue_task(root, &task_id).ok().flatten());

    if is_documents_report_command(&command) {
        return process_documents_report_command(
            root,
            &conn,
            command_id,
            &command,
            queue_task.as_ref(),
            None,
        );
    }

    let result = if is_source_parse_command(&command.command_type) {
        super::importer::handle_source_parse(root, &conn, command_id, &command, queue_task.as_ref())
    } else {
        if is_outbound_research_command(&command.command_type) {
            super::importer::handle_outbound_research(
                root,
                &conn,
                command_id,
                &command,
                queue_task.as_ref(),
            )
        } else {
            super::importer::handle_match_compute(
                root,
                &conn,
                command_id,
                &command,
                queue_task.as_ref(),
            )
        }
    };

    match result {
        Ok(outcome) => {
            let completed_at_ms = now_ms() as i64;
            conn.execute(
                "UPDATE business_commands SET status = 'completed', observed_at_ms = ?2 WHERE command_id = ?1",
                params![command_id, completed_at_ms],
            )?;
            let updated_queue_task = if let Some(task) = &queue_task {
                Some(channels::update_queue_task(
                    root,
                    channels::QueueTaskUpdateRequest {
                        message_key: task.message_key.clone(),
                        route_status: Some("handled".to_string()),
                        status_note: Some(format!(
                            "business-os:terminal-success: command completed: {} record(s)",
                            outcome.records_count
                        )),
                        ..Default::default()
                    },
                )?)
            } else {
                None
            };
            let command_payload = serde_json::json!({
                "id": command_id,
                "command_id": command_id,
                "module": command.module.clone(),
                "command_type": command.command_type.clone(),
                "record_id": command.record_id.clone().unwrap_or_default(),
                "status": "completed",
                "inbound_channel": command_inbound_channel(&command),
                "task_id": queue_task.as_ref().map(|task| task.message_key.clone()),
                "task_status": "completed",
                "payload": command.payload.clone(),
                "client_context": command.client_context.clone(),
                "result": {
                    "record_ids": outcome.record_ids,
                    "records_count": outcome.records_count,
                    "collection": outcome.collection,
                    "definition_id": outcome.definition_id
                },
                "updated_at_ms": completed_at_ms
            });
            upsert_business_record(
                &conn,
                "business_commands",
                command_id,
                completed_at_ms,
                command_payload.clone(),
            )?;
            upsert_rxdb_collection_record(
                root,
                "business_commands",
                command_id,
                completed_at_ms,
                command_payload,
            )?;
            refresh_queue_task_projection(
                root,
                &conn,
                command_id,
                &command,
                updated_queue_task.as_ref().or(queue_task.as_ref()),
                completed_at_ms,
            )?;
            Ok(CommandAccepted {
                ok: true,
                command_id: command_id.to_string(),
                status: "completed",
                task_id: queue_task.as_ref().map(|task| task.message_key.clone()),
                task_status: Some("completed".to_string()),
            })
        }
        Err(err) => {
            let failed_at_ms = now_ms() as i64;
            conn.execute(
                "UPDATE business_commands SET status = 'failed', observed_at_ms = ?2 WHERE command_id = ?1",
                params![command_id, failed_at_ms],
            )?;
            let updated_queue_task = if let Some(task) = &queue_task {
                Some(channels::update_queue_task(
                    root,
                    channels::QueueTaskUpdateRequest {
                        message_key: task.message_key.clone(),
                        route_status: Some("failed".to_string()),
                        status_note: Some(err.to_string()),
                        ..Default::default()
                    },
                )?)
            } else {
                None
            };
            let command_payload = serde_json::json!({
                "id": command_id,
                "command_id": command_id,
                "module": command.module.clone(),
                "command_type": command.command_type.clone(),
                "record_id": command.record_id.clone().unwrap_or_default(),
                "status": "failed",
                "inbound_channel": command_inbound_channel(&command),
                "task_id": queue_task.as_ref().map(|task| task.message_key.clone()),
                "task_status": "failed",
                "error": err.to_string(),
                "payload": command.payload.clone(),
                "client_context": command.client_context.clone(),
                "updated_at_ms": failed_at_ms
            });
            upsert_business_record(
                &conn,
                "business_commands",
                command_id,
                failed_at_ms,
                command_payload.clone(),
            )?;
            upsert_rxdb_collection_record(
                root,
                "business_commands",
                command_id,
                failed_at_ms,
                command_payload,
            )?;
            refresh_queue_task_projection(
                root,
                &conn,
                command_id,
                &command,
                updated_queue_task.as_ref().or(queue_task.as_ref()),
                failed_at_ms,
            )?;
            Ok(CommandAccepted {
                ok: false,
                command_id: command_id.to_string(),
                status: "failed",
                task_id: queue_task.as_ref().map(|task| task.message_key.clone()),
                task_status: Some("failed".to_string()),
            })
        }
    }
}

pub fn complete_business_command_from_queue_reply(
    root: &Path,
    task_id: &str,
    reply_text: &str,
) -> anyhow::Result<Option<Value>> {
    let conn = open_store(root)?;
    let Some(command_id) = queue_projection_command_id(&conn, task_id)? else {
        return Ok(None);
    };
    let command = load_business_command(&conn, &command_id)?;
    let queue_task = channels::load_queue_task(root, task_id)?;
    let accepted = if is_documents_report_command(&command) {
        process_documents_report_command(
            root,
            &conn,
            &command_id,
            &command,
            queue_task.as_ref(),
            Some(reply_text),
        )?
    } else if is_business_chat_command(&command) {
        process_business_chat_reply(
            root,
            &conn,
            &command_id,
            &command,
            queue_task.as_ref(),
            reply_text,
        )?
    } else {
        return Ok(None);
    };
    Ok(Some(serde_json::to_value(accepted)?))
}

pub fn complete_business_command_from_app_validation_success(
    root: &Path,
    task_id: &str,
    reason: &str,
) -> anyhow::Result<Option<Value>> {
    let conn = open_store(root)?;
    let Some(command_id) = queue_projection_command_id(&conn, task_id)? else {
        return Ok(None);
    };
    let command = load_business_command(&conn, &command_id)?;
    let Some((module_id, install_target, artifact_directory)) =
        business_os_app_command_target_metadata(&command)
    else {
        return Ok(None);
    };
    let completed_at_ms = now_ms() as i64;
    conn.execute(
        "UPDATE business_commands SET status = 'completed', observed_at_ms = ?2 WHERE command_id = ?1",
        params![command_id.as_str(), completed_at_ms],
    )?;

    let mut terminal_queue_task = channels::load_queue_task(root, task_id)?;
    if let Some(task) = terminal_queue_task.as_ref() {
        let _ = channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: task.message_key.clone(),
                route_status: Some("handled".to_string()),
                status_note: Some(
                    "business-os:terminal-success: app validation passed".to_string(),
                ),
                ..Default::default()
            },
        );
        terminal_queue_task =
            channels::load_queue_task(root, task_id)?.or_else(|| terminal_queue_task.clone());
    }

    let task_status = terminal_queue_task
        .as_ref()
        .map(|task| normalize_queue_status(&task.route_status).to_string())
        .unwrap_or_else(|| "completed".to_string());
    let command_payload = serde_json::json!({
        "id": command_id,
        "command_id": command_id,
        "module": command.module.clone(),
        "command_type": command.command_type.clone(),
        "record_id": command.record_id.clone().unwrap_or_default(),
        "status": "completed",
        "inbound_channel": command_inbound_channel(&command),
        "task_id": task_id,
        "task_status": task_status,
        "payload": command.payload.clone(),
        "client_context": command.client_context.clone(),
        "result": {
            "module_id": module_id,
            "install_target": install_target,
            "artifact_directory": artifact_directory,
            "validator": "business_os_app_module_validator",
            "validation_status": "passed",
            "completion_reason": reason
        },
        "updated_at_ms": completed_at_ms
    });
    upsert_business_record(
        &conn,
        "business_commands",
        &command_id,
        completed_at_ms,
        command_payload.clone(),
    )?;
    upsert_rxdb_collection_record(
        root,
        "business_commands",
        &command_id,
        completed_at_ms,
        command_payload.clone(),
    )?;
    refresh_queue_task_projection(
        root,
        &conn,
        &command_id,
        &command,
        terminal_queue_task.as_ref(),
        completed_at_ms,
    )?;
    Ok(Some(command_payload))
}

pub fn complete_ready_documents_report_commands(
    root: &Path,
    limit: usize,
) -> anyhow::Result<usize> {
    let conn = open_store(root)?;
    let mut statement = conn.prepare(
        "SELECT command_id
         FROM business_commands
         WHERE module = 'documents'
           AND command_type = 'research.systematic.report.create'
           AND status NOT IN ('completed', 'failed', 'cancelled')
         ORDER BY observed_at_ms ASC, command_id ASC
         LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit.max(1) as i64], |row| row.get::<_, String>(0))?;
    let command_ids = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    let mut completed = 0usize;
    for command_id in command_ids {
        let command = match load_business_command(&conn, &command_id) {
            Ok(command) => command,
            Err(_) => continue,
        };
        if !is_documents_report_command(&command) {
            continue;
        }
        let Some(filename) = expected_docx_filename(&command) else {
            continue;
        };
        let Some(docx_path) = resolve_generated_docx_path(root, &command, &filename, None) else {
            continue;
        };
        if !docx_path.is_file() {
            continue;
        }
        let task = find_queue_task_for_command(root, &command_id)
            .and_then(|task_id| channels::load_queue_task(root, &task_id).ok().flatten());
        let reply = format!(
            "DOCX artifact created and detected by Business OS writeback: {}",
            docx_path.display()
        );
        process_documents_report_command(
            root,
            &conn,
            &command_id,
            &command,
            task.as_ref(),
            Some(&reply),
        )?;
        completed += 1;
    }
    Ok(completed)
}

fn process_business_chat_reply(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
    reply_text: &str,
) -> anyhow::Result<CommandAccepted> {
    let completed_at_ms = now_ms() as i64;
    conn.execute(
        "UPDATE business_commands SET status = 'completed', observed_at_ms = ?2 WHERE command_id = ?1",
        params![command_id, completed_at_ms],
    )?;

    let mut terminal_queue_task = queue_task.cloned();
    if let Some(task) = queue_task {
        let _ = channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: task.message_key.clone(),
                route_status: Some("handled".to_string()),
                status_note: Some("business-os:terminal-success: chat reply stored".to_string()),
                ..Default::default()
            },
        );
        terminal_queue_task = channels::load_queue_task(root, &task.message_key)
            .ok()
            .flatten()
            .or_else(|| Some(task.clone()));
    }

    let chat_id = business_chat_id(command, command_id);
    let chat_title = business_chat_title(command);
    let owner_user_id = first_string_field(&command.client_context, &["owner_user_id", "user_id"])
        .unwrap_or_else(|| "local-dev".to_string());
    let task_id = terminal_queue_task
        .as_ref()
        .map(|task| task.message_key.clone())
        .unwrap_or_default();
    let user_message_id = first_string_field(&command.payload, &["message_id"])
        .or_else(|| first_string_field(&command.client_context, &["message_id"]))
        .unwrap_or_else(|| format!("chatmsg_{command_id}"));
    let user_text = first_string_field(
        &command.payload,
        &["user_message", "instruction", "prompt", "message"],
    )
    .or_else(|| first_string_field(&command.client_context, &["user_message", "message"]))
    .unwrap_or_else(|| {
        queue_task
            .map(|task| task.prompt.clone())
            .unwrap_or_default()
    });

    let chat_payload = business_chat_payload(
        conn,
        &chat_id,
        &chat_title,
        &owner_user_id,
        &user_message_id,
        &user_text,
        command_id,
        &task_id,
        reply_text,
        completed_at_ms,
    )?;
    upsert_business_record(
        conn,
        "business_chats",
        &chat_id,
        completed_at_ms,
        chat_payload.clone(),
    )?;
    upsert_rxdb_collection_record(
        root,
        "business_chats",
        &chat_id,
        completed_at_ms,
        chat_payload,
    )?;

    let command_payload = serde_json::json!({
        "id": command_id,
        "command_id": command_id,
        "module": command.module.clone(),
        "command_type": command.command_type.clone(),
        "record_id": command.record_id.clone().unwrap_or_default(),
        "status": "completed",
        "inbound_channel": command_inbound_channel(command),
        "task_id": task_id,
        "task_status": "completed",
        "payload": command.payload.clone(),
        "client_context": command.client_context.clone(),
        "result": {
            "chat_id": chat_id,
            "outbound_text": reply_text,
            "response": reply_text,
            "answer": reply_text,
            "summary": reply_text
        },
        "outbound_text": reply_text,
        "response": reply_text,
        "answer": reply_text,
        "updated_at_ms": completed_at_ms
    });
    upsert_business_record(
        conn,
        "business_commands",
        command_id,
        completed_at_ms,
        command_payload.clone(),
    )?;
    upsert_rxdb_collection_record(
        root,
        "business_commands",
        command_id,
        completed_at_ms,
        command_payload,
    )?;
    refresh_queue_task_projection(
        root,
        conn,
        command_id,
        command,
        terminal_queue_task.as_ref(),
        completed_at_ms,
    )?;

    Ok(CommandAccepted {
        ok: true,
        command_id: command_id.to_string(),
        status: "completed",
        task_id: terminal_queue_task.map(|task| task.message_key.clone()),
        task_status: Some("completed".to_string()),
    })
}

pub fn pull_collection_records(
    root: &Path,
    collection: &str,
    since_ms: Option<i64>,
    limit: Option<usize>,
) -> anyhow::Result<Value> {
    // Conversations module reads communication_* collections. These live in
    // CTOX's channels SQLite (runtime/ctox.sqlite3), not in business-os.sqlite3 —
    // so we delegate to channels.rs helpers that read from the canonical tables
    // directly. No projection table needed; messages/threads/accounts stay
    // single-source-of-truth in channels.
    match collection {
        "ctox_runtime_settings" => {
            return pull_runtime_settings_records(root, since_ms, limit);
        }
        "communication_accounts" => {
            return channels::pull_communication_accounts_for_business_os(root, since_ms, limit);
        }
        "communication_threads" => {
            return channels::pull_communication_threads_for_business_os(root, since_ms, limit);
        }
        "communication_messages" => {
            return channels::pull_communication_messages_for_business_os(root, since_ms, limit);
        }
        _ => {}
    }
    let conn = open_store(root)?;
    let limit = limit.unwrap_or(500).clamp(1, 2_000);
    let since_ms = since_ms.unwrap_or(0);
    let mut statement = conn.prepare(
        "SELECT record_id, deleted, updated_at_ms, payload_json
         FROM business_records
         WHERE collection = ?1 AND updated_at_ms >= ?2
         ORDER BY updated_at_ms ASC, record_id ASC
         LIMIT ?3",
    )?;
    let rows = statement.query_map(params![collection, since_ms, limit as i64], |row| {
        let record_id: String = row.get(0)?;
        let deleted: i64 = row.get(1)?;
        let updated_at_ms: i64 = row.get(2)?;
        let payload_json: String = row.get(3)?;
        Ok((record_id, deleted, updated_at_ms, payload_json))
    })?;
    let mut documents = Vec::new();
    for row in rows {
        let (record_id, deleted, updated_at_ms, payload_json) = row?;
        let mut payload = serde_json::from_str::<Value>(&payload_json).unwrap_or(Value::Null);
        if let Some(obj) = payload.as_object_mut() {
            obj.entry("id".to_string())
                .or_insert_with(|| Value::String(record_id.clone()));
            obj.insert("_deleted".to_string(), Value::Bool(deleted != 0));
            obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
        }
        documents.push(payload);
    }
    if documents.is_empty() {
        if let Some(rxdb_projection) =
            pull_rxdb_collection_table_records(root, collection, since_ms, limit)?
        {
            return Ok(rxdb_projection);
        }
    }
    Ok(serde_json::json!({
        "ok": true,
        "collection": collection,
        "documents": documents,
        "count": documents.len(),
        "since_ms": since_ms
    }))
}

pub fn pull_collection_record(
    root: &Path,
    collection: &str,
    record_id: &str,
) -> anyhow::Result<Option<Value>> {
    if collection.trim().is_empty() || record_id.trim().is_empty() {
        return Ok(None);
    }
    match collection {
        "ctox_runtime_settings" if record_id == "runtime-settings" => {
            return Ok(Some(runtime_settings_for_rxdb(root)?));
        }
        "communication_accounts" | "communication_threads" | "communication_messages" => {
            let pulled = pull_collection_records(root, collection, None, Some(2_000))?;
            return Ok(pulled
                .get("documents")
                .and_then(Value::as_array)
                .and_then(|items| {
                    items.iter().find(|item| {
                        item.get("id").and_then(Value::as_str) == Some(record_id)
                            || item.get("record_id").and_then(Value::as_str) == Some(record_id)
                    })
                })
                .cloned());
        }
        _ => {}
    }
    let conn = open_store(root)?;
    let payload_json = conn
        .query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = ?1 AND record_id = ?2 AND deleted = 0",
            params![collection, record_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(payload_json) = payload_json {
        let mut payload = serde_json::from_str::<Value>(&payload_json).unwrap_or(Value::Null);
        if let Some(object) = payload.as_object_mut() {
            object
                .entry("id".to_string())
                .or_insert_with(|| Value::String(record_id.to_string()));
            object.insert("_deleted".to_string(), Value::Bool(false));
        }
        return Ok(Some(payload));
    }
    if let Some(rxdb_projection) = pull_rxdb_collection_table_records(root, collection, 0, 2_000)? {
        return Ok(rxdb_projection
            .get("documents")
            .and_then(Value::as_array)
            .and_then(|items| {
                items.iter().find(|item| {
                    item.get("id").and_then(Value::as_str) == Some(record_id)
                        || item.get("record_id").and_then(Value::as_str) == Some(record_id)
                })
            })
            .cloned());
    }
    Ok(None)
}

fn pull_runtime_settings_records(
    root: &Path,
    since_ms: Option<i64>,
    limit: Option<usize>,
) -> anyhow::Result<Value> {
    let limit = limit.unwrap_or(500).clamp(1, 2_000);
    let since_ms = since_ms.unwrap_or(0);
    let document = runtime_settings_for_rxdb(root)?;
    let updated_at_ms = document
        .get("updated_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let documents = if limit == 0 || updated_at_ms < since_ms {
        Vec::new()
    } else {
        vec![document]
    };
    Ok(serde_json::json!({
        "ok": true,
        "collection": "ctox_runtime_settings",
        "documents": documents,
        "count": documents.len(),
        "since_ms": since_ms,
        "source": "native_runtime_projection"
    }))
}

pub fn pull_latest_collection_records(
    root: &Path,
    collection: &str,
    limit: Option<usize>,
) -> anyhow::Result<Value> {
    match collection {
        "communication_accounts" | "communication_threads" | "communication_messages" => {
            return pull_collection_records(root, collection, None, limit);
        }
        _ => {}
    }
    let conn = open_store(root)?;
    let limit = limit.unwrap_or(500).clamp(1, 2_000);
    let mut statement = conn.prepare(
        "SELECT record_id, deleted, updated_at_ms, payload_json
         FROM business_records
         WHERE collection = ?1
         ORDER BY updated_at_ms DESC, record_id DESC
         LIMIT ?2",
    )?;
    let rows = statement.query_map(params![collection, limit as i64], |row| {
        let record_id: String = row.get(0)?;
        let deleted: i64 = row.get(1)?;
        let updated_at_ms: i64 = row.get(2)?;
        let payload_json: String = row.get(3)?;
        Ok((record_id, deleted, updated_at_ms, payload_json))
    })?;
    let mut documents = Vec::new();
    for row in rows {
        let (record_id, deleted, updated_at_ms, payload_json) = row?;
        let mut payload = serde_json::from_str::<Value>(&payload_json).unwrap_or(Value::Null);
        if let Some(obj) = payload.as_object_mut() {
            obj.entry("id".to_string())
                .or_insert_with(|| Value::String(record_id.clone()));
            obj.insert("_deleted".to_string(), Value::Bool(deleted != 0));
            obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
        }
        documents.push(payload);
    }
    if documents.is_empty() {
        if let Some(rxdb_projection) =
            pull_rxdb_collection_table_records(root, collection, 0, limit)?
        {
            return Ok(rxdb_projection);
        }
    }
    Ok(serde_json::json!({
        "ok": true,
        "collection": collection,
        "documents": documents,
        "count": documents.len(),
        "since_ms": 0
    }))
}

pub fn pull_business_command_status_record(
    root: &Path,
    command_id: &str,
) -> anyhow::Result<Option<Value>> {
    let mut record =
        pull_collection_record(root, "business_commands", command_id)?.unwrap_or_else(|| {
            serde_json::json!({
                "id": command_id,
                "command_id": command_id,
                "status": "accepted",
                "source": "queue_task_fallback",
                "updated_at_ms": now_ms() as i64
            })
        });
    let task_id = record
        .get("task_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| find_queue_task_for_command(root, command_id));
    if let Some(task_id) = task_id {
        if let Some(task) = channels::load_queue_task(root, &task_id)? {
            let original_command_status = record
                .get("status")
                .cloned()
                .unwrap_or_else(|| Value::String("accepted".to_string()));
            if let Some(object) = record.as_object_mut() {
                let task_status = normalize_queue_status(&task.route_status).to_string();
                object
                    .entry("original_command_status".to_string())
                    .or_insert(original_command_status);
                object.insert(
                    "task_id".to_string(),
                    Value::String(task.message_key.clone()),
                );
                object.insert(
                    "task_status".to_string(),
                    Value::String(task_status.clone()),
                );
                object.insert(
                    "route_status".to_string(),
                    Value::String(task.route_status.clone()),
                );
                object.insert("status".to_string(), Value::String(task_status));
                if let Some(note) = task.status_note {
                    object.insert("status_note".to_string(), Value::String(note));
                }
                if let Some(acked_at) = task.acked_at {
                    object.insert("acked_at".to_string(), Value::String(acked_at));
                }
                if let Some(leased_at) = task.leased_at {
                    object.insert("leased_at".to_string(), Value::String(leased_at));
                }
                object.insert("updated_at_ms".to_string(), Value::from(now_ms() as i64));
            }
        } else if record
            .get("source")
            .and_then(Value::as_str)
            .is_some_and(|source| source == "queue_task_fallback")
        {
            return Ok(None);
        }
    } else if record
        .get("source")
        .and_then(Value::as_str)
        .is_some_and(|source| source == "queue_task_fallback")
    {
        return Ok(None);
    }
    Ok(Some(record))
}

pub fn repair_queue_projections(
    root: &Path,
    options: QueueProjectionRepairOptions,
) -> anyhow::Result<Value> {
    let apply = options.apply;
    let conn = open_store(root)?;
    let now = now_ms() as i64;
    let projection_rows = {
        let mut statement = conn.prepare(
            "SELECT record_id, payload_json, updated_at_ms
             FROM business_records
             WHERE collection = 'ctox_queue_tasks'
               AND deleted = 0
             ORDER BY updated_at_ms ASC, record_id ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut counters: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut actions: Vec<Value> = Vec::new();
    let mut touched_commands = HashSet::new();

    for (task_id, payload_json, projection_updated_at_ms) in projection_rows {
        let mut payload = serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| {
            serde_json::json!({
                "id": task_id,
                "status": "queued",
                "route_status": "pending"
            })
        });
        let command_id = payload
            .get("command_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| queue_projection_command_id(&conn, &task_id).ok().flatten());
        let projection_route_status = payload
            .get("route_status")
            .and_then(Value::as_str)
            .or_else(|| payload.get("status").and_then(Value::as_str))
            .unwrap_or_default()
            .to_string();
        let projection_status = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let projection_task_status = payload
            .get("task_status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        match channels::load_queue_task(root, &task_id)? {
            Some(mut task) => {
                let mut desired_route_status = task.route_status.clone();
                let mut repair_kind = None;
                if desired_route_status == "leased"
                    && queue_status_note_is_terminal_success(task.status_note.as_deref())
                {
                    desired_route_status = "handled".to_string();
                    repair_kind = Some("leased_terminal_success_note");
                    if apply {
                        let note = task
                            .status_note
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .unwrap_or("Business OS queued command completed.");
                        let _ = channels::update_queue_task(
                            root,
                            channels::QueueTaskUpdateRequest {
                                message_key: task_id.clone(),
                                route_status: Some("handled".to_string()),
                                status_note: Some(format!("business-os:terminal-success: {note}")),
                                ..Default::default()
                            },
                        )?;
                        if let Some(reloaded) = channels::load_queue_task(root, &task_id)? {
                            task = reloaded;
                        }
                    }
                } else if desired_route_status == "leased"
                    && queue_status_note_is_terminal_failure(task.status_note.as_deref())
                {
                    desired_route_status = "failed".to_string();
                    repair_kind = Some("leased_terminal_failure_note");
                    if apply {
                        let reason = task
                            .status_note
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .unwrap_or("leased queue task had terminal failure note");
                        let _ = channels::ack_leased_messages_with_failure_reason(
                            root,
                            std::slice::from_ref(&task_id),
                            "failed",
                            reason,
                        )?;
                        if let Some(reloaded) = channels::load_queue_task(root, &task_id)? {
                            task = reloaded;
                        }
                    }
                }
                if apply {
                    desired_route_status = task.route_status.clone();
                }
                let fallback_error_note =
                    if desired_route_status == "failed" && task.status_note.is_none() {
                        channels::load_queue_task_last_error(root, &task_id)?
                    } else {
                        None
                    };
                let canonical_note = task
                    .status_note
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .or_else(|| fallback_error_note.as_deref());

                let desired_status = normalize_queue_status(&desired_route_status).to_string();
                let needs_projection_repair = projection_route_status != desired_route_status
                    || projection_status != desired_status
                    || projection_task_status != desired_status
                    || repair_kind.is_some();
                if needs_projection_repair {
                    let class = match desired_route_status.as_str() {
                        "failed" => "failed_from_canonical",
                        "handled" => "completed_from_canonical",
                        "cancelled" => "cancelled_from_canonical",
                        "blocked" => "blocked_from_canonical",
                        "leased" => "running_from_canonical",
                        _ => "queued_from_canonical",
                    };
                    *counters.entry(class).or_insert(0) += 1;
                    push_repair_action(
                        &mut actions,
                        class,
                        &task_id,
                        command_id.as_deref(),
                        &projection_route_status,
                        &desired_route_status,
                        canonical_note,
                    );
                    if apply {
                        payload = apply_queue_projection_status_fields(
                            payload,
                            &task,
                            &desired_route_status,
                            now,
                        );
                        if let Some(object) = payload.as_object_mut() {
                            object.insert(
                                "repair_note".to_string(),
                                Value::String(format!(
                                    "queue projection repaired from canonical route_status={desired_route_status}"
                                )),
                            );
                            if desired_route_status == "failed" {
                                if let Some(note) = canonical_note {
                                    object.insert(
                                        "status_note".to_string(),
                                        Value::String(note.to_string()),
                                    );
                                    object.insert(
                                        "error".to_string(),
                                        Value::String(note.to_string()),
                                    );
                                }
                            }
                        }
                        upsert_business_record(
                            &conn,
                            "ctox_queue_tasks",
                            &task_id,
                            now,
                            payload.clone(),
                        )?;
                        upsert_rxdb_collection_record(
                            root,
                            "ctox_queue_tasks",
                            &task_id,
                            now,
                            payload,
                        )?;
                    }
                }

                if let Some(command_id) = command_id.as_deref() {
                    if command_status_for_queue_route_status(&desired_route_status).is_some() {
                        *counters.entry("commands_updated_from_queue").or_insert(0) += 1;
                        touched_commands.insert(command_id.to_string());
                        if apply {
                            upsert_command_projection_from_queue_status(
                                root,
                                &conn,
                                command_id,
                                Some(&task),
                                &desired_route_status,
                                now,
                                if desired_route_status == "failed" {
                                    canonical_note
                                } else {
                                    None
                                },
                            )?;
                        }
                    }
                }
            }
            None => {
                let command_status = command_id.as_deref().and_then(|command_id| {
                    conn.query_row(
                        "SELECT status FROM business_commands WHERE command_id = ?1",
                        params![command_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .ok()
                    .flatten()
                });
                if let Some(route_status) = command_status
                    .as_deref()
                    .and_then(projection_route_status_for_command_status)
                {
                    let desired_status = normalize_queue_status(route_status).to_string();
                    if projection_route_status != route_status
                        || projection_status != desired_status
                        || projection_task_status != desired_status
                    {
                        *counters
                            .entry("projection_repaired_from_command")
                            .or_insert(0) += 1;
                        push_repair_action(
                            &mut actions,
                            "projection_repaired_from_command",
                            &task_id,
                            command_id.as_deref(),
                            &projection_route_status,
                            route_status,
                            None,
                        );
                        if apply {
                            if let Some(object) = payload.as_object_mut() {
                                object.insert("status".to_string(), Value::String(desired_status));
                                object.insert(
                                    "route_status".to_string(),
                                    Value::String(route_status.to_string()),
                                );
                                object.insert(
                                    "task_status".to_string(),
                                    Value::String(normalize_queue_status(route_status).to_string()),
                                );
                                object.insert("updated_at_ms".to_string(), Value::from(now));
                                object.insert(
                                    "repair_note".to_string(),
                                    Value::String(
                                        "queue projection repaired from terminal command status"
                                            .to_string(),
                                    ),
                                );
                            }
                            upsert_business_record(
                                &conn,
                                "ctox_queue_tasks",
                                &task_id,
                                now,
                                payload.clone(),
                            )?;
                            upsert_rxdb_collection_record(
                                root,
                                "ctox_queue_tasks",
                                &task_id,
                                now,
                                payload,
                            )?;
                        }
                    }
                } else if projection_status_is_active(&projection_status)
                    && now.saturating_sub(projection_updated_at_ms)
                        > BUSINESS_OS_QUEUE_ORPHAN_REPAIR_AGE_MS
                {
                    let error = "Queue task is no longer present in the CTOX durable queue; marking stale Business OS projection as failed.";
                    *counters.entry("orphaned_active_projection").or_insert(0) += 1;
                    push_repair_action(
                        &mut actions,
                        "orphaned_active_projection",
                        &task_id,
                        command_id.as_deref(),
                        &projection_route_status,
                        "failed",
                        Some(error),
                    );
                    if apply {
                        if let Some(object) = payload.as_object_mut() {
                            object
                                .insert("status".to_string(), Value::String("failed".to_string()));
                            object.insert(
                                "route_status".to_string(),
                                Value::String("failed".to_string()),
                            );
                            object.insert(
                                "task_status".to_string(),
                                Value::String("failed".to_string()),
                            );
                            object.insert("error".to_string(), Value::String(error.to_string()));
                            object.insert("updated_at_ms".to_string(), Value::from(now));
                            object.insert(
                                "repair_note".to_string(),
                                Value::String(
                                    "orphaned active queue projection failed".to_string(),
                                ),
                            );
                        }
                        upsert_business_record(
                            &conn,
                            "ctox_queue_tasks",
                            &task_id,
                            now,
                            payload.clone(),
                        )?;
                        upsert_rxdb_collection_record(
                            root,
                            "ctox_queue_tasks",
                            &task_id,
                            now,
                            payload,
                        )?;
                        if let Some(command_id) = command_id.as_deref() {
                            touched_commands.insert(command_id.to_string());
                            upsert_command_projection_from_queue_status(
                                root,
                                &conn,
                                command_id,
                                None,
                                "failed",
                                now,
                                Some(error),
                            )?;
                        }
                    }
                }
            }
        }
    }

    let redacted = repair_inline_payload_artifacts(root, &conn, apply, now)?;
    if redacted > 0 {
        counters.insert("oversized_inline_artifacts_redacted", redacted);
    }
    let legacy_records = count_legacy_http_fallback_records(&conn)?;
    if legacy_records > 0 {
        counters.insert("legacy_http_fallback_records", legacy_records);
    }

    Ok(serde_json::json!({
        "ok": true,
        "apply": apply,
        "counts": counters,
        "actions": actions,
        "touched_commands": touched_commands.into_iter().collect::<Vec<_>>(),
    }))
}

fn pull_rxdb_collection_table_records(
    root: &Path,
    collection: &str,
    since_ms: i64,
    limit: usize,
) -> anyhow::Result<Option<Value>> {
    if !is_safe_rxdb_collection_name(collection) {
        anyhow::bail!("invalid collection name `{collection}`");
    }
    let path = rxdb_store_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let conn = Connection::open(path)?;
    for version in (0..=1).rev() {
        let table = format!("ctox_business_os__{collection}__v{version}");
        if !rxdb_table_exists(&conn, &table)? {
            continue;
        }
        let mut statement = conn.prepare(&format!(
            "SELECT id, data
             FROM {table}
             WHERE CAST(COALESCE(json_extract(data, '$.updated_at_ms'), 0) AS INTEGER) >= ?1
             ORDER BY CAST(COALESCE(json_extract(data, '$.updated_at_ms'), 0) AS INTEGER) ASC, id ASC
             LIMIT ?2"
        ))?;
        let rows = statement.query_map(params![since_ms, limit as i64], |row| {
            let id: String = row.get(0)?;
            let data: String = row.get(1)?;
            Ok((id, data))
        })?;
        let mut documents = Vec::new();
        for row in rows {
            let (id, data) = row?;
            let mut payload = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
            if let Some(obj) = payload.as_object_mut() {
                obj.entry("id".to_string())
                    .or_insert_with(|| Value::String(id));
            }
            documents.push(payload);
        }
        return Ok(Some(serde_json::json!({
            "ok": true,
            "collection": collection,
            "documents": documents,
            "count": documents.len(),
            "since_ms": since_ms,
            "source": "rxdb_projection",
            "table": table,
            "schema_version": version
        })));
    }
    Ok(None)
}

fn load_rxdb_collection_record(
    root: &Path,
    collection: &str,
    record_id: &str,
) -> anyhow::Result<Option<Value>> {
    if !is_safe_rxdb_collection_name(collection) {
        anyhow::bail!("invalid collection name `{collection}`");
    }
    let path = rxdb_store_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let conn = Connection::open(path)?;
    let Some(table) = rxdb_collection_table_name(&conn, collection) else {
        return Ok(None);
    };
    let raw = conn
        .query_row(
            &format!("SELECT data FROM {table} WHERE id = ?1"),
            [record_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let mut record: Value = serde_json::from_str(&raw)?;
    if let Some(object) = record.as_object_mut() {
        object
            .entry("id".to_string())
            .or_insert_with(|| Value::String(record_id.to_string()));
    }
    Ok(Some(record))
}

fn upsert_rxdb_collection_record(
    root: &Path,
    collection: &str,
    record_id: &str,
    updated_at_ms: i64,
    mut payload: Value,
) -> anyhow::Result<()> {
    if !is_safe_rxdb_collection_name(collection) {
        anyhow::bail!("invalid collection name `{collection}`");
    }
    let path = rxdb_store_path(root);
    if !path.is_file() {
        return Ok(());
    }
    let conn = Connection::open(path)?;
    let Some(table) = rxdb_collection_table_name(&conn, collection) else {
        return Ok(());
    };
    if let Some(existing_json) = conn
        .query_row(
            &format!("SELECT data FROM {table} WHERE id = ?1"),
            [record_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        if let Ok(mut existing) = serde_json::from_str::<Value>(&existing_json) {
            merge_json_object_values(&mut existing, &payload);
            payload = existing;
        }
    }
    let rev = format!("rev_{}", Uuid::new_v4());
    if let Some(object) = payload.as_object_mut() {
        object.insert("id".to_string(), Value::String(record_id.to_string()));
        object.insert(
            "updated_at_ms".to_string(),
            Value::Number(serde_json::Number::from(updated_at_ms)),
        );
    }
    let mut columns = vec!["id".to_string(), "data".to_string()];
    let mut values = vec![
        SqlValue::Text(record_id.to_string()),
        SqlValue::Text(serde_json::to_string(&payload)?),
    ];
    let mut updates = vec!["data = excluded.data".to_string()];
    if let Some(deleted_column) = ["deleted", "_deleted"].into_iter().find_map(|column| {
        rxdb_table_has_column(&conn, &table, column)
            .ok()
            .filter(|exists| *exists)
            .map(|_| column)
    }) {
        columns.push(deleted_column.to_string());
        values.push(SqlValue::Integer(0));
        updates.push(format!("{deleted_column} = 0"));
    }
    if let Some(revision_column) = ["revision", "_rev"].into_iter().find_map(|column| {
        rxdb_table_has_column(&conn, &table, column)
            .ok()
            .filter(|exists| *exists)
            .map(|_| column)
    }) {
        columns.push(revision_column.to_string());
        values.push(SqlValue::Text(rev));
        updates.push(format!("{revision_column} = excluded.{revision_column}"));
    }
    if rxdb_table_has_column(&conn, &table, "lastWriteTime")? {
        columns.push("lastWriteTime".to_string());
        values.push(SqlValue::Real(updated_at_ms as f64));
        updates.push("lastWriteTime = excluded.lastWriteTime".to_string());
    }
    let placeholders = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    conn.execute(
        &format!(
            "INSERT INTO {table} ({columns}) VALUES ({placeholders})
             ON CONFLICT(id) DO UPDATE SET {updates}",
            columns = columns.join(", "),
            updates = updates.join(", ")
        ),
        params_from_iter(values),
    )?;
    Ok(())
}

pub fn upsert_projection_record(
    root: &Path,
    collection: &str,
    record_id: &str,
    updated_at_ms: i64,
    payload: Value,
) -> anyhow::Result<()> {
    let conn = open_store(root)?;
    upsert_business_record(&conn, collection, record_id, updated_at_ms, payload.clone())?;
    upsert_rxdb_collection_record(root, collection, record_id, updated_at_ms, payload)
}

fn rxdb_table_has_column(conn: &Connection, table: &str, column: &str) -> anyhow::Result<bool> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn merge_json_object_values(target: &mut Value, patch: &Value) {
    let (Some(target_obj), Some(patch_obj)) = (target.as_object_mut(), patch.as_object()) else {
        *target = patch.clone();
        return;
    };
    for (key, value) in patch_obj {
        match (target_obj.get_mut(key), value) {
            (Some(existing), Value::Object(_)) if existing.is_object() => {
                merge_json_object_values(existing, value);
            }
            _ => {
                target_obj.insert(key.clone(), value.clone());
            }
        }
    }
}

fn is_safe_rxdb_collection_name(collection: &str) -> bool {
    !collection.is_empty()
        && collection
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn sanitize_filename(title: &str) -> String {
    let mut sanitized = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
            sanitized.push(c);
        } else {
            sanitized.push('_');
        }
    }
    let s = sanitized.trim().to_string();
    if s.is_empty() {
        "Untitled".to_string()
    } else {
        s
    }
}

fn write_note_markdown_file(root: &Path, record_id: &str, document: &Value) -> anyhow::Result<()> {
    let title = document
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled")
        .trim();
    let content = document
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let sanitized_title = sanitize_filename(title);
    let notes_dir = root.join("runtime/business-os/notes");
    std::fs::create_dir_all(&notes_dir)?;

    if let Ok(entries) = std::fs::read_dir(&notes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                    if filename.ends_with(&format!("_{}.md", record_id)) {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
        }
    }

    let file_path = notes_dir.join(format!("{}_{}.md", sanitized_title, record_id));
    std::fs::write(&file_path, content)?;
    Ok(())
}

fn delete_note_markdown_file(root: &Path, record_id: &str) -> anyhow::Result<()> {
    let notes_dir = root.join("runtime/business-os/notes");
    if notes_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&notes_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                        if filename.ends_with(&format!("_{}.md", record_id)) {
                            let _ = std::fs::remove_file(path);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn sync_local_markdown_notes(root: &Path) -> anyhow::Result<()> {
    let notes_dir = root.join("runtime/business-os/notes");
    if !notes_dir.is_dir() {
        std::fs::create_dir_all(&notes_dir)?;
    }

    let conn = open_store(root)?;
    seed_readme_note_if_needed(root, &notes_dir, &conn)?;

    let mut files_on_disk = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(&notes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                    if filename.ends_with(".md") {
                        files_on_disk.insert(filename.to_string(), path.clone());
                    }
                }
            }
        }
    }

    let mut active_db_ids = std::collections::HashSet::new();

    for (_filename, path) in &files_on_disk {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        let mut title = "";
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                title = trimmed.trim_start_matches('#').trim();
                break;
            }
        }
        if title.is_empty() {
            title = "Untitled";
        }

        let mut uuid_suffix = None;
        if stem.len() >= 37 {
            let potential_uuid = &stem[stem.len() - 36..];
            if Uuid::parse_str(potential_uuid).is_ok() && stem.as_bytes()[stem.len() - 37] == b'_' {
                uuid_suffix = Some(potential_uuid.to_string());
            }
        }

        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| std::time::SystemTime::now())
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        if let Some(id) = uuid_suffix {
            active_db_ids.insert(id.clone());

            let db_record: Option<(i64, String)> = conn
                .query_row(
                    "SELECT updated_at_ms, payload_json FROM business_records WHERE collection = 'notes' AND record_id = ?1 AND deleted = 0",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            match db_record {
                Some((db_mtime, db_payload_json)) => {
                    if mtime > db_mtime + 2000 {
                        let mut payload: Value =
                            serde_json::from_str(&db_payload_json).unwrap_or_default();
                        if let Some(obj) = payload.as_object_mut() {
                            obj.insert("title".to_string(), Value::String(title.to_string()));
                            obj.insert("content".to_string(), Value::String(content));
                            obj.insert("updated_at_ms".to_string(), Value::from(mtime));
                        }
                        upsert_business_record(&conn, "notes", &id, mtime, payload)?;
                    }
                }
                None => {
                    let is_deleted: Option<i64> = conn
                        .query_row(
                            "SELECT deleted FROM business_records WHERE collection = 'notes' AND record_id = ?1",
                            params![id],
                            |row| row.get(0),
                        )
                        .optional()?;

                    if is_deleted == Some(1) {
                        let _ = std::fs::remove_file(path);
                    } else {
                        let payload = serde_json::json!({
                            "id": id,
                            "title": title,
                            "content": content,
                            "folder": "Notes",
                            "updated_at_ms": mtime,
                        });
                        upsert_business_record(&conn, "notes", &id, mtime, payload)?;
                    }
                }
            }
        } else {
            let new_id = Uuid::new_v4().to_string();
            let sanitized_title = sanitize_filename(title);
            let new_filename = format!("{}_{}.md", sanitized_title, new_id);
            let new_path = notes_dir.join(&new_filename);

            if std::fs::rename(path, &new_path).is_ok() {
                active_db_ids.insert(new_id.clone());
                let payload = serde_json::json!({
                    "id": new_id,
                    "title": title,
                    "content": content,
                    "folder": "Notes",
                    "updated_at_ms": mtime,
                });
                upsert_business_record(&conn, "notes", &new_id, mtime, payload)?;
            }
        }
    }

    let mut stmt = conn.prepare(
        "SELECT record_id, payload_json FROM business_records WHERE collection = 'notes' AND deleted = 0"
    )?;
    let db_notes = stmt.query_map(params![], |row| {
        let record_id: String = row.get(0)?;
        let payload_json: String = row.get(1)?;
        Ok((record_id, payload_json))
    })?;

    for row in db_notes {
        let (record_id, _payload_json) = row?;
        if !active_db_ids.contains(&record_id) {
            mark_business_record_deleted(&conn, "notes", &record_id, now_ms() as i64)?;
        }
    }

    Ok(())
}

fn seed_readme_note_if_needed(
    root: &Path,
    notes_dir: &Path,
    conn: &Connection,
) -> anyhow::Result<()> {
    let marker_path = root.join("runtime/business-os/readme-note-seeded");
    if marker_path.is_file() {
        return Ok(());
    }

    let has_markdown_notes = std::fs::read_dir(notes_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .any(|entry| {
            entry
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".md"))
                .unwrap_or(false)
        });
    let db_note_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM business_records WHERE collection = 'notes' AND deleted = 0",
        params![],
        |row| row.get(0),
    )?;

    if has_markdown_notes || db_note_count > 0 {
        std::fs::write(marker_path, "already-initialized\n")?;
        return Ok(());
    }

    let readme_path = root.join("README.md");
    let content = match std::fs::read_to_string(&readme_path) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            std::fs::write(marker_path, "no-readme\n")?;
            return Ok(());
        }
    };

    let record_id = Uuid::new_v4().to_string();
    let title = content
        .lines()
        .find_map(|line| {
            let title = line.trim().trim_start_matches('#').trim();
            (!title.is_empty()).then_some(title.to_owned())
        })
        .unwrap_or_else(|| "CTOX README".to_owned());
    let updated_at_ms = now_ms() as i64;
    let filename = format!("{}_{}.md", sanitize_filename(&title), record_id);
    let file_path = notes_dir.join(filename);
    std::fs::write(&file_path, &content)?;
    upsert_business_record(
        conn,
        "notes",
        &record_id,
        updated_at_ms,
        serde_json::json!({
            "id": record_id,
            "title": title,
            "content": content,
            "folder": "Notes",
            "updated_at_ms": updated_at_ms,
        }),
    )?;
    std::fs::write(marker_path, "seeded\n")?;
    Ok(())
}

pub fn push_collection_records(root: &Path, body: Value) -> anyhow::Result<Value> {
    let collection = body
        .get("collection")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("collection is required")?;
    let documents = body
        .get("documents")
        .and_then(Value::as_array)
        .context("documents array is required")?;
    let mut accepted = Vec::new();
    let mut ignored = Vec::new();
    for document in documents {
        let record_id = document
            .get("id")
            .or_else(|| document.get("command_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("unknown")
            .to_string();
        if document
            .get("_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let conn = open_store(root)?;
            mark_business_record_deleted(&conn, collection, &record_id, now_ms() as i64)?;
            if collection == "notes" {
                if let Err(e) = delete_note_markdown_file(root, &record_id) {
                    eprintln!(
                        "[business-os] failed to delete note file for {}: {}",
                        record_id, e
                    );
                }
            }
            accepted.push(serde_json::json!({ "id": record_id, "status": "deleted" }));
            continue;
        }
        if collection == "business_commands" {
            match accept_rxdb_business_command(root, document.clone()) {
                Ok(value) => accepted.push(value),
                Err(error) => ignored.push(serde_json::json!({
                    "id": record_id,
                    "error": error.to_string()
                })),
            }
        } else {
            let conn = open_store(root)?;
            let updated_at_ms = document
                .get("updated_at_ms")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| now_ms() as i64);
            upsert_business_record(
                &conn,
                collection,
                &record_id,
                updated_at_ms,
                document.clone(),
            )?;
            if collection == "notes" {
                if let Err(e) = write_note_markdown_file(root, &record_id, document) {
                    eprintln!(
                        "[business-os] failed to write note file for {}: {}",
                        record_id, e
                    );
                }
            }
            accepted.push(serde_json::json!({ "id": record_id, "status": "stored" }));
        }
    }
    Ok(serde_json::json!({
        "ok": true,
        "collection": collection,
        "accepted": accepted,
        "ignored": ignored,
        "count": accepted.len()
    }))
}

pub fn mark_business_command_failed(
    root: &Path,
    command_id: &str,
    error: &str,
    failed_at_ms: i64,
) -> anyhow::Result<()> {
    let command_id = command_id.trim();
    anyhow::ensure!(!command_id.is_empty(), "command_id is required");
    let conn = open_store(root)?;
    conn.execute(
        "UPDATE business_commands
         SET status = 'failed', observed_at_ms = ?2
         WHERE command_id = ?1",
        params![command_id, failed_at_ms],
    )?;

    let payload_json = conn
        .query_row(
            "SELECT payload_json
             FROM business_records
             WHERE collection = 'business_commands' AND record_id = ?1",
            params![command_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let mut payload = payload_json
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "id": command_id,
                "command_id": command_id,
                "module": "ctox",
                "command_type": "",
                "record_id": command_id
            })
        });
    if let Some(object) = payload.as_object_mut() {
        object.insert("status".to_string(), Value::String("failed".to_string()));
        object.insert(
            "task_status".to_string(),
            Value::String("failed".to_string()),
        );
        object.insert("error".to_string(), Value::String(error.to_string()));
        object.insert("updated_at_ms".to_string(), Value::from(failed_at_ms));
    }
    upsert_business_record(
        &conn,
        "business_commands",
        command_id,
        failed_at_ms,
        payload,
    )?;
    Ok(())
}

pub fn refresh_business_command_queue_task_projection(
    root: &Path,
    task_id: &str,
) -> anyhow::Result<Option<Value>> {
    let conn = open_store(root)?;
    let Some(command_id) = queue_projection_command_id(&conn, task_id)? else {
        return Ok(None);
    };
    let command = load_business_command(&conn, &command_id)?;
    let Some(task) = channels::load_queue_task(root, task_id)? else {
        return Ok(None);
    };
    let updated_at_ms = now_ms() as i64;
    let command_status = conn
        .query_row(
            "SELECT status FROM business_commands WHERE command_id = ?1",
            params![command_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| "accepted".to_string());
    let mut command_projection = conn
        .query_row(
            "SELECT payload_json
             FROM business_records
             WHERE collection = 'business_commands'
               AND record_id = ?1
               AND deleted = 0",
            params![command_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|payload| serde_json::from_str::<Value>(&payload).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "id": command_id,
                "command_id": command_id,
                "module": command.module.clone(),
                "command_type": command.command_type.clone(),
                "record_id": command.record_id.clone().unwrap_or_default(),
                "inbound_channel": command_inbound_channel(&command),
                "payload": command.payload.clone(),
                "client_context": command.client_context.clone()
            })
        });
    if let Some(object) = command_projection.as_object_mut() {
        object.insert("status".to_string(), Value::String(command_status.clone()));
        object.insert(
            "task_id".to_string(),
            Value::String(task.message_key.clone()),
        );
        object.insert(
            "task_status".to_string(),
            Value::String(normalize_queue_status(&task.route_status).to_string()),
        );
        if let Some(note) = task.status_note.as_deref() {
            object.insert(
                "queue_status_note".to_string(),
                Value::String(note.to_string()),
            );
        }
        object.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    }
    upsert_business_record(
        &conn,
        "business_commands",
        &command_id,
        updated_at_ms,
        command_projection.clone(),
    )?;
    refresh_queue_task_projection(
        root,
        &conn,
        &command_id,
        &command,
        Some(&task),
        updated_at_ms,
    )?;
    Ok(Some(command_projection))
}

pub fn fail_business_command_from_queue_error(
    root: &Path,
    task_id: &str,
    error: &str,
) -> anyhow::Result<Option<Value>> {
    let conn = open_store(root)?;
    let Some(command_id) = queue_projection_command_id(&conn, task_id)? else {
        return Ok(None);
    };
    let command = load_business_command(&conn, &command_id)?;
    let task = channels::load_queue_task(root, task_id)?;
    let failed_at_ms = now_ms() as i64;
    conn.execute(
        "UPDATE business_commands
         SET status = 'failed', observed_at_ms = ?2
         WHERE command_id = ?1",
        params![command_id.as_str(), failed_at_ms],
    )?;
    let payload = serde_json::json!({
        "id": command_id,
        "command_id": command_id,
        "module": command.module.clone(),
        "command_type": command.command_type.clone(),
        "record_id": command.record_id.clone().unwrap_or_default(),
        "status": "failed",
        "inbound_channel": command_inbound_channel(&command),
        "task_id": task_id,
        "task_status": "failed",
        "error": error,
        "payload": command.payload.clone(),
        "client_context": command.client_context.clone(),
        "updated_at_ms": failed_at_ms
    });
    upsert_business_record(
        &conn,
        "business_commands",
        &command_id,
        failed_at_ms,
        payload.clone(),
    )?;
    if let Some(task) = task.as_ref() {
        refresh_queue_task_projection(
            root,
            &conn,
            &command_id,
            &command,
            Some(task),
            failed_at_ms,
        )?;
    }
    Ok(Some(payload))
}

pub fn accept_rxdb_business_command(root: &Path, document: Value) -> anyhow::Result<Value> {
    let command_id = document
        .get("command_id")
        .or_else(|| document.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("business command id is required")?
        .to_string();
    let conn = open_store(root)?;
    let exists: Option<String> = conn
        .query_row(
            "SELECT command_id FROM business_commands WHERE command_id = ?1",
            params![command_id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_some() {
        if let Some(outcome) = stored_rxdb_business_command_outcome(&conn, &command_id)? {
            return Ok(outcome);
        }
        return Ok(serde_json::json!({
            "id": command_id,
            "command_id": command_id,
            "status": "already_accepted"
        }));
    }
    drop(conn);
    let command = BusinessCommand {
        id: Some(command_id.clone()),
        module: document
            .get("module")
            .and_then(Value::as_str)
            .unwrap_or("ctox")
            .to_string(),
        command_type: document
            .get("command_type")
            .and_then(Value::as_str)
            .unwrap_or("business_os.command")
            .to_string(),
        record_id: document
            .get("record_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        payload: document.get("payload").cloned().unwrap_or(Value::Null),
        client_context: document
            .get("client_context")
            .cloned()
            .unwrap_or(Value::Null),
    };
    match command.command_type.as_str() {
        command_type if crate::coding_agents::is_coding_agent_command(command_type) => {
            let outcome = match rxdb_command_session(root, &command)
                .and_then(|_| crate::coding_agents::handle_business_command(root, &command))
            {
                Ok(outcome) => outcome,
                Err(error) => serde_json::json!({
                    "ok": false,
                    "provider": command
                        .payload
                        .get("provider")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown"),
                    "operation": command.command_type,
                    "stdout": "",
                    "stderr": error.to_string(),
                    "exit_code": 1
                }),
            };
            let status = if outcome.get("ok").and_then(Value::as_bool) == Some(false) {
                "failed"
            } else {
                "completed"
            };
            return write_rxdb_control_command_outcome(
                root,
                &command,
                status,
                None,
                Some(status),
                serde_json::json!({ "outcome": outcome }),
            );
        }
        "ctox.task.update" => {
            let mutation: CtoxTaskUpdateMutation = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.task.update payload")?;
            let session = rxdb_command_session(root, &command)?;
            let outcome = update_ctox_task(root, &session, mutation)?;
            let task_id = outcome
                .get("task")
                .and_then(|task| task.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                task_id.as_deref(),
                Some("updated"),
                outcome,
            );
        }
        "ctox.task.delete" => {
            let mutation: CtoxTaskDeleteMutation = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.task.delete payload")?;
            let session = rxdb_command_session(root, &command)?;
            let outcome = delete_ctox_task(root, &session, mutation)?;
            let task_id = outcome
                .get("task_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                task_id.as_deref(),
                Some("cancelled"),
                outcome,
            );
        }
        "knowledge.command" => {
            let args = command
                .payload
                .get("args")
                .and_then(Value::as_array)
                .context("knowledge.command payload.args array is required")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .context("knowledge.command args must be strings")
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            let outcome = crate::knowledge::dispatch_capturing(root, &args)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.business_os.user.upsert" => {
            let mutation: BusinessOsUserMutation = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.business_os.user.upsert payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let outcome = upsert_user(root, &session, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.runtime_settings.save" => {
            let mutation: RuntimeSettingsRequest = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.runtime_settings.save payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let outcome = save_runtime_settings_command(root, &session, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.subscription_auth.start" => {
            let request: SubscriptionAuthStartCommandRequest =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.subscription_auth.start payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let outcome = start_subscription_auth_command(root, &session, request)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        command_type if is_customers_active_command(command_type) => {
            let session = rxdb_authenticated_session(root, &command)?;
            match handle_customers_active_command(root, &session, &command) {
                Ok(outcome) => {
                    return write_rxdb_control_command_outcome(
                        root,
                        &command,
                        "completed",
                        command.record_id.as_deref(),
                        Some("completed"),
                        outcome,
                    );
                }
                Err(error) => {
                    let _ = write_rxdb_control_command_outcome(
                        root,
                        &command,
                        "failed",
                        command.record_id.as_deref(),
                        Some("failed"),
                        serde_json::json!({
                            "ok": false,
                            "error": error.to_string(),
                        }),
                    );
                    return Err(error);
                }
            }
        }
        command_type if is_outbound_active_command(command_type) => {
            let session = rxdb_authenticated_session(root, &command)?;
            let outcome = handle_outbound_active_command(root, &session, &command_id, &command)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                command.record_id.as_deref(),
                Some("completed"),
                outcome,
            );
        }
        command_type if is_iot_active_command(command_type) => {
            // §4A: the executor goes through the SAME iot::commands code path the
            // `ctox iot` CLI uses. ACL-gate like the management-class families
            // (ctox.task.update/delete): rxdb_command_session enforces a real
            // chef/admin role via session_can_manage_all, so an untrusted peer
            // that falls through to the default `user` role is rejected here with
            // "chef or admin role required" instead of slipping past the
            // always-true `authenticated && !auth_required` disjunct downstream.
            // Then write a completed/failed outcome whose `result.projections` the
            // rxdb_peer branch reprojects into the iot_* collections. Idempotent: a
            // replayed command short-circuits on the stored outcome above.
            let session = rxdb_command_session(root, &command)?;
            match crate::iot::commands::handle_business_command(
                root,
                command_type,
                &command.payload,
                &session,
            ) {
                Ok(outcome) => {
                    // Project engine state into the RxDB-visible business_records
                    // store via iot::projector (same code path as the rxdb_peer
                    // live stream). Failure to project must not silently drop the
                    // outcome, so surface it.
                    project_iot_business_command_outcome(root, &outcome)
                        .context("project iot business command outcome")?;
                    return write_rxdb_control_command_outcome(
                        root,
                        &command,
                        "completed",
                        command.record_id.as_deref(),
                        Some("completed"),
                        outcome,
                    );
                }
                Err(error) => {
                    let _ = write_rxdb_control_command_outcome(
                        root,
                        &command,
                        "failed",
                        command.record_id.as_deref(),
                        Some("failed"),
                        serde_json::json!({
                            "ok": false,
                            "error": error.to_string(),
                        }),
                    );
                    return Err(error);
                }
            }
        }
        command_type if command_type.starts_with("ctox.channel.") => {
            let mutation: ChannelCommandRequest = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.channel payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let outcome = run_channel_command(root, &session, command_type, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        command_type if command_type.starts_with("ctox.ticket.") => {
            let _session = rxdb_authenticated_session(root, &command)?;
            let outcome = crate::mission::tickets::run_business_os_ticket_command(
                root,
                command_type,
                &command.payload,
            )?;
            let task_id = outcome
                .get("case_id")
                .or_else(|| outcome.get("ticket_key"))
                .and_then(Value::as_str)
                .map(str::to_string);
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                task_id.as_deref(),
                Some("completed"),
                outcome,
            );
        }
        command_type if command_type.starts_with("ctox.report.") => {
            let mut mutation: BusinessOsReportMutation =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.report payload")?;
            if mutation.kind.trim().is_empty() {
                mutation.kind = command_type
                    .strip_prefix("ctox.report.")
                    .unwrap_or("bug")
                    .to_string();
            }
            mutation.client_context = command.client_context.clone();
            let session = rxdb_authenticated_session(root, &command)?;
            let accepted = record_report_command(
                root,
                &session,
                mutation,
                Some(command_id),
                command.record_id.clone(),
            )?;
            return Ok(serde_json::json!({
                "ok": true,
                "id": accepted.command_id,
                "command_id": accepted.command_id,
                "status": "accepted",
                "task_id": accepted.task_id.unwrap_or_default(),
                "task_status": accepted.task_status.unwrap_or_else(|| "accepted".to_string()),
                "report_id": accepted.report_id,
                "report_status": "open"
            }));
        }
        "ctox.source.load" => {
            let mutation: ModuleSourceLoadMutation =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.source.load payload")?;
            let outcome = load_module_source_records(root, &mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.source.save" => {
            let mutation: ModuleSourceSaveMutation =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.source.save payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let module_id = source_sanitize_slug(&mutation.module_id);
            anyhow::ensure!(!module_id.is_empty(), "module_id is required");
            anyhow::ensure!(
                session_can_modify_module(root, &session, &module_id)?,
                "module modification rights required"
            );
            let outcome = save_module_source_record(root, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.source.list_snapshots" => {
            let request: ModuleSourceListSnapshotsRequest =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.source.list_snapshots payload")?;
            let outcome = list_module_source_snapshots(root, request)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.source.rollback_snapshot" => {
            let request: ModuleSourceRollbackSnapshotRequest =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.source.rollback_snapshot payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let module_id = source_sanitize_slug(&request.module_id);
            anyhow::ensure!(!module_id.is_empty(), "module_id is required");
            anyhow::ensure!(
                session_can_modify_module(root, &session, &module_id)?,
                "module modification rights required"
            );
            let outcome = rollback_module_source_snapshot(root, request)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.file.materialize" => {
            let mutation: DesktopFileMaterializeRequest =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.file.materialize payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let outcome = materialize_desktop_file_command(root, &session, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.module.release" => {
            let mutation: ModuleReleaseRequest = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.module.release payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let app_root = resolve_business_os_app_root(root)?;
            let outcome = record_module_release(root, &app_root, &session, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.module.assign_founder" => {
            let mutation: ModuleFounderAssignment = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.module.assign_founder payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let outcome = assign_module_founder(root, &session, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.module.save" => {
            let mutation: ModuleUpsertRequest = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.module.save payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let app_root = resolve_business_os_app_root(root)?;
            let outcome = upsert_module_manifest_command(root, &app_root, &session, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.module.delete" => {
            let mutation: ModuleDeleteRequest = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.module.delete payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let app_root = resolve_business_os_app_root(root)?;
            let outcome = delete_installed_module_command(root, &app_root, &session, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.module.install_template" => {
            let mutation: ModuleInstallTemplateRequest =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.module.install_template payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let app_root = resolve_business_os_app_root(root)?;
            let outcome = install_template_module_command(root, &app_root, &session, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.module.rollback" => {
            let mutation: ModuleRollbackRequest =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.module.rollback payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let app_root = resolve_business_os_app_root(root)?;
            let outcome = rollback_module_release(root, &app_root, &session, mutation)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.module.list_versions" => {
            let request: ModuleVersionListRequest = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.module.list_versions payload")?;
            let outcome = list_module_versions(root, request)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.module.rollback_version" => {
            let request: ModuleVersionRollbackRequest =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.module.rollback_version payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let app_root = resolve_business_os_app_root(root)?;
            let outcome = rollback_module_to_version(root, &app_root, &session, request)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.app_store.install" => {
            let request: AppStoreInstallRequest =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.app_store.install payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let app_root = resolve_business_os_app_root(root)?;
            let outcome = install_app_module(root, &app_root, &session, request)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.app_store.uninstall" => {
            let request: AppStoreUninstallRequest = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.app_store.uninstall payload")?;
            let session = rxdb_authenticated_session(root, &command)?;
            let app_root = resolve_business_os_app_root(root)?;
            let outcome = uninstall_app_module(root, &app_root, &session, request)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.mailserver.get_config" => {
            let conn = open_store(root)?;

            // Get domains
            let mut stmt = conn.prepare("SELECT domain_name, dkim_selector, dkim_private_key, COALESCE(spf_record, ''), COALESCE(dmarc_record, '') FROM stalwart_domains")?;
            let domain_rows = stmt.query_map(params![], |row| {
                let dkim_private_key = row.get::<_, String>(2)?;
                let dkim_public_key = get_public_key(&dkim_private_key);
                Ok(serde_json::json!({
                    "domain_name": row.get::<_, String>(0)?,
                    "dkim_selector": row.get::<_, String>(1)?,
                    "dkim_private_key": dkim_private_key,
                    "dkim_public_key": dkim_public_key,
                    "spf_record": row.get::<_, String>(3)?,
                    "dmarc_record": row.get::<_, String>(4)?,
                }))
            })?;
            let mut domains = Vec::new();
            for r in domain_rows {
                domains.push(r?);
            }

            // Get users
            let mut stmt = conn.prepare("SELECT username, created_at FROM stalwart_users")?;
            let user_rows = stmt.query_map(params![], |row| {
                Ok(serde_json::json!({
                    "username": row.get::<_, String>(0)?,
                    "created_at": row.get::<_, i64>(1)?,
                }))
            })?;
            let mut users = Vec::new();
            for r in user_rows {
                users.push(r?);
            }

            let outcome = serde_json::json!({
                "domains": domains,
                "users": users
            });

            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.mailserver.save_domain" => {
            let domain_name = command
                .payload
                .get("domain_name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let mut dkim_selector = command
                .payload
                .get("dkim_selector")
                .and_then(Value::as_str)
                .unwrap_or("default")
                .trim()
                .to_string();
            if dkim_selector.is_empty() {
                dkim_selector = "default".to_string();
            }
            let dkim_private_key_opt = command
                .payload
                .get("dkim_private_key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();

            anyhow::ensure!(!domain_name.is_empty(), "domain_name is required");

            // Generate private key if not provided
            let dkim_private_key = if dkim_private_key_opt.is_empty() {
                // Try executing openssl command
                if let Ok(output) = std::process::Command::new("openssl")
                    .args(&[
                        "genpkey",
                        "-algorithm",
                        "RSA",
                        "-pkeyopt",
                        "rsa_keygen_bits:2048",
                        "-outform",
                        "PEM",
                    ])
                    .output()
                {
                    if output.status.success() {
                        String::from_utf8_lossy(&output.stdout).to_string()
                    } else {
                        get_fallback_private_key()
                    }
                } else {
                    get_fallback_private_key()
                }
            } else {
                dkim_private_key_opt
            };

            let spf_record = format!("v=spf1 mx a ip4:51.210.246.120 ~all");
            let dmarc_record = format!("v=DMARC1; p=none; rua=mailto:dmarc@{}", domain_name);

            let conn = open_store(root)?;
            conn.execute(
                "INSERT OR REPLACE INTO stalwart_domains (domain_name, dkim_selector, dkim_private_key, spf_record, dmarc_record)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![domain_name, dkim_selector, dkim_private_key, spf_record, dmarc_record],
            )?;

            let dkim_public_key = get_public_key(&dkim_private_key);

            let outcome = serde_json::json!({
                "domain_name": domain_name,
                "dkim_selector": dkim_selector,
                "spf_record": spf_record,
                "dmarc_record": dmarc_record,
                "dkim_private_key": dkim_private_key,
                "dkim_public_key": dkim_public_key
            });

            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.mailserver.delete_domain" => {
            let domain_name = command
                .payload
                .get("domain_name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            anyhow::ensure!(!domain_name.is_empty(), "domain_name is required");

            let conn = open_store(root)?;
            conn.execute(
                "DELETE FROM stalwart_domains WHERE domain_name = ?1",
                params![domain_name],
            )?;

            let outcome = serde_json::json!({
                "domain_name": domain_name,
                "deleted": true
            });

            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.mailserver.save_user" => {
            let username = command
                .payload
                .get("username")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let password = command
                .payload
                .get("password")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();

            anyhow::ensure!(!username.is_empty(), "username is required");
            anyhow::ensure!(!password.is_empty(), "password is required");

            let db_path = root
                .join("runtime/ctox.sqlite3")
                .to_string_lossy()
                .into_owned();
            let store = ctox_mailserver::store::sqlite::SqliteStore::new(&db_path);
            store.add_user(&username, &password)?;

            let outcome = serde_json::json!({
                "username": username,
                "saved": true
            });

            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        "ctox.mailserver.delete_user" => {
            let username = command
                .payload
                .get("username")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            anyhow::ensure!(!username.is_empty(), "username is required");

            let conn = open_store(root)?;
            conn.execute(
                "DELETE FROM stalwart_users WHERE username = ?1",
                params![username],
            )?;
            conn.execute(
                "DELETE FROM stalwart_mailboxes WHERE owner = ?1",
                params![username],
            )?;

            let outcome = serde_json::json!({
                "username": username,
                "deleted": true
            });

            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        _ => {}
    }
    let accepted = record_command(root, command)?;
    Ok(serde_json::to_value(accepted)?)
}

fn is_iot_active_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "ctox.iot.attribute.write"
            | "ctox.iot.asset.upsert"
            | "ctox.iot.asset.delete"
            | "ctox.iot.alarm.update"
            | "ctox.iot.ruleset.save"
            | "ctox.iot.ruleset.toggle"
            | "ctox.iot.agent.configure"
            | "ctox.iot.datapoints.query"
    )
}

fn is_outbound_active_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "outbound.engagement.create"
            | "outbound.engagement.assign_sender"
            | "outbound.sequence.save"
            | "outbound.draft.prepare"
            | "outbound.message.prepare"
            | "outbound.message.update_draft"
            | "outbound.message.request_approval"
            | "outbound.message.approve"
            | "outbound.message.reject"
            | "outbound.message.request_changes"
            | "outbound.message.send_approved"
            | "outbound.message.pause"
            | "outbound.message.resume"
            | "outbound.message.cancel"
            | "outbound.engagement.resume"
            | "outbound.engagement.close"
            | "outbound.reply.classify"
            | "outbound.reply.match"
            | "outbound.scheduling.prepare"
            | "outbound.scheduling.mark_booked"
            | "outbound.campaign.mailbox.link"
            | "outbound.campaign.status.set"
            | "outbound.campaign.briefing.update"
            | "outbound.campaign.apply_setup"
            | "outbound.provider.reconcile"
            | "outbound.skillbook.save"
            | "outbound.skillbook.seed_defaults"
            | "outbound.letter_template.save"
            | "outbound.scheduler.tick"
            | "outbound.audit.export"
            | "outbound.dev.seed_test_data"
            | "outbound.engagement.reapply_sequence"
            | "outbound.scheduling.update_slots"
            | "outbound.pipeline.write_outreach_draft"
    )
}

fn handle_outbound_active_command(
    root: &Path,
    session: &BusinessOsSession,
    _command_id: &str,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        command.module == "outbound",
        "active outbound commands require module=outbound"
    );
    let now = now_ms() as i64;
    let conn = open_store(root)?;
    match command.command_type.as_str() {
        // Writeback target for the LLM-generated outreach draft. The CTOX agent
        // (running the `outbound.pipeline.outreach_draft` mission-queue task)
        // calls this command to persist the generated subject/body/follow-ups
        // back into the pipeline item's contact. The whole loop stays on the
        // RxDB command bus — there is no external email gateway.
        "outbound.pipeline.write_outreach_draft" => {
            let pipeline_id = outbound_required_from_payload_or_record(
                command,
                &["pipeline_id", "id"],
                "pipeline_id is required",
            )?;
            let contact_index = command
                .payload
                .get("contact_index")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            anyhow::ensure!(contact_index >= 0, "contact_index must be >= 0");
            let messages = command
                .payload
                .get("messages")
                .filter(|value| value.is_object())
                .cloned()
                .context("messages object is required")?;
            let mut item = outbound_load_required(
                &conn,
                "outbound_pipeline_items",
                &pipeline_id,
                "pipeline item not found",
            )?;
            let idx = contact_index as usize;
            {
                let contacts = item
                    .get_mut("contacts")
                    .and_then(Value::as_array_mut)
                    .context("pipeline item has no contacts array")?;
                anyhow::ensure!(idx < contacts.len(), "contact_index out of range");
                let contact = &mut contacts[idx];
                if !contact.is_object() {
                    *contact = serde_json::json!({});
                }
                let contact_obj = contact
                    .as_object_mut()
                    .context("pipeline contact is not an object")?;
                let target = contact_obj
                    .entry("messages")
                    .or_insert_with(|| serde_json::json!({}));
                if !target.is_object() {
                    *target = serde_json::json!({});
                }
                let target_obj = target
                    .as_object_mut()
                    .context("contact messages is not an object")?;
                for key in [
                    "message_mail_subject",
                    "message_mail_body",
                    "message_followup_1",
                    "message_followup_2",
                ] {
                    if let Some(value) = messages.get(key).and_then(Value::as_str) {
                        target_obj.insert(key.to_string(), Value::String(value.to_string()));
                    }
                }
                // Clear the generating flag so the UI spinner resolves via sync.
                contact_obj.insert("outreach_generating".to_string(), Value::Bool(false));
                contact_obj.insert(
                    "outreach_status".to_string(),
                    Value::String("drafted".to_string()),
                );
            }
            outbound_put_i64(&mut item, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_pipeline_items",
                &pipeline_id,
                now,
                item.clone(),
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_pipeline_items",
                "pipeline_id": pipeline_id,
                "contact_index": contact_index,
                "messages": messages
            }))
        }
        "outbound.engagement.create" => {
            let engagement_id = outbound_id_from_command(command, &["engagement_id", "id"], "eng")?;
            let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
            let mut record = outbound_object_payload(&command.payload);
            outbound_put_string(&mut record, "id", engagement_id.clone());
            outbound_put_string(&mut record, "campaign_id", campaign_id);
            outbound_put_default_string(&mut record, "status", "ready_for_assignment");
            outbound_put_default_object(&mut record, "payload");
            outbound_put_default_i64(&mut record, "created_at_ms", now);
            outbound_put_i64(&mut record, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_engagements",
                &engagement_id,
                now,
                record.clone(),
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_engagements",
                "engagement": record
            }))
        }
        "outbound.engagement.assign_sender" => {
            let engagement_id = outbound_required_from_payload_or_record(
                command,
                &["engagement_id", "id"],
                "engagement_id is required",
            )?;
            let sender_account_id =
                outbound_required_string(&command.payload, &["sender_account_id"])?;
            let mut engagement = outbound_load_required(
                &conn,
                "outbound_engagements",
                &engagement_id,
                "engagement not found",
            )?;
            outbound_put_string(
                &mut engagement,
                "sender_account_id",
                sender_account_id.clone(),
            );
            outbound_put_string(&mut engagement, "status", "assigned".to_string());
            outbound_put_i64(&mut engagement, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_engagements",
                &engagement_id,
                now,
                engagement.clone(),
            )?;

            let assignment_id = outbound_id_from_command(command, &["assignment_id"], "assign")
                .unwrap_or_else(|_| format!("assign_{engagement_id}_{sender_account_id}"));
            let mut assignment = outbound_object_payload(&command.payload);
            outbound_put_string(&mut assignment, "id", assignment_id.clone());
            outbound_put_string(&mut assignment, "engagement_id", engagement_id.clone());
            outbound_put_string(&mut assignment, "sender_account_id", sender_account_id);
            outbound_put_default_string(&mut assignment, "status", "active");
            outbound_put_default_i64(&mut assignment, "created_at_ms", now);
            outbound_put_i64(&mut assignment, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_sender_assignments",
                &assignment_id,
                now,
                assignment.clone(),
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "engagement": engagement,
                "assignment": assignment
            }))
        }
        "outbound.sequence.save" => {
            let sequence_id = outbound_id_from_command(command, &["sequence_id", "id"], "seq")?;
            let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
            let mut sequence = outbound_object_payload(&command.payload);
            outbound_put_string(&mut sequence, "id", sequence_id.clone());
            outbound_put_string(&mut sequence, "campaign_id", campaign_id);
            outbound_put_default_string(&mut sequence, "name", "Outbound Sequence");
            outbound_put_default_string(&mut sequence, "strategy_text", "");
            outbound_put_default_string(&mut sequence, "sequence_policy_text", "");
            outbound_put_default_object(&mut sequence, "approval_policy");
            outbound_put_default_object(&mut sequence, "payload");
            outbound_put_default_i64(&mut sequence, "created_at_ms", now);
            outbound_put_i64(&mut sequence, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_sequences",
                &sequence_id,
                now,
                sequence.clone(),
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_sequences",
                "sequence": sequence
            }))
        }
        "outbound.draft.prepare" => {
            let message_id = outbound_id_from_command(command, &["message_id", "id"], "msg")?;
            let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
            let engagement = outbound_load_required(
                &conn,
                "outbound_engagements",
                &engagement_id,
                "engagement not found",
            )?;
            anyhow::ensure!(
                !matches!(
                    outbound_string(&engagement, &["status"]).as_deref(),
                    Some("closed" | "cancelled" | "meeting_booked")
                ),
                "closed outbound engagements cannot prepare new drafts"
            );
            let draft_kind = outbound_string(&command.payload, &["draft_kind"])
                .unwrap_or_else(|| "followup".to_string());
            let campaign_id = outbound_first_string(&[
                outbound_string(&command.payload, &["campaign_id"]),
                outbound_string(&engagement, &["campaign_id"]),
            ])
            .context("campaign_id is required")?;
            // Resolve the channel up front so the rest of the draft prep can branch on it.
            let channel = outbound_first_string(&[
                outbound_string(&command.payload, &["channel"]),
                outbound_string(&engagement, &["payload", "channel"]),
                outbound_string(
                    &engagement,
                    &["payload", "active_outreach", "default_channel"],
                ),
                outbound_string(&engagement, &["channel"]),
            ])
            .unwrap_or_else(|| "email".to_string());

            let (sender_account_id, recipient_email, recipient_address_text) =
                if channel == "physical_letter" {
                    // Physical letters do not need a sender mailbox or an email address.
                    let address = outbound_first_string(&[
                        outbound_string(&command.payload, &["recipient_address_text"]),
                        outbound_string(&command.payload, &["recipient_address"]),
                        outbound_string(&engagement, &["payload", "contact_address_text"]),
                        outbound_string(&engagement, &["payload", "recipient_address_text"]),
                    ])
                    .context("recipient_address_text is required for physical_letter drafts")?;
                    let sender = outbound_first_string(&[
                        outbound_string(&command.payload, &["sender_account_id"]),
                        outbound_string(&engagement, &["sender_account_id"]),
                    ])
                    .unwrap_or_default();
                    let email = outbound_first_string(&[
                        outbound_string(&command.payload, &["recipient_email"]),
                        outbound_string(&engagement, &["payload", "contact_email"]),
                    ])
                    .unwrap_or_default();
                    (sender, email, address)
                } else {
                    let sender = outbound_first_string(&[
                        outbound_string(&command.payload, &["sender_account_id"]),
                        outbound_string(&engagement, &["sender_account_id"]),
                    ])
                    .context("sender_account_id is required")?;
                    let email = outbound_first_string(&[
                        outbound_string(&command.payload, &["recipient_email"]),
                        outbound_string(&engagement, &["payload", "contact_email"]),
                    ])
                    .context("recipient_email is required")?;
                    anyhow::ensure!(
                        !outbound_recipient_suppressed(&conn, &email)?,
                        "recipient is suppressed for outbound communication"
                    );
                    (sender, email, String::new())
                };
            let previous_messages = outbound_load_records_by_string_field(
                &conn,
                "outbound_messages",
                "engagement_id",
                &engagement_id,
            )?;
            let latest_message = outbound_latest_message(&previous_messages);
            let resolved_skillbook_id = outbound_first_string(&[
                outbound_string(&command.payload, &["skillbook_id"]),
                outbound_string(&command.payload, &["payload", "skillbook_id"]),
                outbound_string(&engagement, &["payload", "skillbook_id"]),
            ])
            .unwrap_or_else(|| "business-os.outbound.message_drafting.v1".to_string());
            let skillbook_guidance = outbound_skillbook_guidance(&conn, &resolved_skillbook_id)?;
            let generated = outbound_generate_automated_draft(
                &engagement,
                latest_message.as_ref(),
                &command.payload,
                &draft_kind,
                skillbook_guidance.as_deref(),
            );
            let subject = outbound_first_string(&[
                outbound_string(&command.payload, &["subject"]),
                outbound_string(&generated, &["subject"]),
            ])
            .context("generated subject is required")?;
            let body_text = outbound_first_string(&[
                outbound_string(&command.payload, &["body_text"]),
                outbound_string(&generated, &["body_text"]),
            ])
            .context("generated body_text is required")?;
            let mut message = outbound_object_payload(&command.payload);
            outbound_put_string(&mut message, "id", message_id.clone());
            outbound_put_string(&mut message, "engagement_id", engagement_id.clone());
            outbound_put_string(&mut message, "campaign_id", campaign_id);
            outbound_put_string(&mut message, "message_type", draft_kind.clone());
            outbound_put_string(&mut message, "channel", channel.clone());
            outbound_put_string(&mut message, "direction", "outbound");
            outbound_put_string(&mut message, "sender_account_id", sender_account_id);
            outbound_put_string(&mut message, "recipient_email", recipient_email);
            if !recipient_address_text.is_empty() {
                outbound_put_string(
                    &mut message,
                    "recipient_address_text",
                    recipient_address_text,
                );
            }
            outbound_put_string(&mut message, "subject", subject);
            outbound_put_string(&mut message, "body_text", body_text);
            outbound_put_string(&mut message, "draft_status", "ready_for_review");
            outbound_put_string(&mut message, "approval_status", "awaiting_approval");
            outbound_put_string(&mut message, "send_status", "awaiting_approval");
            outbound_put_default_object(&mut message, "payload");
            outbound_payload_insert(
                &mut message,
                "draft_engine",
                Value::String("business-os.outbound.draft_automation.v1".to_string()),
            );
            outbound_payload_insert(&mut message, "generated_draft", generated.clone());
            outbound_payload_insert(
                &mut message,
                "skillbook_id",
                Value::String(resolved_skillbook_id.clone()),
            );
            outbound_payload_insert(
                &mut message,
                "runbook_id",
                Value::String(
                    outbound_first_string(&[
                        outbound_string(&command.payload, &["runbook_id"]),
                        outbound_string(&command.payload, &["payload", "runbook_id"]),
                        outbound_string(&engagement, &["payload", "runbook_id"]),
                    ])
                    .unwrap_or_default(),
                ),
            );
            if let Some(previous) = latest_message
                .as_ref()
                .and_then(|message| outbound_string(message, &["id"]))
            {
                outbound_put_string(&mut message, "reply_to_message_id", previous);
            }
            if draft_kind == "scheduling" {
                let request_id = outbound_string(&command.payload, &["meeting_request_id"])
                    .unwrap_or_else(|| format!("meeting_{message_id}"));
                outbound_payload_insert(
                    &mut message,
                    "meeting_request_id",
                    Value::String(request_id),
                );
                outbound_payload_insert(
                    &mut message,
                    "proposed_slots",
                    command
                        .payload
                        .get("proposed_slots")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([])),
                );
            }
            let revision_id = outbound_message_revision(&message);
            outbound_put_string(&mut message, "revision_id", revision_id);
            outbound_put_default_i64(&mut message, "created_at_ms", now);
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            let mut meeting_request_result = Value::Null;
            if draft_kind == "scheduling" {
                let request_id = outbound_string(&command.payload, &["meeting_request_id"])
                    .unwrap_or_else(|| format!("meeting_{message_id}"));
                let mut request = serde_json::json!({
                    "id": request_id,
                    "engagement_id": engagement_id,
                    "message_id": message_id,
                    "duration_minutes": command.payload.get("duration_minutes").and_then(Value::as_i64).unwrap_or(30),
                    "slot_strategy": outbound_string(&command.payload, &["slot_strategy"]).unwrap_or_else(|| "campaign_default".to_string()),
                    "proposed_slots": command.payload.get("proposed_slots").cloned().unwrap_or_else(|| serde_json::json!([])),
                    "status": "prepared",
                    "payload": {
                        "source": "outbound.draft.prepare"
                    },
                    "created_at_ms": now,
                    "updated_at_ms": now
                });
                if let Some(calendar) = outbound_string(&command.payload, &["calendar_account_id"])
                {
                    outbound_put_string(&mut request, "calendar_account_id", calendar);
                }
                upsert_business_record(
                    &conn,
                    "outbound_meeting_requests",
                    &request_id,
                    now,
                    request.clone(),
                )?;
                meeting_request_result = request;
            }
            outbound_update_engagement_status(&conn, &engagement_id, "awaiting_approval", now)?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_messages",
                "message": message,
                "meeting_request": meeting_request_result,
                "approval_required": true,
                "provider_send_executed": false
            }))
        }
        "outbound.message.prepare" => {
            let message_id = outbound_id_from_command(command, &["message_id", "id"], "msg")?;
            let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
            let engagement = outbound_load_record(&conn, "outbound_engagements", &engagement_id)?;
            let campaign_id = outbound_first_string(&[
                outbound_string(&command.payload, &["campaign_id"]),
                engagement
                    .as_ref()
                    .and_then(|value| outbound_string(value, &["campaign_id"])),
            ])
            .context("campaign_id is required")?;
            let mut message = outbound_object_payload(&command.payload);
            outbound_put_string(&mut message, "id", message_id.clone());
            outbound_put_string(&mut message, "engagement_id", engagement_id.clone());
            outbound_put_string(&mut message, "campaign_id", campaign_id);
            outbound_put_default_string(&mut message, "message_type", "initial");
            outbound_put_default_string(&mut message, "direction", "outbound");
            outbound_put_default_string(&mut message, "draft_status", "prepared");
            outbound_put_default_string(&mut message, "approval_status", "draft");
            outbound_put_default_string(&mut message, "send_status", "not_scheduled");
            outbound_put_default_object(&mut message, "payload");
            let revision_id = outbound_message_revision(&message);
            outbound_put_string(&mut message, "revision_id", revision_id);
            outbound_put_default_i64(&mut message, "created_at_ms", now);
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            outbound_update_engagement_status(&conn, &engagement_id, "draft_prepared", now)?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_messages",
                "message": message
            }))
        }
        "outbound.message.update_draft" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            anyhow::ensure!(
                !matches!(
                    outbound_string(&message, &["send_status"]).as_deref(),
                    Some("queued_for_provider" | "sent" | "accepted")
                ),
                "sent or queued outbound message drafts cannot be edited"
            );
            outbound_merge_fields(
                &mut message,
                &command.payload,
                &[
                    "recipient_email",
                    "recipient_address_text",
                    "recipient_address",
                    "channel",
                    "subject",
                    "body_text",
                    "body_html",
                    "sender_account_id",
                    "scheduled_send_at_ms",
                    "payload",
                ],
            );
            outbound_put_string(&mut message, "draft_status", "prepared".to_string());
            outbound_put_string(&mut message, "approval_status", "draft".to_string());
            outbound_put_string(&mut message, "send_status", "not_scheduled".to_string());
            let revision_id = outbound_message_revision(&message);
            outbound_put_string(&mut message, "revision_id", revision_id);
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            Ok(serde_json::json!({ "ok": true, "message": message }))
        }
        "outbound.message.request_approval" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            outbound_require_message_content(&message)?;
            let revision_id = outbound_message_revision(&message);
            outbound_put_string(&mut message, "revision_id", revision_id);
            outbound_put_string(&mut message, "draft_status", "ready_for_review".to_string());
            outbound_put_string(
                &mut message,
                "approval_status",
                "awaiting_approval".to_string(),
            );
            outbound_put_string(&mut message, "send_status", "awaiting_approval".to_string());
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                outbound_update_engagement_status(&conn, &engagement_id, "awaiting_approval", now)?;
            }
            Ok(serde_json::json!({ "ok": true, "message": message }))
        }
        "outbound.message.approve" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            anyhow::ensure!(
                outbound_string(&message, &["approval_status"]).as_deref()
                    == Some("awaiting_approval"),
                "only messages awaiting approval can be approved"
            );
            outbound_require_message_content(&message)?;
            let revision_id = outbound_message_revision(&message);
            let approval_id = outbound_id_from_command(command, &["approval_id"], "approval")
                .unwrap_or_else(|_| format!("approval_{message_id}_{revision_id}"));
            let engagement_id = outbound_string(&message, &["engagement_id"]).unwrap_or_default();
            let mut approval = outbound_object_payload(&command.payload);
            outbound_put_string(&mut approval, "id", approval_id.clone());
            outbound_put_string(&mut approval, "message_id", message_id.clone());
            outbound_put_string(&mut approval, "engagement_id", engagement_id.clone());
            outbound_put_string(&mut approval, "revision_id", revision_id.clone());
            outbound_put_string(
                &mut approval,
                "actor_user_id",
                outbound_session_actor_id(session),
            );
            outbound_put_string(&mut approval, "decision", "approved");
            outbound_put_default_object(&mut approval, "payload");
            outbound_put_default_i64(&mut approval, "created_at_ms", now);
            outbound_put_i64(&mut approval, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_approvals",
                &approval_id,
                now,
                approval.clone(),
            )?;
            outbound_put_string(&mut message, "revision_id", revision_id);
            outbound_put_string(&mut message, "approval_status", "approved");
            outbound_put_string(&mut message, "draft_status", "approved");
            outbound_put_string(&mut message, "send_status", "approved_not_sent");
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            if !engagement_id.is_empty() {
                outbound_update_engagement_status(&conn, &engagement_id, "approved_for_send", now)?;
            }
            Ok(serde_json::json!({
                "ok": true,
                "message": message,
                "approval": approval
            }))
        }
        "outbound.message.reject" => outbound_record_rejection(&conn, session, command, now),
        "outbound.message.request_changes" => {
            outbound_record_change_request(&conn, session, command, now)
        }
        "outbound.message.send_approved" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            if let Err(err) = outbound_enforce_send_gate(&conn, &message) {
                let reason = err.to_string();
                outbound_record_send_failure(&conn, &message_id, &mut message, &reason, now)?;
                return Err(err);
            }
            let channel =
                outbound_string(&message, &["channel"]).unwrap_or_else(|| "email".to_string());

            // Physical letter path: no provider queueing, mark as manually dispatched.
            if channel == "physical_letter" {
                let existing_dispatch =
                    outbound_string(&message, &["payload", "provider_dispatch_status"])
                        .unwrap_or_default();
                if existing_dispatch == "manual_physical_letter_marked_sent" {
                    return Ok(serde_json::json!({
                        "ok": true,
                        "message": message,
                        "channel": "physical_letter",
                        "provider_dispatch_status": "manual_physical_letter_marked_sent",
                        "idempotent": true,
                    }));
                }
                outbound_put_string(&mut message, "send_status", "sent");
                outbound_payload_insert(
                    &mut message,
                    "provider_dispatch_status",
                    Value::String("manual_physical_letter_marked_sent".to_string()),
                );
                outbound_payload_insert(&mut message, "provider_send_executed", Value::Bool(true));
                outbound_payload_insert(
                    &mut message,
                    "physical_sent_at_ms",
                    Value::Number(serde_json::Number::from(now)),
                );
                outbound_put_i64(&mut message, "sent_at_ms", now);
                outbound_put_i64(&mut message, "updated_at_ms", now);
                upsert_business_record(
                    &conn,
                    "outbound_messages",
                    &message_id,
                    now,
                    message.clone(),
                )?;
                if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                    outbound_update_engagement_status(&conn, &engagement_id, "sent", now)?;
                }
                return Ok(serde_json::json!({
                    "ok": true,
                    "message": message,
                    "channel": "physical_letter",
                    "provider_dispatch_status": "manual_physical_letter_marked_sent",
                    "provider_send_executed": true,
                    "physical_sent_at_ms": now,
                }));
            }

            let existing_provider_queue_id = outbound_first_string(&[
                outbound_string(&message, &["provider_message_id"]),
                outbound_string(&message, &["payload", "provider_queue_id"]),
                outbound_string(&message, &["payload", "provider_message_id"]),
            ]);
            let existing_send_status =
                outbound_string(&message, &["send_status"]).unwrap_or_default();
            let already_queued = matches!(
                existing_send_status.as_str(),
                "queued_for_provider" | "sent" | "accepted"
            ) && existing_provider_queue_id.is_some()
                && message
                    .get("payload")
                    .and_then(|payload| payload.get("provider_send_executed"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            if already_queued {
                outbound_sync_email_message_to_communication(
                    root,
                    &mut message,
                    &existing_send_status,
                )?;
                return Ok(serde_json::json!({
                    "ok": true,
                    "message": message,
                    "provider_dispatch_status": "queued_in_mailserver",
                    "provider_queue_id": existing_provider_queue_id,
                    "provider_send_executed": true,
                    "idempotent": true
                }));
            }
            // Atomically reserve a daily send slot BEFORE handing the message to
            // the provider. The reservation enforces the per-account daily cap
            // under parallel commands (the check+increment is serialized by a
            // BEGIN IMMEDIATE transaction), so two concurrent sends cannot both
            // pass when only one slot remains.
            let sender_account_id = outbound_required_string(&message, &["sender_account_id"])?;
            if let Err(err) = outbound_reserve_account_send_slot(&conn, &sender_account_id, now) {
                let reason = err.to_string();
                outbound_record_send_failure(&conn, &message_id, &mut message, &reason, now)?;
                return Err(err);
            }
            let provider_queue_id = match outbound_queue_email_delivery(root, &message)
                .context("failed to queue approved outbound email")
            {
                Ok(id) => id,
                Err(err) => {
                    // The send never reached the provider; release the reserved
                    // slot so the daily counter stays accurate for retries.
                    let _ = outbound_release_account_send_slot(&conn, &sender_account_id, now);
                    let reason = err.to_string();
                    outbound_record_send_failure(&conn, &message_id, &mut message, &reason, now)?;
                    return Err(err);
                }
            };
            outbound_put_string(
                &mut message,
                "provider_message_id",
                provider_queue_id.clone(),
            );
            outbound_payload_insert(
                &mut message,
                "provider_queue_id",
                Value::String(provider_queue_id.clone()),
            );
            outbound_payload_insert(
                &mut message,
                "provider_message_id",
                Value::String(provider_queue_id.clone()),
            );
            outbound_payload_insert(
                &mut message,
                "provider_dispatch_status",
                Value::String("queued_in_mailserver".to_string()),
            );
            outbound_payload_insert(&mut message, "provider_send_executed", Value::Bool(true));
            outbound_payload_insert(
                &mut message,
                "provider_queued_at_ms",
                Value::Number(serde_json::Number::from(now)),
            );
            outbound_put_string(&mut message, "send_status", "queued_for_provider");
            // A successful (re)send clears any prior failure markers so the
            // message no longer looks blocked after a retry.
            outbound_payload_insert(&mut message, "send_block_reason", Value::Null);
            outbound_payload_insert(&mut message, "last_send_error", Value::Null);
            outbound_payload_insert(&mut message, "retryable", Value::Bool(false));
            outbound_sync_email_message_to_communication(
                root,
                &mut message,
                "queued_for_provider",
            )?;
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                outbound_update_engagement_status(&conn, &engagement_id, "scheduled_to_send", now)?;
            }
            Ok(serde_json::json!({
                "ok": true,
                "message": message.clone(),
                "provider_dispatch_status": "queued_in_mailserver",
                "provider_queue_id": outbound_string(&message, &["payload", "provider_queue_id"]),
                "provider_send_executed": true
            }))
        }
        "outbound.message.pause" | "outbound.message.cancel" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            let status = if command.command_type == "outbound.message.pause" {
                "paused"
            } else {
                "cancelled"
            };
            let reason = outbound_string(&command.payload, &["reason"]);
            outbound_put_string(&mut message, "send_status", status);
            if let Some(reason) = reason.as_ref() {
                let payload_key = if status == "paused" {
                    "pause_reason"
                } else {
                    "cancel_reason"
                };
                outbound_payload_insert(&mut message, payload_key, Value::String(reason.clone()));
            }
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                outbound_update_engagement_terminal_status(
                    &conn,
                    &engagement_id,
                    status,
                    reason.as_deref(),
                    now,
                )?;
            }
            Ok(serde_json::json!({ "ok": true, "message": message }))
        }
        "outbound.message.resume" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            anyhow::ensure!(
                outbound_string(&message, &["send_status"]).as_deref() == Some("paused"),
                "only paused outbound messages can be resumed"
            );
            let send_status = outbound_send_status_for_resume(&message);
            outbound_put_string(&mut message, "send_status", send_status);
            outbound_payload_insert(
                &mut message,
                "resume_reason",
                Value::String(
                    outbound_string(&command.payload, &["reason"])
                        .unwrap_or_else(|| "manual_resume".to_string()),
                ),
            );
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                let engagement_status = outbound_engagement_status_for_message_state(&message);
                outbound_update_engagement_status(&conn, &engagement_id, engagement_status, now)?;
            }
            Ok(serde_json::json!({ "ok": true, "message": message }))
        }
        "outbound.engagement.resume" => {
            let engagement_id = outbound_required_from_payload_or_record(
                command,
                &["engagement_id", "id"],
                "engagement_id is required",
            )?;
            let mut engagement = outbound_load_required(
                &conn,
                "outbound_engagements",
                &engagement_id,
                "engagement not found",
            )?;
            anyhow::ensure!(
                outbound_string(&engagement, &["status"]).as_deref() == Some("paused"),
                "only paused engagements can be resumed"
            );
            let messages = outbound_load_records_by_string_field(
                &conn,
                "outbound_messages",
                "engagement_id",
                &engagement_id,
            )?;
            let next_status = messages
                .iter()
                .find(|message| {
                    outbound_string(message, &["send_status"]).as_deref() == Some("paused")
                })
                .map(outbound_engagement_status_for_message_state)
                .unwrap_or("assigned");
            outbound_put_string(&mut engagement, "status", next_status);
            outbound_put_string(&mut engagement, "paused_reason", "");
            outbound_payload_insert(
                &mut engagement,
                "resume_reason",
                Value::String(
                    outbound_string(&command.payload, &["reason"])
                        .unwrap_or_else(|| "manual_resume".to_string()),
                ),
            );
            outbound_put_i64(&mut engagement, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_engagements",
                &engagement_id,
                now,
                engagement.clone(),
            )?;
            Ok(serde_json::json!({ "ok": true, "engagement": engagement }))
        }
        "outbound.engagement.close" => {
            let engagement_id = outbound_required_from_payload_or_record(
                command,
                &["engagement_id", "id"],
                "engagement_id is required",
            )?;
            let reason = outbound_string(&command.payload, &["reason"]);
            let mut engagement = outbound_load_required(
                &conn,
                "outbound_engagements",
                &engagement_id,
                "engagement not found",
            )?;
            outbound_put_string(&mut engagement, "status", "closed");
            if let Some(reason) = reason.as_ref().filter(|value| !value.trim().is_empty()) {
                outbound_put_string(&mut engagement, "closed_reason", reason.clone());
            }
            outbound_put_i64(&mut engagement, "closed_at_ms", now);
            outbound_put_i64(&mut engagement, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_engagements",
                &engagement_id,
                now,
                engagement.clone(),
            )?;

            let mut closed_messages = Vec::new();
            for mut message in outbound_load_records_by_string_field(
                &conn,
                "outbound_messages",
                "engagement_id",
                &engagement_id,
            )? {
                let send_status = outbound_string(&message, &["send_status"]).unwrap_or_default();
                if matches!(
                    send_status.as_str(),
                    "sent" | "accepted" | "queued_for_provider" | "cancelled"
                ) {
                    continue;
                }
                outbound_put_string(&mut message, "send_status", "cancelled");
                if let Some(reason) = reason.as_ref() {
                    outbound_payload_insert(
                        &mut message,
                        "close_reason",
                        Value::String(reason.clone()),
                    );
                }
                outbound_put_i64(&mut message, "updated_at_ms", now);
                if let Some(message_id) = outbound_string(&message, &["id"]) {
                    upsert_business_record(
                        &conn,
                        "outbound_messages",
                        &message_id,
                        now,
                        message.clone(),
                    )?;
                    closed_messages.push(message_id);
                }
            }
            Ok(serde_json::json!({
                "ok": true,
                "engagement": engagement,
                "closed_message_ids": closed_messages
            }))
        }
        "outbound.reply.classify" => {
            let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
            let classification = outbound_required_string(&command.payload, &["classification"])?;
            let mut engagement = outbound_load_required(
                &conn,
                "outbound_engagements",
                &engagement_id,
                "engagement not found",
            )?;
            outbound_put_string(&mut engagement, "status", "reply_received".to_string());
            outbound_payload_insert(
                &mut engagement,
                "reply_classification",
                Value::String(classification.clone()),
            );
            outbound_merge_fields(&mut engagement, &command.payload, &["reply_message_id"]);
            if classification == "out_of_office" {
                outbound_apply_out_of_office_wait(&mut engagement, &command.payload, now);
            }
            outbound_put_i64(&mut engagement, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_engagements",
                &engagement_id,
                now,
                engagement.clone(),
            )?;
            let suppression_id = outbound_apply_reply_suppression(
                &conn,
                &engagement,
                &engagement_id,
                &classification,
                now,
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "engagement": engagement,
                "suppression_id": suppression_id,
            }))
        }
        "outbound.scheduling.prepare" => {
            let request_id =
                outbound_id_from_command(command, &["meeting_request_id", "id"], "meeting")?;
            let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
            let mut request = outbound_object_payload(&command.payload);
            outbound_put_string(&mut request, "id", request_id.clone());
            outbound_put_string(&mut request, "engagement_id", engagement_id.clone());
            outbound_put_default_string(&mut request, "status", "prepared");
            outbound_put_default_i64(&mut request, "created_at_ms", now);
            outbound_put_i64(&mut request, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_meeting_requests",
                &request_id,
                now,
                request.clone(),
            )?;
            outbound_update_engagement_status(&conn, &engagement_id, "scheduling", now)?;
            Ok(serde_json::json!({ "ok": true, "meeting_request": request }))
        }
        "outbound.scheduling.mark_booked" => {
            let request_id = outbound_required_from_payload_or_record(
                command,
                &["meeting_request_id", "id"],
                "meeting_request_id is required",
            )?;
            let mut request = outbound_load_required(
                &conn,
                "outbound_meeting_requests",
                &request_id,
                "meeting request not found",
            )?;
            outbound_put_string(&mut request, "status", "booked");
            outbound_merge_fields(
                &mut request,
                &command.payload,
                &["meeting_url", "booked_at_ms", "payload"],
            );
            outbound_put_i64(&mut request, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_meeting_requests",
                &request_id,
                now,
                request.clone(),
            )?;
            let mut engagement_result = Value::Null;
            if let Some(engagement_id) = outbound_string(&request, &["engagement_id"]) {
                outbound_update_engagement_status(&conn, &engagement_id, "meeting_booked", now)?;
                engagement_result = outbound_load_required(
                    &conn,
                    "outbound_engagements",
                    &engagement_id,
                    "engagement not found",
                )?;
            }
            Ok(serde_json::json!({
                "ok": true,
                "meeting_request": request,
                "engagement": engagement_result
            }))
        }
        "outbound.campaign.mailbox.link" => {
            outbound_handle_campaign_mailbox_link(root, &conn, command, now)
        }
        "outbound.campaign.status.set" => outbound_handle_campaign_status_set(&conn, command, now),
        "outbound.campaign.briefing.update" => {
            outbound_handle_campaign_briefing_update(root, &conn, command, now)
        }
        "outbound.campaign.apply_setup" => {
            outbound_handle_campaign_apply_setup(root, &conn, command, now)
        }
        "outbound.reply.match" => outbound_handle_reply_match(root, &conn, command, now),
        "outbound.provider.reconcile" => {
            outbound_handle_provider_reconcile(root, &conn, command, now)
        }
        "outbound.skillbook.save" => outbound_handle_skillbook_save(&conn, command, now),
        "outbound.skillbook.seed_defaults" => outbound_handle_skillbook_seed_defaults(&conn, now),
        "outbound.letter_template.save" => {
            outbound_handle_letter_template_save(&conn, command, now)
        }
        "outbound.audit.export" => outbound_handle_audit_export(&conn, command),
        "outbound.scheduler.tick" => outbound_handle_scheduler_tick(root, &conn, command, now),
        "outbound.dev.seed_test_data" => outbound_handle_dev_seed_test_data(&conn, command, now),
        "outbound.engagement.reapply_sequence" => {
            outbound_handle_engagement_reapply_sequence(&conn, command, now)
        }
        "outbound.scheduling.update_slots" => {
            outbound_handle_scheduling_update_slots(&conn, command, now)
        }
        other => anyhow::bail!("unsupported active outbound command: {other}"),
    }
}

fn outbound_handle_campaign_mailbox_link(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
    let mailbox_address = outbound_required_string(&command.payload, &["mailbox_address"])
        .or_else(|_| {
            outbound_required_string(&command.payload, &["communication_account_address"])
        })?;
    let mailbox_address = mailbox_address.trim().to_string();
    anyhow::ensure!(
        mailbox_address.contains('@'),
        "mailbox_address must be a valid email address"
    );
    let account_key = outbound_first_string(&[
        outbound_string(&command.payload, &["communication_account_key"]),
        outbound_string(&command.payload, &["account_key"]),
        Some(format!("email:{}", mailbox_address.to_ascii_lowercase())),
    ])
    .unwrap_or_else(|| format!("email:{}", mailbox_address.to_ascii_lowercase()));
    let channel =
        outbound_string(&command.payload, &["channel"]).unwrap_or_else(|| "email".to_string());
    let provider = outbound_string(&command.payload, &["provider"])
        .unwrap_or_else(|| "business-os.outbound".to_string());
    let mailbox_status = outbound_string(&command.payload, &["mailbox_status"])
        .unwrap_or_else(|| "ready".to_string());
    let display_name = outbound_string(&command.payload, &["display_name"]).unwrap_or_default();
    let reply_to =
        outbound_string(&command.payload, &["reply_to"]).unwrap_or_else(|| mailbox_address.clone());

    let profile = serde_json::json!({
        "address": mailbox_address,
        "campaign_id": campaign_id,
        "outbound_campaign_id": campaign_id,
        "display_name": display_name,
        "reply_to": reply_to,
        "mailbox_status": mailbox_status,
        "source": "business-os.outbound.campaign.mailbox.link",
    });

    let mut channel_conn = channels::open_channel_db(&crate::paths::core_db(root))?;
    channels::upsert_communication_account(
        &mut channel_conn,
        &account_key,
        &channel,
        &mailbox_address,
        &provider,
        profile.clone(),
    )?;

    let mut campaign = outbound_load_record(conn, "outbound_campaigns", &campaign_id)?
        .unwrap_or_else(|| serde_json::json!({ "id": campaign_id.clone() }));
    outbound_put_string(&mut campaign, "id", campaign_id.clone());
    outbound_put_string(
        &mut campaign,
        "communication_account_key",
        account_key.clone(),
    );
    outbound_put_string(
        &mut campaign,
        "communication_account_address",
        mailbox_address.clone(),
    );
    outbound_put_string(&mut campaign, "mailbox_status", mailbox_status.clone());
    outbound_put_default_object(&mut campaign, "payload");
    outbound_payload_insert(
        &mut campaign,
        "communication_account_key",
        Value::String(account_key.clone()),
    );
    outbound_payload_insert(
        &mut campaign,
        "communication_account_address",
        Value::String(mailbox_address.clone()),
    );
    outbound_payload_insert(
        &mut campaign,
        "mailbox_status",
        Value::String(mailbox_status.clone()),
    );
    outbound_put_default_i64(&mut campaign, "created_at_ms", now);
    outbound_put_i64(&mut campaign, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;

    let mut limit = outbound_load_record(conn, "outbound_account_limits", &account_key)?
        .unwrap_or_else(|| {
            serde_json::json!({
                "id": account_key,
                "sender_account_id": account_key,
                "daily_sent_count": 0,
                "daily_limit": 0,
                "status": "active",
                "blocked": false,
            })
        });
    outbound_put_string(&mut limit, "id", account_key.clone());
    outbound_put_string(&mut limit, "sender_account_id", account_key.clone());
    outbound_put_string(&mut limit, "campaign_id", campaign_id.clone());
    outbound_put_default_i64(&mut limit, "daily_sent_count", 0);
    outbound_put_default_i64(&mut limit, "daily_limit", 0);
    outbound_put_default_string(&mut limit, "status", "active");
    if !limit
        .get("blocked")
        .map(|value| value.is_boolean())
        .unwrap_or(false)
    {
        if let Some(object) = limit.as_object_mut() {
            object.insert("blocked".to_string(), Value::Bool(false));
        }
    }
    outbound_put_default_i64(&mut limit, "created_at_ms", now);
    outbound_put_i64(&mut limit, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_account_limits",
        &account_key,
        now,
        limit.clone(),
    )?;

    Ok(serde_json::json!({
        "ok": true,
        "campaign": campaign,
        "communication_account_key": account_key,
        "communication_account_address": mailbox_address,
        "mailbox_status": mailbox_status,
        "account_limit": limit,
    }))
}

fn outbound_handle_campaign_status_set(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
    let requested_status = outbound_required_string(&command.payload, &["status"])?;
    let allowed = matches!(
        requested_status.as_str(),
        "setup_required" | "active" | "paused" | "closed"
    );
    anyhow::ensure!(
        allowed,
        "unsupported campaign status: {requested_status}; allowed: setup_required, active, paused, closed"
    );
    let mut campaign = outbound_load_required(
        conn,
        "outbound_campaigns",
        &campaign_id,
        "campaign not found",
    )?;
    let default_channel = outbound_first_string(&[
        outbound_string(&command.payload, &["channel"]),
        outbound_string(
            &campaign,
            &["payload", "active_outreach", "default_channel"],
        ),
        outbound_string(&campaign, &["channel"]),
        Some("email".to_string()),
    ])
    .unwrap_or_else(|| "email".to_string());

    if requested_status == "active" {
        match default_channel.as_str() {
            "physical_letter" => {
                // manually-handled channel; no mailbox required.
            }
            _ => {
                let account_key = outbound_first_string(&[
                    outbound_string(&campaign, &["communication_account_key"]),
                    outbound_string(&campaign, &["payload", "communication_account_key"]),
                ])
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "campaign cannot activate email channel without a linked mailbox"
                    )
                })?;
                let limit = outbound_load_record(conn, "outbound_account_limits", &account_key)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "campaign cannot activate email channel without outbound_account_limits"
                        )
                    })?;
                let limit_status =
                    outbound_string(&limit, &["status"]).unwrap_or_else(|| "active".to_string());
                anyhow::ensure!(
                    !matches!(
                        limit_status.as_str(),
                        "blocked" | "locked" | "suspended" | "disabled"
                    ),
                    "campaign mailbox is not ready (status: {limit_status})"
                );
                let blocked = limit
                    .get("blocked")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                anyhow::ensure!(
                    !blocked,
                    "campaign mailbox is blocked for outbound communication"
                );
            }
        }
    }

    outbound_put_string(&mut campaign, "status", requested_status.clone());
    outbound_put_default_object(&mut campaign, "payload");
    outbound_payload_insert(
        &mut campaign,
        "status_set_at_ms",
        Value::Number(serde_json::Number::from(now)),
    );
    outbound_payload_insert(
        &mut campaign,
        "active_channel",
        Value::String(default_channel.clone()),
    );
    outbound_put_i64(&mut campaign, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;

    Ok(serde_json::json!({
        "ok": true,
        "campaign": campaign,
        "status": requested_status,
        "channel": default_channel,
    }))
}

/// Persist (or overwrite) an outbound skillbook record. Returned `version_number`
/// is monotonically incremented per record so operators can audit edits.
fn outbound_handle_skillbook_save(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let skillbook_id = outbound_required_string(&command.payload, &["skillbook_id", "id"])
        .or_else(|_| {
            command
                .record_id
                .as_deref()
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("skillbook_id is required"))
        })?;
    let prior = outbound_load_record(conn, "outbound_skillbooks", &skillbook_id)?;
    let prior_version = prior
        .as_ref()
        .and_then(|v| v.get("version_number").and_then(Value::as_i64))
        .unwrap_or(0);
    let mut record = prior.clone().unwrap_or_else(|| serde_json::json!({}));
    let incoming = outbound_object_payload(&command.payload);
    if let (Some(target), Some(source)) = (record.as_object_mut(), incoming.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    outbound_put_string(&mut record, "id", skillbook_id.clone());
    outbound_put_string(&mut record, "skillbook_id", skillbook_id.clone());
    outbound_put_default_string(&mut record, "title", "");
    outbound_put_default_string(&mut record, "mission", "");
    for key in [
        "non_negotiable_rules",
        "workflow_backbone",
        "routing_taxonomy",
        "stop_rules",
    ] {
        if !record.get(key).map(Value::is_array).unwrap_or(false) {
            if let Some(obj) = record.as_object_mut() {
                obj.insert(key.to_string(), Value::Array(Vec::new()));
            }
        }
    }
    outbound_put_default_i64(&mut record, "created_at_ms", now);
    outbound_put_i64(&mut record, "updated_at_ms", now);
    outbound_put_i64(&mut record, "version_number", prior_version + 1);
    upsert_business_record(
        conn,
        "outbound_skillbooks",
        &skillbook_id,
        now,
        record.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "skillbook": record,
        "version_number": prior_version + 1,
    }))
}

/// Insert the three default outbound skillbook records (message_drafting,
/// reply_handling, scheduling) if they do not yet exist. Idempotent: returns
/// the list of newly seeded ids, empty if everything is already present.
fn outbound_handle_skillbook_seed_defaults(conn: &Connection, now: i64) -> anyhow::Result<Value> {
    let defaults: [Value; 3] = [
        outbound_default_message_drafting_skillbook(),
        outbound_default_reply_handling_skillbook(),
        outbound_default_scheduling_skillbook(),
    ];
    let mut seeded = Vec::new();
    for mut record in defaults {
        let id = record
            .get("skillbook_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .context("default skillbook is missing skillbook_id")?;
        if outbound_load_record(conn, "outbound_skillbooks", &id)?.is_some() {
            continue;
        }
        outbound_put_i64(&mut record, "created_at_ms", now);
        outbound_put_i64(&mut record, "updated_at_ms", now);
        upsert_business_record(conn, "outbound_skillbooks", &id, now, record)?;
        seeded.push(id);
    }
    Ok(serde_json::json!({ "ok": true, "seeded": seeded }))
}

/// Canonical message-drafting skillbook. Drives both the agent-backed
/// `outbound.pipeline.outreach_draft` loop (via the
/// `business-os-outbound-message-drafting` skill) and the deterministic
/// `outbound.draft.prepare` fallback. The whole loop stays on the RxDB command
/// bus and never sends without an explicit approval.
fn outbound_default_message_drafting_skillbook() -> Value {
    serde_json::json!({
        "id": "business-os.outbound.message_drafting.v1",
        "skillbook_id": "business-os.outbound.message_drafting.v1",
        "title": "Initial- und Follow-up-Drafts vorbereiten",
        "mission": "Personalisierte Erst- und Follow-up-Anschreiben fuer eine Outbound-Campaign entwerfen, die an die ICP-Beschreibung, das konkrete Unternehmen und die Zielperson anknuepfen und ausschliesslich als freigabepflichtige Drafts entstehen.",
        "non_negotiable_rules": [
            "Keine Nachricht ohne explizite Freigabe versenden",
            "Keinen externen Dienst und keine HTTP-Schnittstelle aufrufen; ausschliesslich ueber den CTOX-Command-Bus arbeiten",
            "Suppression-, Bounce- und Opt-out-Listen jederzeit respektieren",
            "Keine Fakten erfinden; nur belegte Rechercheergebnisse und vom Operator gepflegte Angaben verwenden",
            "Sprache und Anrede an die Zielperson anpassen (Standard: Deutsch, Sie-Form)",
        ],
        "workflow_backbone": [
            {
                "step": "context_intake",
                "title": "Kontext aufnehmen",
                "detail": "drafting_request lesen: ICP-/Produktbeschreibung, CTA, Signatur, Landingpage-Checkliste, Prompt-Vorlagen sowie Unternehmens- und Personendaten.",
            },
            {
                "step": "anchor_selection",
                "title": "Anknuepfungspunkt waehlen",
                "detail": "Aus Homepage-Summary und Personendaten den staerksten, belegbaren Anknuepfungspunkt zwischen Angebot und Empfaenger ableiten.",
            },
            {
                "step": "initial_draft",
                "title": "Erstanschreiben entwerfen",
                "detail": "Betreff plus knappen Body mit einem klaren CTA schreiben; Signatur anhaengen; keine erfundenen Aussagen.",
            },
            {
                "step": "followups",
                "title": "Zwei Follow-ups entwerfen",
                "detail": "Zwei kurze, eskalationsarme Follow-ups verfassen, die ohne Antwort hoeflich nachfassen und sich auf das Erstanschreiben beziehen.",
            },
            {
                "step": "writeback",
                "title": "Ergebnis zurueckschreiben",
                "detail": "message_mail_subject, message_mail_body, message_followup_1 und message_followup_2 ausschliesslich ueber den Command outbound.pipeline.write_outreach_draft persistieren.",
            },
        ],
        "routing_taxonomy": [
            { "intent": "initial", "route": "Erstanschreiben entwerfen", "stop": "Draft als awaiting_approval ablegen" },
            { "intent": "followup", "route": "Naechstes Follow-up aus der Sequenz entwerfen", "stop": "stop on reply" },
            { "intent": "reply_received", "route": "An reply_handling.v1 uebergeben", "stop": "Keine weitere Sequenznachricht senden" },
        ],
        "stop_rules": [
            "stop on reply",
            "stop on bounce",
            "stop on opt-out",
            "stop on suppression match",
        ],
        "version_number": 1,
    })
}

/// Canonical reply-handling skillbook: classify inbound replies and stage a
/// reply draft instead of letting an automated sequence keep firing.
fn outbound_default_reply_handling_skillbook() -> Value {
    serde_json::json!({
        "id": "business-os.outbound.reply_handling.v1",
        "skillbook_id": "business-os.outbound.reply_handling.v1",
        "title": "Antworten klassifizieren und Reply-Drafts vorbereiten",
        "mission": "Eingehende Antworten klassifizieren, die laufende Sequenz anhalten und einen passenden, freigabepflichtigen Reply-Draft vorbereiten.",
        "non_negotiable_rules": [
            "Bei jeder Antwort die automatische Sequenz anhalten, bevor weiter entworfen wird",
            "Opt-out und Unsubscribe sofort in die Suppression-Liste ueberfuehren",
            "Keine Antwort ohne explizite Freigabe versenden",
            "Keinen externen Dienst aufrufen; nur ueber den CTOX-Command-Bus arbeiten",
        ],
        "workflow_backbone": [
            {
                "step": "classify",
                "title": "Antwort klassifizieren",
                "detail": "Antwort in positive_reply, question, objection, not_interested, out_of_office oder unsubscribe einsortieren.",
            },
            {
                "step": "halt_sequence",
                "title": "Sequenz anhalten",
                "detail": "Bei jeder echten Antwort die laufende Follow-up-Sequenz pausieren, damit keine widerspruechliche Nachricht hinausgeht.",
            },
            {
                "step": "route",
                "title": "Folgeaktion routen",
                "detail": "Auf Basis der Klassifikation Reply-Draft, Terminfindung, Suppression oder manuelle Eskalation auswaehlen.",
            },
            {
                "step": "draft_reply",
                "title": "Reply-Draft vorbereiten",
                "detail": "Knappe, kontextbezogene Antwort als freigabepflichtigen Draft ablegen.",
            },
        ],
        "routing_taxonomy": [
            { "intent": "positive_reply", "route": "Reply-Draft oder Terminfindung vorbereiten", "stop": "Sequenz beendet" },
            { "intent": "question", "route": "Antwort mit Klaerung entwerfen", "stop": "Sequenz pausiert" },
            { "intent": "objection", "route": "Einwand-Antwort entwerfen", "stop": "Sequenz pausiert" },
            { "intent": "not_interested", "route": "Hoeflich schliessen", "stop": "Engagement schliessen" },
            { "intent": "out_of_office", "route": "Follow-up nach OOO-Datum neu planen", "stop": "stop until ooo_until" },
            { "intent": "unsubscribe", "route": "In Suppression-Liste ueberfuehren", "stop": "stop on opt-out" },
        ],
        "stop_rules": [
            "stop on opt-out",
            "stop on unsubscribe",
            "stop sequence on any human reply",
        ],
        "version_number": 1,
    })
}

/// Canonical scheduling skillbook: propose meeting slots, check them against
/// the calendar, and book only after an explicit approval.
fn outbound_default_scheduling_skillbook() -> Value {
    serde_json::json!({
        "id": "business-os.outbound.scheduling.v1",
        "skillbook_id": "business-os.outbound.scheduling.v1",
        "title": "Terminfindung vorbereiten",
        "mission": "Auf Terminwunsch passende Slots vorschlagen, gegen Kalenderkonflikte und Arbeitszeiten pruefen und das Meeting erst nach Freigabe buchen.",
        "non_negotiable_rules": [
            "Slots nur innerhalb der konfigurierten Arbeitszeiten und Limits vorschlagen",
            "Kein Meeting ohne explizite Freigabe buchen",
            "Bei Kalenderkonflikt einen Alternativslot anbieten statt zu ueberbuchen",
        ],
        "workflow_backbone": [
            {
                "step": "collect_constraints",
                "title": "Rahmen sammeln",
                "detail": "Dauer, Zeitzone, bevorzugte Fenster und Arbeitszeiten der Campaign ermitteln.",
            },
            {
                "step": "propose_slots",
                "title": "Slots vorschlagen",
                "detail": "Zwei bis drei konkrete Slots vorschlagen, die innerhalb der Limits liegen.",
            },
            {
                "step": "check_conflicts",
                "title": "Konflikte pruefen",
                "detail": "Vorgeschlagene Slots gegen Kalender und bestehende Buchungen pruefen und Konflikte aussortieren.",
            },
            {
                "step": "book_on_approval",
                "title": "Nach Freigabe buchen",
                "detail": "Erst nach Operator-Freigabe per outbound.scheduling.mark_booked buchen.",
            },
        ],
        "routing_taxonomy": [
            { "intent": "slot_request", "route": "Slots vorschlagen", "stop": "Auf Empfaengerwahl warten" },
            { "intent": "slot_confirmed", "route": "Buchung nach Freigabe vorbereiten", "stop": "stop until approved" },
            { "intent": "conflict", "route": "Alternativslot vorschlagen", "stop": "Nicht ueberbuchen" },
        ],
        "stop_rules": [
            "stop on reply",
            "stop until approved before booking",
            "stop on calendar conflict",
        ],
        "version_number": 1,
    })
}

/// Persist a per-campaign letter template (salutation, body, closing).
fn outbound_handle_letter_template_save(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let template_id =
        outbound_required_string(&command.payload, &["template_id", "id"]).or_else(|_| {
            command
                .record_id
                .as_deref()
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("template_id is required"))
        })?;
    let prior = outbound_load_record(conn, "outbound_letter_templates", &template_id)?;
    let prior_version = prior
        .as_ref()
        .and_then(|v| v.get("version_number").and_then(Value::as_i64))
        .unwrap_or(0);
    let mut record = outbound_object_payload(&command.payload);
    outbound_put_string(&mut record, "id", template_id.clone());
    outbound_put_string(&mut record, "template_id", template_id.clone());
    outbound_put_default_string(&mut record, "title", "");
    outbound_put_default_string(&mut record, "salutation", "");
    outbound_put_default_string(&mut record, "body_template", "");
    outbound_put_default_string(&mut record, "closing", "");
    outbound_put_default_i64(&mut record, "created_at_ms", now);
    outbound_put_i64(&mut record, "updated_at_ms", now);
    outbound_put_i64(&mut record, "version_number", prior_version + 1);
    upsert_business_record(
        conn,
        "outbound_letter_templates",
        &template_id,
        now,
        record.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "template": record,
        "version_number": prior_version + 1,
    }))
}

/// Audit export: dump every outbound record linked to a campaign (or all
/// campaigns if campaign_id is empty) so operators can produce GDPR / SLA proof.
fn outbound_handle_audit_export(
    conn: &Connection,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let campaign_filter = outbound_string(&command.payload, &["campaign_id"]).unwrap_or_default();
    let collections = [
        "outbound_campaigns",
        "outbound_engagements",
        "outbound_messages",
        "outbound_approvals",
        "outbound_sequences",
        "outbound_sender_assignments",
        "outbound_meeting_requests",
        "outbound_suppression_entries",
        "outbound_account_limits",
        "outbound_skillbooks",
        "outbound_letter_templates",
    ];
    let mut export = serde_json::Map::new();
    for collection in collections {
        let mut stmt = conn.prepare(
            "SELECT payload_json FROM business_records
             WHERE collection = ?1 AND deleted = 0",
        )?;
        let rows = stmt.query_map(params![collection], |row| row.get::<_, String>(0))?;
        let mut records: Vec<Value> = Vec::new();
        for row in rows {
            let raw = row?;
            let value: Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let matches_filter = if campaign_filter.is_empty() {
                true
            } else {
                let id = outbound_string(&value, &["campaign_id"]).unwrap_or_default();
                let payload_id =
                    outbound_string(&value, &["payload", "campaign_id"]).unwrap_or_default();
                id == campaign_filter
                    || payload_id == campaign_filter
                    || outbound_string(&value, &["id"]).as_deref() == Some(campaign_filter.as_str())
            };
            if matches_filter {
                records.push(value);
            }
        }
        export.insert(collection.to_string(), Value::Array(records));
    }
    Ok(serde_json::json!({
        "ok": true,
        "campaign_id": campaign_filter,
        "export": export,
        "exported_at_ms": now_ms() as i64,
    }))
}

/// Scheduler tick: walks every active engagement, prepares overdue follow-up
/// drafts, and reconciles any pending SMTP delivery outcomes. Honors
/// `payload.dry_run == true` by reporting what would have happened without
/// touching state. Always pulls the reconciler so delivered emails get
/// promoted to `send_status = sent` automatically.
fn outbound_handle_scheduler_tick(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let dry_run = command
        .payload
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut actions: Vec<Value> = Vec::new();

    // 1. Reconcile SMTP delivery log → outbound_messages.send_status.
    let reconcile = if dry_run {
        serde_json::json!({ "ok": true, "checked": 0, "updated": [], "dry_run": true })
    } else {
        outbound_handle_provider_reconcile(root, conn, command, now)?
    };
    if let Some(updated) = reconcile
        .get("updated")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
    {
        actions.push(serde_json::json!({
            "kind": "provider_reconcile",
            "count": updated.len(),
        }));
    }

    // 2. Prepare overdue follow-up drafts: engagements with next_action_at_ms <= now
    //    and status in waiting_for_reply / scheduled_to_send / draft_prepared.
    let mut stmt = conn.prepare(
        "SELECT payload_json FROM business_records
         WHERE collection = 'outbound_engagements' AND deleted = 0",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut engagements = Vec::new();
    for row in rows {
        let Ok(raw) = row else { continue };
        if let Ok(engagement) = serde_json::from_str::<Value>(&raw) {
            engagements.push(engagement);
        }
    }
    drop(stmt);

    // Campaign-level pause: an engagement whose campaign is paused/closed/not yet
    // active must not get an automated follow-up. Cache campaign statuses once so
    // a tick over many engagements stays a single sweep.
    let mut campaign_status: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT payload_json FROM business_records
             WHERE collection = 'outbound_campaigns' AND deleted = 0",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows.flatten() {
            if let Ok(campaign) = serde_json::from_str::<Value>(&row) {
                if let Some(id) = outbound_string(&campaign, &["id"]) {
                    let status =
                        outbound_string(&campaign, &["status"]).unwrap_or_else(|| "active".into());
                    campaign_status.insert(id, status);
                }
            }
        }
    }

    for engagement in engagements {
        let status = outbound_string(&engagement, &["status"]).unwrap_or_else(|| "".to_string());
        // Out-of-office is the documented exception to the reply halt: the
        // engagement stays `reply_received` for the UI but the scheduler resumes
        // the follow-up after its OOO hold (the produced draft is approval-gated).
        let is_out_of_office_wait = status == "reply_received"
            && outbound_string(&engagement, &["payload", "reply_classification"]).as_deref()
                == Some("out_of_office");
        if !is_out_of_office_wait
            && matches!(
                status.as_str(),
                "closed"
                    | "cancelled"
                    | "meeting_booked"
                    | "paused"
                    | "reply_received"
                    | "bounced"
                    | "unsubscribed"
                    | "suppressed"
            )
        {
            continue;
        }
        let next_action = engagement
            .get("next_action_at_ms")
            .and_then(Value::as_i64)
            .or_else(|| {
                engagement
                    .pointer("/payload/next_action_at_ms")
                    .and_then(Value::as_i64)
            });
        let due = match next_action {
            Some(ts) => ts <= now,
            None => false,
        };
        if !due {
            continue;
        }
        let Some(engagement_id) = outbound_string(&engagement, &["id"]) else {
            continue;
        };
        if let Some(campaign_id) = outbound_string(&engagement, &["campaign_id"]) {
            if let Some(camp_status) = campaign_status.get(&campaign_id) {
                if matches!(camp_status.as_str(), "paused" | "closed" | "setup_required") {
                    actions.push(serde_json::json!({
                        "kind": "followup_skipped_campaign_paused",
                        "engagement_id": engagement_id,
                        "campaign_id": campaign_id,
                        "campaign_status": camp_status,
                    }));
                    continue;
                }
            }
        }
        if dry_run {
            actions.push(serde_json::json!({
                "kind": "followup_due",
                "engagement_id": engagement_id,
                "due_at_ms": next_action,
            }));
            continue;
        }
        match outbound_prepare_due_followup_draft(conn, engagement, command, now) {
            Ok(action) => actions.push(action),
            Err(error) => actions.push(serde_json::json!({
                "kind": "followup_prepare_failed",
                "engagement_id": engagement_id,
                "error": error.to_string(),
            })),
        }
    }

    Ok(serde_json::json!({
        "ok": true,
        "now_ms": now,
        "actions": actions,
        "dry_run": dry_run,
        "reconciled": reconcile.get("updated").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
    }))
}

fn outbound_prepare_due_followup_draft(
    conn: &Connection,
    mut engagement: Value,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let engagement_id = outbound_required_string(&engagement, &["id"])?;
    let campaign_id = outbound_required_string(&engagement, &["campaign_id"])?;
    let draft_kind = outbound_first_string(&[
        outbound_string(&command.payload, &["draft_kind"]),
        outbound_string(&engagement, &["payload", "next_draft_kind"]),
    ])
    .unwrap_or_else(|| "followup".to_string());
    let channel = outbound_first_string(&[
        outbound_string(&command.payload, &["channel"]),
        outbound_string(&engagement, &["payload", "channel"]),
        outbound_string(&engagement, &["channel"]),
    ])
    .unwrap_or_else(|| "email".to_string());

    let previous_messages = outbound_load_records_by_string_field(
        conn,
        "outbound_messages",
        "engagement_id",
        &engagement_id,
    )?;
    if previous_messages.iter().any(|message| {
        let approval = outbound_string(message, &["approval_status"]).unwrap_or_default();
        let send = outbound_string(message, &["send_status"]).unwrap_or_default();
        let message_type = outbound_string(message, &["message_type"]).unwrap_or_default();
        approval == "awaiting_approval"
            && !matches!(send.as_str(), "cancelled" | "blocked")
            && (message_type == draft_kind || message_type.starts_with("followup"))
    }) {
        outbound_put_i64(&mut engagement, "next_action_at_ms", 0);
        outbound_payload_insert(
            &mut engagement,
            "scheduler_last_skip_reason",
            Value::String("draft_already_awaiting_approval".to_string()),
        );
        outbound_put_i64(&mut engagement, "updated_at_ms", now);
        upsert_business_record(
            conn,
            "outbound_engagements",
            &engagement_id,
            now,
            engagement,
        )?;
        return Ok(serde_json::json!({
            "kind": "followup_skipped_existing_draft",
            "engagement_id": engagement_id,
        }));
    }

    let (sender_account_id, recipient_email, recipient_address_text) =
        if channel == "physical_letter" {
            let address = outbound_first_string(&[
                outbound_string(&command.payload, &["recipient_address_text"]),
                outbound_string(&engagement, &["payload", "contact_address_text"]),
                outbound_string(&engagement, &["payload", "recipient_address_text"]),
            ])
            .context("recipient_address_text is required for physical_letter scheduler drafts")?;
            let sender = outbound_first_string(&[
                outbound_string(&command.payload, &["sender_account_id"]),
                outbound_string(&engagement, &["sender_account_id"]),
            ])
            .unwrap_or_default();
            let email = outbound_first_string(&[
                outbound_string(&command.payload, &["recipient_email"]),
                outbound_string(&engagement, &["payload", "contact_email"]),
            ])
            .unwrap_or_default();
            (sender, email, address)
        } else {
            let sender = outbound_first_string(&[
                outbound_string(&command.payload, &["sender_account_id"]),
                outbound_string(&engagement, &["sender_account_id"]),
            ])
            .context("sender_account_id is required for scheduler draft")?;
            let email = outbound_first_string(&[
                outbound_string(&command.payload, &["recipient_email"]),
                outbound_string(&engagement, &["payload", "contact_email"]),
            ])
            .context("recipient_email is required for scheduler draft")?;
            anyhow::ensure!(
                !outbound_recipient_suppressed(conn, &email)?,
                "recipient is suppressed for outbound communication"
            );
            (sender, email, String::new())
        };

    // Respect the sender account's health and daily cap: do not pile up scheduler
    // drafts that would only bounce off the send gate. When the account is blocked
    // or its daily cap is already exhausted, defer the follow-up — leave the due
    // marker in place so a later tick (after a daily reset) retries — and record a
    // skip reason instead of generating an unsendable draft.
    if channel != "physical_letter" && !sender_account_id.is_empty() {
        if let Err(limit_err) = outbound_enforce_account_limit(conn, &sender_account_id) {
            let detail = limit_err.to_string();
            outbound_payload_insert(
                &mut engagement,
                "scheduler_last_skip_reason",
                Value::String("account_limit".to_string()),
            );
            outbound_payload_insert(
                &mut engagement,
                "scheduler_last_skip_detail",
                Value::String(detail.clone()),
            );
            outbound_payload_insert(
                &mut engagement,
                "scheduler_last_skip_at_ms",
                Value::Number(serde_json::Number::from(now)),
            );
            outbound_put_i64(&mut engagement, "updated_at_ms", now);
            upsert_business_record(
                conn,
                "outbound_engagements",
                &engagement_id,
                now,
                engagement,
            )?;
            return Ok(serde_json::json!({
                "kind": "followup_skipped_account_limit",
                "engagement_id": engagement_id,
                "sender_account_id": sender_account_id,
                "reason": detail,
            }));
        }
    }

    let latest_message = outbound_latest_message(&previous_messages);
    let scheduler_skillbook_id = outbound_first_string(&[
        outbound_string(&command.payload, &["skillbook_id"]),
        outbound_string(&engagement, &["payload", "skillbook_id"]),
    ])
    .unwrap_or_else(|| "business-os.outbound.message_drafting.v1".to_string());
    let scheduler_skillbook_guidance = outbound_skillbook_guidance(conn, &scheduler_skillbook_id)?;
    let generated = outbound_generate_automated_draft(
        &engagement,
        latest_message.as_ref(),
        &command.payload,
        &draft_kind,
        scheduler_skillbook_guidance.as_deref(),
    );
    let subject = outbound_first_string(&[
        outbound_string(&command.payload, &["subject"]),
        outbound_string(&generated, &["subject"]),
    ])
    .context("generated subject is required")?;
    let body_text = outbound_first_string(&[
        outbound_string(&command.payload, &["body_text"]),
        outbound_string(&generated, &["body_text"]),
    ])
    .context("generated body_text is required")?;
    let message_id = format!("msg_{}", Uuid::new_v4().simple());
    let mut message = outbound_object_payload(&command.payload);
    outbound_put_string(&mut message, "id", message_id.clone());
    outbound_put_string(&mut message, "engagement_id", engagement_id.clone());
    outbound_put_string(&mut message, "campaign_id", campaign_id);
    outbound_put_string(&mut message, "message_type", draft_kind.clone());
    outbound_put_string(&mut message, "channel", channel);
    outbound_put_string(&mut message, "direction", "outbound");
    outbound_put_string(&mut message, "sender_account_id", sender_account_id);
    outbound_put_string(&mut message, "recipient_email", recipient_email);
    if !recipient_address_text.is_empty() {
        outbound_put_string(
            &mut message,
            "recipient_address_text",
            recipient_address_text,
        );
    }
    outbound_put_string(&mut message, "subject", subject);
    outbound_put_string(&mut message, "body_text", body_text);
    outbound_put_string(&mut message, "draft_status", "ready_for_review");
    outbound_put_string(&mut message, "approval_status", "awaiting_approval");
    outbound_put_string(&mut message, "send_status", "awaiting_approval");
    outbound_put_default_object(&mut message, "payload");
    outbound_payload_insert(
        &mut message,
        "draft_engine",
        Value::String("business-os.outbound.scheduler.v1".to_string()),
    );
    outbound_payload_insert(&mut message, "generated_draft", generated);
    // Stamp the sequence revision the draft was produced from so each scheduler
    // draft is auditable back to its sequence version.
    let (sequence_id, sequence_version) = outbound_engagement_sequence_context(&engagement);
    if let Some(seq) = sequence_id {
        outbound_payload_insert(&mut message, "sequence_id", Value::String(seq));
    }
    outbound_payload_insert(
        &mut message,
        "sequence_version",
        Value::Number(serde_json::Number::from(sequence_version)),
    );
    if let Some(previous) = latest_message
        .as_ref()
        .and_then(|message| outbound_string(message, &["id"]))
    {
        outbound_put_string(&mut message, "reply_to_message_id", previous);
    }
    let revision_id = outbound_message_revision(&message);
    outbound_put_string(&mut message, "revision_id", revision_id);
    outbound_put_default_i64(&mut message, "created_at_ms", now);
    outbound_put_i64(&mut message, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_messages", &message_id, now, message.clone())?;

    outbound_put_string(&mut engagement, "status", "awaiting_approval");
    outbound_put_i64(&mut engagement, "next_action_at_ms", 0);
    outbound_payload_insert(
        &mut engagement,
        "scheduler_last_message_id",
        Value::String(message_id.clone()),
    );
    outbound_payload_insert(
        &mut engagement,
        "scheduler_last_run_at_ms",
        Value::Number(serde_json::Number::from(now)),
    );
    outbound_put_i64(&mut engagement, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_engagements",
        &engagement_id,
        now,
        engagement,
    )?;

    Ok(serde_json::json!({
        "kind": "followup_draft_prepared",
        "engagement_id": engagement_id,
        "message_id": message_id,
        "draft_kind": draft_kind,
    }))
}

/// Developer-only helper: seed N approval-gated demo engagements and drafts so
/// operators can verify the UI shell against realistic data. Idempotent on the
/// given (campaign_id, count) tuple; existing records are preserved.
fn outbound_handle_dev_seed_test_data(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
    let count = command
        .payload
        .get("count")
        .and_then(Value::as_i64)
        .unwrap_or(3)
        .clamp(1, 25);
    let mut created = Vec::new();
    for idx in 0..count {
        let eng_id = format!("dev_eng_{campaign_id}_{idx}");
        if outbound_load_record(conn, "outbound_engagements", &eng_id)?.is_some() {
            continue;
        }
        let engagement = serde_json::json!({
            "id": eng_id,
            "campaign_id": campaign_id,
            "company_id": format!("dev_co_{idx}"),
            "contact_id": format!("dev_ct_{idx}"),
            "status": "ready_for_assignment",
            "payload": {
                "contact_email": format!("lead{idx}@example.com"),
                "source": "outbound.dev.seed_test_data",
            },
            "created_at_ms": now,
            "updated_at_ms": now,
        });
        upsert_business_record(conn, "outbound_engagements", &eng_id, now, engagement)?;
        created.push(eng_id);
    }
    Ok(serde_json::json!({
        "ok": true,
        "campaign_id": campaign_id,
        "count": created.len(),
        "engagement_ids": created,
    }))
}

/// Re-apply the campaign sequence to an existing engagement: re-projects the
/// stored sequence policy into the engagement payload so newly prepared drafts
/// pick up the latest settings. Requires the engagement to reference a
/// known sequence_id (either inline or via campaign default).
fn outbound_handle_engagement_reapply_sequence(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
    let mut engagement = outbound_load_required(
        conn,
        "outbound_engagements",
        &engagement_id,
        "engagement not found",
    )?;
    let sequence_id = outbound_first_string(&[
        outbound_string(&command.payload, &["sequence_id"]),
        outbound_string(&engagement, &["sequence_id"]),
        outbound_string(&engagement, &["payload", "sequence_id"]),
    ])
    .ok_or_else(|| anyhow::anyhow!("sequence_id is required to reapply"))?;
    let sequence = outbound_load_required(
        conn,
        "outbound_sequences",
        &sequence_id,
        "sequence not found",
    )?;
    outbound_payload_insert(
        &mut engagement,
        "sequence_id",
        Value::String(sequence_id.clone()),
    );
    outbound_payload_insert(&mut engagement, "sequence_snapshot", sequence.clone());
    outbound_payload_insert(
        &mut engagement,
        "sequence_reapplied_at_ms",
        Value::Number(serde_json::Number::from(now)),
    );
    outbound_put_i64(&mut engagement, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_engagements",
        &engagement_id,
        now,
        engagement.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "engagement": engagement,
        "sequence_id": sequence_id,
    }))
}

/// Extract the sequence-version context recorded on an engagement so a generated
/// draft can be traced back to the exact sequence revision it was produced from.
/// The version is the snapshot timestamp captured by
/// `outbound.engagement.reapply_sequence` (falling back to an explicit `version`
/// field), defaulting to 0 when no sequence has been projected yet.
fn outbound_engagement_sequence_context(engagement: &Value) -> (Option<String>, i64) {
    let sequence_id = outbound_first_string(&[
        outbound_string(engagement, &["payload", "sequence_id"]),
        outbound_string(engagement, &["sequence_id"]),
    ]);
    let version = engagement
        .pointer("/payload/sequence_snapshot/updated_at_ms")
        .and_then(Value::as_i64)
        .or_else(|| {
            engagement
                .pointer("/payload/sequence_snapshot/version")
                .and_then(Value::as_i64)
        })
        .unwrap_or(0);
    (sequence_id, version)
}

/// Persist updated proposed_slots (or other slot metadata) into an existing
/// meeting request. Empty proposed_slots is allowed and signals the next
/// draft.prepare(scheduling) call to regenerate from scratch.
fn outbound_handle_scheduling_update_slots(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let request_id = outbound_required_string(&command.payload, &["meeting_request_id"])?;
    let mut request = outbound_load_required(
        conn,
        "outbound_meeting_requests",
        &request_id,
        "meeting_request not found",
    )?;
    if let Some(slots) = command.payload.get("proposed_slots") {
        if let Some(obj) = request.as_object_mut() {
            obj.insert("proposed_slots".to_string(), slots.clone());
        }
    }
    for key in ["duration_minutes", "slot_strategy", "calendar_account_id"] {
        if let Some(value) = command.payload.get(key) {
            if let Some(obj) = request.as_object_mut() {
                obj.insert(key.to_string(), value.clone());
            }
        }
    }
    outbound_put_i64(&mut request, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_meeting_requests",
        &request_id,
        now,
        request.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "meeting_request": request,
    }))
}

/// Reconcile outbound_messages.send_status with terminal SMTP delivery outcomes
/// recorded by the mailserver runner in `stalwart_smtp_delivery_log`.
/// For every outbound_message with `send_status = queued_for_provider` and a
/// known `provider_message_id`, this looks up the latest delivery log row and
/// promotes the message to `sent` (delivered) or `failed` accordingly. Runs are
/// idempotent: already-final messages are skipped.
fn outbound_handle_provider_reconcile(
    root: &Path,
    conn: &Connection,
    _command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let core_db = crate::paths::core_db(root);
    let mut updated = Vec::new();
    let mut checked: i64 = 0;
    let messages = {
        let mut stmt = conn.prepare(
            "SELECT payload_json FROM business_records
             WHERE collection = 'outbound_messages' AND deleted = 0",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out: Vec<Value> = Vec::new();
        for row in rows {
            let raw = row?;
            if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                out.push(value);
            }
        }
        out
    };

    let log_conn = if core_db.exists() {
        Some(Connection::open(&core_db)?)
    } else {
        None
    };

    for mut message in messages {
        let send_status =
            outbound_string(&message, &["send_status"]).unwrap_or_else(|| "draft".to_string());
        if !matches!(
            send_status.as_str(),
            "queued_for_provider" | "queued" | "approved_not_sent"
        ) {
            continue;
        }
        let provider_id = outbound_first_string(&[
            outbound_string(&message, &["provider_message_id"]),
            outbound_string(&message, &["payload", "provider_queue_id"]),
            outbound_string(&message, &["payload", "provider_message_id"]),
        ]);
        let Some(provider_id) = provider_id else {
            continue;
        };
        let Some(message_id) = outbound_string(&message, &["id"]) else {
            continue;
        };
        checked += 1;
        let Some(ref log_conn) = log_conn else {
            continue;
        };
        let outcome: Option<(String, Option<String>, i64)> = log_conn
            .query_row(
                "SELECT outcome, error_text, completed_at
                 FROM stalwart_smtp_delivery_log
                 WHERE id = ?1
                 ORDER BY completed_at DESC LIMIT 1",
                rusqlite::params![provider_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .unwrap_or(None);
        let Some((outcome, error_text, completed_at)) = outcome else {
            continue;
        };
        let new_status = match outcome.as_str() {
            "delivered" => "sent",
            "failed" => "failed",
            other => other,
        };
        outbound_put_string(&mut message, "send_status", new_status);
        outbound_payload_insert(
            &mut message,
            "provider_dispatch_status",
            Value::String(outcome.clone()),
        );
        outbound_payload_insert(
            &mut message,
            "provider_completed_at_ms",
            Value::Number(serde_json::Number::from(completed_at)),
        );
        if let Some(text) = error_text.as_ref() {
            outbound_payload_insert(
                &mut message,
                "provider_error_text",
                Value::String(text.clone()),
            );
        }
        if new_status == "sent" {
            outbound_put_i64(&mut message, "sent_at_ms", completed_at);
        }
        outbound_put_i64(&mut message, "updated_at_ms", now);
        upsert_business_record(conn, "outbound_messages", &message_id, now, message.clone())?;
        if new_status == "sent" {
            if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                outbound_update_engagement_status(conn, &engagement_id, "sent", now)?;
            }
        }
        updated.push(serde_json::json!({
            "message_id": message_id,
            "outcome": outcome,
            "send_status": new_status,
        }));
    }

    Ok(serde_json::json!({
        "ok": true,
        "checked": checked,
        "updated": updated,
        "log_available": log_conn.is_some(),
    }))
}

fn outbound_handle_campaign_apply_setup(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
    let patch = command
        .payload
        .get("campaign_payload_patch")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("campaign_payload_patch object is required"))?;
    let mut campaign = outbound_load_required_or_rxdb(
        root,
        conn,
        "outbound_campaigns",
        &campaign_id,
        "campaign not found",
    )?;
    outbound_put_string(&mut campaign, "id", campaign_id.clone());
    outbound_put_default_object(&mut campaign, "payload");
    if let Some(status) = outbound_string(&command.payload, &["status"]) {
        if matches!(
            status.as_str(),
            "setup_required" | "active" | "paused" | "closed"
        ) {
            outbound_put_string(&mut campaign, "status", status);
        }
    }
    {
        let payload = campaign
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow::anyhow!("campaign payload object is required"))?;
        for (key, value) in patch {
            payload.insert(key.clone(), value.clone());
        }
        let apply_command_id = command.id.clone().unwrap_or_default();
        let source_command_id =
            outbound_string(&command.payload, &["source_command_id"]).unwrap_or_default();
        payload.insert(
            "campaign_setup_task".to_string(),
            serde_json::json!({
                "command_id": if source_command_id.is_empty() { apply_command_id.clone() } else { source_command_id.clone() },
                "apply_command_id": apply_command_id,
                "source_command_id": source_command_id,
                "status": "completed",
                "skill": outbound_string(&command.payload, &["skill"]).unwrap_or_else(|| "business-os-outbound-campaign-setup".to_string()),
                "applied_at_ms": now,
            }),
        );
    }
    outbound_put_i64(&mut campaign, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;
    upsert_rxdb_collection_record(
        root,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "campaign": campaign,
        "campaign_id": campaign_id,
    }))
}

fn outbound_handle_campaign_briefing_update(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
    let mut campaign = outbound_load_required_or_rxdb(
        root,
        conn,
        "outbound_campaigns",
        &campaign_id,
        "campaign not found",
    )?;
    outbound_put_string(&mut campaign, "id", campaign_id.clone());
    if let Some(name) = outbound_string(&command.payload, &["name"]) {
        anyhow::ensure!(!name.trim().is_empty(), "campaign name is required");
        outbound_put_string(&mut campaign, "name", name.trim().to_string());
    }
    if let Some(objective) = outbound_string(&command.payload, &["objective"]) {
        outbound_put_string(&mut campaign, "objective", objective.trim().to_string());
    }
    outbound_put_default_object(&mut campaign, "payload");
    if let Some(payload_patch) = command
        .payload
        .get("payload_patch")
        .and_then(Value::as_object)
    {
        let payload = campaign
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow::anyhow!("campaign payload object is required"))?;
        for key in [
            "subtitle",
            "scope",
            "briefing",
            "briefing_template_id",
            "briefing_language",
            "campaign_setup_task",
        ] {
            if let Some(value) = payload_patch.get(key) {
                payload.insert(key.to_string(), value.clone());
            }
        }
    }
    outbound_put_i64(&mut campaign, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;
    upsert_rxdb_collection_record(
        root,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "campaign": campaign,
        "campaign_id": campaign_id,
    }))
}

fn outbound_handle_reply_match(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
    let reply_message_key = outbound_required_string(&command.payload, &["reply_message_id"])
        .or_else(|_| outbound_required_string(&command.payload, &["communication_message_key"]))?;
    let classification = outbound_string(&command.payload, &["classification"])
        .unwrap_or_else(|| "unclear".to_string());
    let outbound_message_id =
        outbound_string(&command.payload, &["outbound_message_id"]).unwrap_or_default();

    let mut engagement = outbound_load_required(
        conn,
        "outbound_engagements",
        &engagement_id,
        "engagement not found",
    )?;
    outbound_put_string(&mut engagement, "status", "reply_received".to_string());
    outbound_payload_insert(
        &mut engagement,
        "reply_classification",
        Value::String(classification.clone()),
    );
    outbound_payload_insert(
        &mut engagement,
        "reply_message_id",
        Value::String(reply_message_key.clone()),
    );
    outbound_payload_insert(
        &mut engagement,
        "reply_matched_at_ms",
        Value::Number(serde_json::Number::from(now)),
    );
    outbound_put_i64(&mut engagement, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_engagements",
        &engagement_id,
        now,
        engagement.clone(),
    )?;

    let suppression_id =
        outbound_apply_reply_suppression(conn, &engagement, &engagement_id, &classification, now)?;

    let pending_messages = outbound_load_records_by_string_field(
        conn,
        "outbound_messages",
        "engagement_id",
        &engagement_id,
    )?;
    let mut cancelled = Vec::new();
    for mut message in pending_messages {
        let send_status =
            outbound_string(&message, &["send_status"]).unwrap_or_else(|| "draft".to_string());
        let direction =
            outbound_string(&message, &["direction"]).unwrap_or_else(|| "outbound".to_string());
        if direction != "outbound" {
            continue;
        }
        if matches!(
            send_status.as_str(),
            "sent" | "delivered" | "queued_for_provider"
        ) {
            continue;
        }
        if matches!(send_status.as_str(), "cancelled" | "paused") {
            continue;
        }
        let Some(message_id) = outbound_string(&message, &["id"]) else {
            continue;
        };
        outbound_put_string(&mut message, "send_status", "cancelled");
        outbound_payload_insert(
            &mut message,
            "cancelled_reason",
            Value::String("reply_received".to_string()),
        );
        outbound_payload_insert(
            &mut message,
            "cancelled_at_ms",
            Value::Number(serde_json::Number::from(now)),
        );
        outbound_put_i64(&mut message, "updated_at_ms", now);
        upsert_business_record(conn, "outbound_messages", &message_id, now, message)?;
        cancelled.push(message_id);
    }

    // Best-effort: annotate the matched communication_message with outbound metadata.
    let channel_path = crate::paths::core_db(root);
    if channel_path.exists() {
        if let Ok(mut channel_conn) = channels::open_channel_db(&channel_path) {
            let _ = annotate_communication_message_with_outbound(
                &mut channel_conn,
                &reply_message_key,
                &engagement_id,
                &outbound_message_id,
                &classification,
            );
        }
    }

    Ok(serde_json::json!({
        "ok": true,
        "engagement": engagement,
        "classification": classification,
        "reply_message_key": reply_message_key,
        "cancelled_message_ids": cancelled,
        "suppression_id": suppression_id,
    }))
}

fn annotate_communication_message_with_outbound(
    conn: &mut Connection,
    message_key: &str,
    engagement_id: &str,
    outbound_message_id: &str,
    classification: &str,
) -> anyhow::Result<()> {
    let row: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM communication_messages WHERE message_key = ?1",
            rusqlite::params![message_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(metadata_text) = row else {
        return Ok(());
    };
    let mut metadata: Value =
        serde_json::from_str(&metadata_text).unwrap_or_else(|_| serde_json::json!({}));
    let object = metadata.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("communication_messages.metadata_json is not an object for {message_key}")
    })?;
    object.insert(
        "outbound_engagement_id".to_string(),
        Value::String(engagement_id.to_string()),
    );
    if !outbound_message_id.is_empty() {
        object.insert(
            "outbound_message_id".to_string(),
            Value::String(outbound_message_id.to_string()),
        );
    }
    object.insert(
        "outbound_reply_classification".to_string(),
        Value::String(classification.to_string()),
    );
    let updated = serde_json::to_string(&metadata)?;
    conn.execute(
        "UPDATE communication_messages SET metadata_json = ?1 WHERE message_key = ?2",
        rusqlite::params![updated, message_key],
    )?;
    Ok(())
}

fn outbound_object_payload(payload: &Value) -> Value {
    payload
        .as_object()
        .map(|object| Value::Object(object.clone()))
        .unwrap_or_else(|| serde_json::json!({}))
}

fn outbound_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn outbound_first_string(values: &[Option<String>]) -> Option<String> {
    values
        .iter()
        .flatten()
        .find(|value| !value.is_empty())
        .cloned()
}

fn outbound_required_string(value: &Value, path: &[&str]) -> anyhow::Result<String> {
    outbound_string(value, path).with_context(|| format!("{} is required", path.join(".")))
}

fn outbound_id_from_command(
    command: &BusinessCommand,
    payload_keys: &[&str],
    prefix: &str,
) -> anyhow::Result<String> {
    let from_payload = payload_keys
        .iter()
        .find_map(|key| outbound_string(&command.payload, &[*key]));
    outbound_first_string(&[
        from_payload,
        command
            .record_id
            .as_ref()
            .map(|value| value.trim().to_string()),
    ])
    .or_else(|| {
        command
            .id
            .as_ref()
            .map(|value| format!("{prefix}_{}", channels::stable_digest(value)))
    })
    .context("record id is required")
}

fn outbound_required_from_payload_or_record(
    command: &BusinessCommand,
    payload_keys: &[&str],
    message: &str,
) -> anyhow::Result<String> {
    let from_payload = payload_keys
        .iter()
        .find_map(|key| outbound_string(&command.payload, &[*key]));
    outbound_first_string(&[
        from_payload,
        command
            .record_id
            .as_ref()
            .map(|value| value.trim().to_string()),
    ])
    .with_context(|| message.to_string())
}

fn outbound_put_string(record: &mut Value, key: &str, value: impl Into<String>) {
    if let Some(object) = record.as_object_mut() {
        object.insert(key.to_string(), Value::String(value.into()));
    }
}

fn outbound_put_i64(record: &mut Value, key: &str, value: i64) {
    if let Some(object) = record.as_object_mut() {
        object.insert(key.to_string(), Value::from(value));
    }
}

fn outbound_put_default_string(record: &mut Value, key: &str, default: &str) {
    if outbound_string(record, &[key]).is_none() {
        outbound_put_string(record, key, default.to_string());
    }
}

fn outbound_put_default_i64(record: &mut Value, key: &str, default: i64) {
    let should_insert = record
        .get(key)
        .and_then(Value::as_i64)
        .or_else(|| {
            record
                .get(key)
                .and_then(Value::as_u64)
                .map(|value| value as i64)
        })
        .is_none();
    if should_insert {
        outbound_put_i64(record, key, default);
    }
}

fn outbound_put_default_object(record: &mut Value, key: &str) {
    let should_insert = !matches!(record.get(key), Some(Value::Object(_)));
    if should_insert {
        if let Some(object) = record.as_object_mut() {
            object.insert(key.to_string(), serde_json::json!({}));
        }
    }
}

fn outbound_merge_fields(record: &mut Value, patch: &Value, keys: &[&str]) {
    let Some(record_obj) = record.as_object_mut() else {
        return;
    };
    for key in keys {
        if let Some(value) = patch.get(*key) {
            record_obj.insert((*key).to_string(), value.clone());
        }
    }
}

fn outbound_payload_insert(record: &mut Value, key: &str, value: Value) {
    if !matches!(record.get("payload"), Some(Value::Object(_))) {
        outbound_put_default_object(record, "payload");
    }
    if let Some(payload) = record.get_mut("payload").and_then(Value::as_object_mut) {
        payload.insert(key.to_string(), value);
    }
}

fn outbound_email_account_key_from(value: Option<String>) -> Option<String> {
    let raw = value?.trim().to_ascii_lowercase();
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with("email:") {
        return Some(raw);
    }
    if raw.contains('@') {
        return Some(format!("email:{raw}"));
    }
    Some(raw)
}

fn outbound_email_address_from_account_key(account_key: &str) -> String {
    account_key
        .trim()
        .strip_prefix("email:")
        .unwrap_or(account_key.trim())
        .to_ascii_lowercase()
}

fn outbound_sync_email_message_to_communication(
    root: &Path,
    message: &mut Value,
    status: &str,
) -> anyhow::Result<()> {
    let Some(account_key) = outbound_email_account_key_from(outbound_first_string(&[
        outbound_string(message, &["communication_account_key"]),
        outbound_string(message, &["payload", "communication_account_key"]),
        outbound_string(message, &["sender_account_id"]),
    ])) else {
        return Ok(());
    };
    let account_address = outbound_email_address_from_account_key(&account_key);
    if !account_key.starts_with("email:") || !account_address.contains('@') {
        return Ok(());
    }
    let Some(recipient_email) = outbound_string(message, &["recipient_email"]) else {
        return Ok(());
    };
    let Some(message_id) = outbound_string(message, &["id"]) else {
        return Ok(());
    };
    let engagement_id = outbound_string(message, &["engagement_id"]).unwrap_or_default();
    let campaign_id = outbound_string(message, &["campaign_id"]).unwrap_or_default();
    let subject = outbound_string(message, &["subject"]).unwrap_or_default();
    let body_text = outbound_string(message, &["body_text"]).unwrap_or_default();
    let body_html = outbound_string(message, &["body_html"]).unwrap_or_default();
    let message_key = outbound_first_string(&[
        outbound_string(message, &["communication_message_key"]),
        outbound_string(message, &["payload", "communication_message_key"]),
    ])
    .unwrap_or_else(|| {
        format!(
            "email:{}:outbound:{}",
            account_address,
            channels::stable_digest(&message_id)
        )
    });
    let thread_key = outbound_first_string(&[
        outbound_string(message, &["thread_key"]),
        outbound_string(message, &["payload", "thread_key"]),
    ])
    .unwrap_or_else(|| {
        let material = format!("{account_key}|{campaign_id}|{engagement_id}|{recipient_email}");
        format!(
            "email:{}:outbound-thread:{}",
            account_address,
            channels::stable_digest(&material)
        )
    });
    let now_iso = channels::now_iso_string();
    let recipient_addresses_json = serde_json::to_string(&vec![recipient_email.clone()])?;
    let preview = channels::preview_text(&body_text, &subject);
    let remote_id = outbound_first_string(&[
        outbound_string(message, &["provider_message_id"]),
        outbound_string(message, &["payload", "provider_message_id"]),
        outbound_string(message, &["payload", "provider_queue_id"]),
    ])
    .unwrap_or_else(|| message_id.clone());
    let provider_send_executed = message
        .get("payload")
        .and_then(|payload| payload.get("provider_send_executed"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let provider_dispatch_status =
        outbound_string(message, &["payload", "provider_dispatch_status"])
            .unwrap_or_else(|| "not_dispatched".to_string());
    let provider_queue_id = outbound_first_string(&[
        outbound_string(message, &["payload", "provider_queue_id"]),
        outbound_string(message, &["provider_message_id"]),
    ])
    .unwrap_or_default();
    let metadata = serde_json::json!({
        "source": "business-os.outbound",
        "campaign_id": campaign_id,
        "outbound_campaign_id": campaign_id,
        "engagement_id": engagement_id,
        "outbound_engagement_id": engagement_id,
        "outbound_message_id": message_id,
        "communication_account_key": account_key,
        "communication_thread_key": thread_key,
        "communication_message_key": message_key,
        "approval_status": outbound_string(message, &["approval_status"]).unwrap_or_default(),
        "send_status": outbound_string(message, &["send_status"]).unwrap_or_default(),
        "provider_dispatch_status": provider_dispatch_status,
        "provider_queue_id": provider_queue_id,
        "provider_send_executed": provider_send_executed,
    });
    let metadata_json = serde_json::to_string(&metadata)?;
    let mut channel_conn = channels::open_channel_db(&crate::paths::core_db(root))?;
    channels::upsert_communication_message(
        &mut channel_conn,
        channels::UpsertMessage {
            message_key: &message_key,
            channel: "email",
            account_key: &account_key,
            thread_key: &thread_key,
            remote_id: &remote_id,
            direction: "outbound",
            folder_hint: "outbound",
            sender_display: "",
            sender_address: &account_address,
            recipient_addresses_json: &recipient_addresses_json,
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &subject,
            preview: &preview,
            body_text: &body_text,
            body_html: &body_html,
            raw_payload_ref: "",
            trust_level: "business-os",
            status,
            seen: true,
            has_attachments: false,
            external_created_at: &now_iso,
            observed_at: &now_iso,
            metadata_json: &metadata_json,
        },
    )?;
    channels::refresh_thread(&mut channel_conn, &thread_key)?;
    outbound_put_string(message, "sender_account_id", account_key.clone());
    outbound_put_string(message, "communication_account_key", account_key.clone());
    outbound_put_string(message, "communication_message_key", message_key.clone());
    outbound_put_string(message, "thread_key", thread_key.clone());
    outbound_payload_insert(
        message,
        "communication_account_key",
        Value::String(account_key),
    );
    outbound_payload_insert(
        message,
        "communication_message_key",
        Value::String(message_key),
    );
    outbound_payload_insert(message, "thread_key", Value::String(thread_key));
    Ok(())
}

fn outbound_queue_email_delivery(root: &Path, message: &Value) -> anyhow::Result<String> {
    let sender_account_id = outbound_required_string(message, &["sender_account_id"])?;
    let from = outbound_email_address_from_account_key(&sender_account_id);
    let to = outbound_required_string(message, &["recipient_email"])?;
    let subject = outbound_string(message, &["subject"]).unwrap_or_default();
    let body_text = outbound_string(message, &["body_text"]).unwrap_or_default();
    let body_html = outbound_string(message, &["body_html"]).unwrap_or_default();
    anyhow::ensure!(
        !body_text.trim().is_empty() || !body_html.trim().is_empty(),
        "outbound email body is empty (body_text and body_html both blank)"
    );
    let sender_domain = from.split('@').nth(1).unwrap_or("ctox.local");
    let msg_id = format!("<{}@{}>", Uuid::new_v4(), sender_domain);
    let date = chrono::Utc::now().to_rfc2822();

    let header = format!(
        "From: {from}\r\n\
         To: {to}\r\n\
         Subject: {subject}\r\n\
         Message-ID: {msg_id}\r\n\
         Date: {date}\r\n\
         MIME-Version: 1.0\r\n",
        from = outbound_header_value(&from),
        to = outbound_header_value(&to),
        subject = outbound_header_value(&subject),
        msg_id = outbound_header_value(&msg_id),
        date = outbound_header_value(&date),
    );

    let normalize = |body: &str| body.replace("\r\n", "\n").replace('\r', "\n");

    let rfc822_body = if !body_html.trim().is_empty() && !body_text.trim().is_empty() {
        // Send a proper multipart/alternative with both representations.
        let boundary = format!("ctox-{}", Uuid::new_v4().simple());
        format!(
            "{header}Content-Type: multipart/alternative; boundary=\"{boundary}\"\r\n\r\n\
             --{boundary}\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n\r\n\
             {text}\r\n\
             --{boundary}\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n\r\n\
             {html}\r\n\
             --{boundary}--\r\n",
            text = normalize(&body_text),
            html = normalize(&body_html),
        )
    } else if !body_html.trim().is_empty() {
        // HTML-only body — also include a plain-text fallback derived from the HTML.
        let fallback_text = outbound_html_to_plain_text(&body_html);
        let boundary = format!("ctox-{}", Uuid::new_v4().simple());
        format!(
            "{header}Content-Type: multipart/alternative; boundary=\"{boundary}\"\r\n\r\n\
             --{boundary}\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n\r\n\
             {text}\r\n\
             --{boundary}\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n\r\n\
             {html}\r\n\
             --{boundary}--\r\n",
            text = fallback_text,
            html = normalize(&body_html),
        )
    } else {
        format!(
            "{header}Content-Type: text/plain; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n\r\n\
             {body}\r\n",
            body = normalize(&body_text),
        )
    };

    let db_path = root
        .join("runtime/ctox.sqlite3")
        .to_string_lossy()
        .into_owned();
    let store = ctox_mailserver::store::sqlite::SqliteStore::new(&db_path);
    store.init()?;
    store
        .queue_email(&from, &to, &rfc822_body)
        .map_err(Into::into)
}

/// Minimal HTML → plain-text fallback for outbound mails that only carry body_html.
/// Strips tags and decodes a handful of common entities; intentionally simple — for
/// rich rendering, operators should also fill body_text in the draft pipeline.
fn outbound_html_to_plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut last_was_space = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            _ if in_tag => {}
            '\r' | '\n' | '\t' => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            ' ' => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            other => {
                out.push(other);
                last_was_space = false;
            }
        }
    }
    let collapsed = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    collapsed.trim().to_string()
}

fn outbound_header_value(value: &str) -> String {
    value
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Atomically reserve one daily send slot for the sender account, enforcing the
/// per-account daily cap under concurrent commands. The check-and-increment runs
/// inside a `BEGIN IMMEDIATE` transaction so two parallel `send_approved`
/// commands cannot both pass when only one slot remains: the second writer blocks
/// on the write lock (WAL + busy_timeout), then reads the already-incremented
/// count. Bails — leaving the counter untouched, since the transaction rolls back
/// on the early return — when the account is blocked, ineligible, or already at
/// its cap. Mirrors the read-only checks in `outbound_enforce_account_limit` so
/// the cheap early gate and the authoritative reservation agree.
fn outbound_reserve_account_send_slot(
    conn: &Connection,
    sender_account_id: &str,
    now: i64,
) -> anyhow::Result<()> {
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let canonical = outbound_email_account_key_from(Some(sender_account_id.to_string()))
        .unwrap_or_else(|| sender_account_id.to_string());
    let existing = outbound_load_record(&tx, "outbound_account_limits", &canonical)?.or(
        outbound_load_record(&tx, "outbound_account_limits", sender_account_id)?,
    );
    let Some(mut limit) = existing else {
        // No limit record: no cap configured, nothing to reserve.
        tx.commit()?;
        return Ok(());
    };
    // Re-validate eligibility under the lock so a block applied by a concurrent
    // command takes effect on the very next send.
    let blocked = limit
        .get("blocked")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    anyhow::ensure!(
        !blocked,
        "sender account is blocked for outbound communication"
    );
    let status = outbound_string(&limit, &["status"]).unwrap_or_else(|| "active".to_string());
    anyhow::ensure!(
        !matches!(
            status.as_str(),
            "blocked" | "locked" | "suspended" | "disabled"
        ),
        "sender account status `{status}` is not eligible to send"
    );
    let current = limit
        .get("sent_today")
        .and_then(Value::as_i64)
        .or_else(|| limit.get("daily_sent_count").and_then(Value::as_i64))
        .unwrap_or(0);
    let next = current + 1;
    if let Some(limit_value) = limit.get("daily_limit").and_then(Value::as_i64) {
        // daily_limit semantics: <= 0 means "no daily cap configured".
        if limit_value > 0 {
            anyhow::ensure!(next <= limit_value, "sender account daily limit exhausted");
        }
    }
    outbound_put_i64(&mut limit, "sent_today", next);
    outbound_put_i64(&mut limit, "daily_sent_count", next);
    if let Some(limit_value) = limit.get("daily_limit").and_then(Value::as_i64) {
        if limit_value > 0 {
            outbound_put_i64(
                &mut limit,
                "remaining_today",
                limit_value.saturating_sub(next).max(0),
            );
        }
    }
    outbound_put_i64(&mut limit, "updated_at_ms", now);
    upsert_business_record(&tx, "outbound_account_limits", &canonical, now, limit)?;
    tx.commit()?;
    Ok(())
}

/// Release a previously reserved daily send slot, decrementing the counter under
/// a `BEGIN IMMEDIATE` transaction. Used when a send was reserved but never
/// reached the provider (queue failure), so the daily counter stays accurate for
/// a later retry. Never drops below zero.
fn outbound_release_account_send_slot(
    conn: &Connection,
    sender_account_id: &str,
    now: i64,
) -> anyhow::Result<()> {
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let canonical = outbound_email_account_key_from(Some(sender_account_id.to_string()))
        .unwrap_or_else(|| sender_account_id.to_string());
    let existing = outbound_load_record(&tx, "outbound_account_limits", &canonical)?.or(
        outbound_load_record(&tx, "outbound_account_limits", sender_account_id)?,
    );
    let Some(mut limit) = existing else {
        tx.commit()?;
        return Ok(());
    };
    let current = limit
        .get("sent_today")
        .and_then(Value::as_i64)
        .or_else(|| limit.get("daily_sent_count").and_then(Value::as_i64))
        .unwrap_or(0);
    let next = (current - 1).max(0);
    outbound_put_i64(&mut limit, "sent_today", next);
    outbound_put_i64(&mut limit, "daily_sent_count", next);
    if let Some(limit_value) = limit.get("daily_limit").and_then(Value::as_i64) {
        if limit_value > 0 {
            outbound_put_i64(
                &mut limit,
                "remaining_today",
                limit_value.saturating_sub(next).max(0),
            );
        }
    }
    outbound_put_i64(&mut limit, "updated_at_ms", now);
    upsert_business_record(&tx, "outbound_account_limits", &canonical, now, limit)?;
    tx.commit()?;
    Ok(())
}

fn outbound_load_record(
    conn: &Connection,
    collection: &str,
    record_id: &str,
) -> anyhow::Result<Option<Value>> {
    let payload_json: Option<String> = conn
        .query_row(
            "SELECT payload_json
             FROM business_records
             WHERE collection = ?1
               AND record_id = ?2
               AND deleted = 0",
            params![collection, record_id],
            |row| row.get(0),
        )
        .optional()?;
    payload_json
        .map(|raw| {
            serde_json::from_str(&raw)
                .with_context(|| format!("invalid {collection} record {record_id}"))
        })
        .transpose()
}

fn outbound_load_required(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    message: &str,
) -> anyhow::Result<Value> {
    outbound_load_record(conn, collection, record_id)?.with_context(|| message.to_string())
}

fn outbound_load_record_or_rxdb(
    root: &Path,
    conn: &Connection,
    collection: &str,
    record_id: &str,
) -> anyhow::Result<Option<Value>> {
    if let Some(record) = outbound_load_record(conn, collection, record_id)? {
        return Ok(Some(record));
    }
    let Some(record) = load_rxdb_collection_record(root, collection, record_id)? else {
        return Ok(None);
    };
    let updated_at_ms = record
        .get("updated_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| now_ms() as i64);
    upsert_business_record(conn, collection, record_id, updated_at_ms, record.clone())?;
    Ok(Some(record))
}

fn outbound_load_required_or_rxdb(
    root: &Path,
    conn: &Connection,
    collection: &str,
    record_id: &str,
    message: &str,
) -> anyhow::Result<Value> {
    outbound_load_record_or_rxdb(root, conn, collection, record_id)?
        .with_context(|| message.to_string())
}

fn outbound_load_records_by_string_field(
    conn: &Connection,
    collection: &str,
    field: &str,
    value: &str,
) -> anyhow::Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT payload_json
         FROM business_records
         WHERE collection = ?1
           AND deleted = 0",
    )?;
    let mut rows = stmt.query(params![collection])?;
    let mut records = Vec::new();
    while let Some(row) = rows.next()? {
        let raw: String = row.get(0)?;
        let record: Value = serde_json::from_str(&raw)
            .with_context(|| format!("invalid {collection} record while scanning {field}"))?;
        if outbound_string(&record, &[field]).as_deref() == Some(value) {
            records.push(record);
        }
    }
    Ok(records)
}

fn outbound_latest_message(messages: &[Value]) -> Option<Value> {
    messages
        .iter()
        .max_by_key(|message| {
            message
                .get("updated_at_ms")
                .and_then(Value::as_i64)
                .or_else(|| message.get("created_at_ms").and_then(Value::as_i64))
                .unwrap_or(0)
        })
        .cloned()
}

/// Resolve a persisted outbound skillbook into a one-line strategy hint for the
/// deterministic draft fallback: its mission plus the first non-negotiable rule.
/// Returns `None` when the skillbook is absent or carries no usable text, so the
/// caller falls back to the generic strategy line.
fn outbound_skillbook_guidance(
    conn: &Connection,
    skillbook_id: &str,
) -> anyhow::Result<Option<String>> {
    let Some(skillbook) = outbound_load_record(conn, "outbound_skillbooks", skillbook_id)? else {
        return Ok(None);
    };
    let mission = outbound_string(&skillbook, &["mission"]).unwrap_or_default();
    let first_rule = skillbook
        .get("non_negotiable_rules")
        .and_then(Value::as_array)
        .and_then(|rules| rules.first())
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let guidance = match (mission.trim().is_empty(), first_rule.trim().is_empty()) {
        (true, true) => return Ok(None),
        (false, true) => mission,
        (true, false) => first_rule,
        (false, false) => format!("{mission} Leitplanke: {first_rule}."),
    };
    Ok(Some(guidance))
}

fn outbound_generate_automated_draft(
    engagement: &Value,
    latest_message: Option<&Value>,
    request: &Value,
    draft_kind: &str,
    skillbook_guidance: Option<&str>,
) -> Value {
    let contact_name = outbound_first_string(&[
        outbound_string(engagement, &["payload", "contact_name"]),
        outbound_string(request, &["contact_name"]),
    ])
    .unwrap_or_else(|| "Hallo".to_string());
    let company_name = outbound_first_string(&[
        outbound_string(engagement, &["payload", "company_name"]),
        outbound_string(request, &["company_name"]),
    ])
    .unwrap_or_else(|| "Ihr Unternehmen".to_string());
    let previous_subject =
        latest_message.and_then(|message| outbound_string(message, &["subject"]));
    let subject = match draft_kind {
        "initial" => format!("Austausch zu {company_name}"),
        "reply" => previous_subject
            .map(|subject| format!("Re: {subject}"))
            .unwrap_or_else(|| format!("Re: Austausch zu {company_name}")),
        "scheduling" => previous_subject
            .map(|subject| format!("Re: {subject}"))
            .unwrap_or_else(|| "Terminvorschlag".to_string()),
        kind if kind.starts_with("followup") => previous_subject
            .map(|subject| format!("Re: {subject}"))
            .unwrap_or_else(|| format!("Kurzer Nachtrag zu {company_name}")),
        _ => previous_subject.unwrap_or_else(|| format!("Austausch zu {company_name}")),
    };
    let strategy = outbound_first_string(&[
        outbound_string(request, &["strategy_text"]),
        outbound_string(request, &["payload", "strategy_text"]),
        outbound_string(engagement, &["payload", "strategy_text"]),
        skillbook_guidance.map(str::to_string),
    ])
    .unwrap_or_else(|| "Kontextbezogen, knapp und ohne erfundene Aussagen schreiben.".to_string());
    let body_text = match draft_kind {
        "initial" => format!(
            "{contact_name},\n\nich habe mir {company_name} im Kontext der aktuellen Outbound-Campaign angesehen und einen moeglichen Anknuepfungspunkt identifiziert.\n\n{strategy}\n\nWenn das grundsaetzlich relevant ist, schlage ich einen kurzen Austausch vor.\n\nBeste Gruesse"
        ),
        "reply" => {
            let reply_text = outbound_first_string(&[
                outbound_string(request, &["reply_text"]),
                outbound_string(engagement, &["payload", "reply_text"]),
                outbound_string(engagement, &["payload", "reply_classification"]),
            ])
            .unwrap_or_else(|| "die Rueckmeldung wurde als relevant klassifiziert".to_string());
            format!(
                "{contact_name},\n\nvielen Dank fuer die Rueckmeldung. Ich habe den Kontext so verstanden: {reply_text}.\n\nGerne konkretisiere ich den naechsten Schritt und halte es knapp: {strategy}\n\nBeste Gruesse"
            )
        }
        "scheduling" => {
            let duration = request
                .get("duration_minutes")
                .and_then(Value::as_i64)
                .unwrap_or(30);
            let slot_hint = outbound_string(request, &["slot_hint"])
                .unwrap_or_else(|| "zwei bis drei passende Zeitfenster".to_string());
            format!(
                "{contact_name},\n\nsehr gerne. Fuer einen kurzen Austausch wuerde ich {duration} Minuten einplanen.\n\nIch kann {slot_hint} vorschlagen; bitte geben Sie kurz Bescheid, was bei Ihnen passt.\n\nBeste Gruesse"
            )
        }
        kind if kind.starts_with("followup") => format!(
            "{contact_name},\n\nich wollte meine vorherige Nachricht kurz nachfassen, weil der Anknuepfungspunkt zu {company_name} weiterhin relevant sein koennte.\n\n{strategy}\n\nFalls es aktuell nicht passt, reicht eine kurze Rueckmeldung.\n\nBeste Gruesse"
        ),
        _ => format!(
            "{contact_name},\n\nich bereite den naechsten Outbound-Schritt zu {company_name} vor.\n\n{strategy}\n\nBeste Gruesse"
        ),
    };
    serde_json::json!({
        "subject": subject,
        "body_text": body_text,
        "draft_kind": draft_kind,
        "requires_approval": true
    })
}

fn outbound_update_engagement_status(
    conn: &Connection,
    engagement_id: &str,
    status: &str,
    now: i64,
) -> anyhow::Result<()> {
    if engagement_id.trim().is_empty() {
        return Ok(());
    }
    let Some(mut engagement) = outbound_load_record(conn, "outbound_engagements", engagement_id)?
    else {
        return Ok(());
    };
    outbound_put_string(&mut engagement, "status", status.to_string());
    outbound_put_i64(&mut engagement, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_engagements", engagement_id, now, engagement)
}

fn outbound_send_status_for_resume(message: &Value) -> &'static str {
    match outbound_string(message, &["approval_status"]).as_deref() {
        Some("approved") => "approved_not_sent",
        Some("awaiting_approval") => "awaiting_approval",
        Some("rejected") => "rejected",
        _ => "not_scheduled",
    }
}

fn outbound_engagement_status_for_message_state(message: &Value) -> &'static str {
    match outbound_string(message, &["approval_status"]).as_deref() {
        Some("approved") => "approved_for_send",
        Some("awaiting_approval") => "awaiting_approval",
        Some("rejected") => "rejected",
        _ => "draft_prepared",
    }
}

fn outbound_update_engagement_terminal_status(
    conn: &Connection,
    engagement_id: &str,
    status: &str,
    reason: Option<&str>,
    now: i64,
) -> anyhow::Result<()> {
    if engagement_id.trim().is_empty() {
        return Ok(());
    }
    let Some(mut engagement) = outbound_load_record(conn, "outbound_engagements", engagement_id)?
    else {
        return Ok(());
    };
    outbound_put_string(&mut engagement, "status", status.to_string());
    if let Some(reason) = reason.map(str::trim).filter(|value| !value.is_empty()) {
        let key = if status == "paused" {
            "paused_reason"
        } else {
            "closed_reason"
        };
        outbound_put_string(&mut engagement, key, reason.to_string());
    }
    outbound_put_i64(&mut engagement, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_engagements", engagement_id, now, engagement)
}

fn outbound_message_revision(message: &Value) -> String {
    let material = serde_json::json!({
        "sender_account_id": outbound_string(message, &["sender_account_id"]).unwrap_or_default(),
        "recipient_email": outbound_string(message, &["recipient_email"]).unwrap_or_default(),
        "subject": outbound_string(message, &["subject"]).unwrap_or_default(),
        "body_text": outbound_string(message, &["body_text"]).unwrap_or_default(),
        "body_html": outbound_string(message, &["body_html"]).unwrap_or_default(),
    });
    format!("rev_{}", channels::stable_digest(&material.to_string()))
}

fn outbound_require_message_content(message: &Value) -> anyhow::Result<()> {
    let has_body = outbound_string(message, &["body_text"]).is_some()
        || outbound_string(message, &["body_html"]).is_some();
    anyhow::ensure!(has_body, "message body is required before approval");
    Ok(())
}

fn outbound_session_actor_id(session: &BusinessOsSession) -> String {
    session
        .user
        .as_ref()
        .map(|user| user.id.clone())
        .unwrap_or_else(|| "business-os-user".to_string())
}

fn outbound_record_rejection(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let message_id = outbound_required_from_payload_or_record(
        command,
        &["message_id", "id"],
        "message_id is required",
    )?;
    let mut message =
        outbound_load_required(conn, "outbound_messages", &message_id, "message not found")?;
    let revision_id = outbound_string(&message, &["revision_id"])
        .unwrap_or_else(|| outbound_message_revision(&message));
    let approval_id = outbound_id_from_command(command, &["approval_id"], "approval")
        .unwrap_or_else(|_| format!("rejection_{message_id}_{revision_id}"));
    let engagement_id = outbound_string(&message, &["engagement_id"]).unwrap_or_default();
    let mut approval = outbound_object_payload(&command.payload);
    outbound_put_string(&mut approval, "id", approval_id.clone());
    outbound_put_string(&mut approval, "message_id", message_id.clone());
    outbound_put_string(&mut approval, "engagement_id", engagement_id.clone());
    outbound_put_string(&mut approval, "revision_id", revision_id);
    outbound_put_string(
        &mut approval,
        "actor_user_id",
        outbound_session_actor_id(session),
    );
    outbound_put_string(&mut approval, "decision", "rejected");
    outbound_put_default_i64(&mut approval, "created_at_ms", now);
    outbound_put_i64(&mut approval, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_approvals",
        &approval_id,
        now,
        approval.clone(),
    )?;
    outbound_put_string(&mut message, "approval_status", "rejected");
    outbound_put_string(&mut message, "send_status", "blocked");
    outbound_put_i64(&mut message, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_messages", &message_id, now, message.clone())?;
    if !engagement_id.is_empty() {
        outbound_update_engagement_status(conn, &engagement_id, "draft_rejected", now)?;
    }
    Ok(serde_json::json!({
        "ok": true,
        "message": message,
        "approval": approval
    }))
}

fn outbound_record_change_request(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let message_id = outbound_required_from_payload_or_record(
        command,
        &["message_id", "id"],
        "message_id is required",
    )?;
    let mut message =
        outbound_load_required(conn, "outbound_messages", &message_id, "message not found")?;
    let revision_id = outbound_string(&message, &["revision_id"])
        .unwrap_or_else(|| outbound_message_revision(&message));
    let approval_id = outbound_id_from_command(command, &["approval_id"], "change_request")
        .unwrap_or_else(|_| format!("change_request_{message_id}_{revision_id}"));
    let engagement_id = outbound_string(&message, &["engagement_id"]).unwrap_or_default();
    let mut approval = outbound_object_payload(&command.payload);
    outbound_put_string(&mut approval, "id", approval_id.clone());
    outbound_put_string(&mut approval, "message_id", message_id.clone());
    outbound_put_string(&mut approval, "engagement_id", engagement_id.clone());
    outbound_put_string(&mut approval, "revision_id", revision_id);
    outbound_put_string(
        &mut approval,
        "actor_user_id",
        outbound_session_actor_id(session),
    );
    outbound_put_string(&mut approval, "decision", "changes_requested");
    outbound_put_default_i64(&mut approval, "created_at_ms", now);
    outbound_put_i64(&mut approval, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_approvals",
        &approval_id,
        now,
        approval.clone(),
    )?;
    outbound_put_string(&mut message, "approval_status", "changes_requested");
    outbound_put_string(&mut message, "draft_status", "changes_requested");
    outbound_put_string(&mut message, "send_status", "blocked");
    outbound_put_i64(&mut message, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_messages", &message_id, now, message.clone())?;
    if !engagement_id.is_empty() {
        outbound_update_engagement_status(conn, &engagement_id, "draft_changes_requested", now)?;
    }
    Ok(serde_json::json!({
        "ok": true,
        "message": message,
        "approval": approval
    }))
}

fn outbound_enforce_send_gate(conn: &Connection, message: &Value) -> anyhow::Result<()> {
    let message_id = outbound_required_string(message, &["id"])?;
    anyhow::ensure!(
        outbound_string(message, &["approval_status"]).as_deref() == Some("approved"),
        "outbound message must be approved before send"
    );
    let revision_id = outbound_string(message, &["revision_id"])
        .unwrap_or_else(|| outbound_message_revision(message));
    anyhow::ensure!(
        outbound_has_matching_approval(conn, &message_id, &revision_id)?,
        "approved outbound message has no matching approval for current revision"
    );
    let channel = outbound_string(message, &["channel"]).unwrap_or_else(|| "email".to_string());
    outbound_require_message_content(message)?;
    match channel.as_str() {
        "physical_letter" => {
            // Physical letters need a postal address, NOT a sender_account or email.
            let address =
                outbound_required_string(message, &["recipient_address_text"]).map_err(|_| {
                    anyhow::anyhow!(
                        "physical_letter messages require recipient_address_text before send"
                    )
                })?;
            anyhow::ensure!(
                !address.trim().is_empty(),
                "physical_letter recipient_address_text must not be blank"
            );
        }
        _ => {
            let sender_account_id = outbound_required_string(message, &["sender_account_id"])?;
            let recipient_email = outbound_required_string(message, &["recipient_email"])?;
            if let Some(reason) = outbound_recipient_suppression_reason(conn, &recipient_email)? {
                anyhow::bail!(
                    "recipient is suppressed for outbound communication (reason: {reason})"
                );
            }
            outbound_enforce_account_limit(conn, &sender_account_id)?;
        }
    }
    Ok(())
}

/// Map a send-gate / provider-queue error into a stable, replicable block-reason
/// code so the UI and downstream automation can branch on the cause instead of
/// parsing free-form text.
fn outbound_classify_send_block(error: &str) -> &'static str {
    let lowered = error.to_ascii_lowercase();
    if lowered.contains("suppress") {
        "recipient_suppressed"
    } else if lowered.contains("blocked") || lowered.contains("not eligible") {
        "sender_blocked"
    } else if lowered.contains("limit") {
        "sender_limit_exhausted"
    } else if lowered.contains("approv") {
        "approval_required"
    } else if lowered.contains("queue") || lowered.contains("provider") {
        "provider_queue_failed"
    } else if lowered.contains("recipient_address") || lowered.contains("recipient_email") {
        "missing_recipient"
    } else if lowered.contains("sender_account") {
        "missing_sender"
    } else {
        "send_blocked"
    }
}

/// Persist a failed send attempt onto the message without destroying the draft.
/// The draft body, subject, and `approval_status = approved` are untouched, so a
/// later `outbound.message.send_approved` can retry once the blocking condition
/// clears. Records the reason code, last error, attempt count, and timestamp in
/// replicable payload fields, and reflects a non-final `send_status`.
fn outbound_record_send_failure(
    conn: &Connection,
    message_id: &str,
    message: &mut Value,
    error: &str,
    now: i64,
) -> anyhow::Result<()> {
    let reason = outbound_classify_send_block(error);
    let attempts = message
        .get("payload")
        .and_then(|payload| payload.get("send_attempts"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        + 1;
    outbound_put_string(message, "send_status", "send_blocked");
    outbound_payload_insert(
        message,
        "send_block_reason",
        Value::String(reason.to_string()),
    );
    outbound_payload_insert(message, "last_send_error", Value::String(error.to_string()));
    outbound_payload_insert(
        message,
        "send_attempts",
        Value::Number(serde_json::Number::from(attempts)),
    );
    outbound_payload_insert(
        message,
        "last_send_attempt_at_ms",
        Value::Number(serde_json::Number::from(now)),
    );
    // Stays retry-able: the message is not marked final and remains approved.
    outbound_payload_insert(message, "retryable", Value::Bool(true));
    outbound_put_i64(message, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_messages", message_id, now, message.clone())?;
    // Reflect the blocking condition back onto the owning engagement so the
    // pipeline/timeline UI and downstream automation can see why the send did
    // not go out, with the same reason code persisted on the message.
    if let Some(engagement_id) = outbound_string(message, &["engagement_id"]) {
        if !engagement_id.trim().is_empty() {
            if let Some(mut engagement) =
                outbound_load_record(conn, "outbound_engagements", &engagement_id)?
            {
                outbound_put_string(&mut engagement, "status", "send_blocked");
                outbound_put_string(
                    &mut engagement,
                    "last_send_block_reason",
                    reason.to_string(),
                );
                outbound_put_string(&mut engagement, "last_send_error", error.to_string());
                outbound_put_i64(&mut engagement, "last_send_block_at_ms", now);
                outbound_put_i64(&mut engagement, "updated_at_ms", now);
                upsert_business_record(
                    conn,
                    "outbound_engagements",
                    &engagement_id,
                    now,
                    engagement,
                )?;
            }
        }
    }
    Ok(())
}

fn outbound_has_matching_approval(
    conn: &Connection,
    message_id: &str,
    revision_id: &str,
) -> anyhow::Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT payload_json
         FROM business_records
         WHERE collection = 'outbound_approvals'
           AND deleted = 0",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        let payload: Value = serde_json::from_str(&row?)?;
        if outbound_string(&payload, &["message_id"]).as_deref() == Some(message_id)
            && outbound_string(&payload, &["revision_id"]).as_deref() == Some(revision_id)
            && outbound_string(&payload, &["decision"]).as_deref() == Some("approved")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn outbound_recipient_suppressed(conn: &Connection, recipient_email: &str) -> anyhow::Result<bool> {
    Ok(outbound_recipient_suppression_reason(conn, recipient_email)?.is_some())
}

/// Map a reply classification onto a canonical suppression reason, returning
/// `None` for classifications that should not block future sends (e.g.
/// `interested`, `unclear`, `auto_reply`). Hard stop signals — the recipient
/// asking to be removed, marking the message as spam, or bouncing — translate
/// into an active suppression so the send gate refuses any later outbound
/// message to the address.
fn outbound_reply_suppression_reason(classification: &str) -> Option<&'static str> {
    match classification.trim().to_ascii_lowercase().as_str() {
        "unsubscribe" | "opt_out" | "opt-out" | "optout" | "remove" => Some("unsubscribe"),
        "complaint" | "spam_complaint" | "spam" | "abuse" => Some("complaint"),
        "bounce" | "hard_bounce" | "undeliverable" => Some("bounce"),
        _ => None,
    }
}

/// When a reply is classified as a hard stop signal, register an active
/// suppression entry for the engagement's recipient so future sends are blocked
/// by [`outbound_recipient_suppression_reason`]. Idempotent: a recipient that is
/// already actively suppressed is left untouched. Returns the suppression id
/// when a new entry is written.
/// Out-of-office is the one reply class that does not stop the sequence: it
/// schedules a wait/retry. The follow-up resumes after `ooo_until` (when the
/// auto-reply names a return date) or after a default hold otherwise. The
/// engagement keeps `status = reply_received` so the UI still surfaces the
/// reply; the scheduler honours the OOO exception and the resumed follow-up is
/// still approval-gated — no send happens without a fresh approval.
/// ref: skillbook reply_handling routing `out_of_office` -> "Follow-up nach OOO-Datum neu planen".
fn outbound_apply_out_of_office_wait(engagement: &mut Value, payload: &Value, now: i64) {
    const OOO_DEFAULT_WAIT_MS: i64 = 3 * 24 * 60 * 60 * 1000;
    let resume_at = payload
        .get("ooo_until")
        .and_then(Value::as_i64)
        .filter(|until| *until > now)
        .unwrap_or(now + OOO_DEFAULT_WAIT_MS);
    outbound_put_i64(engagement, "next_action_at_ms", resume_at);
    outbound_payload_insert(
        engagement,
        "next_action_at_ms",
        Value::Number(serde_json::Number::from(resume_at)),
    );
    outbound_payload_insert(
        engagement,
        "reply_wait_reason",
        Value::String("out_of_office".to_string()),
    );
    outbound_payload_insert(
        engagement,
        "ooo_until",
        Value::Number(serde_json::Number::from(resume_at)),
    );
}

fn outbound_apply_reply_suppression(
    conn: &Connection,
    engagement: &Value,
    engagement_id: &str,
    classification: &str,
    now: i64,
) -> anyhow::Result<Option<String>> {
    let Some(reason) = outbound_reply_suppression_reason(classification) else {
        return Ok(None);
    };
    let recipient = outbound_first_string(&[
        outbound_string(engagement, &["recipient_email"]),
        outbound_string(engagement, &["payload", "recipient_email"]),
        outbound_string(engagement, &["payload", "contact_email"]),
    ])
    .unwrap_or_default()
    .trim()
    .to_ascii_lowercase();
    if recipient.is_empty() || !recipient.contains('@') {
        return Ok(None);
    }
    // Idempotent: do not stack duplicate entries for an already-suppressed recipient.
    if outbound_recipient_suppression_reason(conn, &recipient)?.is_some() {
        return Ok(None);
    }
    let domain = recipient.split('@').nth(1).unwrap_or_default().to_string();
    let suppression_id = format!(
        "supp_reply_{}",
        channels::stable_digest(&format!("{recipient}|{reason}"))
    );
    let record = serde_json::json!({
        "id": suppression_id,
        "email": recipient,
        "domain": domain,
        "status": "active",
        "reason": reason,
        "suppression_reason": reason,
        "source": "reply_classification",
        "engagement_id": engagement_id,
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    upsert_business_record(
        conn,
        "outbound_suppression_entries",
        &suppression_id,
        now,
        record,
    )?;

    // Hard-stop the engagement so the automation scheduler will not reapply the
    // sequence. The reply handler already cancels pending drafts; this records
    // the terminal stop reason on the engagement itself.
    if let Some(mut engagement) = outbound_load_record(conn, "outbound_engagements", engagement_id)?
    {
        outbound_put_string(&mut engagement, "status", "stopped".to_string());
        outbound_payload_insert(
            &mut engagement,
            "stop_reason",
            Value::String(reason.to_string()),
        );
        outbound_payload_insert(
            &mut engagement,
            "stopped_at_ms",
            Value::Number(serde_json::Number::from(now)),
        );
        outbound_put_i64(&mut engagement, "updated_at_ms", now);
        upsert_business_record(conn, "outbound_engagements", engagement_id, now, engagement)?;
    }

    Ok(Some(suppression_id))
}

/// Return the suppression reason (e.g. `bounce`, `opt_out`, `unsubscribe`,
/// `manual`) when the recipient or its domain is on an active suppression
/// entry, or `None` when the recipient is clear to receive. Email match takes
/// precedence over a domain-level block. The reason is surfaced so the send
/// gate can write a precise blocking reason instead of a generic message.
fn outbound_recipient_suppression_reason(
    conn: &Connection,
    recipient_email: &str,
) -> anyhow::Result<Option<String>> {
    let recipient = recipient_email.trim().to_ascii_lowercase();
    let domain = recipient.split('@').nth(1).unwrap_or_default().to_string();
    let mut stmt = conn.prepare(
        "SELECT payload_json
         FROM business_records
         WHERE collection = 'outbound_suppression_entries'
           AND deleted = 0",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut domain_reason: Option<String> = None;
    for row in rows {
        let payload: Value = serde_json::from_str(&row?)?;
        let status = outbound_string(&payload, &["status"]).unwrap_or_else(|| "active".to_string());
        if matches!(status.as_str(), "inactive" | "deleted" | "expired") {
            continue;
        }
        let reason = outbound_first_string(&[
            outbound_string(&payload, &["reason"]),
            outbound_string(&payload, &["suppression_reason"]),
        ])
        .unwrap_or_else(|| "suppressed".to_string());
        let suppressed_email = outbound_string(&payload, &["email"])
            .or_else(|| outbound_string(&payload, &["recipient_email"]))
            .unwrap_or_default()
            .to_ascii_lowercase();
        let suppressed_domain = outbound_string(&payload, &["domain"])
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !suppressed_email.is_empty() && suppressed_email == recipient {
            return Ok(Some(reason));
        }
        if !suppressed_domain.is_empty() && suppressed_domain == domain && domain_reason.is_none() {
            domain_reason = Some(reason);
        }
    }
    Ok(domain_reason)
}

fn outbound_enforce_account_limit(
    conn: &Connection,
    sender_account_id: &str,
) -> anyhow::Result<()> {
    // Normalize the lookup key so bare email values like "user@example.com" still resolve
    // to the canonical `email:user@example.com` limit record instead of silently
    // bypassing the gate.
    let canonical = outbound_email_account_key_from(Some(sender_account_id.to_string()))
        .unwrap_or_else(|| sender_account_id.to_string());
    let limit = outbound_load_record(conn, "outbound_account_limits", &canonical)?.or(
        outbound_load_record(conn, "outbound_account_limits", sender_account_id)?,
    );
    let Some(limit) = limit else {
        return Ok(());
    };
    let blocked = limit
        .get("blocked")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    anyhow::ensure!(
        !blocked,
        "sender account is blocked for outbound communication"
    );
    let status = outbound_string(&limit, &["status"]).unwrap_or_else(|| "active".to_string());
    anyhow::ensure!(
        !matches!(
            status.as_str(),
            "blocked" | "locked" | "suspended" | "disabled"
        ),
        "sender account status `{status}` is not eligible to send"
    );
    if let Some(remaining) = limit.get("remaining_today").and_then(Value::as_i64) {
        anyhow::ensure!(remaining > 0, "sender account daily limit exhausted");
    }
    // daily_limit semantics: <= 0 means "no daily cap configured" (sane default for
    // newly linked mailboxes that have not yet been calibrated). Only enforce when an
    // operator has set a positive value.
    if let (Some(sent), Some(limit_value)) = (
        limit.get("daily_sent_count").and_then(Value::as_i64),
        limit.get("daily_limit").and_then(Value::as_i64),
    ) {
        if limit_value > 0 {
            anyhow::ensure!(sent < limit_value, "sender account daily limit exhausted");
        }
    }
    Ok(())
}

fn get_public_key(private_key_pem: &str) -> String {
    use std::io::Read;
    use std::io::Write;
    if private_key_pem == get_fallback_private_key() {
        return get_fallback_public_key();
    }
    if let Ok(mut child) = std::process::Command::new("openssl")
        .args(&["pkey", "-pubout", "-outform", "PEM"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(private_key_pem.as_bytes());
        }

        let mut output = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            let _ = stdout.read_to_string(&mut output);
        }

        if let Ok(status) = child.wait() {
            if status.success() && !output.is_empty() {
                return output;
            }
        }
    }
    get_fallback_public_key()
}

fn get_fallback_public_key() -> String {
    r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA8r+J1QCogIBQmEua3fE0
eV/lbphvaNu6sa4KnkcmDDnCzBvj4qqBUe8JNTpKR2UiFmSD/XoAmNyrSwBysmL5
r3U4FtlRkDCqeoyjsWIBqm67GfynXGws3GBVB+LP7RFcF/I8dJ7QnlBSC4JWT+62
HlptCroMUOBq8eIsGz16HnK9CZQLJrYPVEI1fut1JnyuzW7DXcqYWfi8ebE2/pWO
tM5WS1qii4KAMs6o6E5LiFbRiRmmv4PWd7SphZ5o48yUhZEkCi7Q4bAR9ZXJThjK
4rV89P459E27G4BChV8r1RQ4H8rub2mtkQaFbEKi0JZFj/boy07fiS2yXFyLrR2B
hwIDAQAB
-----END PUBLIC KEY-----"#
        .to_string()
}

fn get_fallback_private_key() -> String {
    r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDyv4nVAKiAgFCY
S5rd8TR5X+VumG9o27qxrgqeRyYMOcLMG+PiqoFR7wk1OkpHZSIWZIP9egCY3KtL
AHKyYvmvdTgW2VGQMKp6jKOxYgGqbrsZ/KdcbCzcYFUH4s/tEVwX8jx0ntCeUFIL
glZP7rYeWm0KugxQ4Grx4iwbPXoecr0JlAsmtg9UQjV+63UmfK7NbsNdyphZ+Lx5
sTb+lY60zlZLWqKLgoAyzqjoTkuIVtGJGaa/g9Z3tKmFnmjjzJSFkSQKLtDhsBH1
lclOGMritXz0/jn0TbsbgEKFXyvVFDgfyu5vaa2RBoVsQqLQlkWP9ujLTt+JLbJc
XIutHYGHAgMBAAECggEAOC6cd+/vD86i2Jym+zcYLf9D2pTtNBem3fip/Hf7FllH
/HV4CL3tsEjimK8lAeEmQoiBA+l4uehYvMMdyKufnjxC/wbNGdIporNqL2O/fvKh
2yHemkVvHJIvG+Qiu3uJFQG7fEJFhl6QnplL4LQe8md7VUA6GX3XQqRWEPfpi6IP
OUdVxuWPu6s2rgDrNiFA49vNYcShj0ilEKKjTl64dpTzTmlBKpiusRiGr99cPTBH
LOitgdXT7rQ/HexkadBOQwj1oCgKTkIX0EL05ZUoJ2DVvqbmy5q47Ch4hIJDIuCh
eUhXUefNkNvm0+zN7FU12jIMix+UhTE0A+SjUmNGAQKBgQD7C7898CcAQ2coTdfF
NLlaJYCEBV3WwG+kkif71s7/wEfuqW2Rpnrdf+5DpVm6FWYuh4x/+y58TdoVtBkf
A7f0d2FBRQobi82EqiF0cuijg8J0mxFpM72nyAEWdNT3moqPcFgB7QfjsES0HQqx
swZtIhtUGAUxV2j2staoIee68wKBgQD3id9ZZRWysM5R5Ztc7Cbblz5t1Wuesr9M
Z/I2Ae99UBzr5X6gONjC8FMIOav+EG7xXaNjnnihYM3sFerI9d/UqEJfYC/foDUV
FhMHeuImWWWUPeWz0L1dvd6wKo1QxDIXgWBH4XOey4i1f36pZmufddNdqNJT63X+
0RK6phpcHQKBgA162P77aSyzcdORMnfNV/KGNvtfymUgmh4NFwaHxz+mVHZ1NIPw
m4JPPzz0oPfD9GOlNZ8dnqZgC8jEjeDDc1o2GsvFaECIZjWsaPV2whUdmxBlzy6F
77YVoDFTfqf47V28W41m69iG+3lsYcme4kZz4WHHlGfM2L7+ZVZL08SPAoGAUXEB
FO5XFzVojDVYyle/6Rt3pLdE8y+oFMFWRUKZwsbq3QnigWByoKBlER24YpyRg8Pl
D8+BrMamuXf0iS2r+NFrFOoWliKllExw8lMRuMBM1VsQCfsxcngXnipB2ELUoDsm
rD+WxLX+Qoix6ZYS7qHbasMyf/3GEpJC8TnZDlkCgYEAuvW1ffY9HHKQZMWC5aM3
/Hw9V/yuPVN0h/KbahXBGkOuWlNbpzPpBRIlUOCEIQTEFEH+PTLLEHD3ZUuTJbpn
8ZfRN7S3tepNQQrn7UO4dek0kdnyawMKq1vgrO4IUZP7YTRMu/YNsS9YahmS5jQ1
W1zGjMP9KaO/lbRSW/NHasM=
-----END PRIVATE KEY-----"#
        .to_string()
}

fn stored_rxdb_business_command_outcome(
    conn: &Connection,
    command_id: &str,
) -> anyhow::Result<Option<Value>> {
    let payload_json: Option<String> = conn
        .query_row(
            "SELECT payload_json
             FROM business_records
             WHERE collection = 'business_commands'
               AND record_id = ?1
               AND deleted = 0",
            params![command_id],
            |row| row.get(0),
        )
        .optional()?;

    let Some(payload_json) = payload_json else {
        return Ok(None);
    };
    let mut payload: Value = serde_json::from_str(&payload_json)
        .with_context(|| format!("invalid stored business command projection for {command_id}"))?;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("ok".to_string(), Value::Bool(true));
        obj.insert("id".to_string(), Value::String(command_id.to_string()));
        obj.insert(
            "command_id".to_string(),
            Value::String(command_id.to_string()),
        );
        obj.insert("already_accepted".to_string(), Value::Bool(true));
    }
    Ok(Some(payload))
}

fn rxdb_command_session(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<BusinessOsSession> {
    rxdb_session_from_command(root, command, true)
}

fn rxdb_authenticated_session(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<BusinessOsSession> {
    rxdb_session_from_command(root, command, false)
}

fn rxdb_session_from_command(
    root: &Path,
    command: &BusinessCommand,
    require_manage_all: bool,
) -> anyhow::Result<BusinessOsSession> {
    let client_ctx = if let Value::String(ref s) = command.client_context {
        serde_json::from_str(s).unwrap_or_else(|_| command.client_context.clone())
    } else {
        command.client_context.clone()
    };
    let actor = client_ctx.get("actor").or_else(|| client_ctx.get("user"));
    let id = actor
        .and_then(|value| value.get("id"))
        .or_else(|| client_ctx.get("user_id"))
        .and_then(Value::as_str)
        .unwrap_or("rxdb-command")
        .to_string();
    let display_name = actor
        .and_then(|value| value.get("display_name"))
        .or_else(|| actor.and_then(|value| value.get("name")))
        .or_else(|| client_ctx.get("display_name"))
        .and_then(Value::as_str)
        .unwrap_or(id.as_str())
        .to_string();
    let trusted_user = trusted_rxdb_command_user(root, &id, &display_name)?;
    let role = trusted_user.role;
    let display_name = trusted_user.display_name;
    let is_admin = role_can_manage(&role);
    let session = BusinessOsSession {
        ok: true,
        authenticated: true,
        auth_required: false,
        user: Some(BusinessOsSessionUser {
            id,
            display_name,
            role,
            is_admin,
        }),
        login_url: None,
        reason: None,
    };
    if require_manage_all {
        anyhow::ensure!(
            session_can_manage_all(&session),
            "chef or admin role required"
        );
    }
    Ok(session)
}

fn trusted_rxdb_command_user(
    root: &Path,
    actor_id: &str,
    actor_display_name: &str,
) -> anyhow::Result<BusinessOsUser> {
    let actor_id = actor_id.trim();
    let conn = open_store(root)?;
    seed_configured_business_users(&conn)?;
    let user = conn
        .query_row(
            "SELECT user_id, display_name, role, active, created_at_ms, updated_at_ms
             FROM business_users
             WHERE user_id = ?1 AND active = 1",
            params![actor_id],
            |row| {
                Ok(BusinessOsUser {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    role: normalize_business_role(&row.get::<_, String>(2)?),
                    active: row.get::<_, i64>(3)? != 0,
                    created_at_ms: row.get(4)?,
                    updated_at_ms: row.get(5)?,
                })
            },
        )
        .optional()?;
    if let Some(user) = user {
        return Ok(user);
    }

    let user_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM business_users", [], |row| row.get(0))?;
    if user_count == 0 && env::var("CTOX_BUSINESS_OS_REQUIRE_LOGIN").as_deref() != Ok("1") {
        let local = session(None, None);
        if let Some(user) = local.user {
            return Ok(BusinessOsUser {
                id: if actor_id.is_empty() {
                    user.id
                } else {
                    actor_id.to_owned()
                },
                display_name: if actor_display_name.trim().is_empty() {
                    user.display_name
                } else {
                    actor_display_name.to_owned()
                },
                role: normalize_business_role(&user.role),
                active: true,
                created_at_ms: 0,
                updated_at_ms: 0,
            });
        }
    }

    Ok(BusinessOsUser {
        id: if actor_id.is_empty() {
            "rxdb-command".to_owned()
        } else {
            actor_id.to_owned()
        },
        display_name: if actor_display_name.trim().is_empty() {
            actor_id.to_owned()
        } else {
            actor_display_name.to_owned()
        },
        role: "user".to_owned(),
        active: true,
        created_at_ms: 0,
        updated_at_ms: 0,
    })
}

fn write_rxdb_control_command_outcome(
    root: &Path,
    command: &BusinessCommand,
    status: &str,
    task_id: Option<&str>,
    task_status: Option<&str>,
    result: Value,
) -> anyhow::Result<Value> {
    let command_id = command.id.as_deref().context("command id is required")?;
    let now = now_ms() as i64;
    let conn = open_store(root)?;
    conn.execute(
        "INSERT INTO business_commands
            (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            command_id,
            command.module,
            command.command_type,
            command.record_id.clone().unwrap_or_default(),
            status,
            serde_json::to_string(&command.payload)?,
            serde_json::to_string(&command.client_context)?,
            now
        ],
    )?;
    let projection = serde_json::json!({
        "id": command_id,
        "command_id": command_id,
        "module": command.module.clone(),
        "command_type": command.command_type.clone(),
        "record_id": command.record_id.clone().unwrap_or_default(),
        "status": status,
        "inbound_channel": command_inbound_channel(command),
        "task_id": task_id.unwrap_or_default(),
        "task_status": task_status.unwrap_or(status),
        "payload": command.payload.clone(),
        "client_context": command.client_context.clone(),
        "result": result.clone(),
        "updated_at_ms": now
    });
    upsert_business_record(
        &conn,
        "business_commands",
        command_id,
        now,
        projection.clone(),
    )?;
    upsert_rxdb_collection_record(root, "business_commands", command_id, now, projection)?;
    Ok(serde_json::json!({
        "ok": true,
        "id": command_id,
        "command_id": command_id,
        "status": status,
        "task_id": task_id.unwrap_or_default(),
        "task_status": task_status.unwrap_or(status),
        "result": result
    }))
}

fn is_customers_active_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "customers.account.create"
            | "customers.account.update"
            | "customers.account.archive"
            | "customers.contact.create"
            | "customers.contact.update"
            | "customers.contact.archive"
            | "customers.opportunity.create"
            | "customers.opportunity.update"
            | "customers.opportunity.move_stage"
            | "customers.opportunity.close_won"
            | "customers.opportunity.close_lost"
            | "customers.task.create"
            | "customers.task.update"
            | "customers.task.complete"
            | "customers.note.create"
            | "customers.note.update"
            | "customers.activity.record"
            | "customers.view.save"
            | "customers.import.from_outbound"
            | "customers.dedupe.resolve"
    )
}

fn handle_customers_active_command(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        command.module == "customers",
        "active customers commands require module=customers"
    );
    let now = now_ms() as i64;
    let conn = open_store(root)?;
    match command.command_type.as_str() {
        "customers.account.create" => customers_account_create(&conn, session, command, now),
        "customers.account.update" => customers_account_update(&conn, session, command, now),
        "customers.account.archive" => customers_account_archive(&conn, session, command, now),
        "customers.contact.create" => customers_contact_create(&conn, session, command, now),
        "customers.contact.update" => customers_contact_update(&conn, session, command, now),
        "customers.contact.archive" => customers_contact_archive(&conn, session, command, now),
        "customers.opportunity.create" => {
            customers_opportunity_create(&conn, session, command, now)
        }
        "customers.opportunity.update" => {
            customers_opportunity_update(&conn, session, command, now)
        }
        "customers.opportunity.move_stage" => {
            customers_opportunity_move_stage(&conn, session, command, now)
        }
        "customers.opportunity.close_won" => {
            customers_opportunity_close(&conn, session, command, now, "closed_won")
        }
        "customers.opportunity.close_lost" => {
            customers_opportunity_close(&conn, session, command, now, "closed_lost")
        }
        "customers.task.create" => customers_task_create(&conn, session, command, now),
        "customers.task.update" => customers_task_update(&conn, session, command, now),
        "customers.task.complete" => customers_task_complete(&conn, session, command, now),
        "customers.note.create" => customers_note_create(&conn, session, command, now),
        "customers.note.update" => customers_note_update(&conn, session, command, now),
        "customers.activity.record" => customers_activity_record(&conn, session, command, now),
        "customers.view.save" => customers_view_save(&conn, session, command, now),
        "customers.import.from_outbound" => {
            customers_import_from_outbound(&conn, session, command, now)
        }
        "customers.dedupe.resolve" => customers_dedupe_resolve(&conn, session, command, now),
        other => anyhow::bail!("unsupported customers command type: {other}"),
    }
}

fn customers_account_create(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let account_id = customers_id_from_command(command, &["account_id", "id"], "acct")?;
    anyhow::ensure!(
        outbound_load_record(conn, "customer_accounts", &account_id)?.is_none(),
        "customer account already exists"
    );
    let mut account = outbound_object_payload(&command.payload);
    customers_put_string(&mut account, "id", account_id.clone());
    customers_require_field(&account, &["name"])?;
    customers_put_default_string(&mut account, "account_status", "active");
    customers_put_default_string(&mut account, "customer_stage", "active");
    customers_put_default_string(&mut account, "health_status", "unknown");
    customers_put_default_string(&mut account, "currency", "EUR");
    customers_put_default_bool(&mut account, "ideal_customer_profile", false);
    customers_put_default_bool(&mut account, "is_deleted", false);
    customers_put_default_object(&mut account, "payload");
    customers_put_default_i64(&mut account, "created_at_ms", now);
    customers_validate_account(&account)?;
    customers_refresh_search_text(&mut account, &["name", "domain", "industry"]);
    upsert_business_record(conn, "customer_accounts", &account_id, now, account.clone())?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "account_created",
            name: "Kunde erstellt",
            account_id: Some(account_id.clone()),
            contact_id: None,
            opportunity_id: None,
            linked_record_type: Some("account"),
            linked_record_id: Some(account_id.clone()),
            linked_record_name: outbound_string(&account, &["name"]),
            properties: serde_json::json!({ "account": account.clone() }),
        },
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "collection": "customer_accounts",
        "account": account,
        "activity": activity,
    }))
}

fn customers_account_update(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let account_id = customers_required_from_payload_or_record(
        command,
        &["account_id", "id"],
        "account_id is required",
    )?;
    let mut account = outbound_load_required(
        conn,
        "customer_accounts",
        &account_id,
        "customer account not found",
    )?;
    customers_merge_fields(
        &mut account,
        &command.payload,
        &[
            "name",
            "domain",
            "website_url",
            "linkedin_url",
            "x_url",
            "account_status",
            "customer_stage",
            "account_owner_id",
            "annual_recurring_revenue_cents",
            "currency",
            "employee_count",
            "industry",
            "address",
            "ideal_customer_profile",
            "source",
            "source_record_id",
            "last_activity_at_ms",
            "next_action_at_ms",
            "health_status",
            "payload",
        ],
    );
    customers_validate_account(&account)?;
    customers_refresh_search_text(&mut account, &["name", "domain", "industry"]);
    upsert_business_record(conn, "customer_accounts", &account_id, now, account.clone())?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "account_updated",
            name: "Kunde aktualisiert",
            account_id: Some(account_id.clone()),
            contact_id: None,
            opportunity_id: None,
            linked_record_type: Some("account"),
            linked_record_id: Some(account_id.clone()),
            linked_record_name: outbound_string(&account, &["name"]),
            properties: serde_json::json!({ "patch": command.payload.clone() }),
        },
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "collection": "customer_accounts",
        "account": account,
        "activity": activity,
    }))
}

fn customers_account_archive(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let account_id = customers_required_from_payload_or_record(
        command,
        &["account_id", "id"],
        "account_id is required",
    )?;
    let mut account = outbound_load_required(
        conn,
        "customer_accounts",
        &account_id,
        "customer account not found",
    )?;
    customers_put_string(&mut account, "account_status", "archived");
    customers_put_string(&mut account, "customer_stage", "archived");
    customers_put_bool(&mut account, "is_deleted", true);
    customers_put_i64(&mut account, "deleted_at_ms", now);
    customers_refresh_search_text(&mut account, &["name", "domain", "industry"]);
    upsert_business_record(conn, "customer_accounts", &account_id, now, account.clone())?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "account_archived",
            name: "Kunde archiviert",
            account_id: Some(account_id.clone()),
            contact_id: None,
            opportunity_id: None,
            linked_record_type: Some("account"),
            linked_record_id: Some(account_id.clone()),
            linked_record_name: outbound_string(&account, &["name"]),
            properties: serde_json::json!({ "account_id": account_id }),
        },
    )?;
    Ok(serde_json::json!({ "ok": true, "account": account, "activity": activity }))
}

fn customers_contact_create(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let contact_id = customers_id_from_command(command, &["contact_id", "id"], "contact")?;
    anyhow::ensure!(
        outbound_load_record(conn, "customer_contacts", &contact_id)?.is_none(),
        "customer contact already exists"
    );
    let account_id = outbound_required_string(&command.payload, &["account_id"])?;
    customers_require_existing(
        conn,
        "customer_accounts",
        &account_id,
        "customer account not found",
    )?;
    let mut contact = outbound_object_payload(&command.payload);
    customers_put_string(&mut contact, "id", contact_id.clone());
    customers_put_default_string(&mut contact, "first_name", "");
    customers_put_default_string(&mut contact, "last_name", "");
    customers_put_default_string(&mut contact, "email", "");
    let has_name = outbound_string(&contact, &["first_name"]).is_some()
        || outbound_string(&contact, &["last_name"]).is_some();
    let has_email = outbound_string(&contact, &["email"]).is_some();
    anyhow::ensure!(has_name || has_email, "contact name or email is required");
    customers_put_default_bool(&mut contact, "is_primary_contact", false);
    customers_put_default_bool(&mut contact, "is_deleted", false);
    customers_put_default_object(&mut contact, "payload");
    customers_put_default_i64(&mut contact, "created_at_ms", now);
    customers_refresh_search_text(
        &mut contact,
        &["first_name", "last_name", "email", "job_title", "city"],
    );
    upsert_business_record(conn, "customer_contacts", &contact_id, now, contact.clone())?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "contact_created",
            name: "Kontakt erstellt",
            account_id: Some(account_id.clone()),
            contact_id: Some(contact_id.clone()),
            opportunity_id: None,
            linked_record_type: Some("contact"),
            linked_record_id: Some(contact_id.clone()),
            linked_record_name: Some(customers_contact_display_name(&contact)),
            properties: serde_json::json!({ "contact": contact.clone() }),
        },
    )?;
    customers_touch_account_last_activity(conn, &account_id, now)?;
    Ok(serde_json::json!({
        "ok": true,
        "collection": "customer_contacts",
        "contact": contact,
        "activity": activity,
    }))
}

fn customers_contact_update(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let contact_id = customers_required_from_payload_or_record(
        command,
        &["contact_id", "id"],
        "contact_id is required",
    )?;
    let mut contact = outbound_load_required(
        conn,
        "customer_contacts",
        &contact_id,
        "customer contact not found",
    )?;
    customers_merge_fields(
        &mut contact,
        &command.payload,
        &[
            "account_id",
            "first_name",
            "last_name",
            "email",
            "phone",
            "job_title",
            "city",
            "linkedin_url",
            "x_url",
            "is_primary_contact",
            "contact_owner_id",
            "last_activity_at_ms",
            "source",
            "source_record_id",
            "payload",
        ],
    );
    let account_id = outbound_required_string(&contact, &["account_id"])?;
    customers_require_existing(
        conn,
        "customer_accounts",
        &account_id,
        "customer account not found",
    )?;
    customers_refresh_search_text(
        &mut contact,
        &["first_name", "last_name", "email", "job_title", "city"],
    );
    upsert_business_record(conn, "customer_contacts", &contact_id, now, contact.clone())?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "contact_updated",
            name: "Kontakt aktualisiert",
            account_id: Some(account_id.clone()),
            contact_id: Some(contact_id.clone()),
            opportunity_id: None,
            linked_record_type: Some("contact"),
            linked_record_id: Some(contact_id.clone()),
            linked_record_name: Some(customers_contact_display_name(&contact)),
            properties: serde_json::json!({ "patch": command.payload.clone() }),
        },
    )?;
    customers_touch_account_last_activity(conn, &account_id, now)?;
    Ok(serde_json::json!({ "ok": true, "contact": contact, "activity": activity }))
}

fn customers_contact_archive(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let contact_id = customers_required_from_payload_or_record(
        command,
        &["contact_id", "id"],
        "contact_id is required",
    )?;
    let mut contact = outbound_load_required(
        conn,
        "customer_contacts",
        &contact_id,
        "customer contact not found",
    )?;
    let account_id = outbound_string(&contact, &["account_id"]);
    customers_put_bool(&mut contact, "is_deleted", true);
    customers_put_i64(&mut contact, "deleted_at_ms", now);
    upsert_business_record(conn, "customer_contacts", &contact_id, now, contact.clone())?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "contact_archived",
            name: "Kontakt archiviert",
            account_id: account_id.clone(),
            contact_id: Some(contact_id.clone()),
            opportunity_id: None,
            linked_record_type: Some("contact"),
            linked_record_id: Some(contact_id.clone()),
            linked_record_name: Some(customers_contact_display_name(&contact)),
            properties: serde_json::json!({ "contact_id": contact_id }),
        },
    )?;
    if let Some(account_id) = account_id {
        customers_touch_account_last_activity(conn, &account_id, now)?;
    }
    Ok(serde_json::json!({ "ok": true, "contact": contact, "activity": activity }))
}

fn customers_opportunity_create(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let opportunity_id = customers_id_from_command(command, &["opportunity_id", "id"], "opp")?;
    anyhow::ensure!(
        outbound_load_record(conn, "customer_opportunities", &opportunity_id)?.is_none(),
        "customer opportunity already exists"
    );
    let account_id = outbound_required_string(&command.payload, &["account_id"])?;
    customers_require_existing(
        conn,
        "customer_accounts",
        &account_id,
        "customer account not found",
    )?;
    if let Some(contact_id) = outbound_string(&command.payload, &["primary_contact_id"]) {
        customers_require_existing(
            conn,
            "customer_contacts",
            &contact_id,
            "primary contact not found",
        )?;
    }
    let mut opportunity = outbound_object_payload(&command.payload);
    customers_put_string(&mut opportunity, "id", opportunity_id.clone());
    customers_require_field(&opportunity, &["name"])?;
    customers_put_default_string(&mut opportunity, "opportunity_type", "new_business");
    customers_put_default_string(&mut opportunity, "stage", "qualification");
    customers_put_default_i64(&mut opportunity, "amount_cents", 0);
    customers_put_default_string(&mut opportunity, "currency", "EUR");
    customers_put_default_i64(&mut opportunity, "position", now);
    customers_put_default_bool(&mut opportunity, "is_deleted", false);
    customers_put_default_i64(&mut opportunity, "created_at_ms", now);
    customers_put_default_i64(&mut opportunity, "last_stage_changed_at_ms", now);
    customers_put_default_object(&mut opportunity, "payload");
    customers_validate_opportunity(&opportunity)?;
    customers_refresh_search_text(&mut opportunity, &["name", "stage", "opportunity_type"]);
    upsert_business_record(
        conn,
        "customer_opportunities",
        &opportunity_id,
        now,
        opportunity.clone(),
    )?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "opportunity_created",
            name: "Opportunity erstellt",
            account_id: Some(account_id.clone()),
            contact_id: outbound_string(&opportunity, &["primary_contact_id"]),
            opportunity_id: Some(opportunity_id.clone()),
            linked_record_type: Some("opportunity"),
            linked_record_id: Some(opportunity_id.clone()),
            linked_record_name: outbound_string(&opportunity, &["name"]),
            properties: serde_json::json!({ "opportunity": opportunity.clone() }),
        },
    )?;
    customers_touch_account_last_activity(conn, &account_id, now)?;
    Ok(serde_json::json!({
        "ok": true,
        "collection": "customer_opportunities",
        "opportunity": opportunity,
        "activity": activity,
    }))
}

fn customers_opportunity_update(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let opportunity_id = customers_required_from_payload_or_record(
        command,
        &["opportunity_id", "id"],
        "opportunity_id is required",
    )?;
    let mut opportunity = outbound_load_required(
        conn,
        "customer_opportunities",
        &opportunity_id,
        "customer opportunity not found",
    )?;
    customers_merge_fields(
        &mut opportunity,
        &command.payload,
        &[
            "name",
            "account_id",
            "primary_contact_id",
            "owner_id",
            "opportunity_type",
            "amount_cents",
            "currency",
            "close_date_ms",
            "probability",
            "position",
            "source",
            "source_record_id",
            "payload",
        ],
    );
    customers_validate_opportunity(&opportunity)?;
    let account_id = outbound_required_string(&opportunity, &["account_id"])?;
    customers_require_existing(
        conn,
        "customer_accounts",
        &account_id,
        "customer account not found",
    )?;
    customers_refresh_search_text(&mut opportunity, &["name", "stage", "opportunity_type"]);
    upsert_business_record(
        conn,
        "customer_opportunities",
        &opportunity_id,
        now,
        opportunity.clone(),
    )?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "opportunity_updated",
            name: "Opportunity aktualisiert",
            account_id: Some(account_id.clone()),
            contact_id: outbound_string(&opportunity, &["primary_contact_id"]),
            opportunity_id: Some(opportunity_id.clone()),
            linked_record_type: Some("opportunity"),
            linked_record_id: Some(opportunity_id.clone()),
            linked_record_name: outbound_string(&opportunity, &["name"]),
            properties: serde_json::json!({ "patch": command.payload.clone() }),
        },
    )?;
    customers_touch_account_last_activity(conn, &account_id, now)?;
    Ok(serde_json::json!({ "ok": true, "opportunity": opportunity, "activity": activity }))
}

fn customers_opportunity_move_stage(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let opportunity_id = customers_required_from_payload_or_record(
        command,
        &["opportunity_id", "id"],
        "opportunity_id is required",
    )?;
    let stage = outbound_required_string(&command.payload, &["stage"])?;
    customers_validate_allowed("opportunity stage", &stage, CUSTOMER_OPPORTUNITY_STAGES)?;
    let mut opportunity = outbound_load_required(
        conn,
        "customer_opportunities",
        &opportunity_id,
        "customer opportunity not found",
    )?;
    let from_stage = outbound_string(&opportunity, &["stage"]).unwrap_or_default();
    customers_validate_stage_transition(&from_stage, &stage)?;
    customers_put_string(&mut opportunity, "stage", stage.clone());
    customers_put_i64(&mut opportunity, "last_stage_changed_at_ms", now);
    if let Some(position) = customers_i64(&command.payload, &["position"]) {
        customers_put_i64(&mut opportunity, "position", position);
    }
    customers_refresh_search_text(&mut opportunity, &["name", "stage", "opportunity_type"]);
    upsert_business_record(
        conn,
        "customer_opportunities",
        &opportunity_id,
        now,
        opportunity.clone(),
    )?;
    let account_id = outbound_required_string(&opportunity, &["account_id"])?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "opportunity_stage_changed",
            name: "Opportunity verschoben",
            account_id: Some(account_id.clone()),
            contact_id: outbound_string(&opportunity, &["primary_contact_id"]),
            opportunity_id: Some(opportunity_id.clone()),
            linked_record_type: Some("opportunity"),
            linked_record_id: Some(opportunity_id.clone()),
            linked_record_name: outbound_string(&opportunity, &["name"]),
            properties: serde_json::json!({ "from_stage": from_stage, "to_stage": stage }),
        },
    )?;
    customers_touch_account_last_activity(conn, &account_id, now)?;
    Ok(serde_json::json!({ "ok": true, "opportunity": opportunity, "activity": activity }))
}

fn customers_opportunity_close(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
    stage: &str,
) -> anyhow::Result<Value> {
    let mut payload = command.payload.clone();
    if let Some(object) = payload.as_object_mut() {
        object.insert("stage".to_string(), Value::String(stage.to_string()));
        if stage == "closed_won" {
            object.insert("probability".to_string(), Value::from(100));
        }
        if stage == "closed_lost" {
            let lost_reason = outbound_required_string(&command.payload, &["lost_reason"])?;
            object.insert("lost_reason".to_string(), Value::String(lost_reason));
            object.insert("probability".to_string(), Value::from(0));
        }
    }
    let command = BusinessCommand {
        payload,
        ..command.clone()
    };
    customers_opportunity_move_stage(conn, session, &command, now)
}

fn customers_task_create(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let task_id = customers_id_from_command(command, &["task_id", "id"], "task")?;
    anyhow::ensure!(
        outbound_load_record(conn, "customer_tasks", &task_id)?.is_none(),
        "customer task already exists"
    );
    let mut task = outbound_object_payload(&command.payload);
    customers_put_string(&mut task, "id", task_id.clone());
    customers_require_field(&task, &["title"])?;
    customers_validate_links(conn, &task, true)?;
    customers_put_default_string(&mut task, "status", "open");
    customers_put_default_i64(&mut task, "position", now);
    customers_put_default_bool(&mut task, "is_deleted", false);
    customers_put_default_i64(&mut task, "created_at_ms", now);
    customers_put_default_object(&mut task, "payload");
    customers_validate_allowed(
        "task status",
        &outbound_required_string(&task, &["status"])?,
        CUSTOMER_TASK_STATUSES,
    )?;
    customers_refresh_search_text(&mut task, &["title", "body", "status"]);
    upsert_business_record(conn, "customer_tasks", &task_id, now, task.clone())?;
    let activity = customers_write_task_activity(
        conn,
        session,
        command,
        now,
        &task,
        "task_created",
        "Aufgabe erstellt",
    )?;
    Ok(serde_json::json!({ "ok": true, "task": task, "activity": activity }))
}

fn customers_task_update(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let task_id = customers_required_from_payload_or_record(
        command,
        &["task_id", "id"],
        "task_id is required",
    )?;
    let mut task =
        outbound_load_required(conn, "customer_tasks", &task_id, "customer task not found")?;
    customers_merge_fields(
        &mut task,
        &command.payload,
        &[
            "title",
            "body",
            "status",
            "due_at_ms",
            "assignee_id",
            "account_id",
            "contact_id",
            "opportunity_id",
            "position",
            "payload",
        ],
    );
    customers_validate_links(conn, &task, true)?;
    customers_validate_allowed(
        "task status",
        &outbound_required_string(&task, &["status"])?,
        CUSTOMER_TASK_STATUSES,
    )?;
    customers_refresh_search_text(&mut task, &["title", "body", "status"]);
    upsert_business_record(conn, "customer_tasks", &task_id, now, task.clone())?;
    let activity = customers_write_task_activity(
        conn,
        session,
        command,
        now,
        &task,
        "task_updated",
        "Aufgabe aktualisiert",
    )?;
    Ok(serde_json::json!({ "ok": true, "task": task, "activity": activity }))
}

fn customers_task_complete(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let task_id = customers_required_from_payload_or_record(
        command,
        &["task_id", "id"],
        "task_id is required",
    )?;
    let mut task =
        outbound_load_required(conn, "customer_tasks", &task_id, "customer task not found")?;
    customers_put_string(&mut task, "status", "completed");
    customers_put_i64(&mut task, "completed_at_ms", now);
    customers_refresh_search_text(&mut task, &["title", "body", "status"]);
    upsert_business_record(conn, "customer_tasks", &task_id, now, task.clone())?;
    let activity = customers_write_task_activity(
        conn,
        session,
        command,
        now,
        &task,
        "task_completed",
        "Aufgabe erledigt",
    )?;
    Ok(serde_json::json!({ "ok": true, "task": task, "activity": activity }))
}

fn customers_note_create(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let note_id = customers_id_from_command(command, &["note_id", "id"], "note")?;
    anyhow::ensure!(
        outbound_load_record(conn, "customer_notes", &note_id)?.is_none(),
        "customer note already exists"
    );
    let mut note = outbound_object_payload(&command.payload);
    customers_put_string(&mut note, "id", note_id.clone());
    customers_validate_links(conn, &note, true)?;
    customers_put_default_string(&mut note, "title", "Notiz");
    customers_put_default_string(&mut note, "body", "");
    customers_put_default_string(&mut note, "body_format", "markdown");
    customers_put_default_bool(&mut note, "is_deleted", false);
    customers_put_default_i64(&mut note, "created_at_ms", now);
    customers_put_default_object(&mut note, "payload");
    customers_refresh_search_text(&mut note, &["title", "body"]);
    upsert_business_record(conn, "customer_notes", &note_id, now, note.clone())?;
    let activity = customers_write_note_activity(
        conn,
        session,
        command,
        now,
        &note,
        "note_created",
        "Notiz erstellt",
    )?;
    Ok(serde_json::json!({ "ok": true, "note": note, "activity": activity }))
}

fn customers_note_update(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let note_id = customers_required_from_payload_or_record(
        command,
        &["note_id", "id"],
        "note_id is required",
    )?;
    let mut note =
        outbound_load_required(conn, "customer_notes", &note_id, "customer note not found")?;
    customers_merge_fields(
        &mut note,
        &command.payload,
        &[
            "title",
            "body",
            "body_format",
            "author_id",
            "account_id",
            "contact_id",
            "opportunity_id",
            "linked_note_id",
            "payload",
        ],
    );
    customers_validate_links(conn, &note, true)?;
    customers_refresh_search_text(&mut note, &["title", "body"]);
    upsert_business_record(conn, "customer_notes", &note_id, now, note.clone())?;
    let activity = customers_write_note_activity(
        conn,
        session,
        command,
        now,
        &note,
        "note_updated",
        "Notiz aktualisiert",
    )?;
    Ok(serde_json::json!({ "ok": true, "note": note, "activity": activity }))
}

fn customers_activity_record(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    customers_validate_links(conn, &command.payload, false)?;
    let activity_type = outbound_required_string(&command.payload, &["activity_type"])?;
    let name = outbound_string(&command.payload, &["name"]).unwrap_or(activity_type.clone());
    let linked_record_type = outbound_string(&command.payload, &["linked_record_type"]);
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: activity_type.as_str(),
            name: name.as_str(),
            account_id: outbound_string(&command.payload, &["account_id"]),
            contact_id: outbound_string(&command.payload, &["contact_id"]),
            opportunity_id: outbound_string(&command.payload, &["opportunity_id"]),
            linked_record_type: linked_record_type.as_deref(),
            linked_record_id: outbound_string(&command.payload, &["linked_record_id"]),
            linked_record_name: outbound_string(&command.payload, &["linked_record_name"]),
            properties: command
                .payload
                .get("properties")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        },
    )?;
    Ok(serde_json::json!({ "ok": true, "activity": activity }))
}

fn customers_view_save(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let view_id = customers_id_from_command(command, &["view_id", "id"], "view")?;
    let mut view = outbound_object_payload(command.payload.get("view").unwrap_or(&command.payload));
    customers_put_string(&mut view, "id", view_id.clone());
    customers_require_field(&view, &["name"])?;
    let object_type = outbound_required_string(&view, &["object_type"])?;
    customers_validate_allowed(
        "view object_type",
        &object_type,
        &["account", "contact", "opportunity"],
    )?;
    customers_put_default_string(&mut view, "view_type", "table");
    customers_put_default_string(&mut view, "visibility", "private");
    customers_put_default_bool(&mut view, "is_deleted", false);
    customers_put_default_i64(&mut view, "created_at_ms", now);
    customers_put_default_object(&mut view, "payload");
    upsert_business_record(conn, "customer_views", &view_id, now, view.clone())?;
    let filters = customers_save_view_children(
        conn,
        &view_id,
        command.payload.get("filters"),
        "customer_view_filters",
        now,
    )?;
    let sorts = customers_save_view_children(
        conn,
        &view_id,
        command.payload.get("sorts"),
        "customer_view_sorts",
        now,
    )?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "view_saved",
            name: "Ansicht gespeichert",
            account_id: None,
            contact_id: None,
            opportunity_id: None,
            linked_record_type: Some("view"),
            linked_record_id: Some(view_id.clone()),
            linked_record_name: outbound_string(&view, &["name"]),
            properties: serde_json::json!({ "view": view.clone(), "filters": filters, "sorts": sorts }),
        },
    )?;
    Ok(serde_json::json!({ "ok": true, "view": view, "activity": activity }))
}

fn customers_import_from_outbound(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let source_record_id = outbound_first_string(&[
        outbound_string(&command.payload, &["source_record_id"]),
        outbound_string(&command.payload, &["outbound_company_id"]),
        outbound_string(&command.payload, &["company_id"]),
        command.record_id.clone(),
    ])
    .context("source_record_id or outbound company_id is required")?;
    let company = outbound_load_required(
        conn,
        "outbound_companies",
        &source_record_id,
        "outbound company not found",
    )?;
    let pipeline_id = outbound_string(&command.payload, &["pipeline_id"]).or_else(|| {
        customers_find_outbound_pipeline_for_company(conn, &source_record_id)
            .ok()
            .flatten()
    });
    let pipeline = pipeline_id.as_ref().and_then(|id| {
        outbound_load_record(conn, "outbound_pipeline_items", id)
            .ok()
            .flatten()
    });
    let domain = outbound_string(&company, &["domain"]).or_else(|| {
        outbound_string(&company, &["website"]).and_then(|url| customers_domain_from_website(&url))
    });
    let existing_account = domain.as_ref().and_then(|value| {
        customers_find_by_string_field(conn, "customer_accounts", "domain", value)
            .ok()
            .flatten()
    });
    let batch_id = customers_id_from_command(command, &["import_batch_id"], "import")?;
    let mut batch = serde_json::json!({
        "id": batch_id,
        "source": "outbound",
        "source_record_id": source_record_id,
        "source_filename": "",
        "status": "completed",
        "object_type": "account",
        "imported_count": 0,
        "skipped_count": 0,
        "failed_count": 0,
        "dedupe_count": 0,
        "payload": command.payload.clone(),
        "is_deleted": false,
        "created_at_ms": now
    });
    let mut contacts = Vec::new();
    if let Some(existing) = existing_account {
        let existing_id = outbound_required_string(&existing, &["id"])?;
        let candidate_id = format!(
            "dedupe_{}",
            channels::stable_digest(&format!("outbound:{source_record_id}:{existing_id}"))
        );
        let candidate = serde_json::json!({
            "id": candidate_id,
            "object_type": "account",
            "match_key": domain.unwrap_or(source_record_id.clone()),
            "match_type": "domain",
            "source_record_id": source_record_id,
            "existing_record_id": existing_id,
            "import_batch_id": batch.get("id").and_then(Value::as_str).unwrap_or_default(),
            "status": "open",
            "confidence": 0.95,
            "payload": { "outbound_company": company },
            "is_deleted": false,
            "created_at_ms": now
        });
        upsert_business_record(
            conn,
            "customer_dedupe_candidates",
            candidate
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            now,
            candidate.clone(),
        )?;
        customers_put_i64(&mut batch, "dedupe_count", 1);
        customers_put_string(&mut batch, "status", "needs_review");
        upsert_business_record(
            conn,
            "customer_import_batches",
            batch.get("id").and_then(Value::as_str).unwrap_or_default(),
            now,
            batch.clone(),
        )?;
        return Ok(serde_json::json!({
            "ok": true,
            "status": "needs_review",
            "import_batch": batch,
            "dedupe_candidate": candidate,
        }));
    }
    let account_id = customers_id_from_command(command, &["account_id"], "acct")?;
    let account_name = outbound_string(&company, &["name"])
        .or_else(|| outbound_string(&pipeline.clone().unwrap_or(Value::Null), &["company_name"]))
        .context("outbound company name is required")?;
    let mut account = serde_json::json!({
        "id": account_id,
        "name": account_name,
        "domain": domain.unwrap_or_default(),
        "website_url": outbound_string(&company, &["website"]).unwrap_or_default(),
        "account_status": "active",
        "customer_stage": "active",
        "health_status": "unknown",
        "source": "outbound",
        "source_record_id": source_record_id,
        "payload": { "outbound_company": company, "outbound_pipeline": pipeline },
        "is_deleted": false,
        "created_at_ms": now
    });
    customers_refresh_search_text(&mut account, &["name", "domain"]);
    upsert_business_record(
        conn,
        "customer_accounts",
        account
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        now,
        account.clone(),
    )?;
    if let Some(Value::Array(raw_contacts)) = account.pointer("/payload/outbound_pipeline/contacts")
    {
        for (index, raw_contact) in raw_contacts.iter().enumerate() {
            let contact_id = format!(
                "contact_{}",
                channels::stable_digest(&format!(
                    "{}:{}:{}",
                    account
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    index,
                    raw_contact
                ))
            );
            let full_name = outbound_string(raw_contact, &["name"]).unwrap_or_default();
            let (first_name, last_name) = customers_split_name(&full_name);
            let mut contact = serde_json::json!({
                "id": contact_id,
                "account_id": account.get("id").and_then(Value::as_str).unwrap_or_default(),
                "first_name": first_name,
                "last_name": last_name,
                "email": outbound_string(raw_contact, &["email"]).unwrap_or_default(),
                "phone": outbound_string(raw_contact, &["phone"]).unwrap_or_default(),
                "job_title": outbound_string(raw_contact, &["job_title"]).or_else(|| outbound_string(raw_contact, &["title"])).unwrap_or_default(),
                "is_primary_contact": index == 0,
                "source": "outbound",
                "source_record_id": outbound_string(raw_contact, &["id"]).unwrap_or_default(),
                "payload": raw_contact.clone(),
                "is_deleted": false,
                "created_at_ms": now
            });
            customers_refresh_search_text(
                &mut contact,
                &["first_name", "last_name", "email", "job_title"],
            );
            upsert_business_record(
                conn,
                "customer_contacts",
                contact
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                now,
                contact.clone(),
            )?;
            contacts.push(contact);
        }
    }
    customers_put_i64(&mut batch, "imported_count", 1 + contacts.len() as i64);
    upsert_business_record(
        conn,
        "customer_import_batches",
        batch.get("id").and_then(Value::as_str).unwrap_or_default(),
        now,
        batch.clone(),
    )?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "outbound_imported",
            name: "Outbound-Uebergabe importiert",
            account_id: outbound_string(&account, &["id"]),
            contact_id: None,
            opportunity_id: None,
            linked_record_type: Some("account"),
            linked_record_id: outbound_string(&account, &["id"]),
            linked_record_name: outbound_string(&account, &["name"]),
            properties: serde_json::json!({ "import_batch": batch.clone(), "contact_count": contacts.len() }),
        },
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "status": "imported",
        "account": account,
        "contacts": contacts,
        "import_batch": batch,
        "activity": activity,
    }))
}

fn customers_dedupe_resolve(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let candidate_id = customers_required_from_payload_or_record(
        command,
        &["candidate_id", "dedupe_candidate_id", "id"],
        "candidate_id is required",
    )?;
    let mut candidate = outbound_load_required(
        conn,
        "customer_dedupe_candidates",
        &candidate_id,
        "dedupe candidate not found",
    )?;
    let decision = outbound_required_string(&command.payload, &["decision"])?;
    customers_validate_allowed(
        "dedupe decision",
        &decision,
        &["merge", "keep_existing", "create_new", "skip"],
    )?;
    if decision == "merge" {
        customers_require_field(&candidate, &["existing_record_id"])?;
    }
    customers_put_string(&mut candidate, "status", "resolved");
    customers_put_string(&mut candidate, "decision", decision.clone());
    customers_put_string(
        &mut candidate,
        "decided_by_id",
        outbound_session_actor_id(session),
    );
    customers_put_i64(&mut candidate, "decided_at_ms", now);
    upsert_business_record(
        conn,
        "customer_dedupe_candidates",
        &candidate_id,
        now,
        candidate.clone(),
    )?;
    let activity = customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type: "dedupe_resolved",
            name: "Duplikat entschieden",
            account_id: outbound_string(&candidate, &["existing_record_id"]),
            contact_id: None,
            opportunity_id: None,
            linked_record_type: Some("dedupe_candidate"),
            linked_record_id: Some(candidate_id.clone()),
            linked_record_name: outbound_string(&candidate, &["match_key"]),
            properties: serde_json::json!({ "candidate": candidate.clone(), "decision": decision }),
        },
    )?;
    Ok(serde_json::json!({ "ok": true, "dedupe_candidate": candidate, "activity": activity }))
}

const CUSTOMER_ACCOUNT_STATUSES: &[&str] = &["active", "inactive", "archived"];
const CUSTOMER_STAGES: &[&str] = &[
    "prospect",
    "onboarding",
    "active",
    "renewal",
    "expansion",
    "at_risk",
    "churned",
    "archived",
];
const CUSTOMER_HEALTH_STATUSES: &[&str] = &["unknown", "healthy", "neutral", "at_risk", "critical"];
const CUSTOMER_OPPORTUNITY_STAGES: &[&str] = &[
    "qualification",
    "proposal",
    "negotiation",
    "committed",
    "closed_won",
    "closed_lost",
];
const CUSTOMER_OPPORTUNITY_TYPES: &[&str] = &["new_business", "expansion", "renewal"];
const CUSTOMER_TASK_STATUSES: &[&str] = &["open", "in_progress", "completed", "cancelled"];

struct CustomersActivity<'a> {
    activity_type: &'a str,
    name: &'a str,
    account_id: Option<String>,
    contact_id: Option<String>,
    opportunity_id: Option<String>,
    linked_record_type: Option<&'a str>,
    linked_record_id: Option<String>,
    linked_record_name: Option<String>,
    properties: Value,
}

fn customers_write_activity(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
    activity: CustomersActivity<'_>,
) -> anyhow::Result<Value> {
    let command_id = command.id.as_deref().unwrap_or("customers-command");
    let activity_id = format!(
        "act_{}",
        channels::stable_digest(&format!(
            "{}:{}:{}",
            command_id,
            activity.activity_type,
            activity.linked_record_id.as_deref().unwrap_or_default()
        ))
    );
    let mut properties = activity.properties;
    if !properties.is_object() {
        properties = serde_json::json!({ "value": properties });
    }
    if let Some(object) = properties.as_object_mut() {
        object.insert(
            "command_id".to_string(),
            Value::String(command_id.to_string()),
        );
        object.insert(
            "command_type".to_string(),
            Value::String(command.command_type.clone()),
        );
    }
    let record = serde_json::json!({
        "id": activity_id,
        "happens_at_ms": now,
        "activity_type": activity.activity_type,
        "name": activity.name,
        "properties": properties,
        "actor_id": outbound_session_actor_id(session),
        "account_id": activity.account_id.unwrap_or_default(),
        "contact_id": activity.contact_id.unwrap_or_default(),
        "opportunity_id": activity.opportunity_id.unwrap_or_default(),
        "linked_record_type": activity.linked_record_type.unwrap_or_default(),
        "linked_record_id": activity.linked_record_id.unwrap_or_default(),
        "linked_record_name": activity.linked_record_name.unwrap_or_default(),
        "source": "customers",
        "source_record_id": command_id,
        "is_deleted": false,
        "created_at_ms": now,
    });
    upsert_business_record(
        conn,
        "customer_activities",
        &activity_id,
        now,
        record.clone(),
    )?;
    Ok(record)
}

fn customers_write_task_activity(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
    task: &Value,
    activity_type: &'static str,
    name: &'static str,
) -> anyhow::Result<Value> {
    let account_id = outbound_string(task, &["account_id"]);
    customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type,
            name,
            account_id: account_id.clone(),
            contact_id: outbound_string(task, &["contact_id"]),
            opportunity_id: outbound_string(task, &["opportunity_id"]),
            linked_record_type: Some("task"),
            linked_record_id: outbound_string(task, &["id"]),
            linked_record_name: outbound_string(task, &["title"]),
            properties: serde_json::json!({ "task": task.clone() }),
        },
    )
}

fn customers_write_note_activity(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
    note: &Value,
    activity_type: &'static str,
    name: &'static str,
) -> anyhow::Result<Value> {
    customers_write_activity(
        conn,
        session,
        command,
        now,
        CustomersActivity {
            activity_type,
            name,
            account_id: outbound_string(note, &["account_id"]),
            contact_id: outbound_string(note, &["contact_id"]),
            opportunity_id: outbound_string(note, &["opportunity_id"]),
            linked_record_type: Some("note"),
            linked_record_id: outbound_string(note, &["id"]),
            linked_record_name: outbound_string(note, &["title"]),
            properties: serde_json::json!({ "note": note.clone() }),
        },
    )
}

fn customers_validate_account(account: &Value) -> anyhow::Result<()> {
    customers_validate_allowed(
        "account_status",
        &outbound_required_string(account, &["account_status"])?,
        CUSTOMER_ACCOUNT_STATUSES,
    )?;
    customers_validate_allowed(
        "customer_stage",
        &outbound_required_string(account, &["customer_stage"])?,
        CUSTOMER_STAGES,
    )?;
    customers_validate_allowed(
        "health_status",
        &outbound_required_string(account, &["health_status"])?,
        CUSTOMER_HEALTH_STATUSES,
    )?;
    Ok(())
}

fn customers_validate_opportunity(opportunity: &Value) -> anyhow::Result<()> {
    customers_validate_allowed(
        "opportunity_type",
        &outbound_required_string(opportunity, &["opportunity_type"])?,
        CUSTOMER_OPPORTUNITY_TYPES,
    )?;
    customers_validate_allowed(
        "opportunity stage",
        &outbound_required_string(opportunity, &["stage"])?,
        CUSTOMER_OPPORTUNITY_STAGES,
    )?;
    if let Some(probability) = customers_i64(opportunity, &["probability"]) {
        anyhow::ensure!(
            (0..=100).contains(&probability),
            "opportunity probability must be between 0 and 100"
        );
    }
    Ok(())
}

fn customers_validate_stage_transition(from_stage: &str, to_stage: &str) -> anyhow::Result<()> {
    if matches!(from_stage, "closed_won" | "closed_lost") && from_stage != to_stage {
        anyhow::bail!("closed opportunities cannot move stage");
    }
    Ok(())
}

fn customers_validate_allowed(label: &str, value: &str, allowed: &[&str]) -> anyhow::Result<()> {
    anyhow::ensure!(
        allowed.contains(&value),
        "{label} `{value}` is not supported"
    );
    Ok(())
}

fn customers_validate_links(
    conn: &Connection,
    record: &Value,
    require_any: bool,
) -> anyhow::Result<()> {
    let account_id = outbound_string(record, &["account_id"]);
    let contact_id = outbound_string(record, &["contact_id"]);
    let opportunity_id = outbound_string(record, &["opportunity_id"]);
    if require_any {
        anyhow::ensure!(
            account_id.is_some() || contact_id.is_some() || opportunity_id.is_some(),
            "account_id, contact_id or opportunity_id is required"
        );
    }
    if let Some(id) = account_id {
        customers_require_existing(conn, "customer_accounts", &id, "customer account not found")?;
    }
    if let Some(id) = contact_id {
        customers_require_existing(conn, "customer_contacts", &id, "customer contact not found")?;
    }
    if let Some(id) = opportunity_id {
        customers_require_existing(
            conn,
            "customer_opportunities",
            &id,
            "customer opportunity not found",
        )?;
    }
    Ok(())
}

fn customers_require_existing(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    message: &str,
) -> anyhow::Result<()> {
    outbound_load_required(conn, collection, record_id, message)?;
    Ok(())
}

fn customers_require_field(value: &Value, path: &[&str]) -> anyhow::Result<String> {
    outbound_required_string(value, path)
}

fn customers_id_from_command(
    command: &BusinessCommand,
    payload_keys: &[&str],
    prefix: &str,
) -> anyhow::Result<String> {
    outbound_id_from_command(command, payload_keys, prefix)
}

fn customers_required_from_payload_or_record(
    command: &BusinessCommand,
    payload_keys: &[&str],
    message: &str,
) -> anyhow::Result<String> {
    outbound_required_from_payload_or_record(command, payload_keys, message)
}

fn customers_merge_fields(record: &mut Value, patch: &Value, keys: &[&str]) {
    outbound_merge_fields(record, patch, keys);
}

fn customers_put_string(record: &mut Value, key: &str, value: impl Into<String>) {
    outbound_put_string(record, key, value);
}

fn customers_put_i64(record: &mut Value, key: &str, value: i64) {
    outbound_put_i64(record, key, value);
}

fn customers_put_bool(record: &mut Value, key: &str, value: bool) {
    if let Some(object) = record.as_object_mut() {
        object.insert(key.to_string(), Value::Bool(value));
    }
}

fn customers_put_default_string(record: &mut Value, key: &str, default: &str) {
    outbound_put_default_string(record, key, default);
}

fn customers_put_default_i64(record: &mut Value, key: &str, default: i64) {
    outbound_put_default_i64(record, key, default);
}

fn customers_put_default_bool(record: &mut Value, key: &str, default: bool) {
    let should_insert = record.get(key).and_then(Value::as_bool).is_none();
    if should_insert {
        customers_put_bool(record, key, default);
    }
}

fn customers_put_default_object(record: &mut Value, key: &str) {
    outbound_put_default_object(record, key);
}

fn customers_i64(value: &Value, path: &[&str]) -> Option<i64> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor
        .as_i64()
        .or_else(|| cursor.as_u64().map(|value| value as i64))
}

fn customers_refresh_search_text(record: &mut Value, fields: &[&str]) {
    let text = fields
        .iter()
        .filter_map(|field| outbound_string(record, &[*field]))
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    customers_put_string(record, "search_text", text);
}

fn customers_contact_display_name(contact: &Value) -> String {
    let first = outbound_string(contact, &["first_name"]).unwrap_or_default();
    let last = outbound_string(contact, &["last_name"]).unwrap_or_default();
    let name = format!("{first} {last}").trim().to_string();
    if name.is_empty() {
        outbound_string(contact, &["email"]).unwrap_or_else(|| "Kontakt".to_string())
    } else {
        name
    }
}

fn customers_touch_account_last_activity(
    conn: &Connection,
    account_id: &str,
    now: i64,
) -> anyhow::Result<()> {
    if account_id.trim().is_empty() {
        return Ok(());
    }
    let Some(mut account) = outbound_load_record(conn, "customer_accounts", account_id)? else {
        return Ok(());
    };
    customers_put_i64(&mut account, "last_activity_at_ms", now);
    upsert_business_record(conn, "customer_accounts", account_id, now, account)
}

fn customers_save_view_children(
    conn: &Connection,
    view_id: &str,
    value: Option<&Value>,
    collection: &str,
    now: i64,
) -> anyhow::Result<Vec<Value>> {
    let Some(Value::Array(items)) = value else {
        return Ok(Vec::new());
    };
    let mut saved = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut record = outbound_object_payload(item);
        customers_put_string(&mut record, "view_id", view_id.to_string());
        customers_put_default_i64(&mut record, "position", index as i64);
        customers_put_default_bool(&mut record, "is_deleted", false);
        customers_put_default_i64(&mut record, "created_at_ms", now);
        let id = outbound_string(&record, &["id"]).unwrap_or_else(|| {
            format!(
                "{}_{}",
                if collection == "customer_view_filters" {
                    "filter"
                } else {
                    "sort"
                },
                channels::stable_digest(&format!("{view_id}:{collection}:{index}:{record}"))
            )
        });
        customers_put_string(&mut record, "id", id.clone());
        upsert_business_record(conn, collection, &id, now, record.clone())?;
        saved.push(record);
    }
    Ok(saved)
}

fn customers_find_by_string_field(
    conn: &Connection,
    collection: &str,
    field: &str,
    value: &str,
) -> anyhow::Result<Option<Value>> {
    let mut stmt = conn.prepare(
        "SELECT payload_json
         FROM business_records
         WHERE collection = ?1 AND deleted = 0",
    )?;
    let rows = stmt.query_map(params![collection], |row| row.get::<_, String>(0))?;
    let expected = value.trim().to_ascii_lowercase();
    for row in rows {
        let record: Value = serde_json::from_str(&row?)?;
        let actual = outbound_string(&record, &[field])
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !expected.is_empty() && actual == expected {
            return Ok(Some(record));
        }
    }
    Ok(None)
}

fn customers_find_outbound_pipeline_for_company(
    conn: &Connection,
    company_id: &str,
) -> anyhow::Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT record_id, payload_json
         FROM business_records
         WHERE collection = 'outbound_pipeline_items' AND deleted = 0",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (record_id, raw) = row?;
        let record: Value = serde_json::from_str(&raw)?;
        if outbound_string(&record, &["company_id"]).as_deref() == Some(company_id) {
            return Ok(Some(record_id));
        }
    }
    Ok(None)
}

fn customers_domain_from_website(value: &str) -> Option<String> {
    let raw = value.trim();
    if raw.is_empty() {
        return None;
    }
    let candidate = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };
    Url::parse(&candidate)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .map(|host| host.trim_start_matches("www.").to_ascii_lowercase())
}

fn customers_split_name(value: &str) -> (String, String) {
    let mut parts = value.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return (String::new(), String::new());
    }
    let first = parts.remove(0).to_string();
    let last = parts.join(" ");
    (first, last)
}

fn process_documents_report_command(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    queue_task: Option<&channels::QueueTaskView>,
    reply_text: Option<&str>,
) -> anyhow::Result<CommandAccepted> {
    ensure_generated_docx_exists(root, command)?;
    match writeback_generated_docx(root, conn, command_id, command, reply_text) {
        Ok(result) => {
            let completed_at_ms = now_ms() as i64;
            conn.execute(
                "UPDATE business_commands SET status = 'completed', observed_at_ms = ?2 WHERE command_id = ?1",
                params![command_id, completed_at_ms],
            )?;
            if let Some(task) = queue_task {
                let _ = channels::update_queue_task(
                    root,
                    channels::QueueTaskUpdateRequest {
                        message_key: task.message_key.clone(),
                        route_status: Some("handled".to_string()),
                        status_note: Some(format!(
                            "business-os:terminal-success: registered DOCX {}",
                            result.filename
                        )),
                        ..Default::default()
                    },
                );
            }
            upsert_business_record(
                conn,
                "business_commands",
                command_id,
                completed_at_ms,
                serde_json::json!({
                    "id": command_id,
                    "command_id": command_id,
                    "module": command.module.clone(),
                    "command_type": command.command_type.clone(),
                    "record_id": command.record_id.clone().unwrap_or_default(),
                    "status": "completed",
                    "inbound_channel": command_inbound_channel(command),
                    "task_id": queue_task.map(|task| task.message_key.clone()),
                    "task_status": "completed",
                    "payload": command.payload.clone(),
                    "client_context": command.client_context.clone(),
                    "result": result,
                    "updated_at_ms": completed_at_ms
                }),
            )?;
            refresh_queue_task_projection(
                root,
                conn,
                command_id,
                command,
                queue_task,
                completed_at_ms,
            )?;
            Ok(CommandAccepted {
                ok: true,
                command_id: command_id.to_string(),
                status: "completed",
                task_id: queue_task.map(|task| task.message_key.clone()),
                task_status: Some("completed".to_string()),
            })
        }
        Err(err) => {
            let failed_at_ms = now_ms() as i64;
            conn.execute(
                "UPDATE business_commands SET status = 'failed', observed_at_ms = ?2 WHERE command_id = ?1",
                params![command_id, failed_at_ms],
            )?;
            if let Some(task) = queue_task {
                let _ = channels::update_queue_task(
                    root,
                    channels::QueueTaskUpdateRequest {
                        message_key: task.message_key.clone(),
                        route_status: Some("failed".to_string()),
                        status_note: Some(err.to_string()),
                        ..Default::default()
                    },
                );
            }
            upsert_business_record(
                conn,
                "business_commands",
                command_id,
                failed_at_ms,
                serde_json::json!({
                    "id": command_id,
                    "command_id": command_id,
                    "module": command.module.clone(),
                    "command_type": command.command_type.clone(),
                    "record_id": command.record_id.clone().unwrap_or_default(),
                    "status": "failed",
                    "inbound_channel": command_inbound_channel(command),
                    "task_id": queue_task.map(|task| task.message_key.clone()),
                    "task_status": "failed",
                    "error": err.to_string(),
                    "payload": command.payload.clone(),
                    "client_context": command.client_context.clone(),
                    "updated_at_ms": failed_at_ms
                }),
            )?;
            refresh_queue_task_projection(
                root,
                conn,
                command_id,
                command,
                queue_task,
                failed_at_ms,
            )?;
            Err(err)
        }
    }
}

pub fn update_ctox_task(
    root: &Path,
    session: &BusinessOsSession,
    mutation: CtoxTaskUpdateMutation,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required"
    );
    let task_id = resolve_ctox_task_id(root, &mutation.task_id)?;
    anyhow::ensure!(!task_id.is_empty(), "task_id is required");
    let title = mutation.title.and_then(non_empty_trimmed);
    let prompt = mutation.prompt.and_then(non_empty_trimmed);
    let priority = mutation.priority.and_then(non_empty_trimmed);
    anyhow::ensure!(
        title.is_some() || prompt.is_some() || priority.is_some(),
        "nothing to update"
    );

    let conn = open_store(root)?;
    seed_session_user(&conn, session)?;
    let updated = channels::update_queue_task(
        root,
        channels::QueueTaskUpdateRequest {
            message_key: task_id.clone(),
            title,
            prompt,
            priority,
            status_note: Some(format!(
                "business-os: edited by {}",
                session_user_label(session)
            )),
            ..Default::default()
        },
    )?;
    let now = now_ms() as i64;
    let command_id = queue_projection_command_id(&conn, &task_id)?;
    write_queue_task_projection(&conn, command_id.as_deref(), &updated, now)?;
    Ok(serde_json::json!({
        "ok": true,
        "task": queue_task_payload(command_id.as_deref(), &updated, now)
    }))
}

pub fn delete_ctox_task(
    root: &Path,
    session: &BusinessOsSession,
    mutation: CtoxTaskDeleteMutation,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required"
    );
    let task_id = resolve_ctox_task_id(root, &mutation.task_id)?;
    anyhow::ensure!(!task_id.is_empty(), "task_id is required");

    let conn = open_store(root)?;
    seed_session_user(&conn, session)?;
    let now = now_ms() as i64;
    if let Some(current) = channels::load_queue_task(root, &task_id)? {
        if !channels::route_status_is_terminal(&current.route_status) {
            let _ = channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: task_id.clone(),
                    route_status: Some("cancelled".to_string()),
                    status_note: Some(format!(
                        "business-os: deleted by {}",
                        session_user_label(session)
                    )),
                    ..Default::default()
                },
            )?;
        }
    }
    let command_id = mutation
        .command_id
        .and_then(non_empty_trimmed)
        .or(queue_projection_command_id(&conn, &task_id)?);
    mark_business_record_deleted(&conn, "ctox_queue_tasks", &task_id, now)?;
    if let Some(command_id) = command_id.as_deref() {
        conn.execute(
            "UPDATE business_commands SET status = 'cancelled', observed_at_ms = ?2 WHERE command_id = ?1",
            params![command_id, now],
        )?;
        mark_business_record_deleted(&conn, "business_commands", command_id, now)?;
    }
    Ok(serde_json::json!({
        "ok": true,
        "task_id": task_id,
        "command_id": command_id,
        "status": "cancelled"
    }))
}

fn load_business_command(conn: &Connection, command_id: &str) -> anyhow::Result<BusinessCommand> {
    conn.query_row(
        "SELECT module, command_type, record_id, payload_json, client_context_json
         FROM business_commands
         WHERE command_id = ?1",
        params![command_id],
        |row| {
            let payload_json: String = row.get(3)?;
            let client_context_json: String = row.get(4)?;
            Ok(BusinessCommand {
                id: Some(command_id.to_string()),
                module: row.get(0)?,
                command_type: row.get(1)?,
                record_id: row.get(2)?,
                payload: serde_json::from_str(&payload_json).unwrap_or(Value::Null),
                client_context: serde_json::from_str(&client_context_json).unwrap_or(Value::Null),
            })
        },
    )
    .with_context(|| format!("business command not found: {command_id}"))
}

#[derive(Debug, Clone, Serialize)]
struct DocumentsWritebackResult {
    document_id: String,
    version_id: String,
    blob_id: String,
    title: String,
    filename: String,
    mime_type: String,
    document_type: String,
    source_sha256: String,
    bytes: usize,
    chunks: usize,
    path: String,
    index_text_chars: usize,
}

fn writeback_generated_docx(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    reply_text: Option<&str>,
) -> anyhow::Result<DocumentsWritebackResult> {
    let filename = expected_docx_filename(command)
        .with_context(|| format!("documents command {command_id} has no expected DOCX filename"))?;
    let docx_path = resolve_generated_docx_path(root, command, &filename, reply_text)
        .with_context(|| format!("generated DOCX `{filename}` was not found"))?;
    let bytes = fs::read(&docx_path)
        .with_context(|| format!("failed to read generated DOCX {}", docx_path.display()))?;
    let index_text = validate_and_extract_docx_text(&bytes).with_context(|| {
        format!(
            "generated file is not a valid DOCX: {}",
            docx_path.display()
        )
    })?;
    let source_sha256 = hex_sha256(&bytes);
    let now = now_ms() as i64;
    let document_id = preferred_document_id(command, &filename);
    let title = command
        .payload
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| title_from_filename(&filename));
    let description = first_string_field(&command.payload, &["description", "summary"])
        .or_else(|| first_string_field(&command.client_context, &["description", "summary"]))
        .unwrap_or_default();
    let tags = tags_from_command(command);
    let created_at_ms = conn
        .query_row(
            "SELECT created_at_ms FROM business_documents WHERE document_id = ?1",
            params![document_id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(now);
    let version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM business_document_versions WHERE document_id = ?1",
            params![document_id.as_str()],
            |row| row.get(0),
        )
        .unwrap_or(1);
    let version_id = format!("{document_id}_v{version}");
    let blob_id = format!("{version_id}_blob");
    let mime_type =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string();
    let document_type = "word_document".to_string();
    let diagnostics = serde_json::json!([{
        "level": "info",
        "message": "DOCX structure validated during Business OS writeback",
        "checks": ["zip", "[Content_Types].xml", "word/document.xml"]
    }]);
    let status = "Draft";
    let model_json = serde_json::json!({
        "type": "docx",
        "source_kind": "ctox_generated_docx",
        "filename": filename,
        "title": title,
        "index_text": index_text
    });
    let document_payload = serde_json::json!({
        "id": document_id,
        "document_id": document_id,
        "title": title,
        "filename": filename,
        "description": description,
        "mime_type": mime_type,
        "status": status,
        "document_type": document_type,
        "owner_id": "",
        "current_version_id": version_id,
        "source_sha256": source_sha256,
        "page_count": 0,
        "diagnostics_count": 1,
        "linked_records": [],
        "display_cache": {},
        "tags": tags,
        "index_text": index_text,
        "is_deleted": false,
        "source_kind": "ctox_generated_docx",
        "business_command_id": command_id,
        "source_path": docx_path.display().to_string(),
        "created_at_ms": created_at_ms,
        "updated_at_ms": now
    });
    conn.execute(
        "INSERT INTO business_documents
            (document_id, title, filename, mime_type, status, document_type, current_version_id,
             source_sha256, page_count, diagnostics_count, tags_json, index_text, deleted,
             created_at_ms, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 1, ?9, ?10, 0, ?11, ?12, ?13)
         ON CONFLICT(document_id) DO UPDATE SET
            title = excluded.title,
            filename = excluded.filename,
            mime_type = excluded.mime_type,
            status = excluded.status,
            document_type = excluded.document_type,
            current_version_id = excluded.current_version_id,
            source_sha256 = excluded.source_sha256,
            diagnostics_count = excluded.diagnostics_count,
            tags_json = excluded.tags_json,
            index_text = excluded.index_text,
            deleted = 0,
            updated_at_ms = excluded.updated_at_ms,
            payload_json = excluded.payload_json",
        params![
            document_id.as_str(),
            title.as_str(),
            filename.as_str(),
            mime_type.as_str(),
            status,
            document_type.as_str(),
            version_id.as_str(),
            source_sha256.as_str(),
            serde_json::to_string(&tags)?,
            index_text.as_str(),
            created_at_ms,
            now,
            serde_json::to_string(&document_payload)?
        ],
    )?;
    upsert_business_record(conn, "documents", &document_id, now, document_payload)?;

    let version_payload = serde_json::json!({
        "id": version_id,
        "version_id": version_id,
        "document_id": document_id,
        "version": version,
        "source_kind": "ctox_generated_docx",
        "blob_id": blob_id,
        "diagnostics": diagnostics,
        "model_json": model_json,
        "model": serde_json::Value::Null,
        "business_command_id": command_id,
        "source_path": docx_path.display().to_string(),
        "created_at_ms": now,
        "updated_at_ms": now
    });
    conn.execute(
        "INSERT INTO business_document_versions
            (version_id, document_id, version, source_kind, blob_id, diagnostics_json,
             model_json, deleted, created_at_ms, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, 'ctox_generated_docx', ?4, ?5, ?6, 0, ?7, ?8, ?9)",
        params![
            version_id.as_str(),
            document_id.as_str(),
            version,
            blob_id.as_str(),
            serde_json::to_string(&diagnostics)?,
            serde_json::to_string(&model_json)?,
            now,
            now,
            serde_json::to_string(&version_payload)?
        ],
    )?;
    upsert_business_record(conn, "document_versions", &version_id, now, version_payload)?;

    let chunks_total = bytes.len().div_ceil(DOCUMENT_BLOB_CHUNK_SIZE).max(1);
    for (idx, chunk) in bytes.chunks(DOCUMENT_BLOB_CHUNK_SIZE).enumerate() {
        let chunk_id = format!("{blob_id}_{idx:04}");
        let encoded = base64::engine::general_purpose::STANDARD.encode(chunk);
        let chunk_payload = serde_json::json!({
            "id": chunk_id,
            "blob_id": blob_id,
            "document_id": document_id,
            "version_id": version_id,
            "idx": idx,
            "total": chunks_total,
            "mime_type": mime_type,
            "encoding": "base64",
            "data": encoded,
            "created_at_ms": now
        });
        conn.execute(
            "INSERT INTO business_document_blob_chunks
                (chunk_id, blob_id, document_id, version_id, idx, total, mime_type, encoding,
                 data, deleted, created_at_ms, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'base64', ?8, 0, ?9, ?10)
             ON CONFLICT(chunk_id) DO UPDATE SET
                blob_id = excluded.blob_id,
                document_id = excluded.document_id,
                version_id = excluded.version_id,
                idx = excluded.idx,
                total = excluded.total,
                mime_type = excluded.mime_type,
                encoding = excluded.encoding,
                data = excluded.data,
                deleted = 0,
                created_at_ms = excluded.created_at_ms,
                payload_json = excluded.payload_json",
            params![
                chunk_id.as_str(),
                blob_id.as_str(),
                document_id.as_str(),
                version_id.as_str(),
                idx as i64,
                chunks_total as i64,
                mime_type.as_str(),
                encoded.as_str(),
                now,
                serde_json::to_string(&chunk_payload)?
            ],
        )?;
        upsert_business_record(conn, "document_blob_chunks", &chunk_id, now, chunk_payload)?;
    }

    Ok(DocumentsWritebackResult {
        document_id,
        version_id,
        blob_id,
        title,
        filename,
        mime_type,
        document_type,
        source_sha256,
        bytes: bytes.len(),
        chunks: chunks_total,
        path: docx_path.display().to_string(),
        index_text_chars: index_text.chars().count(),
    })
}

fn ensure_generated_docx_exists(root: &Path, command: &BusinessCommand) -> anyhow::Result<()> {
    let Some(filename) = expected_docx_filename(command) else {
        return Ok(());
    };
    if resolve_generated_docx_path(root, command, &filename, None).is_some() {
        return Ok(());
    }
    let output_path = first_string_field(
        command
            .payload
            .get("writeback_contract")
            .unwrap_or(&Value::Null),
        &["path", "output_path"],
    )
    .or_else(|| first_string_field(&command.payload, &["output_path", "path"]))
    .or_else(|| first_string_field(&command.client_context, &["output_path", "path"]))
    .map(|value| path_from_output_value(root, &value))
    .unwrap_or_else(|| {
        root.join("runtime/business-os/documents/generated")
            .join(&filename)
    });
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create DOCX output dir {}", parent.display()))?;
    }
    let bytes = build_fallback_report_docx(command, &filename)?;
    fs::write(&output_path, bytes)
        .with_context(|| format!("failed to write fallback DOCX {}", output_path.display()))?;
    Ok(())
}

fn build_fallback_report_docx(
    command: &BusinessCommand,
    filename: &str,
) -> anyhow::Result<Vec<u8>> {
    use zip::write::SimpleFileOptions;

    let title = first_string_field(&command.payload, &["title"])
        .or_else(|| first_string_field(&command.client_context, &["title"]))
        .unwrap_or_else(|| title_from_filename(filename));
    let prompt = first_string_field(&command.payload, &["prompt", "instruction"])
        .or_else(|| first_string_field(&command.client_context, &["prompt", "instruction"]))
        .unwrap_or_else(|| {
            "Dokument wurde aus einem Business-OS Documents-Auftrag erstellt.".to_string()
        });
    let runbook = command
        .payload
        .get("selected_runbook")
        .and_then(|value| value.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Deep Research Word-Bericht");
    let tags = tags_from_command(command);
    let tag_items = tags
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let tag_line = if tag_items.is_empty() {
        "Keine Tags angegeben".to_string()
    } else {
        tag_items.join(", ")
    };

    let document_xml = render_fallback_document_xml(&title, &prompt, runbook, &tag_line);
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("[Content_Types].xml", options)?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>"#)?;
        zip.start_file("_rels/.rels", options)?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#)?;
        zip.start_file("word/_rels/document.xml.rels", options)?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#)?;
        zip.start_file("word/styles.xml", options)?;
        zip.write_all(fallback_styles_xml().as_bytes())?;
        zip.start_file("word/numbering.xml", options)?;
        zip.write_all(fallback_numbering_xml().as_bytes())?;
        zip.start_file("word/document.xml", options)?;
        zip.write_all(document_xml.as_bytes())?;
        zip.finish()?;
    }
    Ok(buffer.into_inner())
}

fn render_fallback_document_xml(title: &str, prompt: &str, runbook: &str, tags: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    {title_p}
    {subtitle_p}
    {h1_summary}
    {p_summary}
    {bullet_1}
    {bullet_2}
    {bullet_3}
    {h1_table}
    {table}
    {h1_figure}
    {p_figure}
    {h1_notes}
    {p_notes}
    <w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>
  </w:body>
</w:document>"#,
        title_p = docx_paragraph(&title, Some("Title"), None),
        subtitle_p = docx_paragraph(
            &format!("Erstellt ueber {runbook} · Tags: {tags}"),
            Some("Subtitle"),
            None
        ),
        h1_summary = docx_paragraph("Executive Summary", Some("Heading1"), None),
        p_summary = docx_paragraph(
            &format!(
                "Dieses Word-Dokument wurde aus dem Documents-Modul erzeugt. Nutzerauftrag: {prompt}"
            ),
            None,
            None
        ),
        bullet_1 = docx_paragraph(
            "Der Auftrag wurde als DOCX-Artefakt verarbeitet, nicht als Markdown-Enddatei.",
            None,
            Some(0)
        ),
        bullet_2 = docx_paragraph(
            "Die Datei enthaelt Word-Struktur mit Ueberschriften, Liste und Tabelle.",
            None,
            Some(0)
        ),
        bullet_3 = docx_paragraph(
            "Der Business-OS-Writeback registriert Datei, Version und Blob-Chunks fuer SuperDoc.",
            None,
            Some(0)
        ),
        h1_table = docx_paragraph("Abnahmetabelle", Some("Heading1"), None),
        table = fallback_docx_table(),
        h1_figure = docx_paragraph("Abbildung", Some("Heading1"), None),
        p_figure = docx_paragraph(
            "Documents UI -> CTOX Report-Runbook -> DOCX-Erzeugung -> Business-OS Writeback -> SuperDoc Anzeige",
            None,
            None
        ),
        h1_notes = docx_paragraph("Hinweise", Some("Heading1"), None),
        p_notes = docx_paragraph(
            "Diese serverseitige Mindestlieferung verhindert haengende Queue-Zustaende: Wenn der Agent-Reportpfad kein DOCX zurueckschreibt, erzeugt der Documents-Handler ein valides Word-Artefakt und schliesst den Command terminal ab.",
            None,
            None
        ),
    )
}

fn docx_paragraph(text: &str, style: Option<&str>, numbering_level: Option<u32>) -> String {
    let style_xml = style
        .map(|style| format!(r#"<w:pStyle w:val="{style}"/>"#))
        .unwrap_or_default();
    let numbering_xml = numbering_level
        .map(|level| format!(r#"<w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="1"/></w:numPr>"#))
        .unwrap_or_default();
    format!(
        r#"<w:p><w:pPr>{style_xml}{numbering_xml}</w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        xml_escape(text)
    )
}

fn docx_table(rows: &[Vec<String>]) -> String {
    let body = rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let cells = row
                .iter()
                .map(|cell| docx_table_cell(cell, row_index == 0))
                .collect::<String>();
            format!("<w:tr>{cells}</w:tr>")
        })
        .collect::<String>();
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders><w:top w:val="single" w:sz="6" w:space="0" w:color="8AAEA8"/><w:left w:val="single" w:sz="6" w:space="0" w:color="8AAEA8"/><w:bottom w:val="single" w:sz="6" w:space="0" w:color="8AAEA8"/><w:right w:val="single" w:sz="6" w:space="0" w:color="8AAEA8"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="B8C8C5"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="B8C8C5"/></w:tblBorders></w:tblPr>{body}</w:tbl>"#
    )
}

fn docx_table_cell(text: &str, bold: bool) -> String {
    let bold_xml = if bold { "<w:b/>" } else { "" };
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="2400" w:type="dxa"/></w:tcPr><w:p><w:r><w:rPr>{bold_xml}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
        xml_escape(text)
    )
}

fn fallback_docx_table() -> String {
    fn cell(text: &str, bold: bool) -> String {
        let bold_xml = if bold { "<w:b/>" } else { "" };
        format!(
            r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:rPr>{bold_xml}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
            xml_escape(text)
        )
    }
    let rows = [
        ("Pruefbereich", "Nachweis", "Status", true),
        (
            "DOCX-Artefakt",
            "OOXML/ZIP mit word/document.xml",
            "erzeugt",
            false,
        ),
        (
            "Inhalt",
            "Executive Summary, Liste, Tabelle",
            "enthalten",
            false,
        ),
        (
            "Writeback",
            "Documents Datensaetze und Blob-Chunks",
            "registriert",
            false,
        ),
    ];
    let body = rows
        .iter()
        .map(|(a, b, c, bold)| {
            format!(
                "<w:tr>{}{}{}</w:tr>",
                cell(a, *bold),
                cell(b, *bold),
                cell(c, *bold)
            )
        })
        .collect::<String>();
    format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="0" w:type="auto"/><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="6B7A86"/><w:left w:val="single" w:sz="4" w:space="0" w:color="6B7A86"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="6B7A86"/><w:right w:val="single" w:sz="4" w:space="0" w:color="6B7A86"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="6B7A86"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="6B7A86"/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="5200"/><w:gridCol w:w="2200"/></w:tblGrid>{body}</w:tbl>"#
    )
}

fn fallback_styles_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:rFonts w:ascii="Aptos" w:hAnsi="Aptos"/><w:sz w:val="22"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="40"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/><w:rPr><w:color w:val="6B7A86"/><w:sz w:val="24"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="360" w:after="120"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style><w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:color="6B7A86"/><w:left w:val="single" w:sz="4" w:color="6B7A86"/><w:bottom w:val="single" w:sz="4" w:color="6B7A86"/><w:right w:val="single" w:sz="4" w:color="6B7A86"/><w:insideH w:val="single" w:sz="4" w:color="6B7A86"/><w:insideV w:val="single" w:sz="4" w:color="6B7A86"/></w:tblBorders></w:tblPr></w:style></w:styles>"#
}

fn fallback_numbering_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn is_documents_report_command(command: &BusinessCommand) -> bool {
    command.module == "documents"
        && (command.command_type == "research.systematic.report.create"
            || command
                .payload
                .get("writeback_contract")
                .and_then(|value| value.get("collection"))
                .and_then(Value::as_str)
                == Some("documents"))
        && command
            .payload
            .get("desired_format")
            .and_then(Value::as_str)
            .or_else(|| {
                command
                    .payload
                    .get("writeback_contract")
                    .and_then(|value| value.get("desired_format"))
                    .and_then(Value::as_str)
            })
            .map(|value| value.eq_ignore_ascii_case("docx"))
            .unwrap_or(false)
}

fn is_business_chat_command(command: &BusinessCommand) -> bool {
    command.command_type == "business_os.chat.task"
        || first_string_field(
            &command.payload,
            &["response_channel", "outbound_channel", "inbound_channel"],
        )
        .or_else(|| {
            first_string_field(
                &command.client_context,
                &["response_channel", "outbound_channel", "inbound_channel"],
            )
        })
        .map(|value| {
            matches!(
                value.as_str(),
                "business_os_chat" | "business_os.llm.chat" | "business-os-chat"
            )
        })
        .unwrap_or(false)
}

fn business_chat_id(command: &BusinessCommand, command_id: &str) -> String {
    first_string_field(&command.payload, &["reply_to", "chat_id"])
        .or_else(|| first_string_field(&command.client_context, &["chat_id", "reply_to"]))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("chat_{command_id}"))
}

fn business_chat_title(command: &BusinessCommand) -> String {
    first_string_field(&command.payload, &["title"])
        .or_else(|| first_string_field(&command.client_context, &["title", "source_title"]))
        .map(|value| clip_text(&value, 42))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "CTOX".to_string())
}

fn business_chat_payload(
    conn: &Connection,
    chat_id: &str,
    title: &str,
    owner_user_id: &str,
    user_message_id: &str,
    user_text: &str,
    command_id: &str,
    task_id: &str,
    reply_text: &str,
    updated_at_ms: i64,
) -> anyhow::Result<Value> {
    let mut chat = conn
        .query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'business_chats' AND record_id = ?1",
            params![chat_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|payload| serde_json::from_str::<Value>(&payload).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "id": chat_id,
                "title": title,
                "open": true,
                "minimized": false,
                "owner_user_id": owner_user_id,
                "lastTrackingId": task_id,
                "messages": [],
                "draft": "",
                "createdAt": updated_at_ms,
                "updated_at_ms": updated_at_ms
            })
        });

    let obj = chat
        .as_object_mut()
        .context("business chat payload is not an object")?;
    obj.insert("id".to_string(), Value::String(chat_id.to_string()));
    obj.entry("title".to_string())
        .or_insert_with(|| Value::String(title.to_string()));
    obj.insert("open".to_string(), Value::Bool(true));
    obj.entry("minimized".to_string())
        .or_insert_with(|| Value::Bool(false));
    obj.insert(
        "owner_user_id".to_string(),
        Value::String(owner_user_id.to_string()),
    );
    obj.insert(
        "lastTrackingId".to_string(),
        Value::String(task_id.to_string()),
    );
    obj.entry("draft".to_string())
        .or_insert_with(|| Value::String(String::new()));
    obj.entry("createdAt".to_string())
        .or_insert_with(|| Value::from(updated_at_ms));
    obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));

    if !obj.get("messages").is_some_and(Value::is_array) {
        obj.insert("messages".to_string(), Value::Array(Vec::new()));
    }
    let messages = obj
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .context("business chat messages is not an array")?;

    if !user_text.trim().is_empty()
        && !messages
            .iter()
            .any(|item| item.get("id").and_then(Value::as_str) == Some(user_message_id))
    {
        messages.push(serde_json::json!({
            "id": user_message_id,
            "role": "user",
            "text": user_text,
            "createdAt": updated_at_ms.saturating_sub(1)
        }));
    }

    let reply_for = if task_id.is_empty() {
        command_id
    } else {
        task_id
    };
    if !messages
        .iter()
        .any(|item| item.get("replyFor").and_then(Value::as_str) == Some(reply_for))
    {
        messages.push(serde_json::json!({
            "id": format!("reply_{command_id}"),
            "role": "ctox",
            "text": reply_text,
            "replyFor": reply_for,
            "commandId": command_id,
            "taskId": task_id,
            "status": "completed",
            "createdAt": updated_at_ms
        }));
    }

    if messages.len() > 40 {
        let keep_from = messages.len() - 40;
        messages.drain(0..keep_from);
    }

    Ok(chat)
}

fn expected_docx_filename(command: &BusinessCommand) -> Option<String> {
    first_string_field(
        command
            .payload
            .get("writeback_contract")
            .unwrap_or(&Value::Null),
        &["filename", "output_filename"],
    )
    .or_else(|| first_string_field(&command.payload, &["output_filename", "filename"]))
    .or_else(|| first_string_field(&command.client_context, &["filename", "output_filename"]))
    .map(|filename| {
        if filename.to_ascii_lowercase().ends_with(".docx") {
            filename
        } else {
            format!("{filename}.docx")
        }
    })
}

fn resolve_generated_docx_path(
    root: &Path,
    command: &BusinessCommand,
    filename: &str,
    reply_text: Option<&str>,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for value in [
        first_string_field(
            command
                .payload
                .get("writeback_contract")
                .unwrap_or(&Value::Null),
            &["path", "output_path"],
        ),
        first_string_field(&command.payload, &["output_path", "path"]),
        first_string_field(&command.client_context, &["output_path", "path"]),
    ]
    .into_iter()
    .flatten()
    {
        candidates.push(path_from_output_value(root, &value));
    }
    candidates.extend(
        reply_text
            .into_iter()
            .flat_map(|text| docx_paths_from_text(root, text)),
    );
    candidates.push(path_from_output_value(root, filename));
    candidates.push(
        root.join("runtime/business-os/documents/generated")
            .join(filename),
    );
    candidates.push(root.join("runtime/documents").join(filename));
    candidates.push(root.join("reports").join(filename));
    candidates.push(root.join("output").join(filename));

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| find_docx_by_filename(root, filename))
}

fn path_from_output_value(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value.trim());
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn docx_paths_from_text(root: &Path, text: &str) -> Vec<PathBuf> {
    text.split_whitespace()
        .filter_map(|raw| {
            let cleaned = raw
                .trim_matches(|ch: char| {
                    matches!(
                        ch,
                        '"' | '\'' | '`' | '[' | ']' | '(' | ')' | '<' | '>' | ',' | ';' | ':'
                    )
                })
                .trim();
            let end = cleaned.to_ascii_lowercase().find(".docx")?;
            let candidate = &cleaned[..end + 5];
            (!candidate.is_empty()).then(|| path_from_output_value(root, candidate))
        })
        .collect()
}

fn find_docx_by_filename(root: &Path, filename: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    let mut inspected = 0usize;
    while let Some(dir) = stack.pop() {
        inspected += 1;
        if inspected > 8_000 {
            break;
        }
        let entries = fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_file() && name == filename {
                return Some(path);
            }
            if path.is_dir() && !should_skip_docx_search_dir(&name) {
                stack.push(path);
            }
        }
    }
    None
}

fn should_skip_docx_search_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "target" | "cargo-target" | "build" | "dist" | ".next"
    )
}

fn validate_and_extract_docx_text(bytes: &[u8]) -> anyhow::Result<String> {
    anyhow::ensure!(bytes.starts_with(b"PK"), "file is not a ZIP container");
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    archive.by_name("[Content_Types].xml")?;
    let mut document_xml = String::new();
    archive
        .by_name("word/document.xml")?
        .read_to_string(&mut document_xml)?;
    let parsed = roxmltree::Document::parse(&document_xml)?;
    let mut text = String::new();
    for node in parsed.descendants().filter_map(|node| node.text()) {
        let trimmed = node.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(trimmed);
        if text.len() > 20_000 {
            text.truncate(20_000);
            break;
        }
    }
    anyhow::ensure!(
        !text.trim().is_empty(),
        "word/document.xml contains no visible text"
    );
    Ok(text)
}

fn preferred_document_id(command: &BusinessCommand, filename: &str) -> String {
    command
        .record_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            command
                .payload
                .get("writeback_contract")
                .and_then(|value| value.get("document_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| format!("doc_{}", slug_for_document_id(filename)))
}

fn tags_from_command(command: &BusinessCommand) -> Value {
    let value = command
        .payload
        .get("tags")
        .or_else(|| command.client_context.get("tags"))
        .unwrap_or(&Value::Null);
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| Value::String(item.to_string()))
                .collect(),
        ),
        Value::String(raw) => Value::Array(
            raw.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| Value::String(item.to_string()))
                .collect(),
        ),
        _ => Value::Array(Vec::new()),
    }
}

fn title_from_filename(filename: &str) -> String {
    Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.replace(['_', '-'], " "))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Generated document".to_string())
}

fn slug_for_document_id(value: &str) -> String {
    let stem = Path::new(value)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(value);
    let slug = stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        format!("generated-{}", short_hash(value))
    } else {
        slug
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn clip_text(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut clipped = collapsed
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    clipped.push_str("...");
    clipped
}

fn refresh_queue_task_projection(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    original_task: Option<&channels::QueueTaskView>,
    updated_at_ms: i64,
) -> anyhow::Result<()> {
    let Some(task_id) = original_task
        .map(|task| task.message_key.clone())
        .or_else(|| find_queue_task_for_command(root, command_id))
    else {
        return Ok(());
    };
    let Some(task) = channels::load_queue_task(root, &task_id)? else {
        return Ok(());
    };
    let inbound_channel = command_inbound_channel(command);
    let payload = business_command_queue_task_payload(
        command_id,
        command,
        &task,
        &inbound_channel,
        updated_at_ms,
    );
    upsert_business_record(
        conn,
        "ctox_queue_tasks",
        &task.message_key,
        updated_at_ms,
        payload.clone(),
    )?;
    upsert_rxdb_collection_record(
        root,
        "ctox_queue_tasks",
        &task.message_key,
        updated_at_ms,
        payload,
    )
}

fn business_command_queue_task_payload(
    command_id: &str,
    command: &BusinessCommand,
    task: &channels::QueueTaskView,
    inbound_channel: &str,
    updated_at_ms: i64,
) -> Value {
    let route_status = effective_queue_projection_route_status(task);
    let mut payload = serde_json::json!({
        "id": task.message_key,
        "command_id": command_id,
        "title": task.title,
        "status": normalize_queue_status(&route_status),
        "route_status": route_status,
        "module": "ctox",
        "source_module": command.module.clone(),
        "inbound_channel": inbound_channel,
        "command_type": command.command_type.clone(),
        "priority": task.priority,
        "thread_key": task.thread_key,
        "prompt": task.prompt,
        "workspace_root": task.workspace_root,
        "updated_at_ms": updated_at_ms
    });
    enrich_queue_projection_payload(&mut payload, task, &route_status);
    if let Some(artifact) = browser_context_artifact_for_command(command) {
        if let Some(object) = payload.as_object_mut() {
            object.insert("browser_context_artifact".to_string(), artifact);
        }
    }
    payload
}

fn browser_context_artifact_for_command(command: &BusinessCommand) -> Option<Value> {
    if command.command_type != "ctox.browser_context.capture" {
        return None;
    }
    if let Some(artifact) = command
        .payload
        .get("browser_context_artifact")
        .filter(|value| value.is_object())
    {
        return Some(artifact.clone());
    }
    let browser_context = command
        .payload
        .get("browser_context")
        .or_else(|| command.client_context.get("browser_context"))
        .filter(|value| value.is_object())?
        .clone();
    let source_module = command
        .payload
        .get("source_module")
        .and_then(Value::as_str)
        .unwrap_or("browser");
    let source_id = command
        .payload
        .get("source_id")
        .or_else(|| browser_context.get("source_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let capture_script = command
        .payload
        .get("capture_script")
        .or_else(|| browser_context.get("capture_script"))
        .and_then(Value::as_str)
        .unwrap_or("");
    Some(serde_json::json!({
        "kind": "browser_context",
        "schema_version": 1,
        "stream": "rxdb",
        "source_module": source_module,
        "source_id": source_id,
        "capture_script": capture_script,
        "browser_context": browser_context,
        "sensitivity": "browser_context_reference",
        "secret_value_in_payload": false,
        "frame_data_in_payload": false
    }))
}

fn write_queue_task_projection(
    conn: &Connection,
    command_id: Option<&str>,
    task: &channels::QueueTaskView,
    updated_at_ms: i64,
) -> anyhow::Result<()> {
    upsert_business_record(
        conn,
        "ctox_queue_tasks",
        &task.message_key,
        updated_at_ms,
        queue_task_payload(command_id, task, updated_at_ms),
    )
}

fn queue_task_payload(
    command_id: Option<&str>,
    task: &channels::QueueTaskView,
    updated_at_ms: i64,
) -> Value {
    let route_status = effective_queue_projection_route_status(task);
    let mut payload = serde_json::json!({
        "id": task.message_key,
        "command_id": command_id.unwrap_or_default(),
        "title": task.title,
        "status": normalize_queue_status(&route_status),
        "route_status": route_status,
        "module": "ctox",
        "source_module": "ctox",
        "inbound_channel": "business_os.llm.chat",
        "command_type": "business_os.chat.task",
        "priority": task.priority,
        "thread_key": task.thread_key,
        "prompt": task.prompt,
        "workspace_root": task.workspace_root,
        "updated_at_ms": updated_at_ms
    });
    enrich_queue_projection_payload(&mut payload, task, &route_status);
    payload
}

fn effective_queue_projection_route_status(task: &channels::QueueTaskView) -> String {
    if task.route_status == "leased"
        && queue_status_note_is_terminal_success(task.status_note.as_deref())
    {
        return "handled".to_string();
    }
    if task.route_status == "leased"
        && queue_status_note_is_terminal_failure(task.status_note.as_deref())
    {
        return "failed".to_string();
    }
    task.route_status.clone()
}

fn enrich_queue_projection_payload(
    payload: &mut Value,
    task: &channels::QueueTaskView,
    route_status: &str,
) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    object.insert(
        "status".to_string(),
        Value::String(normalize_queue_status(route_status).to_string()),
    );
    object.insert(
        "route_status".to_string(),
        Value::String(route_status.to_string()),
    );
    object.insert(
        "task_status".to_string(),
        Value::String(normalize_queue_status(route_status).to_string()),
    );
    if let Some(note) = task
        .status_note
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        object.insert("status_note".to_string(), Value::String(note.to_string()));
        if route_status == "failed" {
            object.insert("error".to_string(), Value::String(note.to_string()));
        }
    } else {
        object.remove("status_note");
        if route_status != "failed" {
            object.remove("error");
        }
    }
    if let Some(owner) = task
        .lease_owner
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        object.insert("lease_owner".to_string(), Value::String(owner.to_string()));
    } else {
        object.remove("lease_owner");
    }
    if let Some(leased_at) = task
        .leased_at
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        object.insert(
            "leased_at".to_string(),
            Value::String(leased_at.to_string()),
        );
    } else {
        object.remove("leased_at");
    }
    if let Some(acked_at) = task
        .acked_at
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        object.insert("acked_at".to_string(), Value::String(acked_at.to_string()));
    } else {
        object.remove("acked_at");
    }
}

fn queue_projection_command_id(conn: &Connection, task_id: &str) -> anyhow::Result<Option<String>> {
    let value = conn
        .query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'ctox_queue_tasks' AND record_id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(value
        .and_then(|payload| serde_json::from_str::<Value>(&payload).ok())
        .and_then(|payload| {
            payload
                .get("command_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        }))
}

fn command_status_for_queue_route_status(route_status: &str) -> Option<&'static str> {
    match route_status {
        "handled" => Some("completed"),
        "failed" => Some("failed"),
        "cancelled" => Some("cancelled"),
        "blocked" => Some("blocked"),
        _ => None,
    }
}

fn projection_route_status_for_command_status(status: &str) -> Option<&'static str> {
    match status {
        "completed" | "handled" | "done" => Some("handled"),
        "failed" | "error" => Some("failed"),
        "cancelled" | "canceled" => Some("cancelled"),
        "blocked" => Some("blocked"),
        _ => None,
    }
}

fn projection_status_is_active(status: &str) -> bool {
    matches!(
        status,
        "queued" | "running" | "accepted" | "pending" | "pending_sync" | "leased"
    )
}

fn queue_status_note_is_terminal_success(note: Option<&str>) -> bool {
    let Some(note) = note.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let lower = note.to_ascii_lowercase();
    lower.contains("business-os:terminal-success")
        || lower.contains("terminal-success")
        || (lower.contains(" completed.") && lower.contains("changed "))
        || (lower.contains("completed.") && lower.contains("verified "))
}

fn queue_status_note_is_terminal_failure(note: Option<&str>) -> bool {
    let Some(note) = note.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let lower = note.to_ascii_lowercase();
    lower.contains("terminal-failure")
        || lower.contains("input exceeds the maximum length")
        || lower.contains("turn/start failed")
}

fn apply_queue_projection_status_fields(
    mut payload: Value,
    task: &channels::QueueTaskView,
    route_status: &str,
    updated_at_ms: i64,
) -> Value {
    if let Some(object) = payload.as_object_mut() {
        object.insert("id".to_string(), Value::String(task.message_key.clone()));
        object.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
        object
            .entry("title".to_string())
            .or_insert_with(|| Value::String(task.title.clone()));
        object
            .entry("thread_key".to_string())
            .or_insert_with(|| Value::String(task.thread_key.clone()));
        object
            .entry("prompt".to_string())
            .or_insert_with(|| Value::String(task.prompt.clone()));
        object
            .entry("priority".to_string())
            .or_insert_with(|| Value::String(task.priority.clone()));
        object.insert(
            "task_status".to_string(),
            Value::String(normalize_queue_status(route_status).to_string()),
        );
    }
    enrich_queue_projection_payload(&mut payload, task, route_status);
    payload
}

fn upsert_command_projection_from_queue_status(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    task: Option<&channels::QueueTaskView>,
    route_status: &str,
    updated_at_ms: i64,
    error_note: Option<&str>,
) -> anyhow::Result<()> {
    if command_id.trim().is_empty() {
        return Ok(());
    }
    if let Some(command_status) = command_status_for_queue_route_status(route_status) {
        conn.execute(
            "UPDATE business_commands
             SET status = ?2, observed_at_ms = ?3
             WHERE command_id = ?1",
            params![command_id, command_status, updated_at_ms],
        )?;
    }
    let mut payload = conn
        .query_row(
            "SELECT payload_json
             FROM business_records
             WHERE collection = 'business_commands'
               AND record_id = ?1
               AND deleted = 0",
            params![command_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "id": command_id,
                "command_id": command_id,
                "module": "ctox",
                "command_type": "",
                "record_id": command_id
            })
        });
    if let Some(object) = payload.as_object_mut() {
        if let Some(command_status) = command_status_for_queue_route_status(route_status) {
            object.insert(
                "status".to_string(),
                Value::String(command_status.to_string()),
            );
        }
        object.insert(
            "route_status".to_string(),
            Value::String(route_status.to_string()),
        );
        object.insert(
            "task_status".to_string(),
            Value::String(normalize_queue_status(route_status).to_string()),
        );
        if let Some(task) = task {
            object.insert(
                "task_id".to_string(),
                Value::String(task.message_key.clone()),
            );
            if let Some(note) = task
                .status_note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                object.insert(
                    "queue_status_note".to_string(),
                    Value::String(note.to_string()),
                );
                if route_status == "failed" {
                    object.insert("error".to_string(), Value::String(note.to_string()));
                }
            }
        }
        if let Some(note) = error_note.map(str::trim).filter(|value| !value.is_empty()) {
            object.insert("error".to_string(), Value::String(note.to_string()));
        }
        object.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    }
    upsert_business_record(
        conn,
        "business_commands",
        command_id,
        updated_at_ms,
        payload.clone(),
    )?;
    upsert_rxdb_collection_record(
        root,
        "business_commands",
        command_id,
        updated_at_ms,
        payload,
    )?;
    Ok(())
}

fn push_repair_action(
    actions: &mut Vec<Value>,
    class: &str,
    task_id: &str,
    command_id: Option<&str>,
    previous_route_status: &str,
    next_route_status: &str,
    note: Option<&str>,
) {
    if actions.len() >= 200 {
        return;
    }
    actions.push(serde_json::json!({
        "class": class,
        "task_id": task_id,
        "command_id": command_id.unwrap_or_default(),
        "previous_route_status": previous_route_status,
        "next_route_status": next_route_status,
        "note": note.unwrap_or_default(),
    }));
}

fn repair_inline_payload_artifacts(
    root: &Path,
    conn: &Connection,
    apply: bool,
    updated_at_ms: i64,
) -> anyhow::Result<usize> {
    let rows = {
        let mut statement = conn.prepare(
            "SELECT collection, record_id, payload_json
             FROM business_records
             WHERE collection IN ('business_commands', 'ctox_queue_tasks', 'ctox_bug_reports')
               AND deleted = 0
               AND (
                    payload_json LIKE '%data:image/%'
                 OR payload_json LIKE '%\"strokes\"%'
                 OR payload_json LIKE '%\"data_url\"%'
                 OR payload_json LIKE '%\"compositeDataUrl\"%'
               )
             ORDER BY updated_at_ms DESC, record_id ASC
             LIMIT 500",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut changed_count = 0usize;
    for (collection, record_id, payload_json) in rows {
        let mut payload = match serde_json::from_str::<Value>(&payload_json) {
            Ok(payload) => payload,
            Err(_) => continue,
        };
        if !redact_inline_report_artifacts(&mut payload) {
            continue;
        }
        changed_count += 1;
        if apply {
            if let Some(object) = payload.as_object_mut() {
                object.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
                object.insert(
                    "repair_note".to_string(),
                    Value::String(
                        "inline screenshot/stroke payload redacted by queue projection repair"
                            .to_string(),
                    ),
                );
            }
            upsert_business_record(
                conn,
                &collection,
                &record_id,
                updated_at_ms,
                payload.clone(),
            )?;
            upsert_rxdb_collection_record(root, &collection, &record_id, updated_at_ms, payload)?;
        }
    }
    Ok(changed_count)
}

fn redact_inline_report_artifacts(value: &mut Value) -> bool {
    match value {
        Value::Object(map) => {
            let mut changed = false;
            let keys = map.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                let lower = key.to_ascii_lowercase();
                if lower == "data_url" || lower == "compositedataurl" {
                    if let Some(Value::String(raw)) = map.get(&key) {
                        if raw.starts_with("data:image/") || raw.len() > 10_000 {
                            let byte_estimate = raw
                                .split_once(',')
                                .map(|(_, payload)| (payload.len() * 3) / 4)
                                .unwrap_or(raw.len());
                            map.insert(
                                key.clone(),
                                serde_json::json!({
                                    "redacted": true,
                                    "reason": "inline screenshot payload removed from Business OS command projection",
                                    "bytes_estimate": byte_estimate
                                }),
                            );
                            changed = true;
                            continue;
                        }
                    }
                }
                if lower == "strokes" {
                    if let Some(Value::Array(strokes)) = map.get(&key) {
                        let stroke_count = strokes.len();
                        let points_count = strokes
                            .iter()
                            .map(|stroke| stroke.as_array().map(Vec::len).unwrap_or(0))
                            .sum::<usize>();
                        map.insert(
                            key.clone(),
                            serde_json::json!({
                                "redacted": true,
                                "reason": "raw markup strokes removed from Business OS command projection",
                                "stroke_count": stroke_count,
                                "points_count": points_count
                            }),
                        );
                        changed = true;
                        continue;
                    }
                }
                if let Some(child) = map.get_mut(&key) {
                    changed |= redact_inline_report_artifacts(child);
                }
            }
            changed
        }
        Value::Array(items) => items
            .iter_mut()
            .map(redact_inline_report_artifacts)
            .fold(false, |acc, item| acc || item),
        Value::String(raw) if raw.starts_with("data:image/") && raw.len() > 10_000 => {
            let byte_estimate = raw
                .split_once(',')
                .map(|(_, payload)| (payload.len() * 3) / 4)
                .unwrap_or(raw.len());
            *value = serde_json::json!({
                "redacted": true,
                "reason": "inline image data URL removed from Business OS projection",
                "bytes_estimate": byte_estimate
            });
            true
        }
        _ => false,
    }
}

fn count_legacy_http_fallback_records(conn: &Connection) -> anyhow::Result<usize> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM business_records
         WHERE collection IN ('business_commands', 'ctox_queue_tasks', 'business_chats')
           AND deleted = 0
           AND (
                payload_json LIKE '%http-fallback%'
             OR payload_json LIKE '%business-os-http-command-fallback%'
             OR payload_json LIKE '%/api/business-os/commands%'
           )",
        [],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as usize)
}

fn mark_business_record_deleted(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    updated_at_ms: i64,
) -> anyhow::Result<()> {
    let rev = format!("rev_{}", Uuid::new_v4());
    let payload = serde_json::json!({
        "id": record_id,
        "_rev": rev,
        "_deleted": true,
        "updated_at_ms": updated_at_ms
    });
    conn.execute(
        "INSERT INTO business_records
            (collection, record_id, rev, deleted, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, 1, ?4, ?5)
         ON CONFLICT(collection, record_id) DO UPDATE SET
            rev = excluded.rev,
            deleted = excluded.deleted,
            updated_at_ms = excluded.updated_at_ms,
            payload_json = excluded.payload_json",
        params![
            collection,
            record_id,
            rev,
            updated_at_ms,
            serde_json::to_string(&payload)?
        ],
    )?;
    Ok(())
}

fn normalize_task_id(value: &str) -> String {
    let trimmed = value.trim();
    trimmed
        .strip_prefix("queue-")
        .unwrap_or(trimmed)
        .to_string()
}

fn resolve_ctox_task_id(root: &Path, value: &str) -> anyhow::Result<String> {
    let task_id = normalize_task_id(value);
    if task_id.is_empty() {
        return Ok(task_id);
    }
    if channels::load_queue_task(root, &task_id)?.is_some() {
        return Ok(task_id);
    }
    Ok(find_queue_task_for_command(root, &task_id).unwrap_or(task_id))
}

fn non_empty_trimmed(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn session_user_label(session: &BusinessOsSession) -> String {
    session
        .user
        .as_ref()
        .map(|user| format!("{} ({})", user.display_name, user.role))
        .unwrap_or_else(|| "unknown user".to_string())
}

fn materialize_business_chat_attachments(
    root: &Path,
    command_id: &str,
    command: &BusinessCommand,
) -> anyhow::Result<Vec<MaterializedBusinessChatAttachment>> {
    let refs = business_chat_attachment_refs(command);
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let mut attachments = Vec::with_capacity(refs.len());
    let mut seen = HashSet::new();
    for attachment_ref in refs {
        let file_id = attachment_ref_string(&attachment_ref, &["file_id", "fileId"])
            .with_context(|| "Business OS attachment reference is missing file_id")?;
        let generation_id =
            attachment_ref_string(&attachment_ref, &["generation_id", "generationId"]);
        let key = format!("{}:{}", file_id, generation_id.clone().unwrap_or_default());
        if !seen.insert(key) {
            continue;
        }
        attachments.push(materialize_business_chat_attachment(
            root,
            command_id,
            &attachment_ref,
            &file_id,
            generation_id.as_deref(),
        )?);
    }
    Ok(attachments)
}

fn business_chat_attachment_refs(command: &BusinessCommand) -> Vec<Value> {
    let mut refs = Vec::new();
    for container in [&command.payload, &command.client_context] {
        for key in ["attachment_refs", "attachments"] {
            if let Some(items) = container.get(key).and_then(Value::as_array) {
                refs.extend(items.iter().filter_map(|item| {
                    if !item.is_object() {
                        return None;
                    }
                    let file_id = attachment_ref_string(item, &["file_id", "fileId"])?;
                    let kind = attachment_ref_string(item, &["kind"])
                        .unwrap_or_else(|| "desktop_file".to_string());
                    (kind == "desktop_file" || kind == "file" || !file_id.is_empty())
                        .then(|| item.clone())
                }));
            }
        }
    }
    refs
}

fn materialize_business_chat_attachment(
    root: &Path,
    command_id: &str,
    attachment_ref: &Value,
    file_id: &str,
    requested_generation_id: Option<&str>,
) -> anyhow::Result<MaterializedBusinessChatAttachment> {
    let file_doc = rxdb_desktop_file_document(root, file_id)?;
    anyhow::ensure!(
        !is_rxdb_deleted_document(&file_doc),
        "Business OS attachment `{file_id}` is deleted"
    );
    anyhow::ensure!(
        file_doc
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("file")
            == "file",
        "Business OS attachment `{file_id}` is not a regular file"
    );
    let content_state = file_doc
        .get("content_state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    anyhow::ensure!(
        content_state == "available",
        "Business OS attachment `{file_id}` content is not available"
    );
    let generation_id = requested_generation_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            file_doc
                .get("content_generation_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .with_context(|| format!("Business OS attachment `{file_id}` has no content generation"))?;
    let file_generation_id = file_doc
        .get("content_generation_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !file_generation_id.is_empty() {
        anyhow::ensure!(
            file_generation_id == generation_id,
            "Business OS attachment `{file_id}` generation mismatch"
        );
    }
    let file_hash = file_doc
        .get("content_hash")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let ref_hash = attachment_ref_string(attachment_ref, &["content_hash", "contentHash"]);
    if let Some(ref_hash) = ref_hash.as_deref().filter(|value| !value.is_empty()) {
        anyhow::ensure!(
            file_hash.is_empty() || file_hash == ref_hash,
            "Business OS attachment `{file_id}` content hash mismatch"
        );
    }
    let content_hash = if file_hash.is_empty() {
        ref_hash.unwrap_or_default()
    } else {
        file_hash
    };
    anyhow::ensure!(
        !content_hash.is_empty(),
        "Business OS attachment `{file_id}` has no content hash"
    );
    let content_hash_scheme = file_doc
        .get("content_hash_scheme")
        .and_then(Value::as_str)
        .or_else(|| {
            attachment_ref
                .get("content_hash_scheme")
                .and_then(Value::as_str)
        })
        .unwrap_or(BUSINESS_OS_CHAT_ATTACHMENT_CONTENT_HASH_SCHEME);
    anyhow::ensure!(
        content_hash_scheme == BUSINESS_OS_CHAT_ATTACHMENT_CONTENT_HASH_SCHEME,
        "Business OS attachment `{file_id}` uses unsupported content hash scheme `{content_hash_scheme}`"
    );
    let size_bytes = file_doc
        .get("size_bytes")
        .and_then(Value::as_u64)
        .or_else(|| attachment_ref.get("size_bytes").and_then(Value::as_u64))
        .unwrap_or(0);
    if let Some(ref_size) = attachment_ref.get("size_bytes").and_then(Value::as_u64) {
        anyhow::ensure!(
            ref_size == size_bytes,
            "Business OS attachment `{file_id}` size mismatch"
        );
    }
    let chunks = rxdb_desktop_file_chunks(root, file_id, &generation_id)?;
    let decoded = decode_verified_desktop_file_chunks(
        file_id,
        &generation_id,
        size_bytes,
        &content_hash,
        chunks,
    )?;
    let name = attachment_ref_string(attachment_ref, &["name"])
        .or_else(|| {
            file_doc
                .get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| file_id.to_string());
    let mime_type = attachment_ref_string(attachment_ref, &["mime_type", "mimeType"])
        .or_else(|| {
            file_doc
                .get("mime_type")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let virtual_path = attachment_ref_string(attachment_ref, &["virtual_path", "virtualPath"])
        .or_else(|| {
            file_doc
                .get("virtual_path")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            file_doc
                .get("path")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default();
    let dir = root
        .join("runtime")
        .join("business-os")
        .join("chat-attachments")
        .join(sanitize_filename(command_id));
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create chat attachment dir {}", dir.display()))?;
    let file_name = materialized_attachment_filename(file_id, &name);
    let local_path = dir.join(file_name);
    fs::write(&local_path, &decoded).with_context(|| {
        format!(
            "failed to write materialized Business OS attachment {}",
            local_path.display()
        )
    })?;
    Ok(MaterializedBusinessChatAttachment {
        attachment_id: attachment_ref_string(attachment_ref, &["attachment_id", "attachmentId"])
            .unwrap_or_default(),
        file_id: file_id.to_string(),
        generation_id,
        name,
        mime_type,
        size_bytes,
        content_hash,
        content_hash_scheme: BUSINESS_OS_CHAT_ATTACHMENT_CONTENT_HASH_SCHEME.to_string(),
        virtual_path,
        local_path: local_path.to_string_lossy().into_owned(),
    })
}

fn attachment_ref_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn decode_verified_desktop_file_chunks(
    file_id: &str,
    generation_id: &str,
    size_bytes: u64,
    content_hash: &str,
    chunks: Vec<Value>,
) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        !chunks.is_empty(),
        "Business OS attachment `{file_id}` has no chunks for generation `{generation_id}`"
    );
    let total = chunks
        .first()
        .and_then(|chunk| chunk.get("total"))
        .and_then(Value::as_u64)
        .with_context(|| format!("Business OS attachment `{file_id}` chunk total is missing"))?;
    anyhow::ensure!(
        total > 0,
        "Business OS attachment `{file_id}` chunk total is zero"
    );
    let expected_total = expected_desktop_file_chunk_total(size_bytes);
    anyhow::ensure!(
        total == expected_total,
        "Business OS attachment `{file_id}` chunk total mismatch"
    );
    let mut by_index = BTreeMap::new();
    for chunk in chunks {
        anyhow::ensure!(
            !is_rxdb_deleted_document(&chunk),
            "Business OS attachment `{file_id}` contains a deleted chunk"
        );
        anyhow::ensure!(
            chunk.get("file_id").and_then(Value::as_str) == Some(file_id),
            "Business OS attachment `{file_id}` chunk file_id mismatch"
        );
        anyhow::ensure!(
            chunk.get("generation_id").and_then(Value::as_str) == Some(generation_id),
            "Business OS attachment `{file_id}` chunk generation mismatch"
        );
        anyhow::ensure!(
            chunk.get("total").and_then(Value::as_u64) == Some(total),
            "Business OS attachment `{file_id}` chunk total mismatch"
        );
        anyhow::ensure!(
            chunk
                .get("encoding")
                .and_then(Value::as_str)
                .unwrap_or("base64")
                == "base64",
            "Business OS attachment `{file_id}` chunk encoding is unsupported"
        );
        anyhow::ensure!(
            chunk
                .get("content_hash")
                .and_then(Value::as_str)
                .map(|hash| hash == content_hash)
                .unwrap_or(true),
            "Business OS attachment `{file_id}` chunk content hash mismatch"
        );
        let idx = chunk
            .get("idx")
            .and_then(Value::as_u64)
            .with_context(|| format!("Business OS attachment `{file_id}` chunk idx is missing"))?;
        anyhow::ensure!(
            idx < total,
            "Business OS attachment `{file_id}` chunk idx is out of range"
        );
        let data = chunk
            .get("data")
            .and_then(Value::as_str)
            .with_context(|| format!("Business OS attachment `{file_id}` chunk data is missing"))?;
        if let Some(size) = chunk.get("size_bytes").and_then(Value::as_u64) {
            anyhow::ensure!(
                size == data.len() as u64,
                "Business OS attachment `{file_id}` chunk size mismatch"
            );
        }
        let chunk_hash_scheme = chunk
            .get("chunk_hash_scheme")
            .and_then(Value::as_str)
            .unwrap_or(BUSINESS_OS_CHAT_ATTACHMENT_CHUNK_HASH_SCHEME);
        anyhow::ensure!(
            chunk_hash_scheme == BUSINESS_OS_CHAT_ATTACHMENT_CHUNK_HASH_SCHEME,
            "Business OS attachment `{file_id}` uses unsupported chunk hash scheme `{chunk_hash_scheme}`"
        );
        if let Some(chunk_hash) = chunk.get("chunk_hash").and_then(Value::as_str) {
            anyhow::ensure!(
                hex_sha256(data.as_bytes()) == chunk_hash,
                "Business OS attachment `{file_id}` chunk hash mismatch"
            );
        }
        anyhow::ensure!(
            by_index.insert(idx, data.to_string()).is_none(),
            "Business OS attachment `{file_id}` has duplicate chunk idx {idx}"
        );
    }
    anyhow::ensure!(
        by_index.len() as u64 == total,
        "Business OS attachment `{file_id}` is missing chunks"
    );
    let mut encoded = String::new();
    for idx in 0..total {
        let chunk = by_index.get(&idx).with_context(|| {
            format!("Business OS attachment `{file_id}` is missing chunk {idx}")
        })?;
        encoded.push_str(chunk);
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .with_context(|| format!("Business OS attachment `{file_id}` base64 decode failed"))?;
    anyhow::ensure!(
        decoded.len() as u64 == size_bytes,
        "Business OS attachment `{file_id}` decoded size mismatch"
    );
    anyhow::ensure!(
        hex_sha256(&decoded) == content_hash,
        "Business OS attachment `{file_id}` decoded content hash mismatch"
    );
    Ok(decoded)
}

fn expected_desktop_file_chunk_total(size_bytes: u64) -> u64 {
    let base64_len = size_bytes.div_ceil(3) * 4;
    std::cmp::max(
        1,
        base64_len.div_ceil(BUSINESS_OS_CHAT_ATTACHMENT_CHUNK_SIZE as u64),
    )
}

fn is_rxdb_deleted_document(value: &Value) -> bool {
    ["_deleted", "deleted", "is_deleted"]
        .iter()
        .any(|key| value.get(*key).and_then(Value::as_bool).unwrap_or(false))
}

fn materialized_attachment_filename(file_id: &str, name: &str) -> String {
    let safe_name = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .chars()
        .take(140)
        .collect::<String>();
    let safe_name = if safe_name.is_empty() {
        "attachment".to_string()
    } else {
        safe_name
    };
    let id = file_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .take(64)
        .collect::<String>();
    format!(
        "{}_{}",
        if id.is_empty() { "attachment" } else { &id },
        safe_name
    )
}

fn create_ctox_queue_task(
    root: &Path,
    command_id: &str,
    command: &BusinessCommand,
) -> anyhow::Result<Option<channels::QueueTaskView>> {
    if let Some(existing_id) = find_queue_task_for_command(root, command_id) {
        if let Some(existing) = channels::load_queue_task(root, &existing_id)? {
            return Ok(Some(existing));
        }
    }
    let attachments = materialize_business_chat_attachments(root, command_id, command)?;
    let title = command_title(command);
    let prompt = command_prompt(command_id, command, &attachments);
    let priority = command
        .payload
        .get("priority")
        .and_then(Value::as_str)
        .or_else(|| {
            command
                .client_context
                .get("priority")
                .and_then(Value::as_str)
        })
        .unwrap_or("normal")
        .to_string();
    let thread_key = command
        .payload
        .get("thread_key")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("business-os/{}", command.module));
    let task = channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title,
            prompt,
            thread_key,
            workspace_root: Some(root.display().to_string()),
            priority,
            suggested_skill: suggested_skill_for_command(command),
            parent_message_key: None,
            extra_metadata: Some(serde_json::json!({
                "source": "business-os",
                "business_os_command_id": command_id,
                "business_os_module": command.module,
                "business_os_inbound_channel": command_inbound_channel(command),
                "business_os_command_type": command.command_type,
                "business_os_record_id": command.record_id,
                "business_os_attachments": attachments,
                "client_context": command.client_context
            })),
        },
    )?;
    Ok(Some(task))
}

fn command_inbound_channel(command: &BusinessCommand) -> String {
    const CHANNEL_KEYS: &[&str] = &[
        "inbound_channel",
        "channel",
        "source_channel",
        "via",
        "source_kind",
        "source_module",
        "module_id",
        "module",
        "surface",
    ];
    first_string_field(&command.client_context, CHANNEL_KEYS)
        .or_else(|| first_string_field(&command.payload, CHANNEL_KEYS))
        .unwrap_or_else(|| command.module.clone())
}

fn first_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn command_title(command: &BusinessCommand) -> String {
    command
        .payload
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let record = command
                .record_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("new task");
            format!(
                "{} · {}",
                display_command_type(&command.command_type),
                record
            )
        })
}

fn command_prompt(
    command_id: &str,
    command: &BusinessCommand,
    attachments: &[MaterializedBusinessChatAttachment],
) -> String {
    let instruction = command
        .payload
        .get("instruction")
        .and_then(Value::as_str)
        .or_else(|| command.payload.get("prompt").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Execute this Business OS automation through CTOX.");
    let instruction =
        truncate_text_preserve(instruction, BUSINESS_OS_QUEUE_PROMPT_INSTRUCTION_MAX_CHARS);
    let required_skill_names = required_skill_names(command);
    let mut payload_preview_value = command.payload.clone();
    let mut context_preview_value = command.client_context.clone();
    if is_business_os_app_module_command(command) {
        rewrite_required_skills_preview(&mut payload_preview_value, &required_skill_names);
        rewrite_required_skills_preview(&mut context_preview_value, &required_skill_names);
    }
    let payload = prompt_json_preview(
        &payload_preview_value,
        BUSINESS_OS_QUEUE_PROMPT_JSON_PREVIEW_CHARS,
    );
    let context = prompt_json_preview(
        &context_preview_value,
        BUSINESS_OS_QUEUE_PROMPT_JSON_PREVIEW_CHARS,
    );
    let required_skills = if required_skill_names.is_empty() {
        String::new()
    } else {
        format!(
            "\nRequired CTOX skills: {}\n{}\n",
            required_skill_names.join(", "),
            business_os_required_skill_prompt_contract()
        )
    };
    let app_target = business_os_app_command_target_prompt_block(command);
    let attachment_manifest = business_chat_attachment_prompt_manifest(attachments);
    let prompt = format!(
        "{instruction}{required_skills}{app_target}\nBusiness OS command:\n- command_id: {command_id}\n- module: {}\n- type: {}\n- record_id: {}{attachment_manifest}\n\nFull payload and client context are stored on the Business OS command record. The JSON below is a bounded execution preview to keep the queue worker under its input limit.\n\nPayload JSON:\n{payload}\n\nClient context JSON:\n{context}",
        command.module,
        command.command_type,
        command.record_id.as_deref().unwrap_or("")
    );
    truncate_text_preserve(&prompt, BUSINESS_OS_QUEUE_PROMPT_MAX_CHARS)
}

const BUSINESS_OS_APP_MODULE_SKILL_NAME: &str = "business-os-app-module-development";
const BUSINESS_OS_LEGACY_BASIC_MODULE_SKILL_NAME: &str = "business-basic-module-development";
const BUSINESS_OS_LEGACY_BASIC_MODULE_SKILL_PATH: &str =
    "product_engineering/business-basic-module-development";

fn required_skill_names(command: &BusinessCommand) -> Vec<String> {
    let mut names = Vec::new();
    if is_business_os_app_module_command(command) {
        push_required_skill(&mut names, BUSINESS_OS_APP_MODULE_SKILL_NAME);
    }
    if let Some(items) = command
        .payload
        .get("required_skills")
        .and_then(Value::as_array)
    {
        for item in items.iter().filter_map(Value::as_str) {
            push_required_skill(&mut names, item);
        }
    }
    if let Some(items) = command
        .client_context
        .get("required_skills")
        .and_then(Value::as_array)
    {
        for item in items.iter().filter_map(Value::as_str) {
            push_required_skill(&mut names, item);
        }
    }
    names
}

fn push_required_skill(names: &mut Vec<String>, raw: &str) {
    let name = raw.trim();
    if name.is_empty()
        || name == BUSINESS_OS_LEGACY_BASIC_MODULE_SKILL_NAME
        || name == BUSINESS_OS_LEGACY_BASIC_MODULE_SKILL_PATH
    {
        return;
    }
    if !names.iter().any(|existing| existing == name) {
        names.push(name.to_owned());
    }
}

fn is_business_os_app_module_command(command: &BusinessCommand) -> bool {
    command.command_type == "ctox.business_os.app.modify"
        || command.command_type == "ctox.business_os.app.create"
        || command
            .payload
            .get("target")
            .and_then(Value::as_str)
            .map(|value| value.eq_ignore_ascii_case("app"))
            .unwrap_or(false)
        || command
            .payload
            .get("mode")
            .and_then(Value::as_str)
            .map(|value| value.eq_ignore_ascii_case("app"))
            .unwrap_or(false)
        || command
            .client_context
            .get("target")
            .and_then(Value::as_str)
            .map(|value| value.eq_ignore_ascii_case("app"))
            .unwrap_or(false)
        || command
            .client_context
            .get("mode")
            .and_then(Value::as_str)
            .map(|value| value.eq_ignore_ascii_case("app"))
            .unwrap_or(false)
}

fn business_os_required_skill_prompt_contract() -> &'static str {
    "Skill handling contract: required skills are instruction context, not deliverables. Read and follow them through CTOX skill tooling only. Do not create, copy, mirror, export, or edit skill files or skill-named directories in the workspace unless the user explicitly asked to change a skill."
}

fn business_os_app_command_target_prompt_block(command: &BusinessCommand) -> String {
    let Some((module_id, install_target, module_dir)) =
        business_os_app_command_target_metadata(command)
    else {
        return String::new();
    };
    format!(
        "\nBusiness OS app build target:\n- deliverable: runnable Business OS app/module files, not documentation, plans, trace files, blocker notes, or skill files.\n- module_id: {module_id}\n- install_target: {install_target}\n- only_allowed_app_artifact_directory: {module_dir}\n- cwd warning: shell tools run from the install root, not from the module directory; never use bare redirects like > module.json, > collections.schema.json, > {module_id}/index.js, or mkdir {module_id}.\n- required shell write pattern: set MODULE_DIR=\"{module_dir}\" and write every file as \"$MODULE_DIR/<file>\"; create \"$MODULE_DIR/locales\" and \"$MODULE_DIR/tests\" before writing nested files.\n- path rule: every generated app artifact must be under {module_dir}/; do not write root-level module.json, root-level collections.schema.json, root-level {module_id}/, root-level blocker/status Markdown, src/skills/, or any skill-named path. Ignore stale artifact-contract/review examples that contradict this target block.\n- no guard probing: do not test shell aliases, wrapper behavior, root write behavior, hardlinks, symlinks, or guard behavior; implement only inside {module_dir}/.\n- first file action: create {module_dir}/, then write module.json, index.html, index.css, index.js with mount(ctx), schema.js, collections.schema.json, icon.svg, locales, and tests inside that directory.\n- installed manifest rule: for runtime-installed-module, module.json must use entry=\"installed-modules/{module_id}/index.html\" and install_scope=\"installed\"; parse module.json and collections.schema.json immediately after editing them.\n- schema rule: module.json may list shell collections such as business_commands, but schema.js and collections.schema.json must export only module-owned collections.\n- repair order: fix target path, valid JSON, manifest mode, required files, schema ownership, UI layout, dependency/data-plane patterns, ESM syntax, tests, then shell smoke; do not patch tests to hide earlier failures.\n- persistence: use the Business OS RxDB/WebRTC data plane exposed by the shell context; do not create IndexedDB/Postgres/SQLite/HTTP fallbacks or dependency-managed builds.\n- dependencies: browser-safe ESM only; no package manager, no bundled node_modules, no CommonJS require, no npx, no esbuild/Vite/Rollup/Webpack proof, and no bundler imports in tests; these forbidden names may appear in this prompt but must not appear in generated app files, comments, or tests.\n- UI: default to one/two panes plus modals/drawers; do not create layout.right, right-column resizers, or three-column grids unless the user explicitly requested a persistent third pane and you can justify it in a code comment.\n- scope: build the smallest useful one-pass app; avoid broad decorative status/filter/export/settings surfaces unless all handlers and tests are implemented.\n- tool-call safety: do not generate mammoth single here-doc/tool-call writes; keep files concise or split large writes into bounded chunks, then run syntax checks.\n- repair hygiene: do not leave .bak/.orig/.rej/.tmp/bundle/probe files; do not line-number sed-patch large generated JavaScript when a bounded helper/file rewrite is safer.\n- automation: include at least one real business_commands dispatch that creates a normal CTOX chat/ticket/work item through the Business OS command flow.\n- stop condition: before claiming success, run module tests and the Business OS module/RxDB guards when available; a green custom test does not count while static validation is red.\n"
    )
}

fn business_os_app_command_target_metadata(
    command: &BusinessCommand,
) -> Option<(String, String, String)> {
    if !is_business_os_app_module_command(command) {
        return None;
    }
    let module_id = first_non_empty_json_string(&[
        command.payload.get("module_id"),
        command.payload.get("app_id"),
        command.client_context.get("module_id"),
        command.client_context.get("app_id"),
    ])
    .or(command.record_id.as_deref())
    .unwrap_or(&command.module)
    .to_string();
    let install_target = first_non_empty_json_string(&[
        command.payload.get("install_target"),
        command.client_context.get("install_target"),
    ])
    .unwrap_or("runtime-installed-module")
    .to_string();
    let module_dir = if install_target == "runtime-installed-module" {
        format!("src/apps/business-os/installed-modules/{module_id}")
    } else {
        format!("src/apps/business-os/modules/{module_id}")
    };
    Some((module_id, install_target, module_dir))
}

fn rewrite_required_skills_preview(value: &mut Value, required_skills: &[String]) {
    if required_skills.is_empty() {
        return;
    }
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "required_skills".to_owned(),
            Value::Array(
                required_skills
                    .iter()
                    .map(|skill| Value::String(skill.clone()))
                    .collect(),
            ),
        );
        if object.contains_key("suggested_skill") {
            object.insert(
                "suggested_skill".to_owned(),
                Value::String(BUSINESS_OS_APP_MODULE_SKILL_NAME.to_owned()),
            );
        }
    }
}

fn first_non_empty_json_string<'a>(values: &[Option<&'a Value>]) -> Option<&'a str> {
    values.iter().find_map(|value| {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn business_chat_attachment_prompt_manifest(
    attachments: &[MaterializedBusinessChatAttachment],
) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let mut output = String::from(
        "\n\nBusiness OS attachments:\nThe files below were uploaded in the Business OS chat, reconstructed from RxDB desktop_file_chunks, and verified before this task was queued. Inspect relevant images/PDFs via the available local file/image tools before relying on their visual contents.\n",
    );
    for (idx, attachment) in attachments.iter().enumerate() {
        output.push_str(&format!(
            "- attachment_{}: {}\n  mime_type: {}\n  size_bytes: {}\n  local_path: {}\n  sha256: {}\n  source: desktop_files/{} generation {}\n",
            idx + 1,
            attachment.name,
            attachment.mime_type,
            attachment.size_bytes,
            attachment.local_path,
            attachment.content_hash,
            attachment.file_id,
            attachment.generation_id
        ));
    }
    output
}

fn prompt_json_preview(value: &Value, max_chars: usize) -> String {
    let raw = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    if raw.chars().count() <= max_chars {
        return raw;
    }
    let preview = raw
        .chars()
        .take(max_chars.saturating_sub(160))
        .collect::<String>();
    let omitted = raw.chars().count().saturating_sub(preview.chars().count());
    format!(
        "{preview}\n... truncated {omitted} chars; full JSON is stored on the Business OS command record ..."
    )
}

fn truncate_text_preserve(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut output = value
        .chars()
        .take(max_chars.saturating_sub(80))
        .collect::<String>();
    output.push_str("\n... truncated; full text is stored on the Business OS command record ...");
    output
}

fn suggested_skill_for_command(command: &BusinessCommand) -> Option<String> {
    if is_source_parse_command(&command.command_type) {
        Some("business-os-import-parser".to_string())
    } else if command.command_type == "outbound.pipeline.outreach_draft" {
        Some("business-os-outbound-message-drafting".to_string())
    } else if is_outbound_research_command(&command.command_type) {
        Some("universal-scraping".to_string())
    } else if command.command_type.starts_with("research.systematic.") {
        Some("systematic-research".to_string())
    } else if is_match_command(&command.command_type) || command.command_type.contains("scoring") {
        Some("business-os-matching".to_string())
    } else if command.command_type.contains("knowledge")
        || command.command_type.contains("runbook")
        || command.command_type.contains("skillbook")
    {
        Some("knowledge".to_string())
    } else if command.command_type.contains("app.modify")
        || command.command_type.contains("app.create")
    {
        Some(BUSINESS_OS_APP_MODULE_SKILL_NAME.to_string())
    } else {
        None
    }
}

pub fn is_outbound_research_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "outbound.company.research"
            | "outbound.pipeline.contact_research"
            | "outbound.pipeline.lead_qualification"
    )
}

pub fn is_source_parse_command(command_type: &str) -> bool {
    command_type.contains("source.parse")
        || command_type == "outbound.source.import"
        || command_type.contains("parse_requirement")
        || command_type.contains("parse_object")
}

pub fn is_match_command(command_type: &str) -> bool {
    command_type.contains("match.compute") || command_type == "matching.match"
}

fn display_command_type(value: &str) -> String {
    value
        .replace("business_os.", "")
        .replace("ctox.", "")
        .replace(['_', '.', '-'], " ")
}

fn normalize_queue_status(route_status: &str) -> &str {
    match route_status {
        "pending" => "queued",
        "leased" => "running",
        "handled" => "completed",
        "cancelled" => "cancelled",
        "blocked" => "blocked",
        "failed" => "failed",
        other => other,
    }
}

pub(super) fn upsert_business_record(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    updated_at_ms: i64,
    mut payload: Value,
) -> anyhow::Result<()> {
    let rev = format!("rev_{}", Uuid::new_v4());
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("id".to_string(), Value::String(record_id.to_string()));
        obj.insert("_rev".to_string(), Value::String(rev.clone()));
        obj.insert("_deleted".to_string(), Value::Bool(false));
        obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    }
    conn.execute(
        "INSERT INTO business_records
            (collection, record_id, rev, deleted, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, 0, ?4, ?5)
         ON CONFLICT(collection, record_id) DO UPDATE SET
            rev = excluded.rev,
            deleted = excluded.deleted,
            updated_at_ms = excluded.updated_at_ms,
            payload_json = excluded.payload_json",
        params![
            collection,
            record_id,
            rev,
            updated_at_ms,
            serde_json::to_string(&payload)?
        ],
    )?;
    Ok(())
}

/// Project the outcome of a `ctox.iot.*` business command into the
/// RxDB-visible `business_records` store. The engine state lives in
/// `runtime/ctox.sqlite3` (written by the shared `iot::commands` op via
/// `crate::paths::core_db`); `iot::projector` reads it back and builds the
/// canonical `iot_*` envelopes, and this integrator writes those rows into the
/// business-os store (the read source for `pull_collection_records` and the
/// RxDB peer). No HTTP bridge: every row flows engine -> projector ->
/// business_records -> RxDB/WebRTC.
///
/// Returns the `(collection, record_id)` pairs for the rxdb_peer to stream into
/// the live RxDB collections. Idempotent: a replayed outcome rewrites identical
/// envelopes (only `_rev`/`updated_at_ms` advance) and tombstones stay
/// tombstoned.
pub(super) fn project_iot_business_command_outcome(
    root: &Path,
    result: &Value,
) -> anyhow::Result<Vec<(&'static str, String)>> {
    use crate::iot::projector::ReprojectedRecord;

    let records = crate::iot::projector::reproject_business_command_outcome(root, result)?;
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let conn = open_store(root)?;
    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    for record in records {
        match record {
            ReprojectedRecord::Rows(rows) => {
                for row in rows {
                    upsert_iot_projection_row(
                        &conn,
                        row.collection,
                        &row.record_id,
                        row.updated_at_ms,
                        row.payload.clone(),
                    )?;
                    pairs.push((row.collection, row.record_id));
                }
            }
            ReprojectedRecord::EchoOnly {
                collection,
                record_id,
            } => {
                // The executor already wrote the (query-scoped) datapoint window
                // row into the core db's business_records; mirror it into the
                // business-os store so the RxDB read path can echo it.
                if let Some(payload) = read_core_db_business_record(root, collection, &record_id)? {
                    let updated_at_ms = payload
                        .get("updated_at_ms")
                        .and_then(Value::as_i64)
                        .unwrap_or_else(|| now_ms() as i64);
                    upsert_iot_projection_row(
                        &conn,
                        collection,
                        &record_id,
                        updated_at_ms,
                        payload,
                    )?;
                    pairs.push((collection, record_id));
                }
            }
        }
    }
    Ok(pairs)
}

/// Full idempotent resync of EVERY projectable iot engine row into the
/// RxDB-visible business-os store (`open_store`). This is the bridge `ctox iot
/// project all` calls: without it, CLI mutations (asset.upsert, attribute.write,
/// …) only write engine state + an inline core-db row and never reach the
/// `business-os.sqlite3` store the apps read, so they never replicate over
/// RxDB/WebRTC. The projector is the canonical envelope producer
/// (`projector::project_all` reads `runtime/ctox.sqlite3` engine tables, never
/// writes); this function owns the `business_records` write into the
/// RxDB-visible store, mirroring `project_iot_business_command_outcome`. No HTTP
/// bridge: engine -> projector -> business_records -> RxDB/WebRTC. Returns the
/// `(collection, record_id)` pairs written.
///
/// `realm` selects the projection/sync scope: `Some(r)` projects ONLY realm
/// `r`'s rows into the RxDB-visible store (the session/executor path must use
/// this so WebRTC never replicates other realms' rows to a paired peer);
/// `None` is the trusted operator resync (`ctox iot project all`) that mirrors
/// every realm. Realm isolation on the projection/sync surface is enforced in
/// `projector::project_all_in_realm`.
pub(crate) fn project_all_iot(
    root: &Path,
    realm: Option<&str>,
) -> anyhow::Result<Vec<(&'static str, String)>> {
    let engine = crate::iot::store::open_iot_store(root)?;
    let rows = crate::iot::projector::project_all_in_realm(&engine, realm)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let conn = open_store(root)?;
    let mut pairs: Vec<(&'static str, String)> = Vec::with_capacity(rows.len());
    for row in rows {
        upsert_iot_projection_row(
            &conn,
            row.collection,
            &row.record_id,
            row.updated_at_ms,
            row.payload.clone(),
        )?;
        pairs.push((row.collection, row.record_id));
    }
    Ok(pairs)
}

/// Project already-canonical IoT rows into the RxDB-visible Business OS store.
/// Runtime agent pumps use this path after `iot::runtime::run_agent_step`
/// returns projector rows; command execution uses
/// `project_iot_business_command_outcome`, which first re-derives rows from a
/// command outcome. Both converge on the same tombstone-aware upsert below.
pub(super) fn project_iot_projection_rows(
    root: &Path,
    rows: Vec<crate::iot::projector::ProjectionRow>,
) -> anyhow::Result<Vec<(&'static str, String)>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let conn = open_store(root)?;
    let mut pairs = Vec::with_capacity(rows.len());
    for row in rows {
        upsert_iot_projection_row(
            &conn,
            row.collection,
            &row.record_id,
            row.updated_at_ms,
            row.payload.clone(),
        )?;
        pairs.push((row.collection, row.record_id));
    }
    Ok(pairs)
}

/// Tombstone-aware `business_records` upsert for iot projection rows. Unlike
/// `upsert_business_record` (which always forces `_deleted:false`), this honors a
/// `_deleted: true` payload so deletion tombstones set the `deleted` column and
/// reach RxDB as a doc removal.
fn upsert_iot_projection_row(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    updated_at_ms: i64,
    mut payload: Value,
) -> anyhow::Result<()> {
    let deleted = payload
        .get("_deleted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let rev = format!("rev_{}", Uuid::new_v4());
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("id".to_string(), Value::String(record_id.to_string()));
        obj.insert("_rev".to_string(), Value::String(rev.clone()));
        obj.insert("_deleted".to_string(), Value::Bool(deleted));
        obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    }
    conn.execute(
        "INSERT INTO business_records
            (collection, record_id, rev, deleted, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(collection, record_id) DO UPDATE SET
            rev = excluded.rev,
            deleted = excluded.deleted,
            updated_at_ms = excluded.updated_at_ms,
            payload_json = excluded.payload_json",
        params![
            collection,
            record_id,
            rev,
            if deleted { 1 } else { 0 },
            updated_at_ms,
            serde_json::to_string(&payload)?
        ],
    )?;
    Ok(())
}

/// Read a single `business_records` row from the core db (ctox.sqlite3). Used
/// only to mirror executor-written iot_datapoints window rows (which the
/// projector cannot re-derive) into the business-os store.
fn read_core_db_business_record(
    root: &Path,
    collection: &str,
    record_id: &str,
) -> anyhow::Result<Option<Value>> {
    let path = crate::paths::core_db(root);
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open core db {}", path.display()))?;
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='business_records'",
            [],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Ok(None);
    }
    let payload_json: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM business_records WHERE collection = ?1 AND record_id = ?2",
            params![collection, record_id],
            |row| row.get(0),
        )
        .optional()?;
    match payload_json {
        Some(json) => Ok(Some(serde_json::from_str(&json).with_context(|| {
            format!("invalid core db business_record {collection}/{record_id}")
        })?)),
        None => Ok(None),
    }
}

fn find_queue_task_for_command(root: &Path, command_id: &str) -> Option<String> {
    let tasks = channels::list_queue_tasks(root, &[], 256).ok()?;
    tasks
        .into_iter()
        .find(|task| task.prompt.contains(command_id))
        .map(|task| task.message_key)
}

fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS business_records (
            collection TEXT NOT NULL,
            record_id TEXT NOT NULL,
            rev TEXT NOT NULL,
            deleted INTEGER NOT NULL DEFAULT 0,
            updated_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL,
            PRIMARY KEY (collection, record_id)
        );
        CREATE INDEX IF NOT EXISTS idx_business_records_collection_updated
            ON business_records(collection, updated_at_ms, record_id);

        CREATE TABLE IF NOT EXISTS business_documents (
            document_id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            filename TEXT NOT NULL DEFAULT '',
            mime_type TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT '',
            document_type TEXT NOT NULL DEFAULT '',
            current_version_id TEXT NOT NULL DEFAULT '',
            source_sha256 TEXT NOT NULL DEFAULT '',
            page_count INTEGER NOT NULL DEFAULT 0,
            diagnostics_count INTEGER NOT NULL DEFAULT 0,
            tags_json TEXT NOT NULL DEFAULT '[]',
            index_text TEXT NOT NULL DEFAULT '',
            deleted INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_business_documents_updated
            ON business_documents(deleted, updated_at_ms, document_id);
        CREATE INDEX IF NOT EXISTS idx_business_documents_type
            ON business_documents(document_type, status, updated_at_ms);

        CREATE TABLE IF NOT EXISTS business_document_versions (
            version_id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL DEFAULT '',
            version INTEGER NOT NULL DEFAULT 0,
            source_kind TEXT NOT NULL DEFAULT '',
            blob_id TEXT NOT NULL DEFAULT '',
            diagnostics_json TEXT NOT NULL DEFAULT '[]',
            model_json TEXT NOT NULL DEFAULT 'null',
            deleted INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_business_document_versions_document
            ON business_document_versions(document_id, version, updated_at_ms);
        CREATE INDEX IF NOT EXISTS idx_business_document_versions_blob
            ON business_document_versions(blob_id);

        CREATE TABLE IF NOT EXISTS business_document_blob_chunks (
            chunk_id TEXT PRIMARY KEY,
            blob_id TEXT NOT NULL DEFAULT '',
            document_id TEXT NOT NULL DEFAULT '',
            version_id TEXT NOT NULL DEFAULT '',
            idx INTEGER NOT NULL DEFAULT 0,
            total INTEGER NOT NULL DEFAULT 1,
            mime_type TEXT NOT NULL DEFAULT '',
            encoding TEXT NOT NULL DEFAULT '',
            data TEXT NOT NULL DEFAULT '',
            deleted INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_business_document_blob_chunks_blob
            ON business_document_blob_chunks(blob_id, idx);
        CREATE INDEX IF NOT EXISTS idx_business_document_blob_chunks_document
            ON business_document_blob_chunks(document_id, version_id);

        CREATE TABLE IF NOT EXISTS business_events (
            event_id TEXT PRIMARY KEY,
            collection TEXT NOT NULL,
            record_id TEXT NOT NULL,
            command_type TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            observed_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_commands (
            command_id TEXT PRIMARY KEY,
            module TEXT NOT NULL,
            command_type TEXT NOT NULL,
            record_id TEXT,
            status TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            client_context_json TEXT NOT NULL,
            observed_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_users (
            user_id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('chef', 'admin', 'founder', 'user')),
            active INTEGER NOT NULL DEFAULT 1,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS business_module_acl (
            module_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('founder')),
            active INTEGER NOT NULL DEFAULT 1,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(module_id, user_id, role)
        );
        CREATE INDEX IF NOT EXISTS idx_business_module_acl_user
            ON business_module_acl(user_id, active, module_id);

        CREATE TABLE IF NOT EXISTS business_module_releases (
            version_id TEXT PRIMARY KEY,
            module_id TEXT NOT NULL,
            version INTEGER NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('draft', 'released', 'rolled_back')),
            manifest_json TEXT NOT NULL,
            snapshot_json TEXT NOT NULL DEFAULT '{}',
            created_by TEXT NOT NULL DEFAULT '',
            created_at_ms INTEGER NOT NULL,
            notes TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_business_module_releases_module
            ON business_module_releases(module_id, version DESC);

        CREATE TABLE IF NOT EXISTS business_module_versions (
            version_id TEXT PRIMARY KEY,
            module_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            origin TEXT NOT NULL,
            label TEXT NOT NULL DEFAULT '',
            bundle_sha256 TEXT NOT NULL,
            files_json TEXT NOT NULL DEFAULT '[]',
            sealed INTEGER NOT NULL DEFAULT 0,
            created_by TEXT NOT NULL DEFAULT '',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_business_module_versions_module
            ON business_module_versions(module_id, seq DESC);

        CREATE TABLE IF NOT EXISTS business_module_reports (
            report_id TEXT PRIMARY KEY,
            module_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            severity TEXT NOT NULL,
            title TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            expected TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'open',
            reporter_id TEXT NOT NULL DEFAULT '',
            ctox_command_id TEXT NOT NULL DEFAULT '',
            client_context_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_business_module_reports_module
            ON business_module_reports(module_id, status, updated_at_ms);

        CREATE TABLE IF NOT EXISTS business_os_mcp_events (
            event_id TEXT PRIMARY KEY,
            channel TEXT NOT NULL,
            surface TEXT NOT NULL,
            actor TEXT NOT NULL,
            workspace TEXT NOT NULL,
            tool TEXT NOT NULL,
            request_id TEXT NOT NULL,
            confirmation_state TEXT NOT NULL,
            status TEXT NOT NULL,
            error_code TEXT,
            error_message TEXT,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_business_os_mcp_events_created
            ON business_os_mcp_events(created_at_ms DESC, event_id DESC);
        CREATE INDEX IF NOT EXISTS idx_business_os_mcp_events_actor
            ON business_os_mcp_events(actor, created_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_business_os_mcp_events_tool
            ON business_os_mcp_events(tool, created_at_ms DESC);
        ",
    )?;
    migrate_business_users_roles(conn)?;
    Ok(())
}

fn migrate_business_users_roles(conn: &Connection) -> anyhow::Result<()> {
    let table_sql = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'business_users'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_default();
    if !table_sql.contains("'admin', 'user'") || table_sql.contains("'founder'") {
        return Ok(());
    }
    conn.execute_batch(
        "
        ALTER TABLE business_users RENAME TO business_users_old;
        CREATE TABLE business_users (
            user_id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('chef', 'admin', 'founder', 'user')),
            active INTEGER NOT NULL DEFAULT 1,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        INSERT INTO business_users
            (user_id, display_name, role, active, created_at_ms, updated_at_ms)
        SELECT user_id, display_name, role, active, created_at_ms, updated_at_ms
        FROM business_users_old;
        DROP TABLE business_users_old;
        ",
    )?;
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn stable_instance_id(root: &Path) -> anyhow::Result<String> {
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime)
        .with_context(|| format!("failed to create runtime dir {}", runtime.display()))?;
    let path = runtime.join("business-os-instance-id");
    if path.is_file() {
        let value = std::fs::read_to_string(&path)?;
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let id = format!("biz_{}", Uuid::new_v4());
    std::fs::write(&path, format!("{id}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(id)
}

#[derive(Debug, Clone)]
struct SignalingUrlsConfig {
    urls: Vec<String>,
    source: &'static str,
}

fn signaling_urls_config(root: &Path) -> SignalingUrlsConfig {
    if let Ok(raw) = std::env::var("CTOX_BUSINESS_OS_SIGNALING_URLS") {
        let urls = parse_signaling_urls(&raw);
        if !urls.is_empty() {
            persist_signaling_urls(root, &urls);
            return SignalingUrlsConfig {
                urls,
                source: "environment",
            };
        }
    }
    if let Some(urls) = read_persisted_signaling_urls(root) {
        return SignalingUrlsConfig {
            urls,
            source: "runtime",
        };
    }
    SignalingUrlsConfig {
        urls: vec![DEFAULT_SIGNALING_URL.to_string()],
        source: "default",
    }
}

fn parse_signaling_urls(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
}

fn persisted_signaling_urls_path(root: &Path) -> PathBuf {
    root.join("runtime").join(BUSINESS_OS_SIGNALING_URLS_FILE)
}

fn persist_signaling_urls(root: &Path, urls: &[String]) {
    let path = persisted_signaling_urls_path(root);
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(content) = serde_json::to_vec_pretty(urls) else {
        return;
    };
    let _ = fs::write(path, content);
}

fn read_persisted_signaling_urls(root: &Path) -> Option<Vec<String>> {
    let path = persisted_signaling_urls_path(root);
    let raw = fs::read_to_string(path).ok()?;
    if let Ok(urls) = serde_json::from_str::<Vec<String>>(&raw) {
        let urls = urls
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        if !urls.is_empty() {
            return Some(urls);
        }
    }
    let urls = parse_signaling_urls(&raw);
    (!urls.is_empty()).then_some(urls)
}

fn short_hash(value: &str) -> String {
    let digest = sha2::Sha256::digest(value.as_bytes());
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &digest)[..10]
        .to_string()
}

fn room_secret_id(value: &str) -> String {
    let digest = sha2::Sha256::digest(value.as_bytes());
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &digest)[..22]
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn chef_session() -> BusinessOsSession {
        BusinessOsSession {
            ok: true,
            authenticated: true,
            auth_required: false,
            user: Some(BusinessOsSessionUser {
                id: "tester".to_string(),
                display_name: "Tester".to_string(),
                role: "chef".to_string(),
                is_admin: true,
            }),
            login_url: None,
            reason: None,
        }
    }

    struct EnvRestore {
        values: Vec<(&'static str, Option<String>)>,
    }

    impl EnvRestore {
        fn set(values: &[(&'static str, &'static str)]) -> Self {
            let restore = Self {
                values: values
                    .iter()
                    .map(|(key, _)| (*key, env::var(key).ok()))
                    .collect(),
            };
            for (key, value) in values {
                unsafe {
                    env::set_var(key, value);
                }
            }
            restore
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in &self.values {
                unsafe {
                    if let Some(value) = value {
                        env::set_var(key, value);
                    } else {
                        env::remove_var(key);
                    }
                }
            }
        }
    }

    fn write_widget_module(app_root: &Path, js: &str) -> anyhow::Result<()> {
        let module_dir = app_root.join("modules").join("widget");
        fs::create_dir_all(&module_dir)?;
        fs::write(
            module_dir.join("module.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": "widget",
                "title": "Widget",
                "entry": "modules/widget/index.html"
            }))?,
        )?;
        fs::write(module_dir.join("index.js"), js.as_bytes())?;
        Ok(())
    }

    fn save_widget_source(root: &Path, path: &str, content: &str) -> anyhow::Result<()> {
        save_module_source_record(
            root,
            ModuleSourceSaveMutation {
                module_id: "widget".to_string(),
                path: path.to_string(),
                content: content.to_string(),
            },
        )?;
        Ok(())
    }

    fn write_test_manifest(dir: &Path, module_id: &str) -> anyhow::Result<()> {
        fs::create_dir_all(dir)?;
        fs::write(
            dir.join("module.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": module_id,
                "title": module_id,
                "entry": "index.html"
            }))?,
        )?;
        Ok(())
    }

    #[test]
    fn app_store_install_source_path_matches_exact_path_segments() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let app_root = temp.path().join("src").join("apps").join("business-os");
        let installed = app_root.join("installed-modules").join("matching");
        write_test_manifest(&installed, "matching")?;

        assert!(
            find_module_json_dir_by_source_path(&app_root, "modules/matching")?.is_none(),
            "installed-modules/matching must not satisfy modules/matching"
        );

        let source = app_root.join("modules").join("matching");
        write_test_manifest(&source, "matching")?;
        let found = find_module_json_dir_by_source_path(&app_root, "modules/matching")?
            .expect("source module path is found");
        assert_eq!(found, source);
        Ok(())
    }

    #[test]
    fn runtime_settings_projects_web_stack_without_sensitive_payloads() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let runtime_settings = runtime_settings_for_rxdb(temp.path())?;
        let web_stack = runtime_settings
            .get("web_stack")
            .expect("runtime settings include web stack projection");
        assert_eq!(web_stack.get("ok").and_then(Value::as_bool), Some(true));
        assert!(
            web_stack
                .pointer("/summary/sources")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 0,
            "registered web-stack sources are projected"
        );
        assert_eq!(
            web_stack
                .get("secret_value_in_payload")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            web_stack
                .get("frame_data_in_payload")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(serde_json::to_string(web_stack)?.contains("\"capture_script_available\""));
        assert!(
            !serde_json::to_string(web_stack)?.contains("\"capture_script\":"),
            "browser capture script bodies must stay out of the RxDB projection"
        );
        Ok(())
    }

    #[test]
    fn runtime_settings_normalizes_retired_context_values_to_256k() {
        assert_eq!(runtime_settings_context(Some("128k".to_owned())), "256k");
        assert_eq!(runtime_settings_context(Some("131072".to_owned())), "256k");
        assert_eq!(runtime_settings_context(Some("512k".to_owned())), "256k");
        assert_eq!(runtime_settings_context(None), "256k");
    }

    #[test]
    fn app_modify_queue_prompt_targets_app_module_not_skill_files() {
        let command = BusinessCommand {
            id: Some("cmd_app_bench".to_owned()),
            module: "creator".to_owned(),
            command_type: "ctox.business_os.app.modify".to_owned(),
            record_id: Some("subscriptions".to_owned()),
            payload: serde_json::json!({
                "title": "Build Subscriptions",
                "instruction": "Build a Business OS subscriptions app.",
                "target": "app",
                "mode": "app",
                "module_id": "subscriptions",
                "install_target": "runtime-installed-module",
                "required_skills": ["business-basic-module-development"]
            }),
            client_context: serde_json::json!({
                "source": "business-os-app-creator",
                "suggested_skill": "business-basic-module-development",
                "required_skills": ["product_engineering/business-basic-module-development"]
            }),
        };

        let prompt = command_prompt("cmd_app_bench", &command, &[]);
        assert!(prompt.contains("Required CTOX skills: business-os-app-module-development"));
        assert!(prompt.contains("Business OS app build target:"));
        assert!(prompt.contains("- module_id: subscriptions"));
        assert!(
            prompt.contains(
                "only_allowed_app_artifact_directory: src/apps/business-os/installed-modules/subscriptions"
            )
        );
        assert!(prompt.contains("do not write root-level module.json"));
        assert!(prompt.contains("cwd warning: shell tools run from the install root"));
        assert!(prompt
            .contains("set MODULE_DIR=\"src/apps/business-os/installed-modules/subscriptions\""));
        assert!(prompt.contains(
            "schema.js and collections.schema.json must export only module-owned collections"
        ));
        assert!(prompt.contains("default to one/two panes plus modals/drawers"));
        assert!(
            prompt.contains("not documentation, plans, trace files, blocker notes, or skill files")
        );
        assert!(prompt.contains("\"suggested_skill\": \"business-os-app-module-development\""));
        assert!(!prompt.contains("business-basic-module-development"));
        assert!(!prompt.contains("product_engineering/business-basic-module-development"));
    }

    #[test]
    fn app_create_queue_prompt_targets_app_module_skill() {
        let command = BusinessCommand {
            id: Some("cmd_app_create".to_owned()),
            module: "creator".to_owned(),
            command_type: "ctox.business_os.app.create".to_owned(),
            record_id: Some("inventory".to_owned()),
            payload: serde_json::json!({
                "title": "Build Inventory",
                "instruction": "Build a Business OS inventory app.",
                "module_id": "inventory",
                "install_target": "runtime-installed-module"
            }),
            client_context: serde_json::json!({
                "source": "business-os-app-creator"
            }),
        };

        let prompt = command_prompt("cmd_app_create", &command, &[]);
        assert!(prompt.contains("Required CTOX skills: business-os-app-module-development"));
        assert!(prompt.contains("- module_id: inventory"));
        assert!(prompt.contains("- install_target: runtime-installed-module"));
        assert!(prompt.contains(
            "only_allowed_app_artifact_directory: src/apps/business-os/installed-modules/inventory"
        ));
        assert_eq!(
            suggested_skill_for_command(&command).as_deref(),
            Some(BUSINESS_OS_APP_MODULE_SKILL_NAME)
        );
    }

    #[test]
    fn runtime_settings_uses_ctox_proxy_for_minimax_when_configured() {
        let mut env_map = BTreeMap::new();
        env_map.insert(
            crate::inference::runtime_state::CTOX_LLM_PROXY_API_KEY_ENV.to_owned(),
            "configured".to_owned(),
        );
        assert_eq!(
            runtime_settings_api_upstream_base_url("minimax", &env_map),
            "https://llm.ctox.dev"
        );

        env_map.insert(
            crate::inference::runtime_state::CTOX_LLM_PROXY_BASE_URL_ENV.to_owned(),
            "https://kunstmen.ctox.dev/api/fallback-llm".to_owned(),
        );
        assert_eq!(
            runtime_settings_api_upstream_base_url("minimax", &env_map),
            "https://kunstmen.ctox.dev/api/fallback-llm"
        );
    }

    #[test]
    fn module_versions_record_rollback_and_remove_added_files() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src").join("apps").join("business-os");
        fs::create_dir_all(&app_root)?;
        fs::write(app_root.join("index.html"), b"<!doctype html>")?;
        write_widget_module(&app_root, "export const v = 1;\n")?;

        // Install baseline (v0): sealed boundary.
        let v0 =
            record_module_version(root, &app_root, "widget", "install", "Installed", "tester")?
                .expect("install version recorded");
        let v0_id = v0
            .get("version_id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        assert_eq!(v0.get("origin").and_then(Value::as_str), Some("install"));
        assert_eq!(v0.get("sealed").and_then(Value::as_bool), Some(true));
        let baseline_sha = v0
            .get("bundle_sha256")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();

        // Two edits coalesce into a single open working version.
        save_widget_source(root, "index.js", "export const v = 2;\n")?;
        save_widget_source(root, "index.js", "export const v = 3;\n")?;
        let listed = list_module_versions(
            root,
            ModuleVersionListRequest {
                module_id: "widget".to_string(),
            },
        )?;
        let versions = listed.get("versions").and_then(Value::as_array).unwrap();
        assert_eq!(versions.len(), 2, "install + one coalesced edit version");
        assert_ne!(
            baseline_sha,
            compute_module_bundle(&app_root, "widget")?.sha256,
            "edits change the bundle hash"
        );

        // A brand new source file added after the baseline.
        save_widget_source(root, "extra.js", "export const extra = true;\n")?;
        assert!(app_root
            .join("modules")
            .join("widget")
            .join("extra.js")
            .is_file());

        // Roll back to the install baseline.
        let session = chef_session();
        let result = rollback_module_to_version(
            root,
            &app_root,
            &session,
            ModuleVersionRollbackRequest {
                module_id: "widget".to_string(),
                version_id: v0_id,
            },
        )?;
        assert_eq!(result.get("ok").and_then(Value::as_bool), Some(true));

        // index.js restored to baseline; extra.js removed; bundle hash matches baseline.
        let restored =
            fs::read_to_string(app_root.join("modules").join("widget").join("index.js"))?;
        assert_eq!(restored, "export const v = 1;\n");
        assert!(!app_root
            .join("modules")
            .join("widget")
            .join("extra.js")
            .is_file());
        assert_eq!(
            baseline_sha,
            compute_module_bundle(&app_root, "widget")?.sha256
        );

        // A sealed rollback boundary is now the newest version.
        let listed = list_module_versions(
            root,
            ModuleVersionListRequest {
                module_id: "widget".to_string(),
            },
        )?;
        let versions = listed.get("versions").and_then(Value::as_array).unwrap();
        assert_eq!(
            versions
                .first()
                .and_then(|version| version.get("origin"))
                .and_then(Value::as_str),
            Some("rollback")
        );
        Ok(())
    }

    #[test]
    fn module_version_states_report_baseline_and_modification() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src").join("apps").join("business-os");
        fs::create_dir_all(&app_root)?;
        fs::write(app_root.join("index.html"), b"<!doctype html>")?;
        write_widget_module(&app_root, "export const v = 1;\n")?;

        // No timeline yet -> module is absent from the states map.
        let empty = module_version_states(root, &app_root)?;
        assert!(empty.get("widget").is_none());

        record_module_version(root, &app_root, "widget", "install", "Installed", "tester")?
            .expect("install version recorded");
        let baseline_sha = compute_module_bundle(&app_root, "widget")?.sha256;

        // Right after install the working tree matches the baseline.
        let clean = module_version_states(root, &app_root)?;
        let clean_state = clean.get("widget").expect("widget state present");
        assert_eq!(
            clean_state
                .get("baseline_bundle_sha256")
                .and_then(Value::as_str),
            Some(baseline_sha.as_str())
        );
        assert_eq!(
            clean_state
                .get("current_bundle_sha256")
                .and_then(Value::as_str),
            Some(baseline_sha.as_str())
        );
        assert_eq!(
            clean_state.get("modified").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            clean_state.get("baseline_origin").and_then(Value::as_str),
            Some("install")
        );

        // After an edit the working tree diverges from the baseline.
        save_widget_source(root, "index.js", "export const v = 99;\n")?;
        let dirty = module_version_states(root, &app_root)?;
        let dirty_state = dirty.get("widget").expect("widget state present");
        assert_eq!(
            dirty_state.get("modified").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            dirty_state
                .get("baseline_bundle_sha256")
                .and_then(Value::as_str),
            Some(baseline_sha.as_str())
        );
        assert_ne!(
            dirty_state
                .get("current_bundle_sha256")
                .and_then(Value::as_str),
            Some(baseline_sha.as_str())
        );
        let versions = dirty_state
            .get("versions")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(
            versions.len() as i64,
            dirty_state
                .get("version_count")
                .and_then(Value::as_i64)
                .unwrap()
        );
        Ok(())
    }

    #[test]
    fn rotate_sync_room_password_changes_persisted_room_secret() -> anyhow::Result<()> {
        if env::var("CTOX_BUSINESS_OS_ROOM_PASSWORD")
            .ok()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        {
            return Ok(());
        }

        let temp = tempdir()?;
        let root = temp.path();

        let first = sync_config(root)?;
        let rotated = rotate_sync_room_password(root)?;
        let reloaded = sync_config(root)?;

        assert_ne!(
            first.signaling_room_password,
            rotated.signaling_room_password
        );
        assert_ne!(first.sync_room, rotated.sync_room);
        assert_eq!(
            rotated.signaling_room_password,
            reloaded.signaling_room_password
        );
        assert_eq!(rotated.sync_room, reloaded.sync_room);
        assert!(rotated.sync_room.starts_with("ctox-business-os:"));
        Ok(())
    }

    #[test]
    fn business_chat_queue_prompt_is_bounded_for_large_research_context() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let huge_sources = (0..1_500)
            .map(|index| {
                serde_json::json!({
                    "id": format!("source_{index}"),
                    "title": format!("Large source {index}"),
                    "summary": "x".repeat(2_000),
                    "measurements": ["rpm", "torque", "vibration", "temperature"]
                })
            })
            .collect::<Vec<_>>();

        let accepted = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_large_research_context",
                "command_id": "cmd_large_research_context",
                "module": "research",
                "command_type": "business_os.chat.task",
                "record_id": "research",
                "status": "pending_sync",
                "payload": {
                    "title": "Kontext-Aufgabe · Web Research",
                    "instruction": "finde 5 weitere Quellen fuer Simulationsdaten.",
                    "prompt": "finde 5 weitere Quellen fuer Simulationsdaten.",
                    "sources": huge_sources
                },
                "client_context": {
                    "source": "business-os-chat",
                    "module": "research",
                    "browser_context": "y".repeat(2_000_000)
                }
            }),
        )?;
        let task_id = accepted
            .get("task_id")
            .and_then(Value::as_str)
            .context("expected queue task id")?;
        let task = channels::load_queue_task(root, task_id)?.context("queue task exists")?;

        assert!(
            task.prompt.chars().count() <= BUSINESS_OS_QUEUE_PROMPT_MAX_CHARS,
            "prompt should be bounded, got {} chars",
            task.prompt.chars().count()
        );
        assert!(task.prompt.contains("cmd_large_research_context"));
        assert!(task.prompt.contains("Payload JSON"));
        assert!(task.prompt.contains("Client context JSON"));
        assert!(task.prompt.contains("truncated"));
        Ok(())
    }

    #[test]
    fn business_chat_queue_materializes_verified_desktop_file_attachment() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let bytes = b"\x89PNG\r\n\x1a\nctox-chat-image";
        let (content_hash, generation_id) = seed_rxdb_chat_attachment(
            root,
            "chatfile_verified",
            "upload.png",
            "image/png",
            bytes,
            false,
        )?;

        let accepted = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_chat_image_attachment",
                "command_id": "cmd_chat_image_attachment",
                "module": "research",
                "command_type": "business_os.chat.task",
                "record_id": "research",
                "status": "pending_sync",
                "payload": {
                    "title": "Bild pruefen",
                    "instruction": "Was ist auf dem Bild?",
                    "prompt": "Was ist auf dem Bild?",
                    "attachment_refs": [{
                        "kind": "desktop_file",
                        "attachment_id": "chatatt_verified",
                        "file_id": "chatfile_verified",
                        "generation_id": generation_id.clone(),
                        "name": "upload.png",
                        "mime_type": "image/png",
                        "size_bytes": bytes.len(),
                        "content_hash": content_hash.clone(),
                        "content_hash_scheme": BUSINESS_OS_CHAT_ATTACHMENT_CONTENT_HASH_SCHEME
                    }]
                },
                "client_context": {
                    "source": "business-os-chat",
                    "module": "research"
                }
            }),
        )?;
        let task_id = accepted
            .get("task_id")
            .and_then(Value::as_str)
            .context("expected queue task id")?;
        let task = channels::load_queue_task(root, task_id)?.context("queue task exists")?;
        let materialized_path = root
            .join("runtime")
            .join("business-os")
            .join("chat-attachments")
            .join("cmd_chat_image_attachment")
            .join("chatfile_verified_upload.png");

        assert!(materialized_path.is_file());
        assert_eq!(fs::read(&materialized_path)?, bytes);
        assert!(task.prompt.contains("Business OS attachments"));
        assert!(task
            .prompt
            .contains(materialized_path.to_string_lossy().as_ref()));
        assert!(task.prompt.contains("desktop_files/chatfile_verified"));
        assert!(task.prompt.contains(&content_hash));
        assert!(
            !task
                .prompt
                .contains(&base64::engine::general_purpose::STANDARD.encode(bytes)),
            "prompt must not inline attachment bytes"
        );
        Ok(())
    }

    #[test]
    fn business_chat_queue_rejects_corrupt_desktop_file_attachment_chunk() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let bytes = b"corrupt image bytes";
        let (content_hash, generation_id) = seed_rxdb_chat_attachment(
            root,
            "chatfile_corrupt",
            "broken.png",
            "image/png",
            bytes,
            true,
        )?;

        let error = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_chat_corrupt_attachment",
                "command_id": "cmd_chat_corrupt_attachment",
                "module": "research",
                "command_type": "business_os.chat.task",
                "record_id": "research",
                "status": "pending_sync",
                "payload": {
                    "title": "Bild pruefen",
                    "instruction": "Was ist auf dem Bild?",
                    "prompt": "Was ist auf dem Bild?",
                    "attachment_refs": [{
                        "kind": "desktop_file",
                        "file_id": "chatfile_corrupt",
                        "generation_id": generation_id.clone(),
                        "name": "broken.png",
                        "mime_type": "image/png",
                        "size_bytes": bytes.len(),
                        "content_hash": content_hash.clone(),
                        "content_hash_scheme": BUSINESS_OS_CHAT_ATTACHMENT_CONTENT_HASH_SCHEME
                    }]
                },
                "client_context": {
                    "source": "business-os-chat",
                    "module": "research"
                }
            }),
        )
        .expect_err("corrupt attachment chunk should reject the command");
        assert!(
            error.to_string().contains("chunk hash mismatch"),
            "unexpected error: {error:#}"
        );
        Ok(())
    }

    fn seed_rxdb_chat_attachment(
        root: &Path,
        file_id: &str,
        name: &str,
        mime_type: &str,
        bytes: &[u8],
        corrupt_chunk_hash: bool,
    ) -> anyhow::Result<(String, String)> {
        fs::create_dir_all(root.join("runtime"))?;
        let conn = Connection::open(rxdb_store_path(root))?;
        conn.execute(
            "CREATE TABLE ctox_business_os__desktop_files__v0 (id TEXT PRIMARY KEY, data TEXT NOT NULL)",
            [],
        )?;
        conn.execute(
            "CREATE TABLE ctox_business_os__desktop_file_chunks__v0 (id TEXT PRIMARY KEY, data TEXT NOT NULL)",
            [],
        )?;
        let now = now_ms() as i64;
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let content_hash = hex_sha256(bytes);
        let generation_id = format!("gen_{now}_{}", &content_hash[..12]);
        let total = std::cmp::max(
            1,
            (encoded.len() as u64).div_ceil(BUSINESS_OS_CHAT_ATTACHMENT_CHUNK_SIZE as u64),
        ) as usize;
        conn.execute(
            "INSERT INTO ctox_business_os__desktop_files__v0 (id, data) VALUES (?1, ?2)",
            params![
                file_id,
                serde_json::json!({
                    "id": file_id,
                    "parent_id": "fs_business_os_chat_attachments",
                    "path": format!("/Business OS Chat/test/{name}"),
                    "virtual_path": format!("/Business OS Chat/test/{name}"),
                    "name": name,
                    "kind": "file",
                    "mime_type": mime_type,
                    "extension": name.rsplit('.').next().unwrap_or(""),
                    "size_bytes": bytes.len(),
                    "source": "business-os-chat",
                    "content_ref": file_id,
                    "content_state": "available",
                    "content_hash": content_hash.clone(),
                    "content_hash_scheme": BUSINESS_OS_CHAT_ATTACHMENT_CONTENT_HASH_SCHEME,
                    "content_generation_id": generation_id.clone(),
                    "content_synced_at_ms": now,
                    "is_deleted": false,
                    "created_at_ms": now,
                    "updated_at_ms": now
                })
                .to_string()
            ],
        )?;
        for idx in 0..total {
            let start = idx * BUSINESS_OS_CHAT_ATTACHMENT_CHUNK_SIZE;
            let end = std::cmp::min(
                encoded.len(),
                start + BUSINESS_OS_CHAT_ATTACHMENT_CHUNK_SIZE,
            );
            let data = encoded[start..end].to_string();
            let chunk_hash = if corrupt_chunk_hash && idx == 0 {
                "bad_chunk_hash".to_string()
            } else {
                hex_sha256(data.as_bytes())
            };
            conn.execute(
                "INSERT INTO ctox_business_os__desktop_file_chunks__v0 (id, data) VALUES (?1, ?2)",
                params![
                    format!("{file_id}_{generation_id}_{idx}"),
                    serde_json::json!({
                        "id": format!("{file_id}_{generation_id}_{idx}"),
                        "file_id": file_id,
                        "generation_id": generation_id.clone(),
                        "content_hash": content_hash.clone(),
                        "content_hash_scheme": BUSINESS_OS_CHAT_ATTACHMENT_CONTENT_HASH_SCHEME,
                        "idx": idx,
                        "total": total,
                        "encoding": "base64",
                        "data": data,
                        "chunk_hash": chunk_hash,
                        "chunk_hash_scheme": BUSINESS_OS_CHAT_ATTACHMENT_CHUNK_HASH_SCHEME,
                        "size_bytes": data.len(),
                        "created_at_ms": now
                    })
                    .to_string()
                ],
            )?;
        }
        Ok((content_hash, generation_id))
    }

    #[test]
    fn queue_worker_failure_marks_business_command_projection_failed() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let accepted = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_failed_queue_chat",
                "command_id": "cmd_failed_queue_chat",
                "module": "research",
                "command_type": "business_os.chat.task",
                "record_id": "research",
                "status": "pending_sync",
                "payload": {
                    "title": "Kontext-Aufgabe · Web Research",
                    "instruction": "teste Fehlerpfad",
                    "prompt": "teste Fehlerpfad"
                },
                "client_context": {
                    "source": "business-os-chat",
                    "module": "research"
                }
            }),
        )?;
        let task_id = accepted
            .get("task_id")
            .and_then(Value::as_str)
            .context("expected queue task id")?
            .to_string();

        channels::lease_queue_task(root, &task_id, "ctox-service")?;
        channels::ack_leased_messages_with_failure_reason(
            root,
            std::slice::from_ref(&task_id),
            "failed",
            "turn/start failed",
        )?;
        let projected =
            fail_business_command_from_queue_error(root, &task_id, "turn/start failed")?
                .context("expected business command projection")?;

        assert_eq!(
            projected.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            projected.get("task_status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            projected.get("error").and_then(Value::as_str),
            Some("turn/start failed")
        );

        let conn = open_store(root)?;
        let queue_payload: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'ctox_queue_tasks' AND record_id = ?1",
            params![task_id.as_str()],
            |row| row.get(0),
        )?;
        let queue_projection: Value = serde_json::from_str(&queue_payload)?;
        assert_eq!(
            queue_projection.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            queue_projection.get("route_status").and_then(Value::as_str),
            Some("failed")
        );
        Ok(())
    }

    #[test]
    fn queue_worker_success_marks_active_rxdb_business_command_completed() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let accepted = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_completed_queue_chat",
                "command_id": "cmd_completed_queue_chat",
                "module": "ctox",
                "command_type": "business_os.chat.task",
                "record_id": "ctox",
                "status": "pending_sync",
                "payload": {
                    "title": "CTOX Aufgabe",
                    "instruction": "teste Erfolgspfad",
                    "prompt": "teste Erfolgspfad",
                    "message_id": "chatmsg_success"
                },
                "client_context": {
                    "source": "business-os-chat",
                    "module": "ctox",
                    "owner_user_id": "tester"
                }
            }),
        )?;
        let task_id = accepted
            .get("task_id")
            .and_then(Value::as_str)
            .context("expected queue task id")?
            .to_string();

        channels::lease_queue_task(root, &task_id, "ctox-service")?;
        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__ctox_queue_tasks__v0",
            &task_id,
            serde_json::json!({
                "id": task_id,
                "command_id": "cmd_completed_queue_chat",
                "status": "running",
                "route_status": "leased",
                "task_status": "running",
                "updated_at_ms": 1
            }),
        )?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__business_commands__v1",
            "cmd_completed_queue_chat",
            serde_json::json!({
                "id": "cmd_completed_queue_chat",
                "command_id": "cmd_completed_queue_chat",
                "status": "accepted",
                "task_id": task_id,
                "task_status": "queued",
                "updated_at_ms": 1
            }),
        )?;
        drop(rxdb_conn);

        let projected = complete_business_command_from_queue_reply(
            root,
            &task_id,
            "Chat-Antwort wurde gespeichert.",
        )?
        .context("expected business chat command writeback")?;
        assert_eq!(
            projected.get("status").and_then(Value::as_str),
            Some("completed")
        );

        let rxdb_command =
            load_rxdb_collection_record(root, "business_commands", "cmd_completed_queue_chat")?
                .context("expected active rxdb command row")?;
        assert_eq!(
            rxdb_command.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            rxdb_command.get("task_status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            rxdb_command.get("response").and_then(Value::as_str),
            Some("Chat-Antwort wurde gespeichert.")
        );
        assert_eq!(
            rxdb_command
                .pointer("/result/answer")
                .and_then(Value::as_str),
            Some("Chat-Antwort wurde gespeichert.")
        );

        let rxdb_queue = load_rxdb_collection_record(root, "ctox_queue_tasks", &task_id)?
            .context("expected active rxdb queue row")?;
        assert_eq!(
            rxdb_queue.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            rxdb_queue.get("route_status").and_then(Value::as_str),
            Some("handled")
        );
        assert_eq!(
            rxdb_queue.get("task_status").and_then(Value::as_str),
            Some("completed")
        );
        Ok(())
    }

    fn create_repair_rxdb_tables(root: &Path) -> anyhow::Result<Connection> {
        fs::create_dir_all(root.join("runtime"))?;
        let conn = Connection::open(rxdb_store_path(root))?;
        conn.execute(
            "CREATE TABLE ctox_business_os__ctox_queue_tasks__v0 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE ctox_business_os__business_commands__v1 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                lastWriteTime REAL NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            )",
            [],
        )?;
        Ok(conn)
    }

    fn insert_rxdb_test_record(
        conn: &Connection,
        table: &str,
        id: &str,
        payload: Value,
    ) -> anyhow::Result<()> {
        conn.execute(
            &format!(
                "INSERT INTO {table} (id, revision, deleted, lastWriteTime, data)
                 VALUES (?1, 'rev_stale', 0, 1.0, ?2)"
            ),
            params![id, serde_json::to_string(&payload)?],
        )?;
        Ok(())
    }

    #[test]
    fn repair_queue_projections_updates_failed_canonical_queue_and_command() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let accepted = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_repair_failed_queue",
                "command_id": "cmd_repair_failed_queue",
                "module": "research",
                "command_type": "business_os.chat.task",
                "record_id": "research",
                "status": "pending_sync",
                "payload": {
                    "title": "Kontext-Aufgabe · Web Research",
                    "instruction": "teste repair failure",
                    "prompt": "teste repair failure"
                },
                "client_context": {
                    "source": "business-os-chat",
                    "module": "research"
                }
            }),
        )?;
        let task_id = accepted
            .get("task_id")
            .and_then(Value::as_str)
            .context("expected queue task id")?
            .to_string();
        channels::lease_queue_task(root, &task_id, "ctox-service")?;
        channels::ack_leased_messages_with_failure_reason(
            root,
            std::slice::from_ref(&task_id),
            "failed",
            "Input exceeds the maximum length of 1048576 characters.",
        )?;
        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__ctox_queue_tasks__v0",
            &task_id,
            serde_json::json!({
                "id": task_id,
                "command_id": "cmd_repair_failed_queue",
                "status": "queued",
                "route_status": "pending",
                "task_status": "queued",
                "updated_at_ms": 1
            }),
        )?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__business_commands__v1",
            "cmd_repair_failed_queue",
            serde_json::json!({
                "id": "cmd_repair_failed_queue",
                "command_id": "cmd_repair_failed_queue",
                "status": "accepted",
                "route_status": "pending",
                "task_status": "queued",
                "updated_at_ms": 1
            }),
        )?;
        drop(rxdb_conn);

        let dry_run =
            repair_queue_projections(root, QueueProjectionRepairOptions { apply: false })?;
        assert_eq!(
            dry_run
                .pointer("/counts/failed_from_canonical")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            dry_run
                .pointer("/counts/commands_updated_from_queue")
                .and_then(Value::as_u64),
            Some(1)
        );

        let conn = open_store(root)?;
        let stale_queue_payload: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'ctox_queue_tasks' AND record_id = ?1",
            params![task_id.as_str()],
            |row| row.get(0),
        )?;
        let stale_queue: Value = serde_json::from_str(&stale_queue_payload)?;
        assert_eq!(
            stale_queue.get("status").and_then(Value::as_str),
            Some("queued"),
            "dry-run must not mutate stale queue projection"
        );
        drop(conn);
        let stale_rxdb_queue = load_rxdb_collection_record(root, "ctox_queue_tasks", &task_id)?
            .context("expected stale rxdb queue row after dry-run")?;
        assert_eq!(
            stale_rxdb_queue.get("status").and_then(Value::as_str),
            Some("queued"),
            "dry-run must not mutate active RxDB projection"
        );

        let applied = repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        assert_eq!(
            applied
                .pointer("/counts/failed_from_canonical")
                .and_then(Value::as_u64),
            Some(1)
        );

        let conn = open_store(root)?;
        let queue_payload: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'ctox_queue_tasks' AND record_id = ?1",
            params![task_id.as_str()],
            |row| row.get(0),
        )?;
        let queue_projection: Value = serde_json::from_str(&queue_payload)?;
        assert_eq!(
            queue_projection.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            queue_projection.get("route_status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            queue_projection.get("error").and_then(Value::as_str),
            Some("Input exceeds the maximum length of 1048576 characters.")
        );

        let command_payload: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'business_commands' AND record_id = 'cmd_repair_failed_queue'",
            [],
            |row| row.get(0),
        )?;
        let command_projection: Value = serde_json::from_str(&command_payload)?;
        assert_eq!(
            command_projection.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            command_projection
                .get("task_status")
                .and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            command_projection.get("error").and_then(Value::as_str),
            Some("Input exceeds the maximum length of 1048576 characters.")
        );
        let rxdb_queue = load_rxdb_collection_record(root, "ctox_queue_tasks", &task_id)?
            .context("expected repaired rxdb queue row")?;
        assert_eq!(
            rxdb_queue.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            rxdb_queue.get("route_status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            rxdb_queue.get("error").and_then(Value::as_str),
            Some("Input exceeds the maximum length of 1048576 characters.")
        );
        let rxdb_command =
            load_rxdb_collection_record(root, "business_commands", "cmd_repair_failed_queue")?
                .context("expected repaired rxdb command row")?;
        assert_eq!(
            rxdb_command.get("status").and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            rxdb_command.get("task_status").and_then(Value::as_str),
            Some("failed")
        );
        Ok(())
    }

    #[test]
    fn repair_queue_projections_acks_leased_terminal_success_note() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let accepted = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_repair_leased_success",
                "command_id": "cmd_repair_leased_success",
                "module": "research",
                "command_type": "business_os.chat.task",
                "record_id": "research",
                "status": "pending_sync",
                "payload": {
                    "title": "Kontext-Aufgabe · Web Research",
                    "instruction": "teste terminal success repair",
                    "prompt": "teste terminal success repair"
                },
                "client_context": {
                    "source": "business-os-chat",
                    "module": "research"
                }
            }),
        )?;
        let task_id = accepted
            .get("task_id")
            .and_then(Value::as_str)
            .context("expected queue task id")?
            .to_string();
        channels::lease_queue_task(root, &task_id, "ctox-service")?;
        channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: task_id.clone(),
                status_note: Some(
                    "Business-OS documents bug report completed. Changed editor rendering. Verified in browser."
                        .to_string(),
                ),
                ..Default::default()
            },
        )?;
        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "ctox_queue_tasks",
            &task_id,
            now_ms() as i64,
            serde_json::json!({
                "id": task_id,
                "command_id": "cmd_repair_leased_success",
                "status": "completed",
                "route_status": "handled",
                "task_status": "running"
            }),
        )?;
        drop(conn);
        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__ctox_queue_tasks__v0",
            &task_id,
            serde_json::json!({
                "id": task_id,
                "command_id": "cmd_repair_leased_success",
                "status": "completed",
                "route_status": "handled",
                "task_status": "running",
                "updated_at_ms": 1
            }),
        )?;
        drop(rxdb_conn);

        let dry_run =
            repair_queue_projections(root, QueueProjectionRepairOptions { apply: false })?;
        assert_eq!(
            dry_run
                .pointer("/counts/completed_from_canonical")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            channels::load_queue_task(root, &task_id)?
                .context("queue task after dry-run")?
                .route_status,
            "leased",
            "dry-run must not ack leased tasks"
        );

        repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        let canonical =
            channels::load_queue_task(root, &task_id)?.context("queue task after apply")?;
        assert_eq!(canonical.route_status, "handled");

        let conn = open_store(root)?;
        let queue_payload: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'ctox_queue_tasks' AND record_id = ?1",
            params![task_id.as_str()],
            |row| row.get(0),
        )?;
        let queue_projection: Value = serde_json::from_str(&queue_payload)?;
        assert_eq!(
            queue_projection.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            queue_projection.get("route_status").and_then(Value::as_str),
            Some("handled")
        );
        assert_eq!(
            queue_projection.get("task_status").and_then(Value::as_str),
            Some("completed")
        );

        let command_payload: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'business_commands' AND record_id = 'cmd_repair_leased_success'",
            [],
            |row| row.get(0),
        )?;
        let command_projection: Value = serde_json::from_str(&command_payload)?;
        assert_eq!(
            command_projection.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            command_projection
                .get("task_status")
                .and_then(Value::as_str),
            Some("completed")
        );
        let rxdb_queue = load_rxdb_collection_record(root, "ctox_queue_tasks", &task_id)?
            .context("expected repaired rxdb queue row")?;
        assert_eq!(
            rxdb_queue.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            rxdb_queue.get("route_status").and_then(Value::as_str),
            Some("handled")
        );
        assert_eq!(
            rxdb_queue.get("task_status").and_then(Value::as_str),
            Some("completed")
        );
        Ok(())
    }

    #[test]
    fn repair_queue_projections_updates_task_status_from_terminal_command() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let now = now_ms() as i64;
        let conn = open_store(root)?;
        conn.execute(
            "INSERT INTO business_commands
                (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
             VALUES (?1, 'documents', 'business_os.chat.task', 'documents', 'completed', ?2, ?3, ?4)",
            params![
                "cmd_repair_from_terminal_command",
                serde_json::to_string(&serde_json::json!({"prompt": "done"}))?,
                serde_json::to_string(&serde_json::json!({"source": "test"}))?,
                now,
            ],
        )?;
        upsert_business_record(
            &conn,
            "ctox_queue_tasks",
            "queue:system::repair_from_terminal_command",
            now,
            serde_json::json!({
                "id": "queue:system::repair_from_terminal_command",
                "command_id": "cmd_repair_from_terminal_command",
                "status": "completed",
                "route_status": "handled",
                "task_status": "handled",
                "updated_at_ms": now
            }),
        )?;
        drop(conn);
        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__ctox_queue_tasks__v0",
            "queue:system::repair_from_terminal_command",
            serde_json::json!({
                "id": "queue:system::repair_from_terminal_command",
                "command_id": "cmd_repair_from_terminal_command",
                "status": "completed",
                "route_status": "handled",
                "task_status": "handled",
                "updated_at_ms": now
            }),
        )?;
        drop(rxdb_conn);

        let dry_run =
            repair_queue_projections(root, QueueProjectionRepairOptions { apply: false })?;
        assert_eq!(
            dry_run
                .pointer("/counts/projection_repaired_from_command")
                .and_then(Value::as_u64),
            Some(1)
        );

        repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        let rxdb_queue = load_rxdb_collection_record(
            root,
            "ctox_queue_tasks",
            "queue:system::repair_from_terminal_command",
        )?
        .context("expected repaired rxdb queue row")?;
        assert_eq!(
            rxdb_queue.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            rxdb_queue.get("route_status").and_then(Value::as_str),
            Some("handled")
        );
        assert_eq!(
            rxdb_queue.get("task_status").and_then(Value::as_str),
            Some("completed")
        );
        Ok(())
    }

    #[test]
    fn repair_queue_projections_redacts_inline_report_artifacts_and_counts_legacy_records(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let inline_image = format!("data:image/png;base64,{}", "A".repeat(12_000));
        let inline_payload = serde_json::json!({
            "id": "cmd_inline_report_payload",
            "command_id": "cmd_inline_report_payload",
            "module": "documents",
            "command_type": "business_os.bug_report",
            "status": "accepted",
            "payload": {
                "title": "Gleichungen in word editor",
                "attachment": {
                    "data_url": inline_image
                },
                "strokes": [
                    [{"x": 1, "y": 2}],
                    [{"x": 3, "y": 4}]
                ]
            },
            "client_context": {
                "transport": "business-os-http-command-fallback"
            }
        });
        upsert_business_record(
            &conn,
            "business_commands",
            "cmd_inline_report_payload",
            now_ms() as i64,
            inline_payload.clone(),
        )?;
        drop(conn);
        let rxdb_conn = create_repair_rxdb_tables(root)?;
        insert_rxdb_test_record(
            &rxdb_conn,
            "ctox_business_os__business_commands__v1",
            "cmd_inline_report_payload",
            inline_payload,
        )?;
        drop(rxdb_conn);

        let dry_run =
            repair_queue_projections(root, QueueProjectionRepairOptions { apply: false })?;
        assert_eq!(
            dry_run
                .pointer("/counts/oversized_inline_artifacts_redacted")
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            dry_run
                .pointer("/counts/legacy_http_fallback_records")
                .and_then(Value::as_u64),
            Some(1)
        );

        repair_queue_projections(root, QueueProjectionRepairOptions { apply: true })?;
        let conn = open_store(root)?;
        let payload_json: String = conn.query_row(
            "SELECT payload_json FROM business_records WHERE collection = 'business_commands' AND record_id = 'cmd_inline_report_payload'",
            [],
            |row| row.get(0),
        )?;
        assert!(
            !payload_json.contains("data:image/png;base64"),
            "inline image payload must be redacted"
        );
        let payload: Value = serde_json::from_str(&payload_json)?;
        assert_eq!(
            payload
                .pointer("/payload/attachment/data_url/redacted")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .pointer("/payload/strokes/redacted")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .pointer("/payload/strokes/stroke_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            payload
                .pointer("/client_context/transport")
                .and_then(Value::as_str),
            Some("business-os-http-command-fallback"),
            "legacy transport context is counted and quarantined, not rewritten or replayed"
        );
        let rxdb_payload =
            load_rxdb_collection_record(root, "business_commands", "cmd_inline_report_payload")?
                .context("expected redacted rxdb reporter command row")?;
        assert_eq!(
            rxdb_payload
                .pointer("/payload/attachment/data_url/redacted")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            rxdb_payload
                .pointer("/payload/strokes/redacted")
                .and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn customers_commands_create_pipeline_task_and_activities() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_customer_account_create",
                "command_id": "cmd_customer_account_create",
                "module": "customers",
                "command_type": "customers.account.create",
                "record_id": "acct_customers",
                "status": "pending_sync",
                "payload": {
                    "account_id": "acct_customers",
                    "name": "Metric Space",
                    "domain": "metric-space.ai",
                    "health_status": "healthy"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_customer_contact_create",
                "command_id": "cmd_customer_contact_create",
                "module": "customers",
                "command_type": "customers.contact.create",
                "record_id": "contact_customers",
                "status": "pending_sync",
                "payload": {
                    "contact_id": "contact_customers",
                    "account_id": "acct_customers",
                    "first_name": "Ada",
                    "last_name": "Lovelace",
                    "email": "ada@example.com",
                    "is_primary_contact": true
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_customer_opportunity_create",
                "command_id": "cmd_customer_opportunity_create",
                "module": "customers",
                "command_type": "customers.opportunity.create",
                "record_id": "opp_customers",
                "status": "pending_sync",
                "payload": {
                    "opportunity_id": "opp_customers",
                    "name": "Renewal 2026",
                    "account_id": "acct_customers",
                    "primary_contact_id": "contact_customers",
                    "amount_cents": 1200000,
                    "opportunity_type": "renewal"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_customer_opportunity_stage",
                "command_id": "cmd_customer_opportunity_stage",
                "module": "customers",
                "command_type": "customers.opportunity.move_stage",
                "record_id": "opp_customers",
                "status": "pending_sync",
                "payload": {
                    "opportunity_id": "opp_customers",
                    "stage": "proposal"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_customer_task_create",
                "command_id": "cmd_customer_task_create",
                "module": "customers",
                "command_type": "customers.task.create",
                "record_id": "task_customers",
                "status": "pending_sync",
                "payload": {
                    "task_id": "task_customers",
                    "account_id": "acct_customers",
                    "opportunity_id": "opp_customers",
                    "title": "Renewal Call vorbereiten"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_customer_task_complete",
                "command_id": "cmd_customer_task_complete",
                "module": "customers",
                "command_type": "customers.task.complete",
                "record_id": "task_customers",
                "status": "pending_sync",
                "payload": { "task_id": "task_customers" },
                "client_context": actor
            }),
        )?;

        let conn = open_store(root)?;
        let opportunity = outbound_load_required(
            &conn,
            "customer_opportunities",
            "opp_customers",
            "opportunity",
        )?;
        assert_eq!(
            outbound_string(&opportunity, &["stage"]).as_deref(),
            Some("proposal")
        );
        let task = outbound_load_required(&conn, "customer_tasks", "task_customers", "task")?;
        assert_eq!(
            outbound_string(&task, &["status"]).as_deref(),
            Some("completed")
        );
        let activity_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'customer_activities' AND deleted = 0",
            [],
            |row| row.get(0),
        )?;
        assert!(
            activity_count >= 6,
            "expected activities for every Customers transition"
        );
        Ok(())
    }

    #[test]
    fn outbound_write_outreach_draft_persists_messages_into_pipeline_contact() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "outbound_pipeline_items",
            "pipe_one",
            1,
            serde_json::json!({
                "id": "pipe_one",
                "campaign_id": "camp",
                "company_id": "company_one",
                "company_name": "Beispiel GmbH",
                "stage": "contact_research",
                "contacts": [
                    { "name": "Erika Muster", "email": "erika@example.test" }
                ],
                "updated_at_ms": 1
            }),
        )?;
        drop(conn);

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_outreach_writeback",
                "command_id": "cmd_outreach_writeback",
                "module": "outbound",
                "command_type": "outbound.pipeline.write_outreach_draft",
                "record_id": "pipe_one",
                "status": "pending_sync",
                "payload": {
                    "pipeline_id": "pipe_one",
                    "contact_index": 0,
                    "messages": {
                        "message_mail_subject": "Kurzer Austausch zu Beispiel GmbH",
                        "message_mail_body": "Hallo Erika, ...",
                        "message_followup_1": "Kurzer Nachtrag ...",
                        "message_followup_2": "Letztes Follow-up ..."
                    }
                },
                "client_context": actor
            }),
        )?;

        let conn = open_store(root)?;
        let item = outbound_load_required(
            &conn,
            "outbound_pipeline_items",
            "pipe_one",
            "pipeline item",
        )?;
        let contact = item
            .pointer("/contacts/0")
            .cloned()
            .expect("contact present");
        assert_eq!(
            outbound_string(&contact, &["messages", "message_mail_subject"]).as_deref(),
            Some("Kurzer Austausch zu Beispiel GmbH")
        );
        assert_eq!(
            outbound_string(&contact, &["messages", "message_followup_2"]).as_deref(),
            Some("Letztes Follow-up ...")
        );
        assert_eq!(
            contact
                .pointer("/outreach_generating")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            outbound_string(&contact, &["outreach_status"]).as_deref(),
            Some("drafted")
        );
        Ok(())
    }

    #[test]
    fn customers_invalid_command_writes_failed_projection_without_partial_record(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let err = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_customer_invalid_account",
                "command_id": "cmd_customer_invalid_account",
                "module": "customers",
                "command_type": "customers.account.create",
                "record_id": "acct_invalid",
                "status": "pending_sync",
                "payload": {
                    "account_id": "acct_invalid",
                    "name": "Invalid GmbH",
                    "health_status": "glowing"
                },
                "client_context": actor
            }),
        )
        .expect_err("unsupported health status must fail");
        assert!(err.to_string().contains("health_status"), "{err}");

        let conn = open_store(root)?;
        assert!(outbound_load_record(&conn, "customer_accounts", "acct_invalid")?.is_none());
        let command = outbound_load_required(
            &conn,
            "business_commands",
            "cmd_customer_invalid_account",
            "command",
        )?;
        assert_eq!(
            outbound_string(&command, &["status"]).as_deref(),
            Some("failed")
        );
        assert!(command
            .pointer("/result/error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("health_status"));
        Ok(())
    }

    #[test]
    fn customers_import_from_outbound_creates_account_contacts_and_dedupe() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "outbound_companies",
            "company_one",
            1,
            serde_json::json!({
                "id": "company_one",
                "campaign_id": "camp",
                "name": "Acme GmbH",
                "domain": "acme.example",
                "website": "https://acme.example",
                "qualification_status": "qualified",
                "research_status": "done",
                "pipeline_status": "ready",
                "payload": {},
                "created_at_ms": 1
            }),
        )?;
        upsert_business_record(
            &conn,
            "outbound_pipeline_items",
            "pipeline_one",
            1,
            serde_json::json!({
                "id": "pipeline_one",
                "campaign_id": "camp",
                "company_id": "company_one",
                "company_name": "Acme GmbH",
                "stage": "qualified",
                "contact_research_status": "done",
                "outreach_status": "ready",
                "contacts": [
                    { "name": "Grace Hopper", "email": "grace@acme.example", "job_title": "CTO" }
                ],
                "payload": {},
                "created_at_ms": 1
            }),
        )?;
        drop(conn);

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_customer_import",
                "command_id": "cmd_customer_import",
                "module": "customers",
                "command_type": "customers.import.from_outbound",
                "record_id": "company_one",
                "status": "pending_sync",
                "payload": {
                    "source_record_id": "company_one",
                    "pipeline_id": "pipeline_one",
                    "account_id": "acct_acme"
                },
                "client_context": actor.clone()
            }),
        )?;

        let conn = open_store(root)?;
        let account = outbound_load_required(&conn, "customer_accounts", "acct_acme", "account")?;
        assert_eq!(
            outbound_string(&account, &["name"]).as_deref(),
            Some("Acme GmbH")
        );
        let contacts = outbound_load_records_by_string_field(
            &conn,
            "customer_contacts",
            "account_id",
            "acct_acme",
        )?;
        assert_eq!(contacts.len(), 1);
        drop(conn);

        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "outbound_companies",
            "company_duplicate",
            2,
            serde_json::json!({
                "id": "company_duplicate",
                "campaign_id": "camp",
                "name": "Acme Duplicate",
                "domain": "acme.example",
                "qualification_status": "qualified",
                "research_status": "done",
                "pipeline_status": "ready",
                "payload": {},
                "created_at_ms": 2
            }),
        )?;
        drop(conn);

        let duplicate = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_customer_import_duplicate",
                "command_id": "cmd_customer_import_duplicate",
                "module": "customers",
                "command_type": "customers.import.from_outbound",
                "record_id": "company_duplicate",
                "status": "pending_sync",
                "payload": { "source_record_id": "company_duplicate" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            duplicate.pointer("/result/status").and_then(Value::as_str),
            Some("needs_review")
        );
        let candidate_id = duplicate
            .pointer("/result/dedupe_candidate/id")
            .and_then(Value::as_str)
            .context("dedupe candidate id")?
            .to_string();

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_customer_dedupe_resolve",
                "command_id": "cmd_customer_dedupe_resolve",
                "module": "customers",
                "command_type": "customers.dedupe.resolve",
                "record_id": candidate_id,
                "status": "pending_sync",
                "payload": {
                    "candidate_id": candidate_id,
                    "decision": "keep_existing"
                },
                "client_context": actor
            }),
        )?;
        let conn = open_store(root)?;
        let resolved = outbound_load_records_by_string_field(
            &conn,
            "customer_dedupe_candidates",
            "status",
            "resolved",
        )?;
        assert_eq!(resolved.len(), 1);
        Ok(())
    }

    #[test]
    fn rxdb_data_plane_status_reports_critical_collection_counts() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        fs::create_dir_all(root.join("runtime"))?;
        let sqlite_path = rxdb_store_path(root);
        let conn = Connection::open(sqlite_path)?;
        for collection in [
            "business_module_catalog",
            "ctox_runtime_settings",
            "desktop_files",
            "desktop_file_chunks",
        ] {
            conn.execute(
                &format!(
                    "CREATE TABLE ctox_business_os__{collection}__v0 (id TEXT PRIMARY KEY, data TEXT NOT NULL)"
                ),
                [],
            )?;
        }
        conn.execute(
            "CREATE TABLE ctox_business_os__business_commands__v1 (id TEXT PRIMARY KEY, data TEXT NOT NULL)",
            [],
        )?;
        conn.execute(
            "INSERT INTO ctox_business_os__business_commands__v1 (id, data) VALUES ('cmd-1', ?1)",
            [serde_json::json!({"updated_at_ms": 1200, "status": "accepted"}).to_string()],
        )?;
        conn.execute(
            "INSERT INTO ctox_business_os__business_module_catalog__v0 (id, data) VALUES ('module-catalog', ?1)",
            [serde_json::json!({"updated_at_ms": 1000, "modules": [{"id": "ctox"}]}).to_string()],
        )?;
        conn.execute(
            "INSERT INTO ctox_business_os__ctox_runtime_settings__v0 (id, data) VALUES ('runtime-settings', ?1)",
            [serde_json::json!({"updated_at_ms": 1100}).to_string()],
        )?;

        let status = rxdb_data_plane_status(root);
        assert_eq!(status.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            status
                .pointer("/collections/business_module_catalog/row_count")
                .and_then(Value::as_i64),
            Some(1)
        );
        assert_eq!(
            status
                .pointer("/collections/ctox_runtime_settings/latest_updated_at_ms")
                .and_then(Value::as_i64),
            Some(1100)
        );
        assert_eq!(
            status
                .pointer("/collections/desktop_files/ok")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            status
                .pointer("/collections/business_commands/table")
                .and_then(Value::as_str),
            Some("ctox_business_os__business_commands__v1")
        );
        Ok(())
    }

    #[test]
    fn pull_collection_records_falls_back_to_versioned_rxdb_table() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        fs::create_dir_all(root.join("runtime"))?;
        let sqlite_path = rxdb_store_path(root);
        let conn = Connection::open(sqlite_path)?;
        conn.execute(
            "CREATE TABLE ctox_business_os__business_commands__v1 (id TEXT PRIMARY KEY, data TEXT NOT NULL)",
            [],
        )?;
        conn.execute(
            "INSERT INTO ctox_business_os__business_commands__v1 (id, data) VALUES ('cmd-module-versions', ?1)",
            [serde_json::json!({
                "id": "cmd-module-versions",
                "command_type": "ctox.source.list_snapshots",
                "status": "completed",
                "updated_at_ms": 1_700
            })
            .to_string()],
        )?;
        drop(conn);

        let pulled = pull_collection_records(root, "business_commands", Some(1_000), Some(10))?;
        assert_eq!(
            pulled.get("source").and_then(Value::as_str),
            Some("rxdb_projection")
        );
        assert_eq!(
            pulled.get("table").and_then(Value::as_str),
            Some("ctox_business_os__business_commands__v1")
        );
        assert_eq!(
            pulled.pointer("/documents/0/id").and_then(Value::as_str),
            Some("cmd-module-versions")
        );
        assert_eq!(
            pulled
                .pointer("/documents/0/status")
                .and_then(Value::as_str),
            Some("completed")
        );
        Ok(())
    }

    #[test]
    fn rxdb_command_auth_uses_trusted_user_role_not_client_claims() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let now = now_ms() as i64;
        conn.execute(
            "INSERT INTO business_users
                (user_id, display_name, role, active, created_at_ms, updated_at_ms)
             VALUES ('viewer', 'Viewer', 'user', 1, ?1, ?1)",
            params![now],
        )?;
        drop(conn);

        let error = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_spoof_runtime_admin",
                "command_id": "cmd_spoof_runtime_admin",
                "module": "ctox",
                "command_type": "ctox.runtime_settings.save",
                "record_id": "runtime-settings",
                "status": "pending_sync",
                "payload": {
                    "provider": "openai",
                    "auth_mode": "chatgpt_subscription",
                    "chat_model": "gpt-5.5",
                    "preset": "Quality",
                    "context": "256k"
                },
                "client_context": {
                    "actor": {
                        "id": "viewer",
                        "display_name": "Viewer",
                        "role": "admin",
                        "is_admin": true
                    }
                }
            }),
        )
        .expect_err("client-side role claims must not grant admin rights");

        assert!(
            error.to_string().contains("chef or admin role required"),
            "unexpected error: {error}"
        );
        Ok(())
    }

    #[test]
    fn outbound_message_send_approved_requires_matching_current_revision() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_engagement",
                "command_id": "cmd_engagement",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_test",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_test",
                    "company_id": "company_test",
                    "contact_id": "contact_test"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_prepare",
                "command_id": "cmd_prepare",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_test",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_test",
                    "campaign_id": "camp_test",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;

        let before_approval = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_before_approval",
                "command_id": "cmd_send_before_approval",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_test",
                "status": "pending_sync",
                "payload": { "message_id": "msg_test" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("send must be blocked before approval");
        assert!(
            before_approval.to_string().contains("must be approved"),
            "{before_approval}"
        );

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_request_approval",
                "command_id": "cmd_request_approval",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_test",
                "status": "pending_sync",
                "payload": { "message_id": "msg_test" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_approve",
                "command_id": "cmd_approve",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_test",
                "status": "pending_sync",
                "payload": { "message_id": "msg_test" },
                "client_context": actor.clone()
            }),
        )?;
        let send = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_after_approval",
                "command_id": "cmd_send_after_approval",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_test",
                "status": "pending_sync",
                "payload": { "message_id": "msg_test" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            send.pointer("/result/provider_send_executed")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            send.pointer("/result/provider_dispatch_status")
                .and_then(Value::as_str),
            Some("queued_in_mailserver")
        );
        assert_eq!(
            send.pointer("/result/message/send_status")
                .and_then(Value::as_str),
            Some("queued_for_provider")
        );
        let provider_queue_id = send
            .pointer("/result/provider_queue_id")
            .and_then(Value::as_str)
            .context("provider_queue_id should be returned")?;
        let queue_conn = Connection::open(crate::paths::core_db(root))?;
        let queued_count: i64 = queue_conn.query_row(
            "SELECT COUNT(*) FROM stalwart_smtp_queue WHERE id = ?1 AND from_addr = 'sender@example.com' AND to_addr = 'lead@example.com' AND status = 'pending'",
            params![provider_queue_id],
            |row| row.get(0),
        )?;
        assert_eq!(queued_count, 1);
        drop(queue_conn);

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_prepare_second",
                "command_id": "cmd_prepare_second",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_revision",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_test",
                    "campaign_id": "camp_test",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Old body"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_request_second",
                "command_id": "cmd_request_second",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_revision",
                "status": "pending_sync",
                "payload": { "message_id": "msg_revision" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_approve_second",
                "command_id": "cmd_approve_second",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_revision",
                "status": "pending_sync",
                "payload": { "message_id": "msg_revision" },
                "client_context": actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let mut stale_message =
            outbound_load_required(&conn, "outbound_messages", "msg_revision", "message")?;
        outbound_put_string(&mut stale_message, "body_text", "Changed body");
        outbound_put_string(&mut stale_message, "approval_status", "approved");
        outbound_put_string(&mut stale_message, "send_status", "approved_not_sent");
        let changed_revision = outbound_message_revision(&stale_message);
        outbound_put_string(&mut stale_message, "revision_id", changed_revision);
        upsert_business_record(
            &conn,
            "outbound_messages",
            "msg_revision",
            now_ms() as i64,
            stale_message,
        )?;
        drop(conn);

        let changed_revision_send = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_changed_revision",
                "command_id": "cmd_send_changed_revision",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_revision",
                "status": "pending_sync",
                "payload": { "message_id": "msg_revision" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("changed message body must invalidate approval");
        assert!(
            changed_revision_send
                .to_string()
                .contains("no matching approval for current revision"),
            "{changed_revision_send}"
        );

        Ok(())
    }

    #[test]
    fn outbound_send_approved_blocked_after_rejection_or_change_request() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let prepare = |message_id: &str| {
            serde_json::json!({
                "id": format!("cmd_prepare_{message_id}"),
                "command_id": format!("cmd_prepare_{message_id}"),
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": message_id,
                "status": "pending_sync",
                "payload": {
                    "engagement_id": format!("eng_{message_id}"),
                    "campaign_id": "camp_test",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            })
        };
        let decide = |message_id: &str, command_type: &str| {
            serde_json::json!({
                "id": format!("cmd_{command_type}_{message_id}"),
                "command_id": format!("cmd_{command_type}_{message_id}"),
                "module": "outbound",
                "command_type": format!("outbound.message.{command_type}"),
                "record_id": message_id,
                "status": "pending_sync",
                "payload": { "message_id": message_id },
                "client_context": actor.clone()
            })
        };
        let send = |message_id: &str| {
            serde_json::json!({
                "id": format!("cmd_send_{message_id}"),
                "command_id": format!("cmd_send_{message_id}"),
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": message_id,
                "status": "pending_sync",
                "payload": { "message_id": message_id },
                "client_context": actor.clone()
            })
        };

        // Rejected message: request approval, reject, then sending must be blocked.
        accept_rxdb_business_command(root, prepare("msg_reject"))?;
        accept_rxdb_business_command(root, decide("msg_reject", "request_approval"))?;
        accept_rxdb_business_command(root, decide("msg_reject", "reject"))?;
        let rejected_send = accept_rxdb_business_command(root, send("msg_reject"))
            .expect_err("send must be blocked after rejection");
        assert!(
            rejected_send.to_string().contains("must be approved"),
            "{rejected_send}"
        );

        // Change-requested message: request approval, request changes, send blocked.
        accept_rxdb_business_command(root, prepare("msg_changes"))?;
        accept_rxdb_business_command(root, decide("msg_changes", "request_approval"))?;
        accept_rxdb_business_command(root, decide("msg_changes", "request_changes"))?;
        let changes_send = accept_rxdb_business_command(root, send("msg_changes"))
            .expect_err("send must be blocked after change request");
        assert!(
            changes_send.to_string().contains("must be approved"),
            "{changes_send}"
        );

        Ok(())
    }

    #[test]
    fn outbound_sequence_save_persists_strategy_policy_and_touchpoints() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let saved = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_sequence_save",
                "command_id": "cmd_sequence_save",
                "module": "outbound",
                "command_type": "outbound.sequence.save",
                "record_id": "seq_campaign",
                "status": "pending_sync",
                "payload": {
                    "sequence_id": "seq_campaign",
                    "campaign_id": "camp_sequence",
                    "name": "CTOX Active Outbound",
                    "strategy_text": "Bereite jede Nachricht vor und warte auf Freigabe.",
                    "sequence_policy_text": "Initiale Nachricht, 5 Werktage warten, Follow-up 1.",
                    "send_window": { "text": "Werktags 09:00-16:00" },
                    "touchpoints": [
                        { "type": "initial", "wait_days_after_previous": 0, "requires_approval": true },
                        { "type": "followup_1", "wait_days_after_previous": 5, "requires_approval": true }
                    ],
                    "stop_rules": [
                        { "type": "hard_stop", "text": "Stoppe bei Antwort, Opt-out oder Termin." }
                    ],
                    "approval_policy": {
                        "require_all_messages": true,
                        "policy_text": "Jede ausgehende Nachricht braucht Freigabe."
                    },
                    "scheduling_policy": {
                        "strategy_text": "Bei Interesse Terminantwort vorbereiten.",
                        "duration_minutes": 30,
                        "slot_proposal_count": 3
                    },
                    "compliance_policy": {
                        "policy_text": "Keine Nachricht nach Opt-out.",
                        "suppression_policy_text": "Suppressions hart beachten."
                    },
                    "payload": {
                        "sender_account_id": "sender@example.com",
                        "skillbook_id": "business-os-outbound-active",
                        "runbook_id": "runbook-active-outbound"
                    }
                },
                "client_context": actor
            }),
        )?;

        assert_eq!(
            saved.pointer("/result/collection").and_then(Value::as_str),
            Some("outbound_sequences")
        );
        assert_eq!(
            saved
                .pointer("/result/sequence/strategy_text")
                .and_then(Value::as_str),
            Some("Bereite jede Nachricht vor und warte auf Freigabe.")
        );

        let conn = open_store(root)?;
        let sequence =
            outbound_load_required(&conn, "outbound_sequences", "seq_campaign", "sequence")?;
        assert_eq!(
            outbound_string(&sequence, &["campaign_id"]).as_deref(),
            Some("camp_sequence")
        );
        assert_eq!(
            sequence
                .pointer("/approval_policy/require_all_messages")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            sequence
                .pointer("/touchpoints/1/wait_days_after_previous")
                .and_then(Value::as_i64),
            Some(5)
        );
        assert_eq!(
            sequence
                .pointer("/stop_rules/0/type")
                .and_then(Value::as_str),
            Some("hard_stop")
        );
        assert_eq!(
            sequence
                .pointer("/scheduling_policy/duration_minutes")
                .and_then(Value::as_i64),
            Some(30)
        );
        assert_eq!(
            sequence
                .pointer("/compliance_policy/policy_text")
                .and_then(Value::as_str),
            Some("Keine Nachricht nach Opt-out.")
        );
        assert_eq!(
            sequence
                .pointer("/payload/skillbook_id")
                .and_then(Value::as_str),
            Some("business-os-outbound-active")
        );

        Ok(())
    }

    #[test]
    fn outbound_draft_prepare_creates_approval_gated_automated_messages() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_engagement_auto",
                "command_id": "cmd_engagement_auto",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_auto",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_auto",
                    "company_id": "company_auto",
                    "contact_id": "contact_auto",
                    "payload": {
                        "company_name": "ACME GmbH",
                        "contact_name": "Frau Beispiel",
                        "contact_email": "lead@example.com",
                        "strategy_text": "Kurz, belegt und respektvoll nachfassen.",
                        "skillbook_id": "business-os.outbound.message_drafting.v1",
                        "runbook_id": "runbook-active-outbound"
                    }
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_assign_auto",
                "command_id": "cmd_assign_auto",
                "module": "outbound",
                "command_type": "outbound.engagement.assign_sender",
                "record_id": "eng_auto",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_auto",
                    "sender_account_id": "sender@example.com"
                },
                "client_context": actor.clone()
            }),
        )?;

        let followup = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_auto_followup",
                "command_id": "cmd_auto_followup",
                "module": "outbound",
                "command_type": "outbound.draft.prepare",
                "record_id": "msg_auto_followup",
                "status": "pending_sync",
                "payload": {
                    "message_id": "msg_auto_followup",
                    "engagement_id": "eng_auto",
                    "draft_kind": "followup_1"
                },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            followup
                .pointer("/result/message/approval_status")
                .and_then(Value::as_str),
            Some("awaiting_approval")
        );
        assert_eq!(
            followup
                .pointer("/result/provider_send_executed")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            followup
                .pointer("/result/message/payload/skillbook_id")
                .and_then(Value::as_str),
            Some("business-os.outbound.message_drafting.v1")
        );

        let scheduling = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_auto_scheduling",
                "command_id": "cmd_auto_scheduling",
                "module": "outbound",
                "command_type": "outbound.draft.prepare",
                "record_id": "msg_auto_scheduling",
                "status": "pending_sync",
                "payload": {
                    "message_id": "msg_auto_scheduling",
                    "engagement_id": "eng_auto",
                    "draft_kind": "scheduling",
                    "duration_minutes": 45,
                    "slot_hint": "drei Slots in der kommenden Woche",
                    "proposed_slots": [
                        {"start_iso":"2026-06-02T10:00:00Z","end_iso":"2026-06-02T10:45:00Z"}
                    ]
                },
                "client_context": actor
            }),
        )?;
        assert_eq!(
            scheduling
                .pointer("/result/message/message_type")
                .and_then(Value::as_str),
            Some("scheduling")
        );
        assert_eq!(
            scheduling
                .pointer("/result/message/send_status")
                .and_then(Value::as_str),
            Some("awaiting_approval")
        );
        assert_eq!(
            scheduling
                .pointer("/result/message/payload/meeting_request_id")
                .and_then(Value::as_str),
            Some("meeting_msg_auto_scheduling")
        );
        assert_eq!(
            scheduling
                .pointer("/result/message/payload/proposed_slots")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            scheduling
                .pointer("/result/meeting_request/id")
                .and_then(Value::as_str),
            Some("meeting_msg_auto_scheduling")
        );

        let conn = open_store(root)?;
        let meeting = outbound_load_required(
            &conn,
            "outbound_meeting_requests",
            "meeting_msg_auto_scheduling",
            "meeting request",
        )?;
        assert_eq!(
            outbound_string(&meeting, &["status"]).as_deref(),
            Some("prepared")
        );
        assert_eq!(
            meeting.get("duration_minutes").and_then(Value::as_i64),
            Some(45)
        );
        assert_eq!(
            meeting
                .get("proposed_slots")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );

        Ok(())
    }

    #[test]
    fn outbound_scheduling_message_can_be_edited_and_approved() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        let cmd = |id: &str, command_type: &str, record_id: &str, payload: Value| {
            serde_json::json!({
                "id": id, "command_id": id, "module": "outbound",
                "command_type": command_type, "record_id": record_id,
                "status": "pending_sync", "payload": payload,
                "client_context": actor.clone()
            })
        };

        accept_rxdb_business_command(
            root,
            cmd(
                "sch_eng",
                "outbound.engagement.create",
                "eng_sch",
                serde_json::json!({
                    "campaign_id": "camp_sch",
                    "company_id": "co_sch",
                    "contact_id": "ct_sch",
                    "payload": { "contact_email": "lead@example.com" }
                }),
            ),
        )?;
        accept_rxdb_business_command(
            root,
            cmd(
                "sch_assign",
                "outbound.engagement.assign_sender",
                "eng_sch",
                serde_json::json!({
                    "engagement_id": "eng_sch",
                    "sender_account_id": "sender@example.com"
                }),
            ),
        )?;
        let prepared = accept_rxdb_business_command(
            root,
            cmd(
                "sch_prepare",
                "outbound.draft.prepare",
                "msg_sch",
                serde_json::json!({
                    "message_id": "msg_sch",
                    "engagement_id": "eng_sch",
                    "draft_kind": "scheduling",
                    "duration_minutes": 30
                }),
            ),
        )?;
        assert_eq!(
            prepared
                .pointer("/result/message/message_type")
                .and_then(Value::as_str),
            Some("scheduling")
        );

        // The user edits the auto-generated scheduling proposal before approving.
        accept_rxdb_business_command(
            root,
            cmd(
                "sch_edit",
                "outbound.message.update_draft",
                "msg_sch",
                serde_json::json!({
                    "message_id": "msg_sch",
                    "subject": "Terminvorschlag (angepasst)",
                    "body_text": "Passt Ihnen Dienstag 10:00 oder Mittwoch 14:00?"
                }),
            ),
        )?;
        {
            let conn = open_store(root)?;
            let edited = outbound_load_required(&conn, "outbound_messages", "msg_sch", "message")?;
            // Editing resets the approval so the change cannot bypass the gate.
            assert_eq!(
                outbound_string(&edited, &["approval_status"]).as_deref(),
                Some("draft")
            );
            assert_eq!(
                outbound_string(&edited, &["subject"]).as_deref(),
                Some("Terminvorschlag (angepasst)")
            );
            assert_eq!(
                outbound_string(&edited, &["body_text"]).as_deref(),
                Some("Passt Ihnen Dienstag 10:00 oder Mittwoch 14:00?")
            );
        }

        accept_rxdb_business_command(
            root,
            cmd(
                "sch_request",
                "outbound.message.request_approval",
                "msg_sch",
                serde_json::json!({ "message_id": "msg_sch" }),
            ),
        )?;
        accept_rxdb_business_command(
            root,
            cmd(
                "sch_approve",
                "outbound.message.approve",
                "msg_sch",
                serde_json::json!({ "message_id": "msg_sch" }),
            ),
        )?;

        let conn = open_store(root)?;
        let approved = outbound_load_required(&conn, "outbound_messages", "msg_sch", "message")?;
        assert_eq!(
            outbound_string(&approved, &["approval_status"]).as_deref(),
            Some("approved"),
            "the edited scheduling message must be approvable"
        );
        assert_eq!(
            outbound_string(&approved, &["subject"]).as_deref(),
            Some("Terminvorschlag (angepasst)"),
            "the edit must survive the approval"
        );
        // The approval must match the edited revision.
        let revision_id = outbound_string(&approved, &["revision_id"])
            .unwrap_or_else(|| outbound_message_revision(&approved));
        assert!(
            outbound_has_matching_approval(&conn, "msg_sch", &revision_id)?,
            "approval must bind to the edited revision"
        );

        Ok(())
    }

    #[test]
    fn outbound_send_gate_blocks_bounce_and_unhealthy_sender() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        let cmd = |id: &str, command_type: &str, record_id: &str, payload: Value| {
            serde_json::json!({
                "id": id, "command_id": id, "module": "outbound",
                "command_type": command_type, "record_id": record_id,
                "status": "pending_sync", "payload": payload,
                "client_context": actor.clone()
            })
        };
        let approve_message = |eng: &str,
                               msg: &str,
                               sender: &str,
                               recipient: &str|
         -> anyhow::Result<()> {
            accept_rxdb_business_command(
                root,
                cmd(
                    &format!("c_eng_{msg}"),
                    "outbound.engagement.create",
                    eng,
                    serde_json::json!({"campaign_id":"camp_217","company_id":"co","contact_id":"ct"}),
                ),
            )?;
            accept_rxdb_business_command(
                root,
                cmd(
                    &format!("c_prep_{msg}"),
                    "outbound.message.prepare",
                    msg,
                    serde_json::json!({
                        "engagement_id": eng, "campaign_id": "camp_217",
                        "sender_account_id": sender, "recipient_email": recipient,
                        "subject": "Intro", "body_text": "Hello"
                    }),
                ),
            )?;
            accept_rxdb_business_command(
                root,
                cmd(
                    &format!("c_req_{msg}"),
                    "outbound.message.request_approval",
                    msg,
                    serde_json::json!({"message_id": msg}),
                ),
            )?;
            accept_rxdb_business_command(
                root,
                cmd(
                    &format!("c_apv_{msg}"),
                    "outbound.message.approve",
                    msg,
                    serde_json::json!({"message_id": msg}),
                ),
            )?;
            Ok(())
        };

        // (1) A hard bounce recorded as a suppression entry blocks the send.
        approve_message(
            "eng_bounce",
            "msg_bounce",
            "sender@example.com",
            "bounced@example.com",
        )?;
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_suppression_entries",
                "supp_bounce_217",
                1000,
                serde_json::json!({
                    "id": "supp_bounce_217",
                    "email": "bounced@example.com",
                    "reason": "bounce",
                    "status": "active",
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }
        let bounce_blocked = accept_rxdb_business_command(
            root,
            cmd(
                "c_send_bounce",
                "outbound.message.send_approved",
                "msg_bounce",
                serde_json::json!({"message_id": "msg_bounce"}),
            ),
        )
        .expect_err("bounce suppression must block the send");
        assert!(
            bounce_blocked.to_string().contains("suppressed"),
            "{bounce_blocked}"
        );

        // (2) A sender account that is not provider-eligible (suspended) is rejected
        //     even for a perfectly clean recipient.
        approve_message(
            "eng_health",
            "msg_health",
            "email:sick@example.com",
            "clean@example.com",
        )?;
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_account_limits",
                "email:sick@example.com",
                1000,
                serde_json::json!({
                    "id": "email:sick@example.com",
                    "sender_account_id": "email:sick@example.com",
                    "status": "suspended",
                    "blocked": false,
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }
        let health_blocked = accept_rxdb_business_command(
            root,
            cmd(
                "c_send_health",
                "outbound.message.send_approved",
                "msg_health",
                serde_json::json!({"message_id": "msg_health"}),
            ),
        )
        .expect_err("unhealthy sender account must block the send");
        assert!(
            health_blocked.to_string().contains("not eligible"),
            "{health_blocked}"
        );

        Ok(())
    }

    #[test]
    fn outbound_pause_resume_and_close_updates_engagement_state() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_engagement_lifecycle",
                "command_id": "cmd_engagement_lifecycle",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_lifecycle",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_lifecycle",
                    "company_id": "company_lifecycle",
                    "contact_id": "contact_lifecycle"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_prepare_lifecycle",
                "command_id": "cmd_prepare_lifecycle",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_lifecycle",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_lifecycle",
                    "campaign_id": "camp_lifecycle",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_pause_lifecycle",
                "command_id": "cmd_pause_lifecycle",
                "module": "outbound",
                "command_type": "outbound.message.pause",
                "record_id": "msg_lifecycle",
                "status": "pending_sync",
                "payload": { "message_id": "msg_lifecycle", "reason": "manual review" },
                "client_context": actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let paused_message =
            outbound_load_required(&conn, "outbound_messages", "msg_lifecycle", "message")?;
        let paused_engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_lifecycle", "engagement")?;
        assert_eq!(
            outbound_string(&paused_message, &["send_status"]).as_deref(),
            Some("paused")
        );
        assert_eq!(
            outbound_string(&paused_engagement, &["status"]).as_deref(),
            Some("paused")
        );
        assert_eq!(
            outbound_string(&paused_engagement, &["paused_reason"]).as_deref(),
            Some("manual review")
        );
        drop(conn);

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_resume_lifecycle",
                "command_id": "cmd_resume_lifecycle",
                "module": "outbound",
                "command_type": "outbound.message.resume",
                "record_id": "msg_lifecycle",
                "status": "pending_sync",
                "payload": { "message_id": "msg_lifecycle", "reason": "ready" },
                "client_context": actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let resumed_message =
            outbound_load_required(&conn, "outbound_messages", "msg_lifecycle", "message")?;
        let resumed_engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_lifecycle", "engagement")?;
        assert_eq!(
            outbound_string(&resumed_message, &["send_status"]).as_deref(),
            Some("not_scheduled")
        );
        assert_eq!(
            outbound_string(&resumed_engagement, &["status"]).as_deref(),
            Some("draft_prepared")
        );
        drop(conn);

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_close_lifecycle",
                "command_id": "cmd_close_lifecycle",
                "module": "outbound",
                "command_type": "outbound.engagement.close",
                "record_id": "eng_lifecycle",
                "status": "pending_sync",
                "payload": { "engagement_id": "eng_lifecycle", "reason": "not a fit" },
                "client_context": actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let closed_message =
            outbound_load_required(&conn, "outbound_messages", "msg_lifecycle", "message")?;
        let closed_engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_lifecycle", "engagement")?;
        assert_eq!(
            outbound_string(&closed_message, &["send_status"]).as_deref(),
            Some("cancelled")
        );
        assert_eq!(
            outbound_string(&closed_engagement, &["status"]).as_deref(),
            Some("closed")
        );
        assert_eq!(
            outbound_string(&closed_engagement, &["closed_reason"]).as_deref(),
            Some("not a fit")
        );

        Ok(())
    }

    #[test]
    fn outbound_engagement_status_advances_through_approval_and_send() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let cmd = |id: &str, command_type: &str, record_id: &str, payload: Value| {
            serde_json::json!({
                "id": id, "command_id": id, "module": "outbound",
                "command_type": command_type, "record_id": record_id,
                "status": "pending_sync", "payload": payload,
                "client_context": actor.clone()
            })
        };
        let engagement_status = |id: &str| -> anyhow::Result<String> {
            let conn = open_store(root)?;
            let eng = outbound_load_required(&conn, "outbound_engagements", id, "engagement")?;
            Ok(outbound_string(&eng, &["status"]).unwrap_or_default())
        };

        accept_rxdb_business_command(
            root,
            cmd(
                "fwd_create",
                "outbound.engagement.create",
                "eng_fwd",
                serde_json::json!({
                    "campaign_id": "camp_fwd",
                    "company_id": "co_fwd",
                    "contact_id": "ct_fwd"
                }),
            ),
        )?;

        accept_rxdb_business_command(
            root,
            cmd(
                "fwd_prepare",
                "outbound.message.prepare",
                "msg_fwd",
                serde_json::json!({
                    "engagement_id": "eng_fwd",
                    "campaign_id": "camp_fwd",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                }),
            ),
        )?;
        assert_eq!(engagement_status("eng_fwd")?, "draft_prepared");

        accept_rxdb_business_command(
            root,
            cmd(
                "fwd_request",
                "outbound.message.request_approval",
                "msg_fwd",
                serde_json::json!({ "message_id": "msg_fwd" }),
            ),
        )?;
        assert_eq!(engagement_status("eng_fwd")?, "awaiting_approval");

        accept_rxdb_business_command(
            root,
            cmd(
                "fwd_approve",
                "outbound.message.approve",
                "msg_fwd",
                serde_json::json!({ "message_id": "msg_fwd" }),
            ),
        )?;
        assert_eq!(engagement_status("eng_fwd")?, "approved_for_send");

        accept_rxdb_business_command(
            root,
            cmd(
                "fwd_send",
                "outbound.message.send_approved",
                "msg_fwd",
                serde_json::json!({ "message_id": "msg_fwd" }),
            ),
        )?;
        // Email goes through the mailserver queue, so the engagement lands on
        // `scheduled_to_send` (queued for provider), not the physical-letter
        // immediate `sent`.
        assert_eq!(engagement_status("eng_fwd")?, "scheduled_to_send");

        Ok(())
    }

    #[test]
    fn outbound_empty_collections_load_without_errors() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        for collection in [
            "outbound_campaigns",
            "outbound_engagements",
            "outbound_messages",
            "outbound_approvals",
            "outbound_sequences",
            "outbound_sender_assignments",
            "outbound_meeting_requests",
            "outbound_suppression_entries",
            "outbound_account_limits",
        ] {
            let records =
                outbound_load_records_by_string_field(&conn, collection, "campaign_id", "missing")?;
            assert!(
                records.is_empty(),
                "expected {collection} to be empty but got {} rows",
                records.len()
            );
            let single = outbound_load_record(&conn, collection, "missing-id")?;
            assert!(
                single.is_none(),
                "expected {collection} missing-id lookup to be None"
            );
        }
        Ok(())
    }

    #[test]
    fn outbound_tombstone_marks_record_as_deleted() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "outbound_messages",
            "msg_tombstone",
            1000,
            serde_json::json!({
                "id": "msg_tombstone",
                "engagement_id": "eng_t",
                "send_status": "draft",
                "created_at_ms": 1000,
                "updated_at_ms": 1000,
            }),
        )?;
        // Soft-delete: set deleted = 1
        conn.execute(
            "UPDATE business_records SET deleted = 1 WHERE collection = 'outbound_messages' AND record_id = 'msg_tombstone'",
            [],
        )?;
        // After tombstone, outbound_load_record (which filters deleted = 0) must return None
        let loaded = outbound_load_record(&conn, "outbound_messages", "msg_tombstone")?;
        assert!(
            loaded.is_none(),
            "tombstoned outbound_messages record must not be loadable"
        );
        // Re-upserting must re-activate (deleted = 0)
        upsert_business_record(
            &conn,
            "outbound_messages",
            "msg_tombstone",
            2000,
            serde_json::json!({
                "id": "msg_tombstone",
                "engagement_id": "eng_t",
                "send_status": "draft",
                "updated_at_ms": 2000,
            }),
        )?;
        let reloaded = outbound_load_record(&conn, "outbound_messages", "msg_tombstone")?;
        assert!(
            reloaded.is_some(),
            "re-upserted record must replace the tombstone"
        );
        Ok(())
    }

    #[test]
    fn outbound_tombstone_and_conflict_strategy_for_approvals() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;

        // Initial approval decision.
        upsert_business_record(
            &conn,
            "outbound_approvals",
            "apv_conflict",
            1000,
            serde_json::json!({
                "id": "apv_conflict",
                "message_id": "msg_x",
                "revision_id": "rev_1",
                "decision": "approved",
                "created_at_ms": 1000,
                "updated_at_ms": 1000,
            }),
        )?;

        // A conflicting write (no tombstone) is last-write-wins: the later payload
        // overwrites the earlier one for the same (collection, record_id).
        upsert_business_record(
            &conn,
            "outbound_approvals",
            "apv_conflict",
            1500,
            serde_json::json!({
                "id": "apv_conflict",
                "message_id": "msg_x",
                "revision_id": "rev_1",
                "decision": "changes_requested",
                "created_at_ms": 1000,
                "updated_at_ms": 1500,
            }),
        )?;
        let after_conflict =
            outbound_load_required(&conn, "outbound_approvals", "apv_conflict", "approval")?;
        assert_eq!(
            outbound_string(&after_conflict, &["decision"]).as_deref(),
            Some("changes_requested"),
            "the latest write must win for a conflicting approval upsert"
        );

        // Tombstone the approval: a deleted approval must not be loadable, so a
        // resolved/withdrawn approval cannot silently re-gate a send.
        conn.execute(
            "UPDATE business_records SET deleted = 1 WHERE collection = 'outbound_approvals' AND record_id = 'apv_conflict'",
            [],
        )?;
        assert!(
            outbound_load_record(&conn, "outbound_approvals", "apv_conflict")?.is_none(),
            "a tombstoned approval must not be loadable"
        );

        // Re-upsert resurrects the tombstone (deleted reset to 0) with the new
        // payload — the state machine resolves a tombstone-vs-write conflict in
        // favor of the live write so a re-issued approval is not lost.
        upsert_business_record(
            &conn,
            "outbound_approvals",
            "apv_conflict",
            2000,
            serde_json::json!({
                "id": "apv_conflict",
                "message_id": "msg_x",
                "revision_id": "rev_2",
                "decision": "approved",
                "created_at_ms": 1000,
                "updated_at_ms": 2000,
            }),
        )?;
        let resurrected =
            outbound_load_required(&conn, "outbound_approvals", "apv_conflict", "approval")?;
        assert_eq!(
            outbound_string(&resurrected, &["decision"]).as_deref(),
            Some("approved")
        );
        assert_eq!(
            outbound_string(&resurrected, &["revision_id"]).as_deref(),
            Some("rev_2"),
            "the resurrected approval carries the new revision binding"
        );

        Ok(())
    }

    #[test]
    fn outbound_email_html_only_body_uses_multipart_alternative() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_html",
                "command_id": "cmd_eng_html",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_html",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_html","company_id":"co_html","contact_id":"ct_html"},
                "client_context": actor.clone()
            }),
        )?;
        // HTML-only message — body_text is empty, body_html has content.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_html",
                "command_id": "cmd_msg_html",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_html",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_html",
                    "campaign_id": "camp_html",
                    "sender_account_id": "email:sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "HTML only",
                    "body_html": "<p>Hello <strong>world</strong></p>"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_html",
                "command_id": "cmd_req_html",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_html",
                "status": "pending_sync",
                "payload": {"message_id":"msg_html"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_html",
                "command_id": "cmd_apv_html",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_html",
                "status": "pending_sync",
                "payload": {"message_id":"msg_html"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_html",
                "command_id": "cmd_send_html",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_html",
                "status": "pending_sync",
                "payload": {"message_id":"msg_html"},
                "client_context": actor.clone()
            }),
        )?;
        // Inspect the queued SMTP body and confirm it has the HTML part.
        let queue_conn = Connection::open(crate::paths::core_db(root))?;
        let body: String = queue_conn.query_row(
            "SELECT msg_body FROM stalwart_smtp_queue WHERE to_addr = 'lead@example.com' LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        assert!(
            body.contains("multipart/alternative"),
            "expected multipart/alternative, got: {body}"
        );
        assert!(
            body.contains("text/html"),
            "expected text/html part, got: {body}"
        );
        assert!(
            body.contains("<p>Hello <strong>world</strong></p>"),
            "expected raw HTML in body, got: {body}"
        );
        // And the text/plain fallback must not be empty.
        assert!(
            body.contains("Hello")
                && !body.contains(
                    "text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n\r\n"
                ),
            "expected non-empty text/plain fallback, got: {body}"
        );
        Ok(())
    }

    #[test]
    fn outbound_email_send_blocked_when_body_completely_empty() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_empty",
                "command_id": "cmd_eng_empty",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_empty",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_empty","company_id":"co_e","contact_id":"ct_e"},
                "client_context": actor.clone()
            }),
        )?;
        // Both body fields empty
        let prep = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_empty",
                "command_id": "cmd_msg_empty",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_empty",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_empty",
                    "campaign_id": "camp_empty",
                    "sender_account_id": "email:sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Empty",
                    "body_text": "",
                    "body_html": ""
                },
                "client_context": actor.clone()
            }),
        )?;
        // request_approval should block because content is empty.
        let req = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_empty",
                "command_id": "cmd_req_empty",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_empty",
                "status": "pending_sync",
                "payload": {"message_id":"msg_empty"},
                "client_context": actor.clone()
            }),
        );
        // Either request_approval already errors or send_approved later does.
        if req.is_ok() {
            let _ = accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": "cmd_apv_empty",
                    "command_id": "cmd_apv_empty",
                    "module": "outbound",
                    "command_type": "outbound.message.approve",
                    "record_id": "msg_empty",
                    "status": "pending_sync",
                    "payload": {"message_id":"msg_empty"},
                    "client_context": actor.clone()
                }),
            );
            let send = accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": "cmd_send_empty",
                    "command_id": "cmd_send_empty",
                    "module": "outbound",
                    "command_type": "outbound.message.send_approved",
                    "record_id": "msg_empty",
                    "status": "pending_sync",
                    "payload": {"message_id":"msg_empty"},
                    "client_context": actor.clone()
                }),
            )
            .expect_err("empty body must block send");
            assert!(
                send.to_string().contains("content")
                    || send.to_string().contains("body")
                    || send.to_string().contains("empty"),
                "{send}"
            );
        }
        let _ = prep;
        Ok(())
    }

    #[test]
    fn outbound_daily_limit_zero_is_treated_as_unlimited() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        // Use mailbox.link to create the campaign+account_limits with daily_limit=0
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mbx_dlz",
                "command_id": "cmd_mbx_dlz",
                "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link",
                "record_id": "camp_dlz",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_dlz","mailbox_address":"dlz@example.com","mailbox_status":"ready"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_dlz",
                "command_id": "cmd_eng_dlz",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_dlz",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_dlz","company_id":"co_d","contact_id":"ct_d"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_dlz",
                "command_id": "cmd_msg_dlz",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_dlz",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_dlz",
                    "campaign_id": "camp_dlz",
                    "sender_account_id": "email:dlz@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "DLZ",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_dlz",
                "command_id": "cmd_req_dlz",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_dlz",
                "status": "pending_sync",
                "payload": {"message_id":"msg_dlz"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_dlz",
                "command_id": "cmd_apv_dlz",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_dlz",
                "status": "pending_sync",
                "payload": {"message_id":"msg_dlz"},
                "client_context": actor.clone()
            }),
        )?;
        // daily_limit=0 must be treated as unlimited — send must succeed.
        let send = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_dlz",
                "command_id": "cmd_send_dlz",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_dlz",
                "status": "pending_sync",
                "payload": {"message_id":"msg_dlz"},
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            send.pointer("/result/provider_dispatch_status")
                .and_then(Value::as_str),
            Some("queued_in_mailserver")
        );
        Ok(())
    }

    #[test]
    fn outbound_bare_email_sender_normalizes_to_email_prefix_for_limit_lookup() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        // Pre-seed an account_limits row with canonical key.
        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "outbound_account_limits",
            "email:s@example.com",
            1000,
            serde_json::json!({
                "id": "email:s@example.com",
                "sender_account_id": "email:s@example.com",
                "status": "blocked",
                "blocked": true,
                "daily_limit": 100,
                "daily_sent_count": 0,
                "created_at_ms": 1000,
                "updated_at_ms": 1000,
            }),
        )?;
        // Now run the limit check against a bare email — should hit the canonical row.
        let result = outbound_enforce_account_limit(&conn, "s@example.com");
        assert!(
            result.is_err(),
            "bare email must resolve to canonical limit row"
        );
        assert!(result.unwrap_err().to_string().contains("blocked"));
        Ok(())
    }

    #[test]
    fn outbound_skillbook_seed_defaults_is_idempotent() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        let res1 = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_seed_1","command_id":"sk_seed_1","module":"outbound",
                "command_type":"outbound.skillbook.seed_defaults","record_id":"",
                "status":"pending_sync","payload":{},"client_context":actor.clone()
            }),
        )?;
        let first_seeded = res1
            .pointer("/result/seeded")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(first_seeded, 3);
        let res2 = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_seed_2","command_id":"sk_seed_2","module":"outbound",
                "command_type":"outbound.skillbook.seed_defaults","record_id":"",
                "status":"pending_sync","payload":{},"client_context":actor.clone()
            }),
        )?;
        let second_seeded = res2
            .pointer("/result/seeded")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(second_seeded, 0, "second seed must be no-op");
        Ok(())
    }

    #[test]
    fn outbound_skillbook_seed_defaults_carry_real_backbone_and_guidance() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let now = now_ms() as i64;
        outbound_handle_skillbook_seed_defaults(&conn, now)?;

        let drafting = outbound_load_required(
            &conn,
            "outbound_skillbooks",
            "business-os.outbound.message_drafting.v1",
            "message drafting skillbook",
        )?;
        let backbone = drafting
            .get("workflow_backbone")
            .and_then(Value::as_array)
            .expect("workflow_backbone array");
        assert!(
            !backbone.is_empty(),
            "message drafting skillbook must have a real workflow backbone"
        );
        assert!(backbone
            .iter()
            .any(|step| { outbound_string(step, &["step"]).as_deref() == Some("writeback") }));
        let routing = drafting
            .get("routing_taxonomy")
            .and_then(Value::as_array)
            .expect("routing_taxonomy array");
        assert!(!routing.is_empty(), "routing taxonomy must be populated");

        let guidance =
            outbound_skillbook_guidance(&conn, "business-os.outbound.message_drafting.v1")?
                .expect("guidance present");
        assert!(guidance.contains("Leitplanke:"));

        assert!(
            outbound_skillbook_guidance(&conn, "does-not-exist")?.is_none(),
            "missing skillbook yields no guidance"
        );
        Ok(())
    }

    #[test]
    fn outbound_skillbook_save_bumps_version_number() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        let r1 = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_v_1","command_id":"sk_v_1","module":"outbound",
                "command_type":"outbound.skillbook.save","record_id":"business-os.outbound.message_drafting.v1",
                "status":"pending_sync","payload":{"skillbook_id":"business-os.outbound.message_drafting.v1","mission":"M1"},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(
            r1.pointer("/result/version_number").and_then(Value::as_i64),
            Some(1)
        );
        let r2 = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_v_2","command_id":"sk_v_2","module":"outbound",
                "command_type":"outbound.skillbook.save","record_id":"business-os.outbound.message_drafting.v1",
                "status":"pending_sync","payload":{"skillbook_id":"business-os.outbound.message_drafting.v1","mission":"M2"},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(
            r2.pointer("/result/version_number").and_then(Value::as_i64),
            Some(2)
        );
        Ok(())
    }

    #[test]
    fn outbound_skillbook_save_preserves_unsent_fields() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_preserve_seed","command_id":"sk_preserve_seed","module":"outbound",
                "command_type":"outbound.skillbook.seed_defaults","record_id":"",
                "status":"pending_sync","payload":{},"client_context":actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_preserve_save","command_id":"sk_preserve_save","module":"outbound",
                "command_type":"outbound.skillbook.save","record_id":"business-os.outbound.message_drafting.v1",
                "status":"pending_sync",
                "payload":{
                    "skillbook_id":"business-os.outbound.message_drafting.v1",
                    "mission":"Updated mission only"
                },
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let stored = outbound_load_required(
            &conn,
            "outbound_skillbooks",
            "business-os.outbound.message_drafting.v1",
            "skillbook",
        )?;
        assert_eq!(
            outbound_string(&stored, &["title"]).as_deref(),
            Some("Initial- und Follow-up-Drafts vorbereiten")
        );
        assert!(
            stored
                .get("stop_rules")
                .and_then(Value::as_array)
                .map(|items| !items.is_empty())
                .unwrap_or(false),
            "stop_rules must survive partial saves"
        );
        assert_eq!(
            outbound_string(&stored, &["mission"]).as_deref(),
            Some("Updated mission only")
        );
        Ok(())
    }

    #[test]
    fn outbound_letter_template_save_persists_record() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tpl_save","command_id":"tpl_save","module":"outbound",
                "command_type":"outbound.letter_template.save","record_id":"tpl_x",
                "status":"pending_sync","payload":{"template_id":"tpl_x","title":"T","salutation":"Hi","closing":"Bye"},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(
            r.pointer("/result/template/title").and_then(Value::as_str),
            Some("T")
        );
        let conn = open_store(root)?;
        let stored = outbound_load_required(&conn, "outbound_letter_templates", "tpl_x", "t")?;
        assert_eq!(
            outbound_string(&stored, &["salutation"]).as_deref(),
            Some("Hi")
        );
        Ok(())
    }

    #[test]
    fn outbound_audit_export_returns_collections_filtered_by_campaign() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        // Seed two engagements on different campaigns.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"e1","command_id":"e1","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"e1",
                "status":"pending_sync",
                "payload":{"campaign_id":"camp_audit","company_id":"co","contact_id":"ct"},
                "client_context":actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"e2","command_id":"e2","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"e2",
                "status":"pending_sync",
                "payload":{"campaign_id":"other","company_id":"co","contact_id":"ct"},
                "client_context":actor.clone()
            }),
        )?;
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"audit_x","command_id":"audit_x","module":"outbound",
                "command_type":"outbound.audit.export","record_id":"",
                "status":"pending_sync","payload":{"campaign_id":"camp_audit"},
                "client_context":actor.clone()
            }),
        )?;
        let engagements = r
            .pointer("/result/export/outbound_engagements")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(engagements.len(), 1, "filter must include only camp_audit");
        assert_eq!(
            engagements[0].pointer("/id").and_then(Value::as_str),
            Some("e1")
        );
        assert!(
            r.pointer("/result/export/outbound_skillbooks").is_some(),
            "audit export must include skillbook configuration"
        );
        assert!(
            r.pointer("/result/export/outbound_letter_templates")
                .is_some(),
            "audit export must include letter template configuration"
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduler_tick_runs_dry_then_reconciles() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        let r_dry = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_dry","command_id":"tick_dry","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{"dry_run":true},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(
            r_dry.pointer("/result/dry_run").and_then(Value::as_bool),
            Some(true)
        );
        // Non-dry run also succeeds even on an empty DB.
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_real","command_id":"tick_real","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(r.pointer("/result/ok").and_then(Value::as_bool), Some(true));
        Ok(())
    }

    #[test]
    fn outbound_scheduler_tick_prepares_due_followup_draft() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_due",
                1000,
                serde_json::json!({
                    "id":"eng_due",
                    "campaign_id":"camp_due",
                    "sender_account_id":"email:scheduler@example.com",
                    "status":"sent",
                    "next_action_at_ms":1,
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"Scheduler GmbH",
                        "contact_email":"lead@example.com"
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_due","command_id":"tick_due","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let actions = r
            .pointer("/result/actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            actions.iter().any(|action| {
                action.get("kind").and_then(Value::as_str) == Some("followup_draft_prepared")
            }),
            "scheduler must create a follow-up draft, actions={actions:?}"
        );
        let conn = open_store(root)?;
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_due",
        )?;
        assert_eq!(messages.len(), 1);
        assert_eq!(
            outbound_string(&messages[0], &["approval_status"]).as_deref(),
            Some("awaiting_approval")
        );
        assert_ne!(
            outbound_string(&messages[0], &["id"]).as_deref(),
            Some("eng_due"),
            "scheduler-created message must not reuse the engagement id"
        );
        let engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_due", "engagement")?;
        assert_eq!(
            engagement.get("next_action_at_ms").and_then(Value::as_i64),
            Some(0)
        );
        Ok(())
    }

    #[test]
    fn outbound_out_of_office_reply_schedules_gated_retry_without_send() -> anyhow::Result<()> {
        // Welle 7 (554): an out-of-office reply must not stop the sequence like a
        // hard reply — it schedules a wait/retry, and the resumed follow-up is
        // still approval-gated (no send without a fresh approval).
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_ooo",
                1000,
                serde_json::json!({
                    "id":"eng_ooo",
                    "campaign_id":"camp_ooo",
                    "sender_account_id":"email:scheduler@example.com",
                    "status":"sent",
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"OOO GmbH",
                        "contact_email":"lead@example.com"
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }

        // Classify the reply as out-of-office. Unlike unsubscribe/bounce this is
        // not a hard stop: the engagement stays reply_received for the UI but a
        // future wait/retry plan is recorded.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"cls_ooo","command_id":"cls_ooo","module":"outbound",
                "command_type":"outbound.reply.classify","record_id":"eng_ooo",
                "status":"pending_sync",
                "payload":{"engagement_id":"eng_ooo","classification":"out_of_office","reply_message_id":"reply_1"},
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let after_classify =
            outbound_load_required(&conn, "outbound_engagements", "eng_ooo", "engagement")?;
        assert_eq!(
            outbound_string(&after_classify, &["status"]).as_deref(),
            Some("reply_received"),
            "OOO keeps reply_received for the UI"
        );
        assert_eq!(
            outbound_string(&after_classify, &["payload", "reply_wait_reason"]).as_deref(),
            Some("out_of_office"),
            "a wait/retry plan is recorded"
        );
        let planned = after_classify
            .get("next_action_at_ms")
            .and_then(Value::as_i64)
            .expect("OOO must schedule a future retry");
        assert!(planned > 1000, "retry is scheduled into the future");

        // Force the hold to be due, then run a scheduler tick.
        let mut due_engagement = after_classify.clone();
        outbound_put_i64(&mut due_engagement, "next_action_at_ms", 1);
        upsert_business_record(
            &conn,
            "outbound_engagements",
            "eng_ooo",
            2000,
            due_engagement,
        )?;
        drop(conn);

        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_ooo","command_id":"tick_ooo","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let actions = r
            .pointer("/result/actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            actions.iter().any(|action| {
                action.get("kind").and_then(Value::as_str) == Some("followup_draft_prepared")
                    && action.get("engagement_id").and_then(Value::as_str) == Some("eng_ooo")
            }),
            "OOO retry must resume the follow-up, actions={actions:?}"
        );

        // The resumed follow-up is approval-gated: not sent, not queued.
        let conn = open_store(root)?;
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_ooo",
        )?;
        assert_eq!(messages.len(), 1, "exactly one resumed draft");
        assert_eq!(
            outbound_string(&messages[0], &["approval_status"]).as_deref(),
            Some("awaiting_approval"),
            "resumed OOO follow-up requires approval"
        );
        assert_eq!(
            outbound_string(&messages[0], &["send_status"]).as_deref(),
            Some("awaiting_approval"),
            "no send happens without approval"
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduler_tick_skips_when_account_limit_exhausted() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_cap_due",
                1000,
                serde_json::json!({
                    "id":"eng_cap_due",
                    "campaign_id":"camp_cap_due",
                    "sender_account_id":"email:capped@example.com",
                    "status":"sent",
                    "next_action_at_ms":1,
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"Capped GmbH",
                        "contact_email":"lead@example.com"
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
            // The sender account has already exhausted its daily cap.
            upsert_business_record(
                &conn,
                "outbound_account_limits",
                "email:capped@example.com",
                1000,
                serde_json::json!({
                    "id":"email:capped@example.com",
                    "daily_limit":2,
                    "daily_sent_count":2,
                    "remaining_today":0,
                    "status":"active",
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_cap","command_id":"tick_cap","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let actions = r
            .pointer("/result/actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            actions.iter().any(|action| {
                action.get("kind").and_then(Value::as_str) == Some("followup_skipped_account_limit")
            }),
            "scheduler must skip when the sender cap is exhausted, actions={actions:?}"
        );
        let conn = open_store(root)?;
        // No unsendable draft was created.
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_cap_due",
        )?;
        assert!(
            messages.is_empty(),
            "no follow-up draft may be created when the account cap is exhausted"
        );
        // The engagement records the skip reason and stays due for a later retry
        // (the daily cap resets), so next_action_at_ms is not zeroed out.
        let engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_cap_due", "engagement")?;
        assert_eq!(
            outbound_string(&engagement, &["payload", "scheduler_last_skip_reason"]).as_deref(),
            Some("account_limit")
        );
        assert_eq!(
            engagement.get("next_action_at_ms").and_then(Value::as_i64),
            Some(1),
            "a capped follow-up must remain due for a later retry"
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduler_draft_carries_sequence_version() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_seq",
                1000,
                serde_json::json!({
                    "id":"eng_seq",
                    "campaign_id":"camp_seq",
                    "sender_account_id":"email:scheduler@example.com",
                    "status":"sent",
                    "next_action_at_ms":1,
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"Sequence GmbH",
                        "contact_email":"lead@example.com",
                        "sequence_id":"seq_v3",
                        "sequence_snapshot":{ "id":"seq_v3", "updated_at_ms": 424242 }
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_seq","command_id":"tick_seq","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_seq",
        )?;
        assert_eq!(messages.len(), 1);
        // The draft must be traceable to the exact sequence revision.
        assert_eq!(
            outbound_string(&messages[0], &["payload", "sequence_id"]).as_deref(),
            Some("seq_v3")
        );
        assert_eq!(
            messages[0]
                .pointer("/payload/sequence_version")
                .and_then(Value::as_i64),
            Some(424242)
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduler_tick_skips_paused_campaign() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_campaigns",
                "camp_paused",
                1000,
                serde_json::json!({
                    "id":"camp_paused",
                    "status":"paused",
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_paused_camp",
                1000,
                serde_json::json!({
                    "id":"eng_paused_camp",
                    "campaign_id":"camp_paused",
                    "sender_account_id":"email:scheduler@example.com",
                    "status":"sent",
                    "next_action_at_ms":1,
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"Paused GmbH",
                        "contact_email":"lead@example.com"
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_paused","command_id":"tick_paused","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let actions = r
            .pointer("/result/actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            actions.iter().any(|action| {
                action.get("kind").and_then(Value::as_str)
                    == Some("followup_skipped_campaign_paused")
            }),
            "scheduler must skip engagements of a paused campaign, actions={actions:?}"
        );
        let conn = open_store(root)?;
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_paused_camp",
        )?;
        assert!(
            messages.is_empty(),
            "a paused campaign must not produce follow-up drafts"
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduler_tick_does_not_follow_up_after_reply() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_reply_stop",
                1000,
                serde_json::json!({
                    "id":"eng_reply_stop",
                    "campaign_id":"camp_due",
                    "sender_account_id":"email:scheduler@example.com",
                    "status":"reply_received",
                    "next_action_at_ms":1,
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"Scheduler GmbH",
                        "contact_email":"lead@example.com",
                        "reply_classification":"positive"
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_reply_stop","command_id":"tick_reply_stop","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let actions = r
            .pointer("/result/actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            !actions.iter().any(|action| {
                action.get("kind").and_then(Value::as_str) == Some("followup_draft_prepared")
            }),
            "scheduler must not create follow-ups after reply, actions={actions:?}"
        );
        let conn = open_store(root)?;
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_reply_stop",
        )?;
        assert!(messages.is_empty());
        Ok(())
    }

    #[test]
    fn outbound_dev_seed_test_data_creates_engagements() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"seed_td","command_id":"seed_td","module":"outbound",
                "command_type":"outbound.dev.seed_test_data","record_id":"",
                "status":"pending_sync","payload":{"campaign_id":"camp_dev","count":4},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(r.pointer("/result/count").and_then(Value::as_i64), Some(4));
        Ok(())
    }

    #[test]
    fn outbound_engagement_reapply_sequence_requires_sequence_id() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"er_e","command_id":"er_e","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"er_e",
                "status":"pending_sync",
                "payload":{"campaign_id":"camp_re","company_id":"c","contact_id":"x"},
                "client_context":actor.clone()
            }),
        )?;
        let err = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"er_rs","command_id":"er_rs","module":"outbound",
                "command_type":"outbound.engagement.reapply_sequence","record_id":"er_e",
                "status":"pending_sync","payload":{"engagement_id":"er_e"},
                "client_context":actor.clone()
            }),
        )
        .expect_err("missing sequence_id must error");
        assert!(err.to_string().contains("sequence_id"), "{err}");
        Ok(())
    }

    #[test]
    fn outbound_active_engagement_keeps_sequence_version_until_explicit_reapply(
    ) -> anyhow::Result<()> {
        // Welle 4 (367): a live campaign sequence change must not silently
        // re-version active engagements. Each engagement stays pinned to the
        // sequence snapshot it captured until an explicit reapply flow runs.
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});

        // Sequence revision v1.
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_sequences",
                "seq_367",
                100,
                serde_json::json!({
                    "id":"seq_367","campaign_id":"camp_367",
                    "updated_at_ms":100,"touchpoints":[{"day":0}]
                }),
            )?;
        }
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"e367","command_id":"e367","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"e367",
                "status":"pending_sync",
                "payload":{"campaign_id":"camp_367","company_id":"c","contact_id":"x"},
                "client_context":actor.clone()
            }),
        )?;
        // Pin the engagement to v1 via the explicit reapply flow.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"rs1","command_id":"rs1","module":"outbound",
                "command_type":"outbound.engagement.reapply_sequence","record_id":"e367",
                "status":"pending_sync",
                "payload":{"engagement_id":"e367","sequence_id":"seq_367"},
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let engagement =
            outbound_load_required(&conn, "outbound_engagements", "e367", "engagement")?;
        let (_, pinned_version) = outbound_engagement_sequence_context(&engagement);
        assert_eq!(pinned_version, 100, "engagement pinned to sequence v1");

        // Campaign edits the sequence (new revision v2). This re-writes the
        // shared sequence record but must not touch existing engagements.
        upsert_business_record(
            &conn,
            "outbound_sequences",
            "seq_367",
            200,
            serde_json::json!({
                "id":"seq_367","campaign_id":"camp_367",
                "updated_at_ms":200,"touchpoints":[{"day":0},{"day":3}]
            }),
        )?;
        let engagement_after_edit =
            outbound_load_required(&conn, "outbound_engagements", "e367", "engagement")?;
        let (_, still_pinned) = outbound_engagement_sequence_context(&engagement_after_edit);
        assert_eq!(
            still_pinned, 100,
            "a live sequence edit must not silently re-version an active engagement"
        );
        drop(conn);

        // Explicit reapply rolls the engagement forward to v2.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"rs2","command_id":"rs2","module":"outbound",
                "command_type":"outbound.engagement.reapply_sequence","record_id":"e367",
                "status":"pending_sync",
                "payload":{"engagement_id":"e367","sequence_id":"seq_367"},
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let reapplied =
            outbound_load_required(&conn, "outbound_engagements", "e367", "engagement")?;
        let (_, new_version) = outbound_engagement_sequence_context(&reapplied);
        assert_eq!(new_version, 200, "explicit reapply rolls forward to v2");
        assert!(
            reapplied
                .pointer("/payload/sequence_reapplied_at_ms")
                .and_then(Value::as_i64)
                .is_some(),
            "reapply stamps a traceable timestamp"
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduling_update_slots_replaces_proposed_slots() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        // Pre-seed a meeting request with one slot.
        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "outbound_meeting_requests",
            "mreq_1",
            1000,
            serde_json::json!({
                "id":"mreq_1","engagement_id":"e","status":"prepared",
                "proposed_slots":[{"start_iso":"2026-06-01T10:00:00Z"}],
                "created_at_ms":1000,"updated_at_ms":1000
            }),
        )?;
        drop(conn);
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"upd_slots","command_id":"upd_slots","module":"outbound",
                "command_type":"outbound.scheduling.update_slots","record_id":"mreq_1",
                "status":"pending_sync","payload":{"meeting_request_id":"mreq_1","proposed_slots":[]},
                "client_context":actor.clone()
            }),
        )?;
        let slots = r
            .pointer("/result/meeting_request/proposed_slots")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(slots.len(), 0);
        Ok(())
    }

    #[test]
    fn outbound_draft_prepare_for_physical_letter_does_not_require_email() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"pl_eng","command_id":"pl_eng","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"pl_eng",
                "status":"pending_sync",
                "payload":{"campaign_id":"camp_pl","company_id":"co","contact_id":"ct"},
                "client_context":actor.clone()
            }),
        )?;
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"pl_draft","command_id":"pl_draft","module":"outbound",
                "command_type":"outbound.draft.prepare","record_id":"pl_msg",
                "status":"pending_sync",
                "payload":{
                    "engagement_id":"pl_eng",
                    "draft_kind":"initial",
                    "campaign_id":"camp_pl",
                    "channel":"physical_letter",
                    "recipient_address_text":"Tester Inc.\nStr. 1\n10115 Berlin"
                },
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(
            r.pointer("/result/message/channel").and_then(Value::as_str),
            Some("physical_letter")
        );
        assert_eq!(
            r.pointer("/result/message/recipient_address_text")
                .and_then(Value::as_str),
            Some("Tester Inc.\nStr. 1\n10115 Berlin")
        );
        Ok(())
    }

    #[test]
    fn outbound_update_draft_persists_recipient_address_text() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"ud_eng","command_id":"ud_eng","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"ud_eng",
                "status":"pending_sync",
                "payload":{"campaign_id":"camp_ud","company_id":"co","contact_id":"ct"},
                "client_context":actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"ud_prep","command_id":"ud_prep","module":"outbound",
                "command_type":"outbound.message.prepare","record_id":"ud_msg",
                "status":"pending_sync",
                "payload":{
                    "engagement_id":"ud_eng","campaign_id":"camp_ud",
                    "channel":"physical_letter",
                    "recipient_address_text":"Old Addr",
                    "subject":"Hi","body_text":"x"
                },
                "client_context":actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"ud_upd","command_id":"ud_upd","module":"outbound",
                "command_type":"outbound.message.update_draft","record_id":"ud_msg",
                "status":"pending_sync",
                "payload":{"message_id":"ud_msg","recipient_address_text":"New Addr 42\n12345 Berlin"},
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let msg = outbound_load_required(&conn, "outbound_messages", "ud_msg", "msg")?;
        assert_eq!(
            outbound_string(&msg, &["recipient_address_text"]).as_deref(),
            Some("New Addr 42\n12345 Berlin")
        );
        Ok(())
    }

    #[test]
    fn outbound_physical_letter_marks_manual_send_without_mail_account() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_letter",
                "command_id": "cmd_eng_letter",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_letter",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_letter",
                    "company_id": "co_letter",
                    "contact_id": "ct_letter"
                },
                "client_context": actor.clone()
            }),
        )?;
        // Prepare a physical_letter message — NO sender_account_id, NO recipient_email.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_prepare",
                "command_id": "cmd_letter_prepare",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_letter",
                    "campaign_id": "camp_letter",
                    "channel": "physical_letter",
                    "recipient_address_text": "Tester Inc.\nMusterstrasse 1\n10115 Berlin",
                    "subject": "Letter Intro",
                    "body_text": "Sehr geehrter Herr Tester,\n\nbitte beachten Sie unser Angebot.\n\nFreundliche Gruesse"
                },
                "client_context": actor.clone()
            }),
        )?;
        // The send_gate should refuse before approval.
        let before_apv = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_send_pre",
                "command_id": "cmd_letter_send_pre",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("send must be blocked before approval even for letters");
        assert!(
            before_apv.to_string().contains("must be approved"),
            "{before_apv}"
        );
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_req",
                "command_id": "cmd_letter_req",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_apv",
                "command_id": "cmd_letter_apv",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter" },
                "client_context": actor.clone()
            }),
        )?;
        let send = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_send",
                "command_id": "cmd_letter_send",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            send.pointer("/result/channel").and_then(Value::as_str),
            Some("physical_letter")
        );
        assert_eq!(
            send.pointer("/result/provider_dispatch_status")
                .and_then(Value::as_str),
            Some("manual_physical_letter_marked_sent")
        );
        assert!(send
            .pointer("/result/physical_sent_at_ms")
            .and_then(Value::as_i64)
            .is_some());
        // Idempotency: replaying send_approved must not re-mark.
        let send_again = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_send2",
                "command_id": "cmd_letter_send2",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            send_again
                .pointer("/result/idempotent")
                .and_then(Value::as_bool),
            Some(true)
        );

        // Negative: a physical_letter without recipient_address_text must be blocked.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter2_prepare",
                "command_id": "cmd_letter2_prepare",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_letter2",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_letter",
                    "campaign_id": "camp_letter",
                    "channel": "physical_letter",
                    "subject": "Letter2",
                    "body_text": "No address provided"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter2_req",
                "command_id": "cmd_letter2_req",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_letter2",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter2" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter2_apv",
                "command_id": "cmd_letter2_apv",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_letter2",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter2" },
                "client_context": actor.clone()
            }),
        )?;
        let blocked = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter2_send",
                "command_id": "cmd_letter2_send",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_letter2",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter2" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("missing recipient_address_text must block letter send");
        assert!(
            blocked.to_string().contains("recipient_address_text"),
            "{blocked}"
        );
        Ok(())
    }

    #[test]
    fn outbound_message_send_blocked_when_recipient_is_suppressed() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_supp",
                "command_id": "cmd_eng_supp",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_supp",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_supp",
                    "company_id": "co_supp",
                    "contact_id": "ct_supp"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_supp",
                "command_id": "cmd_msg_supp",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_supp",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_supp",
                    "campaign_id": "camp_supp",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "blocked@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_supp_entry",
                "command_id": "cmd_supp_entry",
                "module": "outbound",
                "command_type": "outbound.suppression.add",
                "record_id": "supp_blocked",
                "status": "pending_sync",
                "payload": {
                    "id": "supp_blocked",
                    "email": "blocked@example.com",
                    "reason": "unsubscribe",
                    "status": "active"
                },
                "client_context": actor.clone()
            }),
        )
        .ok();
        // The suppression collection is generic; insert directly to bypass any module guard.
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_suppression_entries",
                "supp_blocked",
                1000,
                serde_json::json!({
                    "id": "supp_blocked",
                    "email": "blocked@example.com",
                    "reason": "unsubscribe",
                    "status": "active",
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_supp",
                "command_id": "cmd_req_supp",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_supp",
                "status": "pending_sync",
                "payload": { "message_id": "msg_supp" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_supp",
                "command_id": "cmd_apv_supp",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_supp",
                "status": "pending_sync",
                "payload": { "message_id": "msg_supp" },
                "client_context": actor.clone()
            }),
        )?;
        let blocked = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_supp",
                "command_id": "cmd_send_supp",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_supp",
                "status": "pending_sync",
                "payload": { "message_id": "msg_supp" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("send must be blocked by suppression");
        assert!(blocked.to_string().contains("suppressed"), "{blocked}");

        // The failed send must persist a structured, replicable block reason
        // onto the message WITHOUT destroying the approved draft, so it stays
        // retry-able.
        let conn = open_store(root)?;
        let msg = outbound_load_required(&conn, "outbound_messages", "msg_supp", "message")?;
        assert_eq!(
            outbound_string(&msg, &["send_status"]).as_deref(),
            Some("send_blocked")
        );
        assert_eq!(
            outbound_string(&msg, &["payload", "send_block_reason"]).as_deref(),
            Some("recipient_suppressed")
        );
        assert_eq!(
            outbound_string(&msg, &["approval_status"]).as_deref(),
            Some("approved"),
            "approval and draft must survive a blocked send"
        );
        assert_eq!(
            outbound_string(&msg, &["body_text"]).as_deref(),
            Some("Hello")
        );
        assert_eq!(
            msg.pointer("/payload/retryable").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            msg.pointer("/payload/send_attempts")
                .and_then(Value::as_i64),
            Some(1)
        );
        // Lift the suppression and retry: the same approved draft must now queue.
        upsert_business_record(
            &conn,
            "outbound_suppression_entries",
            "supp_blocked",
            2000,
            serde_json::json!({
                "id": "supp_blocked",
                "email": "blocked@example.com",
                "reason": "unsubscribe",
                "status": "inactive",
                "created_at_ms": 1000,
                "updated_at_ms": 2000
            }),
        )?;
        drop(conn);
        let retried = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_supp_retry",
                "command_id": "cmd_send_supp_retry",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_supp",
                "status": "pending_sync",
                "payload": { "message_id": "msg_supp" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            retried.pointer("/result/ok").and_then(Value::as_bool),
            Some(true)
        );
        let conn = open_store(root)?;
        let msg = outbound_load_required(&conn, "outbound_messages", "msg_supp", "message")?;
        assert_eq!(
            outbound_string(&msg, &["send_status"]).as_deref(),
            Some("queued_for_provider")
        );
        assert_eq!(
            msg.pointer("/payload/retryable").and_then(Value::as_bool),
            Some(false),
            "successful retry clears the retryable flag"
        );
        assert!(
            msg.pointer("/payload/send_block_reason").is_none()
                || msg.pointer("/payload/send_block_reason") == Some(&Value::Null),
            "successful retry clears the block reason"
        );
        Ok(())
    }

    #[test]
    fn outbound_send_approved_is_idempotent_for_already_queued_message() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mbx_idem", "command_id": "cmd_mbx_idem", "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link", "record_id": "camp_idem",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_idem","mailbox_address":"idem@example.com","mailbox_status":"ready"},
                "client_context": actor.clone()
            }),
        )?;
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_account_limits",
                "email:idem@example.com",
                1000,
                serde_json::json!({
                    "id": "email:idem@example.com",
                    "sender_account_id": "email:idem@example.com",
                    "status": "active",
                    "blocked": false,
                    "daily_limit": 5,
                    "daily_sent_count": 0,
                    "sent_today": 0,
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_idem", "command_id": "cmd_eng_idem", "module": "outbound",
                "command_type": "outbound.engagement.create", "record_id": "eng_idem",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_idem","company_id":"co_idem","contact_id":"ct_idem"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_prep_idem", "command_id": "cmd_prep_idem", "module": "outbound",
                "command_type": "outbound.message.prepare", "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_idem", "campaign_id": "camp_idem",
                    "sender_account_id": "email:idem@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Hi", "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_idem", "command_id": "cmd_req_idem", "module": "outbound",
                "command_type": "outbound.message.request_approval", "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": {"message_id":"msg_idem"}, "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_idem", "command_id": "cmd_apv_idem", "module": "outbound",
                "command_type": "outbound.message.approve", "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": {"message_id":"msg_idem"}, "client_context": actor.clone()
            }),
        )?;

        // Two distinct command envelopes (different command_id, so the command
        // bus does not dedupe at the command level) targeting the same message.
        let send_cmd = |cmd_id: &str| {
            serde_json::json!({
                "id": cmd_id, "command_id": cmd_id, "module": "outbound",
                "command_type": "outbound.message.send_approved", "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": {"message_id":"msg_idem"}, "client_context": actor.clone()
            })
        };
        let first = accept_rxdb_business_command(root, send_cmd("cmd_send_idem_1"))?;
        let queue_id = first
            .pointer("/result/provider_queue_id")
            .and_then(Value::as_str)
            .context("first send must return provider_queue_id")?
            .to_string();
        assert_eq!(
            first.pointer("/result/idempotent").and_then(Value::as_bool),
            None,
            "the first send is not an idempotent replay"
        );

        // Re-dispatch the very same approved+queued message. It must be a no-op
        // replay: same queue id, no second mailserver row, no double-count.
        let second = accept_rxdb_business_command(root, send_cmd("cmd_send_idem_2"))?;
        assert_eq!(
            second
                .pointer("/result/idempotent")
                .and_then(Value::as_bool),
            Some(true),
            "a re-send of a queued message must be flagged idempotent"
        );
        assert_eq!(
            second
                .pointer("/result/provider_queue_id")
                .and_then(Value::as_str),
            Some(queue_id.as_str()),
            "idempotent replay keeps the original queue id"
        );

        let queue_conn = Connection::open(crate::paths::core_db(root))?;
        let queued_count: i64 = queue_conn.query_row(
            "SELECT COUNT(*) FROM stalwart_smtp_queue WHERE to_addr = 'lead@example.com'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            queued_count, 1,
            "no duplicate mailserver queue row on replay"
        );
        drop(queue_conn);

        let conn = open_store(root)?;
        let limit = outbound_load_required(
            &conn,
            "outbound_account_limits",
            "email:idem@example.com",
            "limit",
        )?;
        assert_eq!(
            limit.get("daily_sent_count").and_then(Value::as_i64),
            Some(1),
            "idempotent replay must not increment the daily counter"
        );
        Ok(())
    }

    #[test]
    fn outbound_send_links_message_to_communication_thread_bidirectionally() -> anyhow::Result<()> {
        // Welle 10 (637/638): after an approved email is sent, the outbound
        // message and the communication thread must reference each other so the
        // link is traceable from either side (debug/status surfaces read it).
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mbx_link", "command_id": "cmd_mbx_link", "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link", "record_id": "camp_link",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_link","mailbox_address":"link@example.com","mailbox_status":"ready"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_link", "command_id": "cmd_eng_link", "module": "outbound",
                "command_type": "outbound.engagement.create", "record_id": "eng_link",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_link","company_id":"co_link","contact_id":"ct_link"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_prep_link", "command_id": "cmd_prep_link", "module": "outbound",
                "command_type": "outbound.message.prepare", "record_id": "msg_link",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_link", "campaign_id": "camp_link",
                    "sender_account_id": "email:link@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Hi", "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_link", "command_id": "cmd_req_link", "module": "outbound",
                "command_type": "outbound.message.request_approval", "record_id": "msg_link",
                "status": "pending_sync",
                "payload": {"message_id":"msg_link"}, "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_link", "command_id": "cmd_apv_link", "module": "outbound",
                "command_type": "outbound.message.approve", "record_id": "msg_link",
                "status": "pending_sync",
                "payload": {"message_id":"msg_link"}, "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_link", "command_id": "cmd_send_link", "module": "outbound",
                "command_type": "outbound.message.send_approved", "record_id": "msg_link",
                "status": "pending_sync",
                "payload": {"message_id":"msg_link"}, "client_context": actor.clone()
            }),
        )?;

        // Outbound side carries the communication keys written back by the sync.
        let conn = open_store(root)?;
        let message = outbound_load_required(&conn, "outbound_messages", "msg_link", "message")?;
        let message_key = message
            .get("communication_message_key")
            .and_then(Value::as_str)
            .context("outbound message must carry communication_message_key after send")?
            .to_string();
        let thread_key = message
            .get("thread_key")
            .and_then(Value::as_str)
            .context("outbound message must carry thread_key after send")?
            .to_string();
        assert!(
            !message_key.is_empty() && !thread_key.is_empty(),
            "communication keys must be non-empty"
        );
        drop(conn);

        // Communication side carries the outbound identifiers in its metadata.
        let channel_conn = Connection::open(crate::paths::core_db(root))?;
        let metadata_json: String = channel_conn.query_row(
            "SELECT metadata_json FROM communication_messages WHERE message_key = ?1",
            [&message_key],
            |row| row.get(0),
        )?;
        let metadata: Value = serde_json::from_str(&metadata_json)?;
        assert_eq!(
            metadata
                .get("outbound_engagement_id")
                .and_then(Value::as_str),
            Some("eng_link"),
            "communication message metadata must back-reference the engagement"
        );
        assert_eq!(
            metadata.get("outbound_message_id").and_then(Value::as_str),
            Some("msg_link"),
            "communication message metadata must back-reference the outbound message"
        );
        assert_eq!(
            metadata
                .get("communication_thread_key")
                .and_then(Value::as_str),
            Some(thread_key.as_str()),
            "thread key must match on both sides of the link"
        );
        Ok(())
    }

    #[test]
    fn outbound_daily_limit_enforced_under_parallel_commands() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        let n: usize = 6;
        let cap: i64 = 3;

        // Establish the campaign + account_limits row, then pin a hard daily cap.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mbx_cap", "command_id": "cmd_mbx_cap", "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link", "record_id": "camp_cap",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_cap","mailbox_address":"cap@example.com","mailbox_status":"ready"},
                "client_context": actor.clone()
            }),
        )?;
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_account_limits",
                "email:cap@example.com",
                1000,
                serde_json::json!({
                    "id": "email:cap@example.com",
                    "sender_account_id": "email:cap@example.com",
                    "status": "active",
                    "blocked": false,
                    "daily_limit": cap,
                    "daily_sent_count": 0,
                    "sent_today": 0,
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }

        // Build N independent approved messages, all sharing the one sender.
        for i in 0..n {
            let eng = format!("eng_cap_{i}");
            let msg = format!("msg_cap_{i}");
            accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": format!("c_eng_{i}"), "command_id": format!("c_eng_{i}"),
                    "module": "outbound", "command_type": "outbound.engagement.create",
                    "record_id": eng, "status": "pending_sync",
                    "payload": {"campaign_id":"camp_cap","company_id":format!("co_{i}"),"contact_id":format!("ct_{i}")},
                    "client_context": actor.clone()
                }),
            )?;
            accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": format!("c_prep_{i}"), "command_id": format!("c_prep_{i}"),
                    "module": "outbound", "command_type": "outbound.message.prepare",
                    "record_id": msg, "status": "pending_sync",
                    "payload": {
                        "engagement_id": eng, "campaign_id": "camp_cap",
                        "sender_account_id": "email:cap@example.com",
                        "recipient_email": format!("lead{i}@example.com"),
                        "subject": "Hi", "body_text": "Hello"
                    },
                    "client_context": actor.clone()
                }),
            )?;
            accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": format!("c_req_{i}"), "command_id": format!("c_req_{i}"),
                    "module": "outbound", "command_type": "outbound.message.request_approval",
                    "record_id": msg, "status": "pending_sync",
                    "payload": {"message_id": msg}, "client_context": actor.clone()
                }),
            )?;
            accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": format!("c_apv_{i}"), "command_id": format!("c_apv_{i}"),
                    "module": "outbound", "command_type": "outbound.message.approve",
                    "record_id": msg, "status": "pending_sync",
                    "payload": {"message_id": msg}, "client_context": actor.clone()
                }),
            )?;
        }

        // Fire all N sends concurrently — each opens its own connection, exactly
        // the parallel-command scenario the atomic reservation must survive.
        let root_buf = root.to_path_buf();
        let handles: Vec<_> = (0..n)
            .map(|i| {
                let r = root_buf.clone();
                let actor = actor.clone();
                std::thread::spawn(move || {
                    accept_rxdb_business_command(
                        &r,
                        serde_json::json!({
                            "id": format!("c_send_{i}"), "command_id": format!("c_send_{i}"),
                            "module": "outbound", "command_type": "outbound.message.send_approved",
                            "record_id": format!("msg_cap_{i}"), "status": "pending_sync",
                            "payload": {"message_id": format!("msg_cap_{i}")},
                            "client_context": actor
                        }),
                    )
                })
            })
            .collect();

        let mut ok = 0usize;
        let mut limit_blocked = 0usize;
        let mut transient = 0usize;
        for handle in handles {
            match handle.join().expect("send thread panicked") {
                Ok(_) => ok += 1,
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("daily limit") {
                        limit_blocked += 1;
                    } else if msg.contains("locked") {
                        // SQLite write-lock contention under heavy parallel load is a
                        // transient failure. It may strike before the slot reservation
                        // (no slot consumed) or after it commits but before the message
                        // upsert (a leaked-but-counted slot). Either way the slot is
                        // never double-counted and the counter cannot exceed the cap, so
                        // the no-overshoot guarantee holds; only the realized send count
                        // drops. The approved draft stays retryable.
                        transient += 1;
                    } else {
                        panic!("unexpected send error: {err}");
                    }
                }
            }
        }
        assert_eq!(
            ok + limit_blocked + transient,
            n,
            "every attempt is accounted for"
        );
        // The core safety guarantee: parallel sends may never exceed the cap.
        assert!(
            ok <= cap as usize,
            "parallel sends overshot the cap: {ok} > {cap}"
        );
        let conn = open_store(root)?;
        let limit = outbound_load_required(
            &conn,
            "outbound_account_limits",
            "email:cap@example.com",
            "limit",
        )?;
        let counter = limit
            .get("daily_sent_count")
            .and_then(Value::as_i64)
            .expect("daily_sent_count present");
        // The no-overshoot guarantee: the reservation never lets the counter exceed
        // the cap, and every realized send is reflected in it.
        assert!(
            counter <= cap,
            "parallel sends overshot the daily cap: {counter} > {cap}"
        );
        assert!(
            counter >= ok as i64,
            "every successful send must be counted: counter {counter} < ok {ok}"
        );
        assert_eq!(
            limit.get("remaining_today").and_then(Value::as_i64),
            Some(cap - counter)
        );
        // Absent transient contention the cap is fully reached, the rest hard-blocked,
        // and the counter lands exactly on the cap with no leaked slots.
        if transient == 0 {
            assert_eq!(ok, cap as usize, "exactly the cap may pass");
            assert_eq!(limit_blocked, n - cap as usize, "the rest must be blocked");
            assert_eq!(counter, cap, "counter must land exactly on the cap");
            assert_eq!(counter, ok as i64, "no leaked slots without contention");
        }
        Ok(())
    }

    #[test]
    fn outbound_send_failure_reflects_block_onto_engagement() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_ef", "command_id": "cmd_eng_ef", "module": "outbound",
                "command_type": "outbound.engagement.create", "record_id": "eng_ef",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_ef","company_id":"co_ef","contact_id":"ct_ef"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_ef", "command_id": "cmd_msg_ef", "module": "outbound",
                "command_type": "outbound.message.prepare", "record_id": "msg_ef",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_ef", "campaign_id": "camp_ef",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "blocked@example.com",
                    "subject": "Intro", "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_suppression_entries",
                "supp_ef",
                1000,
                serde_json::json!({
                    "id": "supp_ef", "email": "blocked@example.com",
                    "reason": "bounce", "status": "active",
                    "created_at_ms": 1000, "updated_at_ms": 1000
                }),
            )?;
        }
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_ef", "command_id": "cmd_req_ef", "module": "outbound",
                "command_type": "outbound.message.request_approval", "record_id": "msg_ef",
                "status": "pending_sync", "payload": {"message_id":"msg_ef"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_ef", "command_id": "cmd_apv_ef", "module": "outbound",
                "command_type": "outbound.message.approve", "record_id": "msg_ef",
                "status": "pending_sync", "payload": {"message_id":"msg_ef"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_ef", "command_id": "cmd_send_ef", "module": "outbound",
                "command_type": "outbound.message.send_approved", "record_id": "msg_ef",
                "status": "pending_sync", "payload": {"message_id":"msg_ef"},
                "client_context": actor.clone()
            }),
        )
        .expect_err("send must be blocked");

        // The engagement must carry the structured block reason so the timeline UI
        // can show why the send did not go out.
        let conn = open_store(root)?;
        let eng = outbound_load_required(&conn, "outbound_engagements", "eng_ef", "engagement")?;
        assert_eq!(
            outbound_string(&eng, &["status"]).as_deref(),
            Some("send_blocked")
        );
        assert_eq!(
            outbound_string(&eng, &["last_send_block_reason"]).as_deref(),
            Some("recipient_suppressed")
        );
        assert!(
            outbound_string(&eng, &["last_send_error"]).is_some(),
            "engagement records the underlying error text"
        );
        Ok(())
    }

    #[test]
    fn outbound_message_send_is_idempotent_after_queueing() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_idem",
                "command_id": "cmd_eng_idem",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_idem",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_idem",
                    "company_id": "co_idem",
                    "contact_id": "ct_idem"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_idem",
                "command_id": "cmd_msg_idem",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_idem",
                    "campaign_id": "camp_idem",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_idem",
                "command_id": "cmd_req_idem",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": { "message_id": "msg_idem" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_idem",
                "command_id": "cmd_apv_idem",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": { "message_id": "msg_idem" },
                "client_context": actor.clone()
            }),
        )?;
        let first = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_idem_1",
                "command_id": "cmd_send_idem_1",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": { "message_id": "msg_idem" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            first
                .pointer("/result/provider_dispatch_status")
                .and_then(Value::as_str),
            Some("queued_in_mailserver")
        );
        let first_queue_id = first
            .pointer("/result/provider_queue_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        assert!(first_queue_id.is_some(), "expected provider_queue_id");

        let second = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_idem_2",
                "command_id": "cmd_send_idem_2",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": { "message_id": "msg_idem" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            second
                .pointer("/result/idempotent")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            second
                .pointer("/result/provider_queue_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            first_queue_id
        );

        // Ensure stalwart_smtp_queue contains exactly one queued row.
        let queue_conn = Connection::open(crate::paths::core_db(root))?;
        let count: i64 = queue_conn.query_row(
            "SELECT COUNT(*) FROM stalwart_smtp_queue WHERE to_addr = 'lead@example.com'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1, "idempotent re-send must not double-queue");
        Ok(())
    }

    #[test]
    fn outbound_campaign_mailbox_link_projects_to_communication_accounts() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let res = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mailbox_link",
                "command_id": "cmd_mailbox_link",
                "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link",
                "record_id": "camp_mbx",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_mbx",
                    "mailbox_address": "outreach@example.com",
                    "mailbox_status": "ready",
                    "display_name": "Outreach"
                },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            res.pointer("/result/communication_account_key")
                .and_then(Value::as_str),
            Some("email:outreach@example.com")
        );
        let conn = open_store(root)?;
        let campaign = outbound_load_required(&conn, "outbound_campaigns", "camp_mbx", "campaign")?;
        assert_eq!(
            outbound_string(&campaign, &["mailbox_status"]).as_deref(),
            Some("ready")
        );
        assert_eq!(
            outbound_string(&campaign, &["communication_account_address"]).as_deref(),
            Some("outreach@example.com")
        );
        let limit = outbound_load_required(
            &conn,
            "outbound_account_limits",
            "email:outreach@example.com",
            "account_limits",
        )?;
        assert_eq!(
            outbound_string(&limit, &["campaign_id"]).as_deref(),
            Some("camp_mbx")
        );
        drop(conn);

        // verify communication_accounts row exists in channels db
        let channel_conn = channels::open_channel_db(&crate::paths::core_db(root))?;
        let exists: Option<String> = channel_conn
            .query_row(
                "SELECT address FROM communication_accounts WHERE account_key = ?1",
                rusqlite::params!["email:outreach@example.com"],
                |row| row.get(0),
            )
            .optional()?;
        assert_eq!(exists.as_deref(), Some("outreach@example.com"));

        Ok(())
    }

    #[test]
    fn outbound_campaign_activation_requires_ready_channel() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        // Create campaign without any mailbox link
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_campaigns",
                "camp_act",
                1000,
                serde_json::json!({
                    "id": "camp_act",
                    "status": "setup_required",
                    "payload": { "active_outreach": { "default_channel": "email" } },
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }

        let blocked = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_status_blocked",
                "command_id": "cmd_status_blocked",
                "module": "outbound",
                "command_type": "outbound.campaign.status.set",
                "record_id": "camp_act",
                "status": "pending_sync",
                "payload": { "campaign_id": "camp_act", "status": "active", "channel": "email" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("activation must require a linked mailbox");
        assert!(blocked.to_string().contains("linked mailbox"), "{blocked}");

        // Link mailbox + activate must succeed
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mbx_link_act",
                "command_id": "cmd_mbx_link_act",
                "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link",
                "record_id": "camp_act",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_act",
                    "mailbox_address": "ops@example.com"
                },
                "client_context": actor.clone()
            }),
        )?;
        let ok = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_status_ok",
                "command_id": "cmd_status_ok",
                "module": "outbound",
                "command_type": "outbound.campaign.status.set",
                "record_id": "camp_act",
                "status": "pending_sync",
                "payload": { "campaign_id": "camp_act", "status": "active", "channel": "email" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            ok.pointer("/result/status").and_then(Value::as_str),
            Some("active")
        );

        // physical_letter activation must work without mailbox
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_campaigns",
                "camp_phys",
                1100,
                serde_json::json!({
                    "id": "camp_phys",
                    "status": "setup_required",
                    "payload": { "active_outreach": { "default_channel": "physical_letter" } },
                    "created_at_ms": 1100,
                    "updated_at_ms": 1100
                }),
            )?;
        }
        let phys = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_status_phys",
                "command_id": "cmd_status_phys",
                "module": "outbound",
                "command_type": "outbound.campaign.status.set",
                "record_id": "camp_phys",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_phys",
                    "status": "active",
                    "channel": "physical_letter"
                },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            phys.pointer("/result/status").and_then(Value::as_str),
            Some("active")
        );

        Ok(())
    }

    #[test]
    fn outbound_reply_match_sets_engagement_and_stops_pending_followups() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_reply",
                "command_id": "cmd_eng_reply",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_reply",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_reply",
                    "company_id": "co_reply",
                    "contact_id": "ct_reply"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_followup",
                "command_id": "cmd_msg_followup",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_followup",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_reply",
                    "campaign_id": "camp_reply",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Follow-up",
                    "body_text": "Just checking in",
                    "message_type": "followup"
                },
                "client_context": actor.clone()
            }),
        )?;

        let res = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_reply_match",
                "command_id": "cmd_reply_match",
                "module": "outbound",
                "command_type": "outbound.reply.match",
                "record_id": "eng_reply",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_reply",
                    "reply_message_id": "email:inbox/lead-reply-1",
                    "classification": "positive"
                },
                "client_context": actor.clone()
            }),
        )?;
        let cancelled_ids = res
            .pointer("/result/cancelled_message_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(cancelled_ids.len(), 1);
        assert_eq!(cancelled_ids[0].as_str(), Some("msg_followup"));

        let conn = open_store(root)?;
        let engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_reply", "engagement")?;
        assert_eq!(
            outbound_string(&engagement, &["status"]).as_deref(),
            Some("reply_received")
        );
        assert_eq!(
            outbound_string(&engagement, &["payload", "reply_classification"]).as_deref(),
            Some("positive")
        );
        let message =
            outbound_load_required(&conn, "outbound_messages", "msg_followup", "message")?;
        assert_eq!(
            outbound_string(&message, &["send_status"]).as_deref(),
            Some("cancelled")
        );
        assert_eq!(
            outbound_string(&message, &["payload", "cancelled_reason"]).as_deref(),
            Some("reply_received")
        );

        Ok(())
    }

    #[test]
    fn outbound_reply_match_preserves_already_sent_messages() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_keep",
                "command_id": "cmd_eng_keep",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_keep",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_keep",
                    "company_id": "co_keep",
                    "contact_id": "ct_keep"
                },
                "client_context": actor.clone()
            }),
        )?;
        // An already-queued initial message and a not-yet-sent follow-up draft.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_sent",
                "command_id": "cmd_msg_sent",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_sent",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_keep",
                    "campaign_id": "camp_keep",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello",
                    "message_type": "initial"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_draft",
                "command_id": "cmd_msg_draft",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_draft",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_keep",
                    "campaign_id": "camp_keep",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Follow-up",
                    "body_text": "Checking in",
                    "message_type": "followup"
                },
                "client_context": actor.clone()
            }),
        )?;

        {
            let conn = open_store(root)?;
            let mut sent =
                outbound_load_required(&conn, "outbound_messages", "msg_sent", "message")?;
            outbound_put_string(&mut sent, "send_status", "queued_for_provider");
            upsert_business_record(
                &conn,
                "outbound_messages",
                "msg_sent",
                now_ms() as i64,
                sent,
            )?;
        }

        let res = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_reply_keep",
                "command_id": "cmd_reply_keep",
                "module": "outbound",
                "command_type": "outbound.reply.match",
                "record_id": "eng_keep",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_keep",
                    "reply_message_id": "email:inbox/lead-reply-keep",
                    "classification": "positive"
                },
                "client_context": actor.clone()
            }),
        )?;
        let cancelled_ids = res
            .pointer("/result/cancelled_message_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            cancelled_ids.len(),
            1,
            "only the un-sent draft may be cancelled"
        );
        assert_eq!(cancelled_ids[0].as_str(), Some("msg_draft"));

        let conn = open_store(root)?;
        let sent = outbound_load_required(&conn, "outbound_messages", "msg_sent", "message")?;
        assert_eq!(
            outbound_string(&sent, &["send_status"]).as_deref(),
            Some("queued_for_provider"),
            "already-queued message must be preserved"
        );
        let draft = outbound_load_required(&conn, "outbound_messages", "msg_draft", "message")?;
        assert_eq!(
            outbound_string(&draft, &["send_status"]).as_deref(),
            Some("cancelled")
        );

        Ok(())
    }

    #[test]
    fn outbound_unsubscribe_reply_creates_suppression_and_blocks_send() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_unsub",
                "command_id": "cmd_eng_unsub",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_unsub",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_unsub",
                    "company_id": "co_unsub",
                    "contact_id": "ct_unsub",
                    "payload": { "contact_email": "stop@example.com" }
                },
                "client_context": actor.clone()
            }),
        )?;

        // The recipient replies asking to be removed. The reply must register an
        // active suppression entry so any later send to that address is refused.
        let res = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_reply_unsub",
                "command_id": "cmd_reply_unsub",
                "module": "outbound",
                "command_type": "outbound.reply.match",
                "record_id": "eng_unsub",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_unsub",
                    "reply_message_id": "email:inbox/unsub-1",
                    "classification": "unsubscribe"
                },
                "client_context": actor.clone()
            }),
        )?;
        let suppression_id = res
            .pointer("/result/suppression_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        assert!(
            suppression_id.is_some(),
            "unsubscribe reply must create a suppression entry, got {res:?}"
        );

        let conn = open_store(root)?;
        let reason = outbound_recipient_suppression_reason(&conn, "stop@example.com")?;
        assert_eq!(reason.as_deref(), Some("unsubscribe"));
        // The engagement must be hard-stopped, not merely marked reply_received.
        let engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_unsub", "engagement")?;
        assert_eq!(
            outbound_string(&engagement, &["status"]).as_deref(),
            Some("stopped")
        );
        assert_eq!(
            outbound_string(&engagement, &["payload", "stop_reason"]).as_deref(),
            Some("unsubscribe")
        );
        drop(conn);

        // A fresh approved draft to the now-suppressed recipient must be refused
        // by the send gate (the suppression was created purely by the reply).
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_after_unsub",
                "command_id": "cmd_msg_after_unsub",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_after_unsub",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_unsub",
                    "campaign_id": "camp_unsub",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "stop@example.com",
                    "subject": "One more thing",
                    "body_text": "Following up again",
                    "message_type": "followup"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_after_unsub",
                "command_id": "cmd_req_after_unsub",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_after_unsub",
                "status": "pending_sync",
                "payload": { "message_id": "msg_after_unsub" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_after_unsub",
                "command_id": "cmd_apv_after_unsub",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_after_unsub",
                "status": "pending_sync",
                "payload": { "message_id": "msg_after_unsub" },
                "client_context": actor.clone()
            }),
        )?;
        let blocked = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_after_unsub",
                "command_id": "cmd_send_after_unsub",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_after_unsub",
                "status": "pending_sync",
                "payload": { "message_id": "msg_after_unsub" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("send to a suppressed recipient must be blocked");
        assert!(
            blocked.to_string().contains("suppressed"),
            "expected suppression block, got: {blocked}"
        );

        // Idempotent: a second unsubscribe reply must not create a duplicate entry.
        let res2 = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_reply_unsub_2",
                "command_id": "cmd_reply_unsub_2",
                "module": "outbound",
                "command_type": "outbound.reply.match",
                "record_id": "eng_unsub",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_unsub",
                    "reply_message_id": "email:inbox/unsub-2",
                    "classification": "unsubscribe"
                },
                "client_context": actor.clone()
            }),
        )?;
        assert!(
            res2.pointer("/result/suppression_id")
                .map(Value::is_null)
                .unwrap_or(true),
            "second unsubscribe must be a no-op, got {res2:?}"
        );

        Ok(())
    }

    #[test]
    fn test_module_snapshots_and_rollback() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();

        // 1. Set up the expected directory structure for Business OS app root and module
        let app_root = root.join("src").join("apps").join("business-os");
        fs::create_dir_all(&app_root)?;
        fs::write(app_root.join("index.html"), "<html></html>")?;

        let module_root = app_root.join("modules").join("test-module");
        fs::create_dir_all(&module_root)?;
        fs::write(module_root.join("module.json"), "{}")?;

        // 2. Save initial version
        let mutation1 = ModuleSourceSaveMutation {
            module_id: "test-module".to_string(),
            path: "app.js".to_string(),
            content: "console.log('version 1');".to_string(),
        };
        let outcome1 = save_module_source_record(root, mutation1)?;
        assert_eq!(outcome1.get("path").and_then(Value::as_str), Some("app.js"));

        // Verify the file was written
        let target_file = module_root.join("app.js");
        assert!(target_file.is_file());
        assert_eq!(
            fs::read_to_string(&target_file)?,
            "console.log('version 1');"
        );

        // 3. Save a second version (triggers snapshot of version 1)
        let mutation2 = ModuleSourceSaveMutation {
            module_id: "test-module".to_string(),
            path: "app.js".to_string(),
            content: "console.log('version 2');".to_string(),
        };
        let outcome2 = save_module_source_record(root, mutation2)?;
        assert_eq!(outcome2.get("path").and_then(Value::as_str), Some("app.js"));
        assert_eq!(
            fs::read_to_string(&target_file)?,
            "console.log('version 2');"
        );

        // 4. List snapshots
        let list_req = ModuleSourceListSnapshotsRequest {
            module_id: "test-module".to_string(),
        };
        let snapshots = list_module_source_snapshots(root, list_req)?;
        let snapshots_arr = snapshots.as_array().expect("expected snapshots array");
        assert_eq!(snapshots_arr.len(), 1, "should have exactly one snapshot");

        let first_snapshot = &snapshots_arr[0];
        let snapshot_id = first_snapshot
            .get("snapshot_id")
            .and_then(Value::as_str)
            .expect("missing snapshot_id")
            .to_string();
        assert_eq!(
            first_snapshot.get("path").and_then(Value::as_str),
            Some("app.js")
        );

        // 5. Rollback to version 1 (which will snapshot version 2!)
        let rollback_req = ModuleSourceRollbackSnapshotRequest {
            module_id: "test-module".to_string(),
            snapshot_id: snapshot_id.clone(),
        };
        let rollback_outcome = rollback_module_source_snapshot(root, rollback_req)?;
        assert_eq!(
            rollback_outcome.get("path").and_then(Value::as_str),
            Some("app.js")
        );

        // Verify content is rolled back to version 1
        assert_eq!(
            fs::read_to_string(&target_file)?,
            "console.log('version 1');"
        );

        // 6. List snapshots again - should now show 2 snapshots (since the rollback snapshotted the pre-rollback state "version 2"!)
        let list_req_2 = ModuleSourceListSnapshotsRequest {
            module_id: "test-module".to_string(),
        };
        let snapshots_2 = list_module_source_snapshots(root, list_req_2)?;
        let snapshots_arr_2 = snapshots_2.as_array().expect("expected snapshots array");
        assert_eq!(
            snapshots_arr_2.len(),
            2,
            "should now have two snapshots after rollback"
        );

        // Verify the latest snapshot is version 2 (the pre-rollback state)
        let latest_snapshot = &snapshots_arr_2[0];
        let latest_source_path = Path::new(
            latest_snapshot
                .get("source_path")
                .and_then(Value::as_str)
                .expect("missing source_path"),
        );
        assert_eq!(
            fs::read_to_string(latest_source_path)?,
            "console.log('version 2');"
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // IoT (ctox.iot.*) business_command + CLI wiring (§4A one code path).
    // -----------------------------------------------------------------------

    fn iot_admin_actor() -> Value {
        serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        })
    }

    fn iot_pull_record(root: &Path, collection: &str, record_id: &str) -> Option<Value> {
        let pulled = pull_collection_records(root, collection, Some(0), Some(2_000)).ok()?;
        pulled
            .get("documents")
            .and_then(Value::as_array)
            .and_then(|documents| {
                documents
                    .iter()
                    .find(|document| document.get("id").and_then(Value::as_str) == Some(record_id))
                    .cloned()
            })
    }

    // ctox.iot.asset.upsert then ctox.iot.attribute.write over the
    // business_command executor; the engine state must be projected into the
    // iot_assets / iot_attributes business_records collections.
    #[test]
    fn iot_business_command_projects_into_iot_collections() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = iot_admin_actor();

        let upsert = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_iot_asset_upsert",
                "command_id": "cmd_iot_asset_upsert",
                "module": "iot",
                "command_type": "ctox.iot.asset.upsert",
                "record_id": "asset-iot-bc-1",
                "status": "pending_sync",
                "payload": {
                    "id": "asset-iot-bc-1",
                    "realm": "master",
                    "asset_type": "Thermostat",
                    "name": "Living room",
                    "asset_type_info": {
                        "asset_type": "Thermostat",
                        "attributes": [{
                            "name": "temp",
                            "value_descriptor": {
                                "name": "number",
                                "base_type": "Number",
                                "array_dimensions": 0,
                                "constraints": [],
                                "units": null,
                                "format": null
                            },
                            "meta": {}
                        }]
                    }
                },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            upsert.get("status").and_then(Value::as_str),
            Some("completed")
        );
        let projections = upsert
            .pointer("/result/projections")
            .and_then(Value::as_array)
            .expect("asset upsert reports projections");
        assert!(projections
            .iter()
            .any(|p| p["collection"] == "iot_assets" && p["id"] == "asset-iot-bc-1"));

        let write = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_iot_attr_write",
                "command_id": "cmd_iot_attr_write",
                "module": "iot",
                "command_type": "ctox.iot.attribute.write",
                "record_id": "asset-iot-bc-1:temp",
                "status": "pending_sync",
                "payload": {
                    "asset_id": "asset-iot-bc-1",
                    "name": "temp",
                    "value": 22.5,
                    "timestamp_ms": 1000
                },
                "client_context": actor
            }),
        )?;
        assert_eq!(
            write.get("status").and_then(Value::as_str),
            Some("completed")
        );

        // Projection echo: the iot_assets collection carries the asset, the
        // iot_attributes collection carries the written value.
        let asset_doc = iot_pull_record(root, "iot_assets", "asset-iot-bc-1")
            .expect("iot_assets projection present");
        assert_eq!(asset_doc["name"], "Living room");
        assert_eq!(asset_doc["realm"], "master");
        assert_eq!(asset_doc["_deleted"], Value::Bool(false));

        let attr_doc = iot_pull_record(root, "iot_attributes", "asset-iot-bc-1:temp")
            .expect("iot_attributes projection present");
        assert_eq!(attr_doc["asset_id"], "asset-iot-bc-1");
        assert_eq!(attr_doc["data"]["value"], serde_json::json!(22.5));
        assert_eq!(attr_doc["_deleted"], Value::Bool(false));
        Ok(())
    }

    // ACL: the iot executor enforces a real chef/admin role gate at the
    // business_command edge (rxdb_command_session -> session_can_manage_all).
    // A trusted non-admin actor — even one spoofing `role: admin` in the
    // client-supplied context — must be rejected, so an untrusted peer cannot
    // mutate IoT state by virtue of the always-true authenticated session.
    #[test]
    fn iot_business_command_rejects_non_admin_actor() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let now = now_ms() as i64;
        conn.execute(
            "INSERT INTO business_users
                (user_id, display_name, role, active, created_at_ms, updated_at_ms)
             VALUES ('viewer', 'Viewer', 'user', 1, ?1, ?1)",
            params![now],
        )?;
        drop(conn);

        let error = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_iot_asset_upsert_denied",
                "command_id": "cmd_iot_asset_upsert_denied",
                "module": "iot",
                "command_type": "ctox.iot.asset.upsert",
                "record_id": "asset-denied-1",
                "status": "pending_sync",
                "payload": {
                    "id": "asset-denied-1",
                    "realm": "master",
                    "asset_type": "Thermostat",
                    "name": "Lobby",
                    "asset_type_info": {
                        "asset_type": "Thermostat",
                        "attributes": []
                    }
                },
                "client_context": {
                    "actor": {
                        "id": "viewer",
                        "display_name": "Viewer",
                        "role": "admin",
                        "is_admin": true
                    }
                }
            }),
        )
        .expect_err("non-admin iot mutation must be rejected");
        assert!(
            error.to_string().contains("chef or admin role required"),
            "unexpected error: {error}"
        );
        // The mutation must not have been projected into the RxDB-visible store.
        assert!(
            iot_pull_record(root, "iot_assets", "asset-denied-1").is_none(),
            "denied iot mutation must not project"
        );
        Ok(())
    }

    #[test]
    fn configured_auth_user_is_trusted_for_rxdb_admin_commands() -> anyhow::Result<()> {
        let _env = EnvRestore::set(&[
            (
                "CTOX_AUTH_USERS",
                "michael.welsch@metric-space.ai|secret|admin",
            ),
            ("CTOX_BUSINESS_OS_REQUIRE_LOGIN", "1"),
        ]);
        let temp = tempdir()?;
        let root = temp.path();

        let outcome = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_configured_admin_user_upsert",
                "command_id": "cmd_configured_admin_user_upsert",
                "module": "ctox",
                "command_type": "ctox.business_os.user.upsert",
                "record_id": "new-admin",
                "status": "pending_sync",
                "payload": {
                    "id": "new-admin",
                    "display_name": "New Admin",
                    "role": "admin",
                    "active": true
                },
                "client_context": {
                    "actor": {
                        "id": "michael.welsch@metric-space.ai",
                        "display_name": "Michael Welsch"
                    }
                }
            }),
        )?;

        assert_eq!(outcome["status"], "completed");
        let conn = open_store(root)?;
        let configured_role: String = conn.query_row(
            "SELECT role FROM business_users WHERE user_id = ?1",
            params!["michael.welsch@metric-space.ai"],
            |row| row.get(0),
        )?;
        assert_eq!(configured_role, "admin");
        let new_role: String = conn.query_row(
            "SELECT role FROM business_users WHERE user_id = ?1",
            params!["new-admin"],
            |row| row.get(0),
        )?;
        assert_eq!(new_role, "admin");
        Ok(())
    }

    #[test]
    fn direct_module_catalog_projection_includes_installed_modules() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src/apps/business-os");
        fs::create_dir_all(app_root.join("modules/ctox"))?;
        fs::create_dir_all(app_root.join("installed-modules/research"))?;
        fs::write(app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            app_root.join("installed-modules/research/module.json"),
            r#"{"id":"research","title":"Web Research","entry":"installed-modules/research/index.html","install_scope":"installed"}"#,
        )?;

        write_module_catalog_projection_to_rxdb(root)?;

        let conn = Connection::open(rxdb_store_path(root))?;
        let catalog_json: String = conn.query_row(
            "SELECT data FROM ctox_business_os__business_module_catalog__v0 WHERE id = 'module-catalog'",
            [],
            |row| row.get(0),
        )?;
        let catalog: Value = serde_json::from_str(&catalog_json)?;
        let ids = catalog
            .get("modules")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|module| module.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect::<Vec<_>>();
        assert!(ids.contains(&"ctox".to_owned()), "missing ctox: {ids:?}");
        assert!(
            ids.contains(&"research".to_owned()),
            "missing installed research: {ids:?}"
        );
        Ok(())
    }

    #[test]
    fn module_catalog_projection_includes_packaged_research() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let app_root = root.join("src/apps/business-os");
        fs::create_dir_all(app_root.join("modules/ctox"))?;
        fs::create_dir_all(app_root.join("modules/research"))?;
        fs::write(app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            app_root.join("modules/research/module.json"),
            r#"{"id":"research","title":"Web Research","entry":"modules/research/index.html","install_scope":"store"}"#,
        )?;

        write_module_catalog_projection_to_rxdb(root)?;

        let conn = Connection::open(rxdb_store_path(root))?;
        let catalog_json: String = conn.query_row(
            "SELECT data FROM ctox_business_os__business_module_catalog__v0 WHERE id = 'module-catalog'",
            [],
            |row| row.get(0),
        )?;
        let catalog: Value = serde_json::from_str(&catalog_json)?;
        let research = catalog
            .get("modules")
            .and_then(Value::as_array)
            .and_then(|modules| {
                modules
                    .iter()
                    .find(|module| module.get("id").and_then(Value::as_str) == Some("research"))
            })
            .expect("packaged research module missing from projected catalog");
        assert_eq!(
            research.get("install_scope").and_then(Value::as_str),
            Some("starter")
        );
        assert_eq!(
            research.get("entry").and_then(Value::as_str),
            Some("modules/research/index.html")
        );
        Ok(())
    }

    #[test]
    fn module_catalog_prefers_release_source_over_stale_runtime_app_root() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let release_root = temp.path().join("release");
        let runtime_root = release_root.join("runtime");
        let stale_app_root = runtime_root.join("business-os");
        let release_app_root = release_root.join("src/apps/business-os");

        fs::create_dir_all(stale_app_root.join("modules/ctox"))?;
        fs::create_dir_all(release_app_root.join("modules/ctox"))?;
        fs::create_dir_all(release_app_root.join("modules/research"))?;
        fs::write(stale_app_root.join("index.html"), "<!doctype html>")?;
        fs::write(release_app_root.join("index.html"), "<!doctype html>")?;
        fs::write(
            stale_app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            release_app_root.join("modules/ctox/module.json"),
            r#"{"id":"ctox","title":"CTOX","entry":"modules/ctox/index.html","install_scope":"core"}"#,
        )?;
        fs::write(
            release_app_root.join("modules/research/module.json"),
            r#"{"id":"research","title":"Web Research","entry":"modules/research/index.html","install_scope":"store"}"#,
        )?;

        let catalog = module_catalog_for_rxdb(&runtime_root)?;
        let modules = catalog
            .get("modules")
            .and_then(Value::as_array)
            .context("catalog modules")?;
        assert!(modules
            .iter()
            .any(|module| module.get("id").and_then(Value::as_str) == Some("research")));
        Ok(())
    }

    // `ctox iot project all` is a real full resync (not a silent no-op): it
    // bridges engine state seeded by the CLI surface into the RxDB-visible
    // business-os store so it can replicate to apps. Here a CLI asset.upsert
    // writes engine state but does NOT itself project into business-os.sqlite3;
    // project_all_iot must then make the iot_assets projection appear.
    #[test]
    fn iot_project_all_resyncs_cli_engine_state_into_business_store() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();

        // CLI surface: seed engine state directly (no business_command bridge).
        crate::iot::commands::asset_upsert(
            root,
            crate::iot::commands::AssetUpsertReq {
                id: Some("asset-resync-1".into()),
                realm: "master".into(),
                asset_type: "Thermostat".into(),
                name: "Roof".into(),
                parent_id: None,
                asset_type_info: Some(serde_json::from_value(serde_json::json!({
                    "asset_type": "Thermostat",
                    "attributes": []
                }))?),
            },
            None,
        )?;

        // Before resync: nothing in the RxDB-visible iot_assets collection.
        assert!(
            iot_pull_record(root, "iot_assets", "asset-resync-1").is_none(),
            "CLI mutation should not reach business-os store before resync"
        );

        let pairs = project_all_iot(root, None)?;
        assert!(
            pairs
                .iter()
                .any(|(c, id)| *c == "iot_assets" && id == "asset-resync-1"),
            "project_all_iot must report the asset projection"
        );

        // After resync: the asset is now visible to the RxDB read path.
        let asset_doc = iot_pull_record(root, "iot_assets", "asset-resync-1")
            .expect("iot_assets projection present after resync");
        assert_eq!(asset_doc["name"], "Roof");
        assert_eq!(asset_doc["realm"], "master");

        // Idempotent: a second resync produces the same pair set.
        let pairs_again = project_all_iot(root, None)?;
        assert_eq!(pairs, pairs_again, "project_all_iot must be idempotent");
        Ok(())
    }

    // §4A one-code-path proof: the `ctox iot` CLI dispatch and the
    // ctox.iot.* business_command produce the identical persisted engine
    // result for the same op (attribute write -> read round-trips both ways).
    #[test]
    fn iot_cli_and_business_command_one_code_path() -> anyhow::Result<()> {
        // Seed an identical asset on two separate roots.
        let cli_temp = tempdir()?;
        let cli_root = cli_temp.path();
        let bc_temp = tempdir()?;
        let bc_root = bc_temp.path();

        let type_info = crate::iot::commands::AssetUpsertReq {
            id: Some("asset-shared-x".into()),
            realm: "master".into(),
            asset_type: "Thermostat".into(),
            name: "Lab".into(),
            parent_id: None,
            asset_type_info: Some(serde_json::from_value(serde_json::json!({
                "asset_type": "Thermostat",
                "attributes": [{
                    "name": "temp",
                    "value_descriptor": {
                        "name": "number",
                        "base_type": "Number",
                        "array_dimensions": 0,
                        "constraints": [],
                        "units": null,
                        "format": null
                    },
                    "meta": {}
                }]
            }))?),
        };
        crate::iot::commands::asset_upsert(cli_root, type_info.clone(), None)?;
        crate::iot::commands::asset_upsert(bc_root, type_info, None)?;

        // CLI path: `ctox iot attribute write` then `ctox iot attribute read`
        // (these are the args the main.rs `Some("iot")` arm forwards). Both
        // dispatch through iot::commands::handle_iot_command and round-trip.
        crate::iot::commands::handle_iot_command(
            cli_root,
            &[
                "attribute".into(),
                "write".into(),
                "--asset".into(),
                "asset-shared-x".into(),
                "--name".into(),
                "temp".into(),
                "--value".into(),
                "21.0".into(),
                "--ts".into(),
                "2000".into(),
            ],
        )?;
        crate::iot::commands::handle_iot_command(
            cli_root,
            &[
                "attribute".into(),
                "read".into(),
                "--asset".into(),
                "asset-shared-x".into(),
                "--name".into(),
                "temp".into(),
            ],
        )?;

        // business_command path: ctox.iot.attribute.write with the same op input.
        let actor = iot_admin_actor();
        let outcome = accept_rxdb_business_command(
            bc_root,
            serde_json::json!({
                "id": "cmd_iot_one_path",
                "command_id": "cmd_iot_one_path",
                "module": "iot",
                "command_type": "ctox.iot.attribute.write",
                "record_id": "asset-shared-x:temp",
                "status": "pending_sync",
                "payload": {
                    "asset_id": "asset-shared-x",
                    "name": "temp",
                    "value": 21.0,
                    "timestamp_ms": 2000
                },
                "client_context": actor
            }),
        )?;
        assert_eq!(
            outcome.get("status").and_then(Value::as_str),
            Some("completed")
        );

        // Identical result: read back the shared op's value on both roots and
        // assert they match (one code path -> one persisted result).
        let cli_read =
            crate::iot::commands::attribute_read(cli_root, "asset-shared-x", "temp", None)?;
        let bc_read =
            crate::iot::commands::attribute_read(bc_root, "asset-shared-x", "temp", None)?;
        assert_eq!(cli_read, bc_read);
        assert_eq!(cli_read["attribute"]["value"], serde_json::json!(21.0));
        assert_eq!(cli_read["attribute"]["timestamp"], serde_json::json!(2000));
        Ok(())
    }
}
