//! CAP-3 OS primitives (CATALOG.md: audit, config, secrets, lock, browser,
//! net.ingest; platform agent.flow; flat agent.coding; mail.server intake).
//! Same shape as CAP-2: one parent-module declaration loads this root, the
//! CAP-1 registry/policy/protocol stay unchanged. Every function here is an
//! app-independent operating-system resource; no app collection, app name or
//! domain state transition lives in this module (E17, §16.29).
use crate::app_capabilities::caps::{NativeCall, NativeRoute};
use crate::app_capabilities::schema::PayloadShape;
use crate::app_capabilities::{
    cap2, CapCallContext, CapabilityBackend, CapabilityHost, CapabilityRegistry, CtoxError,
    ErrorCode,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::path::Path;
#[path = "audit.rs"]
pub mod audit;
#[path = "browser.rs"]
pub mod browser;
#[path = "config.rs"]
pub mod config;
#[path = "directory.rs"]
pub mod directory;
#[path = "files.rs"]
pub mod files;
#[path = "flow.rs"]
pub mod flow;
#[path = "ingest.rs"]
pub mod ingest;
#[path = "lock.rs"]
pub mod lock;
#[path = "process.rs"]
pub mod process;
#[path = "secrets.rs"]
pub mod secrets;
#[cfg(test)]
#[path = "cap3_tests.rs"]
mod tests;

fn invalid(path: &str) -> CtoxError {
    CtoxError::invalid(path)
}
fn forbidden(message: &str) -> CtoxError {
    CtoxError::new(ErrorCode::Forbidden, message)
}
fn decode<T: DeserializeOwned>(value: &Value) -> Result<T, CtoxError> {
    serde_json::from_value(value.clone()).map_err(|_| invalid("$.args"))
}
/// Native errors can carry paths, SQL or upstream detail; never relay them.
fn native_error(_: impl std::fmt::Display) -> CtoxError {
    CtoxError::new(ErrorCode::Unavailable, "native capability unavailable")
}
/// Bounded printable text (may be empty when `allow_empty`).
fn text(value: &str, max: usize, allow_empty: bool, path: &str) -> Result<(), CtoxError> {
    if (!allow_empty && value.trim().is_empty())
        || value.len() > max
        || value.chars().any(char::is_control)
    {
        return Err(invalid(path));
    }
    Ok(())
}
fn json_size(value: &Value) -> usize {
    serde_json::to_vec(value)
        .map(|v| v.len())
        .unwrap_or(usize::MAX)
}
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
/// Platform callers (MAPPING.md (c)): Core-state mirrors owned by ctox-platform.
pub const PLATFORM_APP_IDS: &[&str] = &["ctox-platform", "ctox"];
pub(crate) fn is_platform(ctx: &CapCallContext) -> bool {
    PLATFORM_APP_IDS.contains(&ctx.app_id.as_str())
}

const fn route(
    cap: &'static str,
    function: &'static str,
    command: &'static str,
    rust: &'static str,
    source: &'static str,
) -> NativeRoute {
    NativeRoute::new(cap, function, command, rust, source, PayloadShape::Object)
}
pub static ROUTES: &[NativeRoute] = &[
    route(
        "audit",
        "event",
        "cap.audit.event",
        "business_os::store::open_store + business_events INSERT",
        "src/core/business_os/store.rs:28104",
    ),
    route(
        "audit",
        "query",
        "cap.audit.query",
        "business_events SELECT (scoped to the calling app)",
        "src/core/business_os/store.rs:28104",
    ),
    route(
        "config",
        "get",
        "cap.config.get",
        "runtime_env::env_or_config",
        "src/core/execution/models/runtime_env.rs:168",
    ),
    route(
        "secret",
        "put",
        "cap.secret.put",
        "secrets::write_secret_record / set_credential",
        "src/core/secrets.rs:545",
    ),
    route(
        "secret",
        "list",
        "cap.secret.list",
        "secrets::list_secret_records",
        "src/core/secrets.rs:179",
    ),
    route(
        "secret",
        "delete",
        "cap.secret.delete",
        "secrets::delete_secret_record",
        "src/core/secrets.rs:651",
    ),
    route(
        "secret",
        "exists",
        "cap.secret.exists",
        "secrets::secret_exists",
        "src/core/secrets.rs:1212",
    ),
    route(
        "secret",
        "scope",
        "cap.secret.scope",
        "app keychain scope (Core binding)",
        "src/core/secrets.rs:1934",
    ),
    route(
        "secrets",
        "exists",
        "cap.secret.exists",
        "secrets::secret_exists",
        "src/core/secrets.rs:1212",
    ),
    route(
        "lock",
        "claim",
        "cap.lock.claim",
        "app_capability_locks (core db, fenced lease)",
        "src/core/app_capabilities/caps/lock.rs",
    ),
    route(
        "lock",
        "release",
        "cap.lock.release",
        "app_capability_locks (core db, fenced lease)",
        "src/core/app_capabilities/caps/lock.rs",
    ),
    route(
        "fence",
        "claim",
        "cap.lock.claim",
        "app_capability_locks (core db, fenced lease)",
        "src/core/app_capabilities/caps/lock.rs",
    ),
    route(
        "fence",
        "release",
        "cap.lock.release",
        "app_capability_locks (core db, fenced lease)",
        "src/core/app_capabilities/caps/lock.rs",
    ),
    route(
        "agent.flow",
        "record",
        "cap.agent.flow.record",
        "harness_flow::record_harness_flow_event",
        "src/core/service/harness_flow.rs:255",
    ),
    route(
        "browser",
        "op",
        "cap.browser.op",
        "rxdb_peer_browser runtime op (manager.request)",
        "src/core/business_os/rxdb_peer_browser.rs:1652",
    ),
    route(
        "browser",
        "session_stop",
        "cap.browser.session_stop",
        "rxdb_peer_browser browser.session.stop",
        "src/core/business_os/rxdb_peer_browser.rs:1535",
    ),
    route(
        "browser",
        "auth_assist_complete",
        "cap.browser.auth_assist_complete",
        "rxdb_peer_browser web_stack.auth_assist.complete",
        "src/core/business_os/rxdb_peer_browser.rs:1334",
    ),
    route(
        "browser",
        "capture",
        "cap.browser.capture",
        "browser engine page capture + files publication (§16.32)",
        "src/core/capabilities/browser.rs:1",
    ),
    route(
        "net.ingest.webhook",
        "register",
        "cap.net.ingest.register",
        "app_capability_ingest_routes (core db) + Core HTTPS ingress",
        "src/core/app_capabilities/caps/ingest.rs",
    ),
    route(
        "net.ingest",
        "subscribe",
        "cap.net.ingest.subscribe",
        "app_capability_ingest_subscriptions (core db)",
        "src/core/app_capabilities/caps/ingest.rs",
    ),
    // ---- CAP-3b
    route(
        "browser.automation",
        "run",
        "cap.browser.automation.run",
        "browser.op sequence via the §16.46 core entry",
        "src/core/app_capabilities/caps/browser.rs",
    ),
    route(
        "files",
        "put",
        "cap.files.put",
        "files collection + transfer CAS (FileStore)",
        "src/core/app_capabilities/caps/files.rs",
    ),
    route(
        "files",
        "get",
        "cap.files.get",
        "files collection + transfer CAS (FileStore)",
        "src/core/app_capabilities/caps/files.rs",
    ),
    route(
        "files",
        "list",
        "cap.files.list",
        "files collection (FileStore)",
        "src/core/app_capabilities/caps/files.rs",
    ),
    route(
        "files",
        "delete",
        "cap.files.delete",
        "files collection (FileStore)",
        "src/core/app_capabilities/caps/files.rs",
    ),
    route(
        "process",
        "run",
        "cap.process.run",
        "std::process::Command (allowlist, env_clear, app workspace)",
        "src/core/app_capabilities/caps/process.rs",
    ),
    route(
        "browser.auth_assist",
        "request",
        "web_stack.auth_assist.request",
        "service::business_os::enqueue_web_stack_auth_assist_request",
        "src/core/service/business_os.rs:5059",
    ),
    route(
        "directory",
        "user",
        "cap.directory.user",
        "business_os::store::pull_business_users_for_rxdb",
        "src/core/business_os/store.rs:2267",
    ),
    route(
        "identity.users",
        "list",
        "cap.directory.users",
        "business_os::store::pull_business_users_for_rxdb",
        "src/core/business_os/store.rs:2267",
    ),
    route(
        "policy",
        "module_decision",
        "cap.policy.module_decision",
        "business_os::policy::evaluate",
        "src/core/business_os/policy.rs:397",
    ),
    route(
        "command",
        "get",
        "cap.command.get",
        "business_commands SELECT (own app)",
        "src/core/business_os/store.rs:28099",
    ),
];

pub fn register(registry: &mut CapabilityRegistry) -> Result<(), CtoxError> {
    for route in ROUTES {
        registry.register(*route)?;
    }
    Ok(())
}
/// CAP-1 + CAP-2 + CAP-3 in one Core-owned registry.
pub fn registry() -> CapabilityRegistry {
    let mut registry = cap2::registry();
    register(&mut registry).expect("duplicate CAP-3 function");
    registry
}
fn all() -> impl Iterator<Item = &'static NativeRoute> {
    crate::app_capabilities::caps::all_routes()
        .chain(cap2::ROUTES)
        .chain(cap2::mail::ROUTES)
        .chain(ROUTES)
}
fn resolve(name: &str, function: &str) -> Option<NativeRoute> {
    all()
        .find(|route| route.capability == name && route.function == function)
        .copied()
        .or_else(|| {
            let path = format!("{name}.{function}");
            all()
                .filter(|route| cap2::call_path(route) == path)
                .max_by_key(|route| route.capability.len())
                .copied()
        })
}
/// §16.30 Nr. 5: flat JS arguments; the CAP-1 payload envelope is internal.
/// Unknown names pass through unchanged and are rejected by the registry.
fn wire_args(name: String, function: String, args: Value) -> (String, String, Value) {
    let Some(route) = resolve(&name, &function) else {
        return (name, function, args);
    };
    let flat = cap2::ROUTES
        .iter()
        .chain(cap2::mail::ROUTES)
        .chain(ROUTES)
        .any(|r| r.capability == route.capability && r.function == route.function);
    let args = if flat {
        // `ctx.cap.secret.scope()` and `mail.server.config.get()` carry no args.
        let args = if args.is_null() { json!({}) } else { args };
        json!({ "payload": args })
    } else if route.capability == "agent.coding" {
        crate::app_capabilities::caps::coding_flat_envelope(route.function, args)
    } else {
        args
    };
    (route.capability.into(), route.function.into(), args)
}
/// Typed JS-facing entry point (supersedes cap2::invoke; same CAP-1 core).
pub fn invoke(
    host: &dyn CapabilityHost,
    backend: &dyn CapabilityBackend,
    ctx: &CapCallContext,
    name: &str,
    function: &str,
    args: Value,
) -> Result<Value, CtoxError> {
    let (name, function, args) = wire_args(name.into(), function.into(), args);
    registry().invoke(host, backend, ctx, &name, &function, args)
}
/// LocalTransport adapter retaining CAP-1 framing, errors and correlation.
pub fn handle(
    host: &dyn CapabilityHost,
    backend: &dyn CapabilityBackend,
    message: crate::app_capabilities::protocol::CapMessage,
) -> Result<crate::app_capabilities::protocol::CapMessage, CtoxError> {
    use crate::app_capabilities::protocol::CapMessage;
    let message = match message {
        CapMessage::Invoke {
            request_id,
            ctx,
            name,
            function,
            args,
        } => {
            let (name, function, args) = wire_args(name, function, args);
            CapMessage::Invoke {
                request_id,
                ctx,
                name,
                function,
                args,
            }
        }
        message => message,
    };
    registry().handle(host, backend, message)
}

/// Instantiate on the Core blocking executor. Every trait object is a
/// Core-owned authority (no wire constructor, no permissive default); the
/// registry has already authorized the Core-bound ctx and holds the fence.
pub struct Cap3Backend<'a> {
    pub root: &'a Path,
    pub blobs: &'a dyn cap2::blob::BlobAccess,
    pub intake: &'a dyn secrets::SecretIntake,
    pub browser: &'a dyn browser::BrowserEngine,
    pub ingest: &'a dyn ingest::IngestRuntime,
    /// Core clock (§16.8), milliseconds.
    pub now_ms: &'a dyn Fn() -> u64,
    /// None in production: the composition root binds files::bind_file_store.
    pub files: Option<&'a dyn files::FileStore>,
}
impl Cap3Backend<'_> {
    fn file_store(&self) -> Result<&dyn files::FileStore, CtoxError> {
        match self.files {
            Some(store) => Ok(store),
            None => files::bound(),
        }
    }
}
impl CapabilityBackend for Cap3Backend<'_> {
    fn execute(&self, ctx: &CapCallContext, call: &NativeCall) -> Result<Value, CtoxError> {
        let function = call.route().function;
        let payload = call.payload();
        match call.route().capability {
            "audit" => audit::execute(self.root, ctx, function, payload, (self.now_ms)()),
            "config" => config::execute(self.root, ctx, function, payload),
            "secret" | "secrets" => {
                secrets::execute(self.root, self.intake, ctx, function, payload)
            }
            "lock" | "fence" => lock::execute(self.root, ctx, function, payload, (self.now_ms)()),
            "agent.flow" => flow::execute(self.root, ctx, payload),
            "browser" => browser::execute(self.browser, self.file_store(), ctx, function, payload),
            "net.ingest.webhook" | "net.ingest" => ingest::execute(
                self.root,
                self.ingest,
                ctx,
                function,
                payload,
                (self.now_ms)(),
            ),
            "mail.server" => cap2::mail::server(self.root, self.intake, ctx, call),
            "files" => files::execute(self.file_store()?, ctx, function, payload),
            "process" => process::execute(self.root, self.file_store(), ctx, payload),
            "browser.auth_assist" => browser::auth_assist_request(self.root, ctx, payload),
            "browser.automation" => {
                browser::automation_run(self.browser, self.file_store(), ctx, payload)
            }
            "directory" | "identity.users" | "policy" | "command" => {
                directory::execute(self.root, ctx, call.route().capability, function, payload)
            }
            _ => cap2::Cap2Backend {
                root: self.root,
                blobs: self.blobs,
            }
            .execute(ctx, call),
        }
    }
}

/// Machine-readable CAP-3 contract: flat JS path, argument and result shape,
/// rejection codes. Kept next to ROUTES; the test pins one entry per route.
pub fn contract() -> Value {
    let s = |v: &str| json!({"type":"string","maxLength":v.parse::<u64>().unwrap_or(256)});
    let scope = json!({"type":"string","description":"own app scope, own user scope or 'credentials' (secrets.manage roles)"});
    let entries = vec![
        (
            "audit.event",
            json!({"required":["collection","record_id","event_type"],"properties":{"collection":s("128"),"record_id":s("256"),"event_type":s("256"),"payload":{"type":"object","maxBytes":65536},"observed_at_ms":{"type":"integer"}}}),
            json!({"event_id":"string","stored":"boolean"}),
            vec!["invalid", "conflict", "unavailable"],
        ),
        (
            "audit.query",
            json!({"properties":{"command_type_prefix":s("256"),"collection":s("128"),"record_id":s("256"),"summary":{"type":"object","maxProperties":8},"order":{"enum":["observed_at_ms","-observed_at_ms"]},"limit":{"type":"integer","minimum":1,"maximum":1000}}}),
            json!({"events":[{"event_id":"string","collection":"string","record_id":"string","event_type":"string","payload":"object","observed_at_ms":"integer","app_id":"string"}]}),
            vec!["invalid", "unavailable"],
        ),
        (
            "config.get",
            json!({"required":["key"],"properties":{"key":{"enum":config::ALLOWED_KEYS.iter().chain(config::PLATFORM_KEYS).collect::<Vec<_>>(),"description":"platform keys only for platform callers"}}}),
            json!({"key":"string","value":"string|null"}),
            vec!["invalid", "forbidden"],
        ),
        (
            "secret.put",
            json!({"required":["command_id","name","value"],"properties":{"command_id":s("256"),"name":s("128"),"value":{"const":{"secret_intake":true}},"scope":scope,"user":{"type":"boolean"},"description":s("512")}}),
            json!({"scope":"string","name":"string","stored":"boolean"}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "secret.list",
            json!({"properties":{"scope":scope,"user":{"type":"boolean"}}}),
            json!({"scope":"string","secrets":[{"name":"string","description":"string|null","created_at":"string","updated_at":"string"}]}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "secret.delete",
            json!({"required":["name"],"properties":{"name":s("128"),"scope":scope,"user":{"type":"boolean"}}}),
            json!({"scope":"string","name":"string","deleted":"boolean"}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "secret.exists",
            json!({"required":["name"],"properties":{"name":s("128"),"scope":scope,"user":{"type":"boolean"}}}),
            json!({"scope":"string","name":"string","exists":"boolean"}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "secret.scope",
            json!({"properties":{"user":{"type":"boolean"}}}),
            json!({"scope":"string"}),
            vec!["invalid"],
        ),
        (
            "secrets.exists",
            json!({"$ref":"secret.exists"}),
            json!({"$ref":"secret.exists"}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "lock.claim",
            json!({"required":["key"],"properties":{"key":s("256"),"ttl_ms":{"type":"integer","minimum":lock::MIN_TTL_MS,"maximum":lock::MAX_TTL_MS}}}),
            json!({"token":"string|null","fence":"integer|null","expires_at_ms":"integer|null"}),
            vec!["invalid", "unavailable"],
        ),
        (
            "lock.release",
            json!({"required":["token"],"properties":{"token":s("256")}}),
            json!({"released":"boolean"}),
            vec!["invalid", "conflict(stale_fence)", "unavailable"],
        ),
        (
            "fence.claim",
            json!({"$ref":"lock.claim"}),
            json!({"$ref":"lock.claim"}),
            vec!["invalid", "unavailable"],
        ),
        (
            "fence.release",
            json!({"$ref":"lock.release"}),
            json!({"$ref":"lock.release"}),
            vec!["invalid", "conflict(stale_fence)", "unavailable"],
        ),
        (
            "agent.flow.record",
            json!({"required":["event_kind","title"],"properties":{"event_kind":s("128"),"title":s("512"),"body_text":s("16384"),"message_key":s("256"),"work_id":s("256"),"ticket_key":s("256"),"attempt_index":{"type":"integer"},"metadata":{"type":"object","maxBytes":16384}}}),
            json!({"event_id":"string","chain_key":"string"}),
            vec!["invalid", "forbidden(platform only)", "unavailable"],
        ),
        (
            "browser.op",
            json!({"required":["session_id","op"],"properties":{"session_id":s("256"),"tab_id":s("256"),"op":{"enum":browser::ALLOWED_OPS},"params":{"type":"object","maxBytes":16384}}}),
            json!({"type":"object","description":"engine nav/op result, credential values removed"}),
            vec!["invalid", "forbidden", "not_found", "unavailable"],
        ),
        (
            "browser.session_stop",
            json!({"required":["session_id"],"properties":{"session_id":s("256")}}),
            json!({"type":"object"}),
            vec!["invalid", "not_found", "unavailable"],
        ),
        (
            "browser.auth_assist_complete",
            json!({"required":["session_id"],"properties":{"session_id":s("256")}}),
            json!({"type":"object"}),
            vec!["invalid", "not_found", "unavailable"],
        ),
        (
            "browser.capture",
            json!({"required":["urls"],"properties":{"urls":{"type":"array","minItems":1,"maxItems":browser::MAX_CAPTURE_URLS,"items":{"format":"http(s) absolute URL"}},"timeout_ms":{"type":"integer","minimum":1000,"maximum":120000}}}),
            json!({"captures":[{"url":"string","final_url":"string","status":"integer","mime":"string","blob":{"ref":"sha256"},"size":"integer"}]}),
            vec!["invalid", "quota", "unavailable"],
        ),
        (
            "net.ingest.webhook.register",
            json!({"required":["path","command"],"properties":{"path":{"pattern":ingest::PATH_PATTERN},"command":{"type":"string","description":"handler command type in the app's active manifest"},"default_response":{"properties":{"status":{"type":"integer"},"headers":{"type":"object"},"body":s("65536"),"body_ref":{"properties":{"ref":{"description":"files.ref of the app, <= 64 KiB; delivered as bytes"}}}}},"pass_headers":{"type":"array","maxItems":16},"aliases":{"type":"array","items":{"enum":ingest::LEGACY_ALIASES.iter().map(|(p,_)|*p).collect::<Vec<_>>()}}}}),
            json!({"ingest_url":"absolute URL","path":"string","command_type":"string"}),
            vec!["invalid", "forbidden", "conflict", "unavailable"],
        ),
        (
            "net.ingest.subscribe",
            json!({"required":["endpoint","protocol"],"properties":{"endpoint":s("180"),"protocol":s("64"),"enabled":{"type":"boolean"},"config":{"type":"object","maxBytes":16384,"description":"no credential values; *_ref names only"}}}),
            json!({"endpoint":"string","protocol":"string","enabled":"boolean"}),
            vec!["invalid", "unavailable"],
        ),
        (
            "agent.coding.models",
            json!({"required":["module_id"],"properties":{"module_id":s("128")}}),
            json!({"type":"object"}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "agent.coding.turn",
            json!({"required":["module_id"],"properties":{"module_id":s("128"),"prompt":s("65536"),"faux":{"type":"boolean"},"preset_id":s("128")},"not":{"required":["model"]}}),
            json!({"type":"object","description":"pi-sidecar turn summary"}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "files.put",
            json!({"required":["name"],"oneOf":[{"required":["text"]},{"required":["base64"]}],"properties":{"name":s("255"),"mime":s("128"),"text":{"type":"string"},"base64":{"type":"string","description":"input only"}},"maxBytes":files::MAX_PUT_BYTES}),
            json!({"id":"string","ref":"sha256"}),
            vec!["invalid", "quota", "unavailable", "unavailable(uncertain)"],
        ),
        (
            "files.get",
            json!({"oneOf":[{"required":["id"]},{"required":["ref"]}]}),
            json!({"id":"string","ref":"sha256","name":"string","mime":"string","size":"integer","owner_app":"string","created_at_ms":"integer","text":"string, only UTF-8 <= 512 KiB; never base64"}),
            vec!["invalid", "not_found", "unavailable"],
        ),
        (
            "files.list",
            json!({"properties":{"limit":{"type":"integer","minimum":1,"maximum":1000}}}),
            json!({"files":["files.get without text"]}),
            vec!["invalid", "unavailable"],
        ),
        (
            "files.delete",
            json!({"required":["id"]}),
            json!({"id":"string","deleted":"boolean"}),
            vec!["invalid", "unavailable"],
        ),
        (
            "browser.automation.run",
            json!({"required":["steps"],"properties":{"session_id":s("256"),"steps":{"type":"array","minItems":1,"maxItems":browser::MAX_AUTOMATION_STEPS,"items":{"required":["op"],"properties":{"op":{"enum":browser::ALLOWED_OPS},"tab_id":s("256"),"params":{"type":"object"}}}},"timeout_ms":{"type":"integer","minimum":1000,"maximum":300000}},"description":"every step validated before the first runs; per-step §16.46 gate and audit"}),
            json!({"session_id":"string","steps":[{"op":"string","ok":"boolean"}],"completed":"integer","timed_out":"boolean","artifact":{"ref":"sha256","file_id":"string"}}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "process.run",
            json!({"required":["program"],"properties":{"program":{"description":"name in platform allowlist business_os.process_allowlist"},"args":{"type":"array","maxItems":128},"cwd_ref":{"description":"relative path inside the app workspace"},"stdin":{"type":"string"},"stdin_ref":{"properties":{"ref":{"description":"files.ref of the calling app"}}},"program_note":{"description":"allowlist entry: program or interpreter+script, plus platform base env (PATH, CTOX_BIN, LANG)"},"env":{"type":"object","maxProperties":32,"description":"the complete child environment; nothing inherited"},"timeout_ms":{"type":"integer","minimum":1,"maximum":process::MAX_TIMEOUT_MS}}}),
            json!({"exit":"integer|null","timed_out":"boolean","duration_ms":"integer","stdout":"string","stdout_truncated":"boolean","stderr":"string","stderr_truncated":"boolean"}),
            vec!["invalid", "forbidden", "not_found", "unavailable"],
        ),
        (
            "browser.auth_assist.request",
            json!({"required":["source_id"],"properties":{"source_id":s("256"),"target_url_override":{"format":"http(s) URL"},"credential_ref":{"pattern":"^ctox-secret://credentials/"},"login_hint":s("256"),"preferred_auth_assist":{"type":"object"},"requesting_task_id":s("256"),"source_module":{"description":"must equal the calling app"},"command_path":s("256")}}),
            json!({"command_id":"string","status":"string","session_id":"string","tab_id":"string","target_url":"string","credential_ref":"string|null"}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "directory.user",
            json!({"required":["user_id"]}),
            json!({"found":"boolean","user_id":"string","display_name":"string","active":"boolean"}),
            vec!["invalid", "unavailable"],
        ),
        (
            "identity.users.list",
            json!({"properties":{"active":{"type":"boolean"},"roles":{"type":"array"},"order":{"items":{"enum":["created_at_ms:asc","created_at_ms:desc","user_id:asc","user_id:desc"]}}}}),
            json!({"users":[{"user_id":"string","display_name":"string","role":"string","active":"boolean"}]}),
            vec!["invalid", "unavailable"],
        ),
        (
            "policy.module_decision",
            json!({"required":["permission","module"],"properties":{"permission":{"enum":["data.read","data.write","apps.modify"]}}}),
            json!({"allowed":"boolean","display_reason":"string","reason_code":"string"}),
            vec!["invalid"],
        ),
        (
            "command.get",
            json!({"required":["command_id"]}),
            json!({"found":"boolean","command_id":"string","command_type":"string","module":"string","record_id":"string|null","status":"string","payload":"object"}),
            vec!["invalid", "unavailable"],
        ),
        (
            "mail.server.user.save",
            json!({"required":["command_id","username","password"],"properties":{"command_id":s("256"),"username":s("320"),"owner_user_id":s("256"),"password":{"const":{"secret_intake":true}},"communication_account":{"type":"object"}}}),
            json!({"type":"object","description":"native outcome, no secret fields"}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "mail.server.domain.save",
            json!({"required":["domain_name"],"properties":{"dkim_private_key":{"const":null,"description":"Core generates the key"}}}),
            json!({"type":"object","description":"native outcome, no secret fields"}),
            vec!["invalid", "forbidden", "unavailable"],
        ),
        (
            "mail.server.config.get",
            json!({"type":"object","maxProperties":0}),
            json!({"type":"object","description":"domains/users/runtime without password, secret or private key fields (§16.28)"}),
            vec!["forbidden", "unavailable"],
        ),
    ];
    Value::Array(
        entries
            .into_iter()
            .map(|(path, args, result, rejections)| {
                json!({"path":path,"args":args,"result":result,"rejections":rejections})
            })
            .collect(),
    )
}
