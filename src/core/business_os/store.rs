// Origin: CTOX
// License: Apache-2.0

use crate::mission::channels;
use anyhow::Context;
use base64::Engine;
use rusqlite::params;
use rusqlite::Connection;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use uuid::Uuid;

const STORE_FILE: &str = "business-os.sqlite3";
const DEFAULT_SIGNALING_URL: &str = "wss://signaling.ctox.dev";

#[derive(Debug, Clone, Serialize)]
pub struct BusinessOsStatus {
    pub ok: bool,
    pub runtime: &'static str,
    pub store_path: String,
    pub now_ms: u128,
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
    migrate(&conn)?;
    Ok(conn)
}

pub fn status(root: &Path) -> anyhow::Result<BusinessOsStatus> {
    let path = root.join("runtime").join(STORE_FILE);
    Ok(BusinessOsStatus {
        ok: true,
        runtime: "native-rust",
        store_path: path.display().to_string(),
        now_ms: now_ms(),
    })
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
            "instruction": format!("Bearbeite diesen Business-OS {} Report für Modul `{}`. Prüfe Reproduktion, Auswirkung, gewünschtes Ergebnis und setze daraus CTOX Arbeit auf.", kind, module_id)
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
        is_source_parse_command(&command.command_type) || is_match_command(&command.command_type),
        "business command {command_id} is not a supported Business OS harness command"
    );
    let queue_task = find_queue_task_for_command(root, command_id)
        .and_then(|task_id| channels::load_queue_task(root, &task_id).ok().flatten());

    let result = if is_source_parse_command(&command.command_type) {
        super::importer::handle_source_parse(root, &conn, command_id, &command, queue_task.as_ref())
    } else {
        super::importer::handle_match_compute(
            root,
            &conn,
            command_id,
            &command,
            queue_task.as_ref(),
        )
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
    format!(
        "{instruction}\n\nBusiness OS command:\n- command_id: {command_id}\n- module: {}\n- type: {}\n- record_id: {}\n\nPayload JSON:\n{payload}\n\nClient context JSON:\n{context}",
        command.module,
        command.command_type,
        command.record_id.as_deref().unwrap_or("")
    )
}

fn suggested_skill_for_command(command: &BusinessCommand) -> Option<String> {
    if is_source_parse_command(&command.command_type) {
        Some("business-os-import-parser".to_string())
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

pub fn is_source_parse_command(command_type: &str) -> bool {
    command_type.contains("source.parse")
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

pub fn pull_collection(root: &Path, collection: &str, body: Value) -> anyhow::Result<Value> {
    let conn = open_store(root)?;
    let checkpoint = body.get("checkpoint").and_then(Value::as_object);
    let checkpoint_updated_at_ms = checkpoint
        .and_then(|item| item.get("updated_at_ms"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let checkpoint_record_id = checkpoint
        .and_then(|item| item.get("record_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let batch_size = body
        .get("batch_size")
        .and_then(Value::as_u64)
        .and_then(|value| i64::try_from(value).ok())
        .map(|value| value.clamp(1, 1000))
        .unwrap_or(200);
    let query_limit = batch_size + 1;
    let mut stmt = conn.prepare(
        "SELECT record_id, rev, deleted, updated_at_ms, payload_json
         FROM business_records
         WHERE collection = ?1
           AND (updated_at_ms > ?2 OR (updated_at_ms = ?2 AND record_id > ?3))
         ORDER BY updated_at_ms ASC, record_id ASC
         LIMIT ?4",
    )?;
    let mut rows = stmt
        .query_map(
            params![
                collection,
                checkpoint_updated_at_ms,
                checkpoint_record_id,
                query_limit
            ],
            |row| {
                let record_id: String = row.get(0)?;
                let rev: String = row.get(1)?;
                let deleted: i64 = row.get(2)?;
                let updated_at_ms: i64 = row.get(3)?;
                let payload_json: String = row.get(4)?;
                let mut payload: Value =
                    serde_json::from_str(&payload_json).unwrap_or_else(|_| serde_json::json!({}));
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("id".to_string(), Value::String(record_id));
                    obj.insert("_rev".to_string(), Value::String(rev));
                    obj.insert("_deleted".to_string(), Value::Bool(deleted != 0));
                    obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
                }
                Ok(payload)
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = rows.len() as i64 > batch_size;
    if has_more {
        rows.truncate(batch_size as usize);
    }
    for payload in &mut rows {
        refresh_live_business_payload(root, collection, payload);
    }
    let checkpoint = rows
        .last()
        .and_then(|payload| {
            Some(serde_json::json!({
                "collection": collection,
                "updated_at_ms": payload.get("updated_at_ms")?.as_i64()?,
                "record_id": payload.get("id")?.as_str()?,
            }))
        })
        .unwrap_or_else(|| {
            serde_json::json!({
                "collection": collection,
                "updated_at_ms": checkpoint_updated_at_ms,
                "record_id": checkpoint_record_id,
            })
        });
    Ok(serde_json::json!({
        "ok": true,
        "has_more": has_more,
        "documents": rows,
        "checkpoint": checkpoint
    }))
}

fn refresh_live_business_payload(root: &Path, collection: &str, payload: &mut Value) {
    let Some(obj) = payload.as_object_mut() else {
        return;
    };
    let command_id = obj
        .get("command_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let task_id = if collection == "ctox_queue_tasks" {
        obj.get("id").and_then(Value::as_str).map(str::to_string)
    } else if collection == "business_commands" {
        obj.get("task_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| {
                command_id
                    .as_deref()
                    .and_then(|id| find_queue_task_for_command(root, id))
            })
    } else {
        None
    };
    let Some(task_id) = task_id else {
        return;
    };
    let Ok(Some(task)) = channels::load_queue_task(root, &task_id) else {
        return;
    };
    let normalized = normalize_queue_status(&task.route_status);
    let stored_status = obj
        .get("status")
        .and_then(Value::as_str)
        .or_else(|| obj.get("task_status").and_then(Value::as_str))
        .map(str::to_string);
    let effective_status = if stored_status.as_deref().is_some_and(is_terminal_status) {
        stored_status.as_deref().unwrap_or(normalized)
    } else {
        normalized
    };
    if collection == "ctox_queue_tasks" {
        obj.insert(
            "status".to_string(),
            Value::String(effective_status.to_string()),
        );
        obj.insert("route_status".to_string(), Value::String(task.route_status));
        obj.insert("title".to_string(), Value::String(task.title));
        obj.insert("priority".to_string(), Value::String(task.priority));
        obj.insert("thread_key".to_string(), Value::String(task.thread_key));
        obj.insert(
            "lease_owner".to_string(),
            task.lease_owner.map(Value::String).unwrap_or(Value::Null),
        );
        obj.insert(
            "leased_at".to_string(),
            task.leased_at.map(Value::String).unwrap_or(Value::Null),
        );
    } else {
        obj.insert("task_id".to_string(), Value::String(task.message_key));
        obj.insert(
            "task_status".to_string(),
            Value::String(effective_status.to_string()),
        );
    }
}

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed" | "done" | "handled" | "failed" | "cancelled" | "canceled"
    )
}

fn find_queue_task_for_command(root: &Path, command_id: &str) -> Option<String> {
    let tasks = channels::list_queue_tasks(root, &[], 256).ok()?;
    tasks
        .into_iter()
        .find(|task| task.prompt.contains(command_id))
        .map(|task| task.message_key)
}

pub fn push_collection(root: &Path, collection: &str, body: Value) -> anyhow::Result<Value> {
    let docs = body
        .get("documents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut conn = open_store(root)?;
    let tx = conn.transaction()?;
    let updated_at_ms = now_ms() as i64;
    let mut accepted = 0usize;
    for doc in docs {
        let Some(id) = doc.get("id").and_then(Value::as_str) else {
            continue;
        };
        let rev = doc
            .get("_rev")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("rev_{}", Uuid::new_v4()));
        let deleted = doc
            .get("_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let payload_json = serde_json::to_string(&doc)?;
        tx.execute(
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
                id,
                rev,
                deleted as i64,
                updated_at_ms,
                payload_json
            ],
        )?;
        materialize_document_record(&tx, collection, id, deleted, updated_at_ms, &doc)?;
        accepted += 1;
    }
    tx.commit()?;
    Ok(serde_json::json!({
        "ok": true,
        "accepted": accepted,
        "conflicts": []
    }))
}

fn materialize_document_record(
    tx: &rusqlite::Transaction<'_>,
    collection: &str,
    id: &str,
    deleted: bool,
    fallback_updated_at_ms: i64,
    doc: &Value,
) -> anyhow::Result<()> {
    match collection {
        "documents" => {
            let tags_json =
                serde_json::to_string(doc.get("tags").unwrap_or(&Value::Array(vec![])))?;
            tx.execute(
                "INSERT INTO business_documents
                    (document_id, title, filename, mime_type, status, document_type,
                     current_version_id, source_sha256, page_count, diagnostics_count,
                     tags_json, index_text, deleted, created_at_ms, updated_at_ms, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                 ON CONFLICT(document_id) DO UPDATE SET
                    title = excluded.title,
                    filename = excluded.filename,
                    mime_type = excluded.mime_type,
                    status = excluded.status,
                    document_type = excluded.document_type,
                    current_version_id = excluded.current_version_id,
                    source_sha256 = excluded.source_sha256,
                    page_count = excluded.page_count,
                    diagnostics_count = excluded.diagnostics_count,
                    tags_json = excluded.tags_json,
                    index_text = excluded.index_text,
                    deleted = excluded.deleted,
                    created_at_ms = excluded.created_at_ms,
                    updated_at_ms = excluded.updated_at_ms,
                    payload_json = excluded.payload_json",
                params![
                    id,
                    value_string(doc, "title"),
                    value_string(doc, "filename"),
                    value_string(doc, "mime_type"),
                    value_string(doc, "status"),
                    value_string(doc, "document_type"),
                    value_string(doc, "current_version_id"),
                    value_string(doc, "source_sha256"),
                    value_i64(doc, "page_count").unwrap_or(0),
                    value_i64(doc, "diagnostics_count").unwrap_or(0),
                    tags_json,
                    value_string(doc, "index_text"),
                    deleted as i64,
                    value_i64(doc, "created_at_ms").unwrap_or(fallback_updated_at_ms),
                    value_i64(doc, "updated_at_ms").unwrap_or(fallback_updated_at_ms),
                    serde_json::to_string(doc)?
                ],
            )?;
        }
        "document_versions" => {
            tx.execute(
                "INSERT INTO business_document_versions
                    (version_id, document_id, version, source_kind, blob_id, diagnostics_json,
                     model_json, deleted, created_at_ms, updated_at_ms, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(version_id) DO UPDATE SET
                    document_id = excluded.document_id,
                    version = excluded.version,
                    source_kind = excluded.source_kind,
                    blob_id = excluded.blob_id,
                    diagnostics_json = excluded.diagnostics_json,
                    model_json = excluded.model_json,
                    deleted = excluded.deleted,
                    created_at_ms = excluded.created_at_ms,
                    updated_at_ms = excluded.updated_at_ms,
                    payload_json = excluded.payload_json",
                params![
                    id,
                    value_string(doc, "document_id"),
                    value_i64(doc, "version").unwrap_or(0),
                    value_string(doc, "source_kind"),
                    value_string(doc, "blob_id"),
                    serde_json::to_string(doc.get("diagnostics").unwrap_or(&Value::Array(vec![])))?,
                    serde_json::to_string(doc.get("model_json").unwrap_or(&Value::Null))?,
                    deleted as i64,
                    value_i64(doc, "created_at_ms").unwrap_or(fallback_updated_at_ms),
                    value_i64(doc, "updated_at_ms").unwrap_or(fallback_updated_at_ms),
                    serde_json::to_string(doc)?
                ],
            )?;
        }
        "document_blob_chunks" => {
            tx.execute(
                "INSERT INTO business_document_blob_chunks
                    (chunk_id, blob_id, document_id, version_id, idx, total, mime_type,
                     encoding, data, deleted, created_at_ms, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(chunk_id) DO UPDATE SET
                    blob_id = excluded.blob_id,
                    document_id = excluded.document_id,
                    version_id = excluded.version_id,
                    idx = excluded.idx,
                    total = excluded.total,
                    mime_type = excluded.mime_type,
                    encoding = excluded.encoding,
                    data = excluded.data,
                    deleted = excluded.deleted,
                    created_at_ms = excluded.created_at_ms,
                    payload_json = excluded.payload_json",
                params![
                    id,
                    value_string(doc, "blob_id"),
                    value_string(doc, "document_id"),
                    value_string(doc, "version_id"),
                    value_i64(doc, "idx").unwrap_or(0),
                    value_i64(doc, "total").unwrap_or(1),
                    value_string(doc, "mime_type"),
                    value_string(doc, "encoding"),
                    value_string(doc, "data"),
                    deleted as i64,
                    value_i64(doc, "created_at_ms").unwrap_or(fallback_updated_at_ms),
                    serde_json::to_string(doc)?
                ],
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn value_string(doc: &Value, key: &str) -> String {
    doc.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn value_i64(doc: &Value, key: &str) -> Option<i64> {
    doc.get(key).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|item| i64::try_from(item).ok()))
            .or_else(|| value.as_f64().map(|item| item as i64))
    })
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
