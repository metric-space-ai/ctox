// Origin: CTOX
// License: Apache-2.0

use crate::mission::channels;
use anyhow::Context;
use base64::Engine;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use uuid::Uuid;

const STORE_FILE: &str = "business-os.sqlite3";
const DEFAULT_SIGNALING_URL: &str = "wss://signaling.ctox.dev";
const DOCUMENT_BLOB_CHUNK_SIZE: usize = 256_000;

#[derive(Debug, Clone, Serialize)]
pub struct BusinessOsStatus {
    pub ok: bool,
    pub runtime: &'static str,
    pub store_path: String,
    pub now_ms: u128,
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
    pub signaling_urls: Vec<String>,
    pub transport: &'static str,
    pub http_bridge_available: bool,
    pub ctox_instance_required: bool,
    pub native_rxdb_peer_available: bool,
    pub native_rxdb_peer_reason: &'static str,
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
pub struct ModuleFounderAssignment {
    pub module_id: String,
    pub user_id: String,
    #[serde(default = "default_true")]
    pub active: bool,
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

pub fn open_store(root: &Path) -> anyhow::Result<Connection> {
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime)
        .with_context(|| format!("failed to create runtime dir {}", runtime.display()))?;
    let path = runtime.join(STORE_FILE);
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open Business OS store {}", path.display()))?;
    conn.busy_timeout(Duration::from_millis(1_000))
        .context("failed to configure Business OS SQLite busy_timeout")?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn status(root: &Path) -> anyhow::Result<BusinessOsStatus> {
    let path = root.join("runtime").join(STORE_FILE);
    let ctox_service = Some(cheap_ctox_service_status(root));
    Ok(BusinessOsStatus {
        ok: true,
        runtime: "native-rust",
        store_path: path.display().to_string(),
        now_ms: now_ms(),
        ctox_service,
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
    let peer_id = format!("ctox-core-{}", short_hash(&instance_id));
    Ok(BusinessOsSyncConfig {
        ok: true,
        app_hosting: "ctox_instance_webserver",
        sync_mode: "p2p-first",
        sync_room: format!("ctox-business-os:{instance_id}"),
        instance_id,
        peer_id,
        peer_role: "ctox_instance",
        signaling_urls: signaling_urls(),
        transport: "webrtc",
        http_bridge_available: true,
        ctox_instance_required: true,
        native_rxdb_peer_available: false,
        native_rxdb_peer_reason: "src/core/rxdb is not a complete CTOX WebRTC replication peer yet",
    })
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
    module_governance_map(root, session)
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
    upsert_business_record(
        &conn,
        "business_module_releases",
        &version_id,
        now,
        serde_json::json!({
            "id": version_id,
            "version_id": version_id,
            "module_id": module_id,
            "version": next_version,
            "status": "released",
            "created_by": created_by,
            "created_at_ms": now,
            "notes": request.notes,
            "updated_at_ms": now
        }),
    )?;
    module_governance_map(root, session)
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
    upsert_business_record(
        &conn,
        "business_module_releases",
        version_id,
        now,
        serde_json::json!({
            "id": version_id,
            "version_id": version_id,
            "module_id": module_id,
            "status": "released",
            "rolled_back_at_ms": now,
            "updated_at_ms": now
        }),
    )?;
    module_governance_map(root, session)
}

pub fn record_report(
    root: &Path,
    session: &BusinessOsSession,
    mutation: BusinessOsReportMutation,
) -> anyhow::Result<Value> {
    anyhow::ensure!(session.authenticated, "login required");
    let module_id = mutation.module_id.trim();
    let title = mutation.title.trim();
    anyhow::ensure!(!module_id.is_empty(), "module_id is required");
    anyhow::ensure!(!title.is_empty(), "title is required");
    let kind = normalize_report_kind(&mutation.kind);
    let severity = normalize_report_severity(&mutation.severity);
    let report_id = format!("report_{}", Uuid::new_v4());
    let reporter_id = session_user_id(session).unwrap_or("");
    let now = now_ms() as i64;
    let command = BusinessCommand {
        id: None,
        module: "ctox".to_owned(),
        command_type: format!("ctox.report.{kind}"),
        record_id: Some(report_id.clone()),
        payload: serde_json::json!({
            "title": title,
            "module_id": module_id,
            "kind": kind,
            "severity": severity,
            "summary": mutation.summary,
            "expected": mutation.expected,
            "reporter_id": reporter_id,
            "instruction": format!("Bearbeite diesen Business-OS {} Report für Modul `{}`. Prüfe Reproduktion, Auswirkung, gewünschtes Ergebnis und setze daraus CTOX Arbeit auf. Wenn du die Aufgabe annimmst oder abschliesst, dokumentiere im Ergebnis konkret, was du geaendert hast, welche Dateien/Module betroffen sind, welche Verifikation gelaufen ist und welche gespeicherte Modulversion als Rollback-Ziel genutzt werden kann.", kind, module_id)
        }),
        client_context: mutation.client_context.clone(),
    };
    let accepted = record_command(root, command)?;
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
            mutation.summary,
            mutation.expected,
            reporter_id,
            accepted.command_id,
            serde_json::to_string(&mutation.client_context)?,
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
            "summary": mutation.summary,
            "expected": mutation.expected,
            "status": "open",
            "reporter_id": reporter_id,
            "ctox_command_id": accepted.command_id,
            "task_id": accepted.task_id,
            "inbound_channel": module_id,
            "client_context": mutation.client_context,
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
            "description": mutation.summary,
            "evidence": mutation.client_context,
            "payload": {
                "kind": kind,
                "expected": mutation.expected,
                "ctox_command_id": accepted.command_id,
                "task_id": accepted.task_id
            },
            "updated_at_ms": now
        }),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "report_id": report_id,
        "command_id": accepted.command_id,
        "task_id": accepted.task_id,
        "status": "open"
    }))
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

fn accept_rxdb_business_command(root: &Path, document: Value) -> anyhow::Result<Value> {
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
            params![command_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_some() {
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
    let accepted = record_command(root, command)?;
    Ok(serde_json::to_value(accepted)?)
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

fn signaling_urls() -> Vec<String> {
    std::env::var("CTOX_BUSINESS_OS_SIGNALING_URLS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| vec![DEFAULT_SIGNALING_URL.to_string()])
}

fn short_hash(value: &str) -> String {
    let digest = sha2::Sha256::digest(value.as_bytes());
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &digest)[..10]
        .to_string()
}
