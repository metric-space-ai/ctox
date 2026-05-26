// Origin: CTOX
// License: Apache-2.0

use crate::mission::channels;
use anyhow::Context;
use base64::Engine;
use ctox_app_server_protocol::AuthMode as ApiAuthMode;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::io;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
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
const CORE_MODULE_IDS: &[&str] = &["ctox", "knowledge", "app-store", "desktop", "reports"];
const STARTER_MODULE_IDS: &[&str] = &["documents", "spreadsheets", "calendar", "notes"];
const CHATGPT_AUTH_ISSUER: &str = "https://auth.openai.com";
const CHATGPT_AUTH_CALLBACK_PORT: u16 = 1455;
const CHATGPT_AUTH_CALLBACK_FALLBACK_PORT: u16 = 1457;
const CHATGPT_AUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
const CHATGPT_AUTH_SECRET_SCOPE: &str = "ctox-auth";
const CHATGPT_AUTH_SECRET_NAME: &str = "chatgpt_subscription_auth_json";
const BUSINESS_OS_SECRET_SCOPE: &str = "business-os";
const BUSINESS_OS_ROOM_PASSWORD_SECRET_NAME: &str = "webrtc_room_password";

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
        let table = format!("ctox_business_os__{collection}__v0");
        let table_exists = rxdb_table_exists(&conn, &table).unwrap_or(false);
        let row_count = if table_exists {
            rxdb_table_row_count(&conn, &table).ok()
        } else {
            None
        };
        let latest_updated_at_ms = if table_exists {
            rxdb_table_latest_updated_at_ms(&conn, &table)
                .ok()
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
    let table_exists = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'ctox_business_os__business_module_catalog__v0'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
        .is_some();
    if !table_exists {
        return serde_json::json!({
            "ok": false,
            "path": path.display().to_string(),
            "reason": "business_module_catalog RxDB collection table is missing",
        });
    }
    let data = match conn
        .query_row(
            "SELECT data FROM ctox_business_os__business_module_catalog__v0 WHERE id = 'module-catalog'",
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
                "table": "ctox_business_os__business_module_catalog__v0",
                "reason": "module-catalog document is missing",
            });
        }
        Err(err) => {
            return serde_json::json!({
                "ok": false,
                "path": path.display().to_string(),
                "table": "ctox_business_os__business_module_catalog__v0",
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
        "table": "ctox_business_os__business_module_catalog__v0",
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
        "SELECT module_id, version_id, version, status, created_by, created_at_ms, notes
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
    let key_name = crate::inference::runtime_state::api_key_env_var_for_provider(&provider);
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
    let updated_at_ms = now_ms() as u64;
    Ok(serde_json::json!({
        "id": "runtime-settings",
        "ok": true,
        "can_manage": true,
        "updated_at_ms": updated_at_ms,
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
            "upstream_base_url": runtime_state.as_ref()
                .filter(|state| !state.source.is_local())
                .map(|state| state.upstream_base_url.clone())
                .unwrap_or_default()
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
    Ok(serde_json::json!({
        "id": "module-catalog",
        "ok": true,
        "modules": modules,
        "marketplace": marketplace,
        "templates": templates,
        "governance": governance,
        "updated_at_ms": now_ms(),
        "_deleted": false,
    }))
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
    app_root: &Path,
    session: &BusinessOsSession,
    request: ModuleInstallTemplateRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required"
    );
    let manifest = install_template_module(app_root, request)?;
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
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_can_manage_all(session),
        "chef or admin role required"
    );
    subscription_auth_start_payload(root)
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
    let context = request.context.trim();
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
            crate::inference::runtime_state::default_api_upstream_base_url_for_provider(provider)
                .to_owned(),
        );
    }
    if !chat_model.is_empty() {
        env_map.insert("CTOX_CHAT_MODEL".to_owned(), chat_model.to_owned());
        env_map.insert("CTOX_CHAT_MODEL_BASE".to_owned(), chat_model.to_owned());
    }
    if let Some(preset) = normalize_runtime_preset(preset) {
        env_map.insert("CTOX_CHAT_LOCAL_PRESET".to_owned(), preset.to_owned());
    }
    if !context.is_empty() {
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

fn runtime_settings_context(value: Option<String>) -> String {
    let Some(value) = value else {
        return "256k".to_owned();
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "131072" | "128000" | "128k" => "128k".to_owned(),
        "262144" | "256000" | "256k" => "256k".to_owned(),
        _ => value,
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

fn subscription_auth_start_payload(root: &Path) -> anyhow::Result<Value> {
    let login = start_chatgpt_subscription_login(root)?;
    Ok(serde_json::json!({
        "ok": true,
        "status": "auth_url",
        "login_id": login.login_id,
        "auth_url": login.auth_url,
        "message": "ChatGPT Subscription Autorisierung gestartet."
    }))
}

struct StartedChatgptSubscriptionLogin {
    login_id: String,
    auth_url: String,
}

#[derive(Clone)]
struct ChatgptLoginPkce {
    verifier: String,
    challenge: String,
}

fn start_chatgpt_subscription_login(
    root: &Path,
) -> anyhow::Result<StartedChatgptSubscriptionLogin> {
    let codex_home = ctox_core::config::find_codex_home()
        .context("Codex/CTOX Auth-Store konnte nicht aufgelöst werden")?;
    let pkce = chatgpt_login_pkce();
    let state = chatgpt_login_state();
    let (server, port) = bind_chatgpt_login_server()
        .context("Lokaler ChatGPT-Login-Callback konnte nicht gestartet werden")?;
    let redirect_uri = format!("http://localhost:{port}/auth/callback");
    let auth_url = build_chatgpt_authorize_url(&redirect_uri, &pkce.challenge, &state);
    let login_id = Uuid::new_v4().to_string();
    let worker_login_id = login_id.clone();
    let root = root.to_path_buf();
    thread::spawn(move || {
        if let Err(err) =
            run_chatgpt_login_callback_server(server, root, codex_home, redirect_uri, pkce, state)
        {
            eprintln!("CTOX ChatGPT subscription login {worker_login_id} failed: {err}");
        }
    });
    Ok(StartedChatgptSubscriptionLogin { login_id, auth_url })
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

fn resolve_business_os_app_root(root: &Path) -> anyhow::Result<PathBuf> {
    [
        root.join("src").join("apps").join("business-os"),
        root.join("apps").join("business-os"),
        root.join("business-os"),
        root.to_path_buf(),
    ]
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
    if dir.join("module.json").is_file()
        && dir
            .to_string_lossy()
            .replace('\\', "/")
            .ends_with(source_path)
    {
        return Ok(Some(dir.to_path_buf()));
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_module_json_dir_by_source_path(&path, source_path)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
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
    _root: &Path,
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
            serde_json::json!({
                "id": task.message_key,
                "command_id": command_id,
                "title": task.title,
                "status": normalize_queue_status(&task.route_status),
                "route_status": task.route_status,
                "module": "ctox",
                "source_module": command.module.clone(),
                "inbound_channel": inbound_channel,
                "command_type": command.command_type.clone(),
                "priority": task.priority,
                "thread_key": task.thread_key,
                "prompt": task.prompt,
                "workspace_root": task.workspace_root,
                "updated_at_ms": observed_at_ms
            }),
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
            upsert_business_record(
                &conn,
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
                }),
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
            upsert_business_record(
                &conn,
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
                    "inbound_channel": command_inbound_channel(&command),
                    "task_id": queue_task.as_ref().map(|task| task.message_key.clone()),
                    "task_status": "failed",
                    "error": err.to_string(),
                    "payload": command.payload.clone(),
                    "client_context": command.client_context.clone(),
                    "updated_at_ms": failed_at_ms
                }),
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
    }

    let chat_id = business_chat_id(command, command_id);
    let chat_title = business_chat_title(command);
    let owner_user_id = first_string_field(&command.client_context, &["owner_user_id", "user_id"])
        .unwrap_or_else(|| "local-dev".to_string());
    let task_id = queue_task
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
        chat_payload,
    )?;

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
        }),
    )?;
    refresh_queue_task_projection(root, conn, command_id, command, queue_task, completed_at_ms)?;

    Ok(CommandAccepted {
        ok: true,
        command_id: command_id.to_string(),
        status: "completed",
        task_id: queue_task.map(|task| task.message_key.clone()),
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
    Ok(serde_json::json!({
        "ok": true,
        "collection": collection,
        "documents": documents,
        "count": documents.len(),
        "since_ms": since_ms
    }))
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
        "ctox.task.update" => {
            let mutation: CtoxTaskUpdateMutation = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.task.update payload")?;
            let session = rxdb_command_session(&command)?;
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
            let session = rxdb_command_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
            let outcome = start_subscription_auth_command(root, &session)?;
            return write_rxdb_control_command_outcome(
                root,
                &command,
                "completed",
                None,
                Some("completed"),
                outcome,
            );
        }
        command_type if is_outbound_active_command(command_type) => {
            let session = rxdb_authenticated_session(&command)?;
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
        command_type if command_type.starts_with("ctox.channel.") => {
            let mutation: ChannelCommandRequest = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.channel payload")?;
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
            let app_root = resolve_business_os_app_root(root)?;
            let outcome = install_template_module_command(&app_root, &session, mutation)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
        "ctox.app_store.install" => {
            let request: AppStoreInstallRequest =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.app_store.install payload")?;
            let session = rxdb_authenticated_session(&command)?;
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
            let session = rxdb_authenticated_session(&command)?;
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
            let sender_account_id = outbound_first_string(&[
                outbound_string(&command.payload, &["sender_account_id"]),
                outbound_string(&engagement, &["sender_account_id"]),
            ])
            .context("sender_account_id is required")?;
            let recipient_email = outbound_first_string(&[
                outbound_string(&command.payload, &["recipient_email"]),
                outbound_string(&engagement, &["payload", "contact_email"]),
            ])
            .context("recipient_email is required")?;
            anyhow::ensure!(
                !outbound_recipient_suppressed(&conn, &recipient_email)?,
                "recipient is suppressed for outbound communication"
            );
            let previous_messages = outbound_load_records_by_string_field(
                &conn,
                "outbound_messages",
                "engagement_id",
                &engagement_id,
            )?;
            let latest_message = outbound_latest_message(&previous_messages);
            let generated = outbound_generate_automated_draft(
                &engagement,
                latest_message.as_ref(),
                &command.payload,
                &draft_kind,
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
            outbound_put_string(&mut message, "direction", "outbound");
            outbound_put_string(&mut message, "sender_account_id", sender_account_id);
            outbound_put_string(&mut message, "recipient_email", recipient_email);
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
                Value::String(
                    outbound_first_string(&[
                        outbound_string(&command.payload, &["skillbook_id"]),
                        outbound_string(&command.payload, &["payload", "skillbook_id"]),
                        outbound_string(&engagement, &["payload", "skillbook_id"]),
                    ])
                    .unwrap_or_else(|| "business-os.outbound.message_drafting.v1".to_string()),
                ),
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
            }
            outbound_update_engagement_status(&conn, &engagement_id, "awaiting_approval", now)?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_messages",
                "message": message,
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
            outbound_enforce_send_gate(&conn, &message)?;
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
            let provider_queue_id = outbound_queue_email_delivery(root, &message)
                .context("failed to queue approved outbound email")?;
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
            outbound_sync_email_message_to_communication(
                root,
                &mut message,
                "queued_for_provider",
            )?;
            let sender_account_id = outbound_required_string(&message, &["sender_account_id"])?;
            outbound_increment_account_send_count(&conn, &sender_account_id, now)?;
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
                Value::String(classification),
            );
            outbound_merge_fields(&mut engagement, &command.payload, &["reply_message_id"]);
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
            if let Some(engagement_id) = outbound_string(&request, &["engagement_id"]) {
                outbound_update_engagement_status(&conn, &engagement_id, "meeting_booked", now)?;
            }
            Ok(serde_json::json!({ "ok": true, "meeting_request": request }))
        }
        "outbound.campaign.mailbox.link" => {
            outbound_handle_campaign_mailbox_link(root, &conn, command, now)
        }
        "outbound.campaign.status.set" => {
            outbound_handle_campaign_status_set(&conn, command, now)
        }
        "outbound.reply.match" => outbound_handle_reply_match(root, &conn, command, now),
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
    let mailbox_address =
        outbound_required_string(&command.payload, &["mailbox_address"]).or_else(|_| {
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
    let channel = outbound_string(&command.payload, &["channel"])
        .unwrap_or_else(|| "email".to_string());
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
    outbound_put_string(&mut campaign, "communication_account_key", account_key.clone());
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
    upsert_business_record(conn, "outbound_campaigns", &campaign_id, now, campaign.clone())?;

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
    upsert_business_record(conn, "outbound_account_limits", &account_key, now, limit.clone())?;

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
        outbound_string(&campaign, &["payload", "active_outreach", "default_channel"]),
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
    upsert_business_record(conn, "outbound_campaigns", &campaign_id, now, campaign.clone())?;

    Ok(serde_json::json!({
        "ok": true,
        "campaign": campaign,
        "status": requested_status,
        "channel": default_channel,
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

    let pending_messages =
        outbound_load_records_by_string_field(conn, "outbound_messages", "engagement_id", &engagement_id)?;
    let mut cancelled = Vec::new();
    for mut message in pending_messages {
        let send_status =
            outbound_string(&message, &["send_status"]).unwrap_or_else(|| "draft".to_string());
        let direction =
            outbound_string(&message, &["direction"]).unwrap_or_else(|| "outbound".to_string());
        if direction != "outbound" {
            continue;
        }
        if matches!(send_status.as_str(), "sent" | "delivered" | "queued_for_provider") {
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
    let mut metadata: Value = serde_json::from_str(&metadata_text).unwrap_or_else(|_| serde_json::json!({}));
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
    let body = outbound_string(message, &["body_text"]).unwrap_or_default();
    let sender_domain = from.split('@').nth(1).unwrap_or("ctox.local");
    let msg_id = format!("<{}@{}>", Uuid::new_v4(), sender_domain);
    let date = chrono::Utc::now().to_rfc2822();
    let rfc822_body = format!(
        "From: {from}\r\n\
         To: {to}\r\n\
         Subject: {subject}\r\n\
         Message-ID: {msg_id}\r\n\
         Date: {date}\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: 8bit\r\n\
         \r\n\
         {body}\r\n",
        from = outbound_header_value(&from),
        to = outbound_header_value(&to),
        subject = outbound_header_value(&subject),
        msg_id = outbound_header_value(&msg_id),
        date = outbound_header_value(&date),
        body = body.replace("\r\n", "\n").replace('\r', "\n")
    );
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

fn outbound_header_value(value: &str) -> String {
    value
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn outbound_increment_account_send_count(
    conn: &Connection,
    sender_account_id: &str,
    now: i64,
) -> anyhow::Result<()> {
    let Some(mut limit) = outbound_load_record(conn, "outbound_account_limits", sender_account_id)?
    else {
        return Ok(());
    };
    let sent_today = limit
        .get("sent_today")
        .and_then(Value::as_i64)
        .or_else(|| limit.get("daily_sent_count").and_then(Value::as_i64))
        .unwrap_or(0)
        + 1;
    outbound_put_i64(&mut limit, "sent_today", sent_today);
    outbound_put_i64(&mut limit, "daily_sent_count", sent_today);
    if let Some(limit_value) = limit.get("daily_limit").and_then(Value::as_i64) {
        outbound_put_i64(
            &mut limit,
            "remaining_today",
            limit_value.saturating_sub(sent_today),
        );
    }
    outbound_put_i64(&mut limit, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_account_limits",
        sender_account_id,
        now,
        limit,
    )
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

fn outbound_generate_automated_draft(
    engagement: &Value,
    latest_message: Option<&Value>,
    request: &Value,
    draft_kind: &str,
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
    let sender_account_id = outbound_required_string(message, &["sender_account_id"])?;
    let recipient_email = outbound_required_string(message, &["recipient_email"])?;
    outbound_require_message_content(message)?;
    anyhow::ensure!(
        !outbound_recipient_suppressed(conn, &recipient_email)?,
        "recipient is suppressed for outbound communication"
    );
    outbound_enforce_account_limit(conn, &sender_account_id)?;
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
    let recipient = recipient_email.trim().to_ascii_lowercase();
    let domain = recipient.split('@').nth(1).unwrap_or_default().to_string();
    let mut stmt = conn.prepare(
        "SELECT payload_json
         FROM business_records
         WHERE collection = 'outbound_suppression_entries'
           AND deleted = 0",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        let payload: Value = serde_json::from_str(&row?)?;
        let status = outbound_string(&payload, &["status"]).unwrap_or_else(|| "active".to_string());
        if matches!(status.as_str(), "inactive" | "deleted" | "expired") {
            continue;
        }
        let suppressed_email = outbound_string(&payload, &["email"])
            .or_else(|| outbound_string(&payload, &["recipient_email"]))
            .unwrap_or_default()
            .to_ascii_lowercase();
        let suppressed_domain = outbound_string(&payload, &["domain"])
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !suppressed_email.is_empty() && suppressed_email == recipient {
            return Ok(true);
        }
        if !suppressed_domain.is_empty() && suppressed_domain == domain {
            return Ok(true);
        }
    }
    Ok(false)
}

fn outbound_enforce_account_limit(
    conn: &Connection,
    sender_account_id: &str,
) -> anyhow::Result<()> {
    let Some(limit) = outbound_load_record(conn, "outbound_account_limits", sender_account_id)?
    else {
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
    if let Some(remaining) = limit.get("remaining_today").and_then(Value::as_i64) {
        anyhow::ensure!(remaining > 0, "sender account daily limit exhausted");
    }
    if let (Some(sent), Some(limit_value)) = (
        limit.get("daily_sent_count").and_then(Value::as_i64),
        limit.get("daily_limit").and_then(Value::as_i64),
    ) {
        anyhow::ensure!(sent < limit_value, "sender account daily limit exhausted");
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

fn rxdb_command_session(command: &BusinessCommand) -> anyhow::Result<BusinessOsSession> {
    rxdb_session_from_command(command, true)
}

fn rxdb_authenticated_session(command: &BusinessCommand) -> anyhow::Result<BusinessOsSession> {
    rxdb_session_from_command(command, false)
}

fn rxdb_session_from_command(
    command: &BusinessCommand,
    require_manage_all: bool,
) -> anyhow::Result<BusinessOsSession> {
    let client_ctx = if let Value::String(ref s) = command.client_context {
        serde_json::from_str(s).unwrap_or_else(|_| command.client_context.clone())
    } else {
        command.client_context.clone()
    };
    let actor = client_ctx.get("actor").or_else(|| client_ctx.get("user"));
    let role = actor
        .and_then(|value| value.get("role"))
        .or_else(|| client_ctx.get("role"))
        .and_then(Value::as_str)
        .unwrap_or("user")
        .to_string();
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
    let is_admin = actor
        .and_then(|value| value.get("is_admin"))
        .or_else(|| client_ctx.get("is_admin"))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| normalize_business_role(&role) == "admin");
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
    upsert_business_record(
        &conn,
        "business_commands",
        command_id,
        now,
        serde_json::json!({
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
        }),
    )?;
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
            &format!("Dieses Word-Dokument wurde aus dem Documents-Modul erzeugt. Nutzerauftrag: {prompt}"),
            None,
            None
        ),
        bullet_1 = docx_paragraph("Der Auftrag wurde als DOCX-Artefakt verarbeitet, nicht als Markdown-Enddatei.", None, Some(0)),
        bullet_2 = docx_paragraph("Die Datei enthaelt Word-Struktur mit Ueberschriften, Liste und Tabelle.", None, Some(0)),
        bullet_3 = docx_paragraph("Der Business-OS-Writeback registriert Datei, Version und Blob-Chunks fuer SuperDoc.", None, Some(0)),
        h1_table = docx_paragraph("Abnahmetabelle", Some("Heading1"), None),
        table = fallback_docx_table(),
        h1_figure = docx_paragraph("Abbildung", Some("Heading1"), None),
        p_figure = docx_paragraph("Documents UI -> CTOX Report-Runbook -> DOCX-Erzeugung -> Business-OS Writeback -> SuperDoc Anzeige", None, None),
        h1_notes = docx_paragraph("Hinweise", Some("Heading1"), None),
        p_notes = docx_paragraph("Diese serverseitige Mindestlieferung verhindert haengende Queue-Zustaende: Wenn der Agent-Reportpfad kein DOCX zurueckschreibt, erzeugt der Documents-Handler ein valides Word-Artefakt und schliesst den Command terminal ab.", None, None),
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
    upsert_business_record(
        conn,
        "ctox_queue_tasks",
        &task.message_key,
        updated_at_ms,
        serde_json::json!({
            "id": task.message_key,
            "command_id": command_id,
            "title": task.title,
            "status": normalize_queue_status(&task.route_status),
            "route_status": task.route_status,
            "module": "ctox",
            "source_module": command.module.clone(),
            "inbound_channel": inbound_channel,
            "command_type": command.command_type.clone(),
            "priority": task.priority,
            "thread_key": task.thread_key,
            "prompt": task.prompt,
            "workspace_root": task.workspace_root,
            "updated_at_ms": updated_at_ms
        }),
    )
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
    serde_json::json!({
        "id": task.message_key,
        "command_id": command_id.unwrap_or_default(),
        "title": task.title,
        "status": normalize_queue_status(&task.route_status),
        "route_status": task.route_status,
        "module": "ctox",
        "source_module": "ctox",
        "inbound_channel": "business_os.llm.chat",
        "command_type": "business_os.chat.task",
        "priority": task.priority,
        "thread_key": task.thread_key,
        "prompt": task.prompt,
        "workspace_root": task.workspace_root,
        "updated_at_ms": updated_at_ms
    })
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
    let title = command_title(command);
    let prompt = command_prompt(command_id, command);
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

fn command_prompt(command_id: &str, command: &BusinessCommand) -> String {
    let instruction = command
        .payload
        .get("instruction")
        .and_then(Value::as_str)
        .or_else(|| command.payload.get("prompt").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Execute this Business OS automation through CTOX.");
    let payload =
        serde_json::to_string_pretty(&command.payload).unwrap_or_else(|_| "{}".to_string());
    let context =
        serde_json::to_string_pretty(&command.client_context).unwrap_or_else(|_| "{}".to_string());
    let required_skills = command
        .payload
        .get("required_skills")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("\nRequired CTOX skills: {value}\n"))
        .unwrap_or_default();
    format!(
        "{instruction}{required_skills}\nBusiness OS command:\n- command_id: {command_id}\n- module: {}\n- type: {}\n- record_id: {}\n\nPayload JSON:\n{payload}\n\nClient context JSON:\n{context}",
        command.module,
        command.command_type,
        command.record_id.as_deref().unwrap_or("")
    )
}

fn suggested_skill_for_command(command: &BusinessCommand) -> Option<String> {
    if is_source_parse_command(&command.command_type) {
        Some("business-os-import-parser".to_string())
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
    } else if command.command_type.contains("app.modify") {
        Some("business-os-module-editor".to_string())
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
                    "slot_hint": "drei Slots in der kommenden Woche"
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
        assert!(
            blocked.to_string().contains("suppressed"),
            "{blocked}"
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
            second.pointer("/result/idempotent").and_then(Value::as_bool),
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
        assert!(
            blocked.to_string().contains("linked mailbox"),
            "{blocked}"
        );

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
        assert_eq!(
            cancelled_ids[0].as_str(),
            Some("msg_followup")
        );

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
}
