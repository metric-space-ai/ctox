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
    blobs: &dyn cap2::blob::BlobAccess,
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
                let reference = cap2::blob::publish(blobs, ctx, &page.bytes, &mime)?;
                captures.push(
                    json!({"url":url,"final_url":page.final_url,"status":page.status,
                    "mime":mime,"size":reference.size,"blob":{"ref":reference.sha256}}),
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
