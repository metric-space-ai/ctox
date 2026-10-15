//! `net.ingest` — generic ingress primitive (CATALOG Nr. 18, §16.38).
//! An app registers a path under the Core HTTPS origin; every request on it
//! becomes exactly one command of the registered type to that app. The
//! handler's `result.response` is the HTTP answer, bounded by a status and
//! header whitelist; without an answer within 3 s the Core sends the
//! registered default and the command still runs to completion. Tracking,
//! signature checks and device models are app logic (§16.35 Nr. 3).
use super::*;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const RESPONSE_WAIT: Duration = Duration::from_secs(3);
pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const PATH_PATTERN: &str = "^[a-z0-9][a-z0-9._-]{0,63}(/[a-z0-9][a-z0-9._-]{0,63}){0,3}$";
pub const ROUTE_PREFIX: &str = "/ingest/";
pub const RESPONSE_HEADERS: &[&str] = &["content-type", "location", "cache-control"];
/// Complete Core legacy list (§16.38 Nr. 4): alias prefix → owning app.
pub const LEGACY_ALIASES: &[(&str, &str)] = &[
    ("/mail/track/o/", "outbound"),
    ("/mail/track/c/", "outbound"),
    ("/ctox/iot/webhook/", "iot"),
];
/// Never forwarded unless the registration names them in pass_headers.
const PRIVATE_HEADERS: &[&str] = &["cookie", "authorization", "x-webhook-token"];

/// Core-owned ingress runtime: public origin, active-manifest lookup, command
/// submission to (app_id, type) and files publication for large bodies.
pub trait IngestRuntime {
    /// Absolute origin of the Core HTTPS endpoint (`https://host`).
    fn public_origin(&self) -> Option<String>;
    /// Whether the app's ACTIVE manifest has a handler for this command type.
    fn has_handler(&self, app_id: &str, command_type: &str) -> Result<bool, CtoxError>;
    /// Submit the command; the receiver yields the handler result once.
    fn dispatch(
        &self,
        app_id: &str,
        command_type: &str,
        payload: Value,
    ) -> Result<std::sync::mpsc::Receiver<Value>, CtoxError>;
    /// Store a large body as a files record owned by app_id; returns files.ref.
    fn store_body(&self, app_id: &str, bytes: &[u8], mime: &str) -> Result<String, CtoxError>;
    /// Bytes of a files record owned by app_id (response `body_ref`).
    fn load_body(&self, app_id: &str, reference: &str) -> Result<Vec<u8>, CtoxError>;
}

/// The HTTP answer the Core writes. The body is raw bytes, never base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: std::collections::BTreeMap<String, String>,
    pub body: Vec<u8>,
}
impl HttpResponse {
    fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Default::default(),
            body: Vec::new(),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRef {
    #[serde(rename = "ref")]
    reference: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResponseWire {
    status: u16,
    #[serde(default)]
    headers: std::collections::BTreeMap<String, String>,
    body: Option<String>,
    body_ref: Option<FileRef>,
}
pub enum ResponseBody {
    Inline(Vec<u8>),
    /// files.ref of the answering app; bytes are delivered by the Core.
    Ref(String),
}
pub struct ResponseSpec {
    pub status: u16,
    pub headers: std::collections::BTreeMap<String, String>,
    pub body: ResponseBody,
}
/// §16.38 Nr. 3. Returns None when the response is not admissible.
pub fn admissible_response(value: &Value) -> Option<ResponseSpec> {
    let response: ResponseWire = serde_json::from_value(value.clone()).ok()?;
    let status_ok = matches!(response.status, 200..=299 | 301..=308 | 400..=410 | 429);
    if !status_ok {
        return None;
    }
    let body = match (response.body, response.body_ref) {
        (Some(_), Some(_)) => return None,
        (Some(body), None) if body.len() <= MAX_BODY_BYTES => {
            ResponseBody::Inline(body.into_bytes())
        }
        (Some(_), None) => return None,
        (None, Some(file)) => {
            let hex = file.reference.len() == 64
                && file
                    .reference
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if !hex {
                return None;
            }
            ResponseBody::Ref(file.reference)
        }
        (None, None) => ResponseBody::Inline(Vec::new()),
    };
    let mut headers = std::collections::BTreeMap::new();
    for (name, value) in response.headers {
        let name = name.to_ascii_lowercase();
        if !RESPONSE_HEADERS.contains(&name.as_str()) {
            continue;
        }
        if value.len() > 2048 || value.chars().any(char::is_control) {
            return None;
        }
        headers.insert(name, value);
    }
    if (301..=308).contains(&response.status) {
        let location = headers.get("location")?;
        if !browser::web_url(location) {
            return None;
        }
    } else if headers
        .get("location")
        .is_some_and(|location| !browser::web_url(location))
    {
        return None;
    }
    Some(ResponseSpec {
        status: response.status,
        headers,
        body,
    })
}
/// Resolves a body_ref against the answering app's own files (≤ 64 KiB).
fn materialize(
    runtime: &dyn IngestRuntime,
    app_id: &str,
    spec: ResponseSpec,
) -> Option<HttpResponse> {
    let body = match spec.body {
        ResponseBody::Inline(bytes) => bytes,
        ResponseBody::Ref(reference) => {
            let bytes = runtime.load_body(app_id, &reference).ok()?;
            if bytes.len() > MAX_BODY_BYTES || sha256_hex(&bytes) != reference {
                return None;
            }
            bytes
        }
    };
    Some(HttpResponse {
        status: spec.status,
        headers: spec.headers,
        body,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Register {
    path: String,
    command: String,
    default_response: Option<Value>,
    #[serde(default)]
    pass_headers: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Subscribe {
    endpoint: String,
    protocol: String,
    #[serde(default = "enabled")]
    enabled: bool,
    #[serde(default)]
    config: Value,
}
fn enabled() -> bool {
    true
}

pub(crate) fn valid_path(path: &str) -> bool {
    let segment = |s: &str| {
        !s.is_empty()
            && s.len() <= 64
            && s.as_bytes()[0].is_ascii_alphanumeric()
            && s.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
            })
    };
    let parts = path.split('/').collect::<Vec<_>>();
    (1..=4).contains(&parts.len()) && parts.iter().all(|p| segment(p))
}
fn header_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn credential_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    !key.ends_with("_ref")
        && (key.contains("password")
            || key.contains("secret")
            || key.contains("token")
            || key == "key")
}
fn credential_free(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .all(|(k, v)| !credential_key(k) && credential_free(v)),
        Value::Array(values) => values.iter().all(credential_free),
        _ => true,
    }
}

fn open(root: &Path) -> Result<rusqlite::Connection, CtoxError> {
    let path = crate::paths::core_db(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(native_error)?;
    }
    let conn = rusqlite::Connection::open(path).map_err(native_error)?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(native_error)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_capability_ingest_routes (
            app_id TEXT NOT NULL,
            path TEXT NOT NULL,
            command_type TEXT NOT NULL,
            default_response_json TEXT NOT NULL,
            pass_headers_json TEXT NOT NULL,
            aliases_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (app_id, path)
        );
        CREATE TABLE IF NOT EXISTS app_capability_ingest_subscriptions (
            app_id TEXT NOT NULL,
            endpoint TEXT NOT NULL,
            protocol TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            config_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (app_id, endpoint)
        );",
    )
    .map_err(native_error)?;
    Ok(conn)
}

pub(super) fn execute(
    root: &Path,
    runtime: &dyn IngestRuntime,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
    now_ms: u64,
) -> Result<Value, CtoxError> {
    match function {
        "register" => register(root, runtime, ctx, decode(args)?, now_ms),
        "subscribe" => {
            let request: Subscribe = decode(args)?;
            crate::app_capabilities::protocol::identifier(&request.endpoint, "$.endpoint")?;
            text(&request.protocol, 64, false, "$.protocol")?;
            let config = match request.config {
                Value::Null => json!({}),
                Value::Object(map) => Value::Object(map),
                _ => return Err(invalid("$.config")),
            };
            if json_size(&config) > 16 * 1024 || !credential_free(&config) {
                return Err(invalid("$.config"));
            }
            open(root)?
                .execute(
                    "INSERT INTO app_capability_ingest_subscriptions
                        (app_id, endpoint, protocol, enabled, config_json, updated_at_ms)
                     VALUES (?1,?2,?3,?4,?5,?6)
                     ON CONFLICT(app_id, endpoint) DO UPDATE SET protocol=excluded.protocol,
                        enabled=excluded.enabled, config_json=excluded.config_json,
                        updated_at_ms=excluded.updated_at_ms",
                    params![
                        ctx.app_id,
                        request.endpoint,
                        request.protocol,
                        request.enabled,
                        config.to_string(),
                        now_ms as i64
                    ],
                )
                .map_err(native_error)?;
            Ok(
                json!({"endpoint":request.endpoint,"protocol":request.protocol,"enabled":request.enabled}),
            )
        }
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown ingest function",
        )),
    }
}

fn register(
    root: &Path,
    runtime: &dyn IngestRuntime,
    ctx: &CapCallContext,
    request: Register,
    now_ms: u64,
) -> Result<Value, CtoxError> {
    if !valid_path(&request.path) {
        return Err(invalid("$.path"));
    }
    crate::app_capabilities::protocol::identifier(&request.command, "$.command")?;
    // Delivery is exactly this command type to this app; it must exist as a
    // handler in the app's active manifest (§16.38 addendum).
    if !runtime.has_handler(&ctx.app_id, &request.command)? {
        return Err(invalid("$.command"));
    }
    let default_response = match &request.default_response {
        None => HttpResponse::empty(202),
        Some(value) => admissible_response(value)
            .and_then(|spec| materialize(runtime, &ctx.app_id, spec))
            .ok_or_else(|| invalid("$.default_response"))?,
    };
    if request.pass_headers.len() > 16 {
        return Err(invalid("$.pass_headers"));
    }
    let mut pass_headers = Vec::new();
    for header in &request.pass_headers {
        if !header_name(header) {
            return Err(invalid("$.pass_headers"));
        }
        pass_headers.push(header.to_ascii_lowercase());
    }
    pass_headers.sort();
    pass_headers.dedup();
    let mut aliases = Vec::new();
    for alias in &request.aliases {
        match LEGACY_ALIASES.iter().find(|(prefix, _)| prefix == alias) {
            Some((_, owner)) if *owner == ctx.app_id => aliases.push(alias.clone()),
            Some(_) => return Err(forbidden("legacy alias belongs to another app")),
            None => return Err(invalid("$.aliases")),
        }
    }
    aliases.sort();
    aliases.dedup();
    let origin = runtime
        .public_origin()
        .filter(|origin| browser::web_url(origin))
        .ok_or_else(|| {
            CtoxError::new(
                ErrorCode::Unavailable,
                "Core HTTPS origin is not configured",
            )
        })?;
    let conn = open(root)?;
    // An alias prefix maps to exactly one registration.
    for alias in &aliases {
        let taken: Option<String> = conn
            .query_row(
                "SELECT r.path FROM app_capability_ingest_routes AS r, json_each(r.aliases_json) AS a
                  WHERE r.app_id=?1 AND a.value=?2 AND r.path<>?3 LIMIT 1",
                params![ctx.app_id, alias, request.path],
                |row| row.get(0),
            )
            .optional()
            .map_err(native_error)?;
        if taken.is_some() {
            return Err(CtoxError::new(
                ErrorCode::Conflict,
                "alias already registered",
            ));
        }
    }
    conn.execute(
        "INSERT INTO app_capability_ingest_routes
            (app_id, path, command_type, default_response_json, pass_headers_json, aliases_json, updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(app_id, path) DO UPDATE SET command_type=excluded.command_type,
            default_response_json=excluded.default_response_json,
            pass_headers_json=excluded.pass_headers_json, aliases_json=excluded.aliases_json,
            updated_at_ms=excluded.updated_at_ms",
        params![
            ctx.app_id,
            request.path,
            request.command,
            serde_json::to_string(&default_response).map_err(native_error)?,
            serde_json::to_string(&pass_headers).map_err(native_error)?,
            serde_json::to_string(&aliases).map_err(native_error)?,
            now_ms as i64
        ],
    )
    .map_err(native_error)?;
    let ingest_url = format!(
        "{}{ROUTE_PREFIX}{}/{}",
        origin.trim_end_matches('/'),
        ctx.app_id,
        request.path
    );
    Ok(json!({"ingest_url":ingest_url,"path":request.path,"command_type":request.command}))
}

/// One incoming HTTP request as seen by the Core HTTPS server.
pub struct IngressRequest {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub received_at_ms: u64,
}
struct Route {
    app_id: String,
    path: String,
    sub_path: String,
    command_type: String,
    default_response: HttpResponse,
    pass_headers: Vec<String>,
}
fn find_route(conn: &rusqlite::Connection, request_path: &str) -> Result<Option<Route>, CtoxError> {
    let load = |app_id: &str, path: &str, sub: &str| -> Result<Option<Route>, CtoxError> {
        conn.query_row(
            "SELECT command_type, default_response_json, pass_headers_json
               FROM app_capability_ingest_routes WHERE app_id=?1 AND path=?2",
            params![app_id, path],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(native_error)?
        .map(|(command_type, default_json, pass_json)| {
            Ok(Route {
                app_id: app_id.into(),
                path: path.into(),
                sub_path: sub.into(),
                command_type,
                default_response: serde_json::from_str(&default_json).map_err(native_error)?,
                pass_headers: serde_json::from_str(&pass_json).map_err(native_error)?,
            })
        })
        .transpose()
    };
    if let Some(rest) = request_path.strip_prefix(ROUTE_PREFIX) {
        let Some((app_id, tail)) = rest.split_once('/') else {
            return Ok(None);
        };
        // Longest registered path prefix wins; the remainder is sub_path.
        let parts = tail.split('/').collect::<Vec<_>>();
        for take in (1..=parts.len().min(4)).rev() {
            let path = parts[..take].join("/");
            if !valid_path(&path) {
                continue;
            }
            if let Some(route) = load(app_id, &path, &parts[take..].join("/"))? {
                return Ok(Some(route));
            }
        }
        return Ok(None);
    }
    for (prefix, owner) in LEGACY_ALIASES {
        if let Some(sub) = request_path.strip_prefix(prefix) {
            let path: Option<String> = conn
                .query_row(
                    "SELECT r.path FROM app_capability_ingest_routes AS r, json_each(r.aliases_json) AS a
                      WHERE r.app_id=?1 AND a.value=?2 LIMIT 1",
                    params![owner, prefix],
                    |row| row.get(0),
                )
                .optional()
                .map_err(native_error)?;
            return match path {
                Some(path) => load(owner, &path, sub),
                None => Ok(None),
            };
        }
    }
    Ok(None)
}

/// The Core HTTPS ingress: request → one app command → admissible response,
/// or the registered default after `wait` (production: RESPONSE_WAIT).
pub fn handle_ingress(
    root: &Path,
    runtime: &dyn IngestRuntime,
    request: &IngressRequest,
    wait: Duration,
) -> HttpResponse {
    let status = HttpResponse::empty;
    let route = match open(root).and_then(|conn| find_route(&conn, &request.path)) {
        Ok(Some(route)) => route,
        Ok(None) => return status(404),
        Err(_) => return status(503),
    };
    let mut headers = serde_json::Map::new();
    for (name, value) in &request.headers {
        let name = name.to_ascii_lowercase();
        if PRIVATE_HEADERS.contains(&name.as_str()) && !route.pass_headers.contains(&name) {
            continue;
        }
        if !header_name(&name) || value.len() > 8192 {
            continue;
        }
        headers.insert(name, Value::String(value.clone()));
    }
    let mut payload = json!({
        "event_id": uuid::Uuid::new_v4().to_string(),
        "method": request.method,
        "path": route.path,
        "sub_path": route.sub_path,
        "query": request.query,
        "headers": headers,
        "received_at_ms": request.received_at_ms,
    });
    match std::str::from_utf8(&request.body) {
        _ if request.body.is_empty() => {}
        Ok(body) if body.len() <= MAX_BODY_BYTES => payload["body"] = json!(body),
        _ => {
            let mime = request
                .headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.as_str())
                .unwrap_or("application/octet-stream");
            match runtime.store_body(&route.app_id, &request.body, mime) {
                Ok(reference) => payload["body_ref"] = json!({ "ref": reference }),
                Err(_) => return status(503),
            }
        }
    }
    let receiver = match runtime.dispatch(&route.app_id, &route.command_type, payload) {
        Ok(receiver) => receiver,
        Err(_) => return status(503),
    };
    // The command keeps running after the deadline; only the answer is fixed.
    match receiver.recv_timeout(wait) {
        Ok(result) => match result.get("response") {
            Some(response) => admissible_response(response)
                .and_then(|spec| materialize(runtime, &route.app_id, spec))
                .unwrap_or_else(|| status(502)),
            None => route.default_response,
        },
        Err(_) => route.default_response,
    }
}

/// The app-side half of the ingress runtime, owned by the v4 app runtime
/// (#830 AppRuntimeIngress over the sync-store writer): active-manifest
/// handler lookup, command submission to (app_id, type) with its result, and
/// files records. Bound exactly once by the composition root; there is no
/// second Core write path into the sync store here.
pub trait AppIngress: Send + Sync {
    fn has_handler(&self, app_id: &str, command_type: &str) -> Result<bool, CtoxError>;
    fn dispatch(
        &self,
        app_id: &str,
        command_type: &str,
        payload: Value,
    ) -> Result<std::sync::mpsc::Receiver<Value>, CtoxError>;
    fn store_body(&self, app_id: &str, bytes: &[u8], mime: &str) -> Result<String, CtoxError>;
    fn load_body(&self, app_id: &str, reference: &str) -> Result<Vec<u8>, CtoxError>;
}
static APP_INGRESS: std::sync::OnceLock<std::sync::Arc<dyn AppIngress>> =
    std::sync::OnceLock::new();
/// Composition root only. A second binding is refused.
pub fn bind_app_ingress(ingress: std::sync::Arc<dyn AppIngress>) -> Result<(), CtoxError> {
    APP_INGRESS
        .set(ingress)
        .map_err(|_| CtoxError::new(ErrorCode::Conflict, "app ingress already bound"))
}
/// Production runtime: public origin from the typed runtime config key
/// `business_os.public_origin`; everything app-side via the bound AppIngress.
/// Unbound means unavailable (register refused, HTTP 503), never a fallback.
pub struct CoreIngestRuntime<'a> {
    pub root: &'a Path,
}
impl CoreIngestRuntime<'_> {
    fn app(&self) -> Result<&dyn AppIngress, CtoxError> {
        APP_INGRESS
            .get()
            .map(|ingress| ingress.as_ref())
            .ok_or_else(|| CtoxError::new(ErrorCode::Unavailable, "app runtime ingress not bound"))
    }
}
impl IngestRuntime for CoreIngestRuntime<'_> {
    fn public_origin(&self) -> Option<String> {
        config::public_origin(self.root)
    }
    fn has_handler(&self, app_id: &str, command_type: &str) -> Result<bool, CtoxError> {
        self.app()?.has_handler(app_id, command_type)
    }
    fn dispatch(
        &self,
        app_id: &str,
        command_type: &str,
        payload: Value,
    ) -> Result<std::sync::mpsc::Receiver<Value>, CtoxError> {
        self.app()?.dispatch(app_id, command_type, payload)
    }
    fn store_body(&self, app_id: &str, bytes: &[u8], mime: &str) -> Result<String, CtoxError> {
        self.app()?.store_body(app_id, bytes, mime)
    }
    fn load_body(&self, app_id: &str, reference: &str) -> Result<Vec<u8>, CtoxError> {
        self.app()?.load_body(app_id, reference)
    }
}
/// HTTP side for business_os/server.rs: is this path served by net.ingest?
/// `/ingest/…` always; a legacy alias only once its owner registered it, so
/// existing native tracking keeps answering until then (no sender reconfig).
pub fn serves(root: &Path, path: &str) -> bool {
    if path.starts_with(ROUTE_PREFIX) {
        return true;
    }
    LEGACY_ALIASES
        .iter()
        .any(|(prefix, _)| path.starts_with(prefix))
        && open(root)
            .and_then(|conn| find_route(&conn, path))
            .is_ok_and(|route| route.is_some())
}
