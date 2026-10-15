//! `browser` — managed browser engine as a device (CATALOG Nr. 3). The app
//! names a session and a fixed engine op; automation scripts, extraction
//! schemas and research flows stay app data/logic. Capture output crosses the
//! boundary only as a files reference (§16.32 BlobRef = files.ref).
use super::*;
use serde::Deserialize;

/// The runtime ops the native engine bridge exposes (rxdb_peer_browser.rs
/// op translation). No script evaluation, no file access op.
pub const ALLOWED_OPS: &[&str] = &[
    "navigate",
    "reload",
    "back",
    "forward",
    "tab_open",
    "tab_activate",
    "tab_close",
    "dialog_respond",
    "permission_respond",
    "clipboard_copy",
    "clipboard_paste",
    "clipboard_clear",
    "credential_fill",
    "nav_state",
];
pub const MAX_CAPTURE_URLS: usize = 10;
const MAX_PARAMS_BYTES: usize = 16 * 1024;

pub struct CapturedPage {
    pub final_url: String,
    pub status: u16,
    pub mime: String,
    pub bytes: Vec<u8>,
}
/// Core-owned engine binding (the native peer's session manager). Session
/// realm/owner binding and the executor fence are enforced by the engine at
/// the effect point; credential values are resolved there from credential_ref,
/// never supplied by the app.
pub trait BrowserEngine {
    fn op(
        &self,
        ctx: &CapCallContext,
        session_id: &str,
        tab_id: Option<&str>,
        op: &str,
        params: &Value,
    ) -> Result<Value, CtoxError>;
    fn session_stop(&self, ctx: &CapCallContext, session_id: &str) -> Result<Value, CtoxError>;
    fn auth_assist_complete(
        &self,
        ctx: &CapCallContext,
        session_id: &str,
    ) -> Result<Value, CtoxError>;
    fn capture(
        &self,
        ctx: &CapCallContext,
        url: &str,
        timeout_ms: u64,
    ) -> Result<CapturedPage, CtoxError>;
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Op {
    session_id: String,
    tab_id: Option<String>,
    op: String,
    #[serde(default)]
    params: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    session_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    urls: Vec<String>,
    timeout_ms: Option<u64>,
}

/// Absolute http(s) URL without userinfo.
pub(crate) fn web_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let rest = if let Some(rest) = lower.strip_prefix("https://") {
        rest
    } else if let Some(rest) = lower.strip_prefix("http://") {
        rest
    } else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !authority.is_empty()
        && !authority.contains('@')
        && value.len() <= 4096
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
}
fn secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    !key.ends_with("_ref")
        && (key.contains("password")
            || key.contains("secret")
            || key.contains("token")
            || key.contains("usernamevalue"))
}
fn strip_secrets(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| !secret_key(key))
                .map(|(k, v)| (k, strip_secrets(v)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(strip_secrets).collect()),
        other => other,
    }
}
fn validate_params(op: &str, params: &Value) -> Result<(), CtoxError> {
    let map = match params {
        Value::Null => return Ok(()),
        Value::Object(map) => map,
        _ => return Err(invalid("$.params")),
    };
    if json_size(params) > MAX_PARAMS_BYTES {
        return Err(invalid("$.params"));
    }
    // Credentials are resolved by the engine from a reference; a value in
    // the request would cross the app boundary and the command log.
    if map.keys().any(|key| secret_key(key)) {
        return Err(invalid("$.params"));
    }
    if matches!(op, "navigate" | "tab_open") {
        if let Some(url) = map.get("url") {
            if !url.as_str().is_some_and(web_url) {
                return Err(invalid("$.params.url"));
            }
        } else if op == "navigate" {
            return Err(invalid("$.params.url"));
        }
    }
    if op == "credential_fill"
        && !map
            .get("credential_ref")
            .and_then(Value::as_str)
            .is_some_and(|r| !r.is_empty() && r.len() <= 256)
    {
        return Err(invalid("$.params.credential_ref"));
    }
    Ok(())
}

pub(super) fn execute(
    engine: &dyn BrowserEngine,
    files: Result<&dyn files::FileStore, CtoxError>,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
) -> Result<Value, CtoxError> {
    use crate::app_capabilities::protocol::identifier;
    match function {
        "op" => {
            let request: Op = decode(args)?;
            identifier(&request.session_id, "$.session_id")?;
            if let Some(tab) = &request.tab_id {
                identifier(tab, "$.tab_id")?;
            }
            if !ALLOWED_OPS.contains(&request.op.as_str()) {
                return Err(invalid("$.op"));
            }
            validate_params(&request.op, &request.params)?;
            let params = if request.params.is_null() {
                json!({})
            } else {
                request.params
            };
            let result = engine.op(
                ctx,
                &request.session_id,
                request.tab_id.as_deref(),
                &request.op,
                &params,
            )?;
            Ok(strip_secrets(result))
        }
        "session_stop" | "auth_assist_complete" => {
            let request: Session = decode(args)?;
            identifier(&request.session_id, "$.session_id")?;
            let result = if function == "session_stop" {
                engine.session_stop(ctx, &request.session_id)?
            } else {
                engine.auth_assist_complete(ctx, &request.session_id)?
            };
            Ok(strip_secrets(result))
        }
        "capture" => {
            let request: Capture = decode(args)?;
            if request.urls.is_empty() || request.urls.len() > MAX_CAPTURE_URLS {
                return Err(invalid("$.urls"));
            }
            if request.urls.iter().any(|url| !web_url(url)) {
                return Err(invalid("$.urls"));
            }
            let timeout = request.timeout_ms.unwrap_or(30_000);
            if !(1_000..=120_000).contains(&timeout) {
                return Err(invalid("$.timeout_ms"));
            }
            let mut captures = Vec::new();
            for url in &request.urls {
                let page = engine.capture(ctx, url, timeout)?;
                if !web_url(&page.final_url) {
                    return Err(CtoxError::uncertain());
                }
                let mime = if page.mime.is_empty() || page.mime.len() > 128 {
                    "application/octet-stream".to_owned()
                } else {
                    page.mime
                };
                // Published as a files record owned by the calling app; the
                // transfer id stays internal (§16.32).
                let store = files.as_ref().map_err(Clone::clone)?;
                let name = format!("capture-{}.html", captures.len() + 1);
                let meta = store.put(&ctx.app_id, &ctx.actor.id, &name, &mime, &page.bytes)?;
                if meta.reference != sha256_hex(&page.bytes) || meta.owner_app != ctx.app_id {
                    return Err(CtoxError::uncertain());
                }
                captures.push(
                    json!({"url":url,"final_url":page.final_url,"status":page.status,
                    "mime":mime,"size":meta.size,"file_id":meta.id,"blob":{"ref":meta.reference}}),
                );
            }
            Ok(json!({ "captures": captures }))
        }
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown browser function",
        )),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthAssistRequest {
    source_id: String,
    #[serde(alias = "target_url")]
    target_url_override: Option<String>,
    credential_ref: Option<String>,
    login_hint: Option<String>,
    preferred_auth_assist: Option<Value>,
    requesting_task_id: Option<String>,
    source_module: Option<String>,
    command_path: Option<String>,
}
/// `browser.auth_assist.request` — parameters 1:1 as the native
/// `enqueue_web_stack_auth_assist_request` (§16.21). Module and session owner
/// are the Core-bound app and actor; a payload claiming another app is refused.
pub(super) fn auth_assist_request(
    root: &Path,
    ctx: &CapCallContext,
    args: &Value,
) -> Result<Value, CtoxError> {
    use crate::app_capabilities::protocol::identifier;
    let request: AuthAssistRequest = decode(args)?;
    identifier(&request.source_id, "$.source_id")?;
    if request
        .source_module
        .as_deref()
        .is_some_and(|module| module != ctx.app_id)
    {
        return Err(forbidden("auth assist belongs to the calling app"));
    }
    if let Some(url) = &request.target_url_override {
        if !web_url(url) {
            return Err(invalid("$.target_url_override"));
        }
    }
    if let Some(reference) = &request.credential_ref {
        // A reference into the instance keychain, never a value.
        let name = reference
            .strip_prefix("ctox-secret://credentials/")
            .ok_or_else(|| invalid("$.credential_ref"))?;
        identifier(name, "$.credential_ref")?;
    }
    if let Some(hint) = &request.login_hint {
        text(hint, 256, true, "$.login_hint")?;
    }
    if request
        .preferred_auth_assist
        .as_ref()
        .is_some_and(|value| !value.is_null() && !value.is_object())
    {
        return Err(invalid("$.preferred_auth_assist"));
    }
    let task = request
        .requesting_task_id
        .clone()
        .unwrap_or_else(|| ctx.command_id.clone());
    identifier(&task, "$.requesting_task_id")?;
    let path = request
        .command_path
        .clone()
        .unwrap_or_else(|| "cap.browser.auth_assist.request".into());
    identifier(&path, "$.command_path")?;
    let effect = crate::service::business_os::enqueue_web_stack_auth_assist_request(
        root,
        &request.source_id,
        request.target_url_override.as_deref(),
        request.credential_ref.as_deref(),
        request.login_hint.as_deref(),
        request
            .preferred_auth_assist
            .as_ref()
            .filter(|v| v.is_object()),
        &task,
        &ctx.app_id,
        &path,
        Some(&ctx.actor.id),
        true,
        true,
    )
    .map_err(native_error)?;
    let payload = effect.get("payload").cloned().unwrap_or(Value::Null);
    let pick = |key: &str| effect.get(key).cloned().unwrap_or(Value::Null);
    let field = |key: &str| payload.get(key).cloned().unwrap_or(Value::Null);
    Ok(strip_secrets(json!({
        "command_id": pick("command_id"), "status": pick("status"), "task_id": pick("task_id"),
        "source_id": field("source_id"), "session_id": field("session_id"), "tab_id": field("tab_id"),
        "target_url": field("target_url"), "allowed_domains": field("allowed_domains"),
        "credential_ref": field("credential_ref"),
        "expires_at_ms": field("expires_at_ms"),
    })))
}

/// §16.46 production engine: the peer's Core-authorized internal entry for
/// session ops (policy, (session → app, user) binding and audit happen there);
/// capture runs in a fresh, stateless automation browser (no user session).
pub struct PeerBrowserEngine<'a> {
    pub root: &'a Path,
}
const OP_COMMANDS: &[(&str, &str)] = &[
    ("navigate", "browser.navigate"),
    ("reload", "browser.reload"),
    ("back", "browser.back"),
    ("forward", "browser.forward"),
    ("tab_open", "browser.tab.open"),
    ("tab_activate", "browser.tab.activate"),
    ("tab_close", "browser.tab.close"),
    ("dialog_respond", "browser.dialog.respond"),
    ("permission_respond", "browser.permission.respond"),
    ("clipboard_copy", "browser.clipboard.copy"),
    ("clipboard_paste", "browser.clipboard.paste"),
    ("clipboard_clear", "browser.clipboard.clear"),
];
impl PeerBrowserEngine<'_> {
    fn run(&self, ctx: &CapCallContext, command: &str, payload: Value) -> Result<Value, CtoxError> {
        let binding = crate::business_os::CoreBrowserBinding {
            app_id: &ctx.app_id,
            user_id: &ctx.actor.id,
            role: &ctx.actor.role,
            command_id: &ctx.command_id,
            call_id: &ctx.call_id,
        };
        crate::business_os::apply_core_browser_command(self.root, &binding, command, payload)
            .map_err(|error| {
                if format!("{error:#}").contains("browser core entry refused") {
                    forbidden("browser session or command not permitted")
                } else {
                    native_error(error)
                }
            })
    }
}
impl BrowserEngine for PeerBrowserEngine<'_> {
    fn op(
        &self,
        ctx: &CapCallContext,
        session_id: &str,
        tab_id: Option<&str>,
        op: &str,
        params: &Value,
    ) -> Result<Value, CtoxError> {
        // credential_fill needs the user's own secrets.manage capability;
        // nav_state has no engine command of its own.
        let command = OP_COMMANDS
            .iter()
            .find(|(name, _)| *name == op)
            .map(|(_, command)| *command)
            .ok_or_else(|| forbidden("browser op is not available to apps"))?;
        let mut payload = params.as_object().cloned().unwrap_or_default();
        payload.remove("tabId");
        payload.remove("timeoutMs");
        payload.insert("session_id".into(), json!(session_id));
        if let Some(tab) = tab_id {
            payload.insert("tab_id".into(), json!(tab));
        }
        self.run(ctx, command, Value::Object(payload))
    }
    fn session_stop(&self, ctx: &CapCallContext, session_id: &str) -> Result<Value, CtoxError> {
        self.run(
            ctx,
            "browser.session.stop",
            json!({ "session_id": session_id }),
        )
    }
    fn auth_assist_complete(
        &self,
        ctx: &CapCallContext,
        session_id: &str,
    ) -> Result<Value, CtoxError> {
        self.run(
            ctx,
            "web_stack.auth_assist.complete",
            json!({ "session_id": session_id, "confirmed": true }),
        )
    }
    fn capture(
        &self,
        _: &CapCallContext,
        url: &str,
        timeout_ms: u64,
    ) -> Result<CapturedPage, CtoxError> {
        let target = serde_json::to_string(url).map_err(native_error)?;
        let source = format!(
            "await page.goto({target}, {{ waitUntil: 'domcontentloaded' }}); return {{ final_url: page.url(), html: await page.content() }};"
        );
        let result = crate::web_stack::run_browser_automation(
            self.root,
            &crate::web_stack::BrowserAutomationRequest {
                dir: None,
                timeout_ms: Some(timeout_ms),
                source,
            },
        )
        .map_err(native_error)?;
        let page = result.get("result").cloned().unwrap_or(Value::Null);
        let html = page
            .get("html")
            .and_then(Value::as_str)
            .ok_or_else(CtoxError::uncertain)?;
        Ok(CapturedPage {
            final_url: page
                .get("final_url")
                .and_then(Value::as_str)
                .unwrap_or(url)
                .to_owned(),
            status: 200,
            mime: "text/html".into(),
            bytes: html.as_bytes().to_vec(),
        })
    }
}
