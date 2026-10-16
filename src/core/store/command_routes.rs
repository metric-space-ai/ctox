//! E25: closed native method registry, shared by the P9 executor and its adapter.
//! This is not a second request protocol. Inputs are committed V4 command
//! documents; completion is written back through the existing fenced writer.
use super::CommandError;
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const CONTROL_LANE: u8 = 1;
pub const INLINE_RESULT_LIMIT: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum RouteBackend {
    Native,
    Core,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Route {
    pub method: &'static str,
    pub backend: RouteBackend,
    pub handler: &'static str,
    pub streaming: bool,
    pub lane: u8,
}
macro_rules! route {
    ($method:literal, $handler:literal) => {
        route!($method, $handler, false)
    };
    ($method:literal, $handler:literal, $streaming:literal) => {
        Route {
            method: $method,
            backend: RouteBackend::Native,
            handler: $handler,
            streaming: $streaming,
            lane: CONTROL_LANE,
        }
    };
}
macro_rules! core_route {
    ($method:literal,$handler:literal) => {
        Route {
            backend: RouteBackend::Core,
            ..route!($method, $handler)
        }
    };
}
/// Exact method names, not the action discriminants inside their native payloads.
pub const ROUTES: &[Route] = &[
    core_route!("ctox.secret.request.list", "src/core/business_os/app_platform/platform_routes.rs"),
    core_route!("ctox.secret.request.fulfill", "src/core/business_os/app_platform/platform_routes.rs"),
    core_route!("ctox.secret.request.decline", "src/core/business_os/app_platform/platform_routes.rs"),
    core_route!("ctox.workjet.remote_worker.admit", "src/core/business_os/app_platform/platform_routes.rs"),
    core_route!("ctox.maintenance.restart", "src/core/business_os/app_platform/platform_routes.rs"),
    core_route!("instance.providers.models.check", "src/core/business_os/app_platform/provider_model_check.rs"),

    route!(
        "ctox.workjet.speech.settings.v1",
        "src/core/business_os/rxdb_peer_speech_settings.rs:372"
    ),
    route!(
        "ctox.workjet.speech.dictation.v1",
        "src/core/business_os/rxdb_peer_dictation.rs:348",
        true
    ),
    route!(
        "ctox.workjet.jour_fixe.speech.v1",
        "src/core/business_os/rxdb_peer_jour_fixe_speech.rs:260",
        true
    ),
    route!(
        "ctox.workjet.device.v1",
        "src/core/business_os/rxdb_peer_workjet_devices.rs:114"
    ),
    route!(
        "ctox.workjet.grok.v1",
        "src/core/business_os/rxdb_peer_grok.rs:390"
    ),
    route!(
        "ctox.workjet.calendar.read.v1",
        "src/core/business_os/mcp_workjet_calendar.rs:27"
    ),
    route!(
        "ctox.browser.live.v1",
        "src/core/business_os/rxdb_peer_browser.rs:188"
    ),
    route!(
        "ctox.outbound.sellify_lookup.v1",
        "src/core/business_os/store_outbound_commands.rs:1673"
    ),
    core_route!(
        "ctox.plan.step.retry",
        "src/core/business_os/command_plane.rs:1734"
    ),
    route!(
        "ctox.workjet.calendar.connect.v1",
        "src/core/business_os/mcp_workjet_calendar_connect.rs:1"
    ),
    core_route!(
        "ctox.workjet.computer.assign",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.computer.endpoint.disable",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.computer.endpoint.upsert",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.computer.list",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.computer.ssh_key.ensure",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.computer.unassign",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.exit_model.read",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.exit_model.refresh",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.exit_model.submit",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.jour_fixe.comment.add",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.jour_fixe.meeting.end",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.jour_fixe.meeting.read",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.jour_fixe.meeting.start",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.jour_fixe.todos.confirm",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.jour_fixe.todos.revise",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.jour_fixe.transcript.append",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.presentation.canvas.save",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.presentation.read",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.chat.create",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.init",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.kpis.configure",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.kpis.read",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.kpis.refresh",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.list",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.supervisor.bind",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.supervisor.route.capabilities.v1",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.supervisor.route.capabilities.v2",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.supervisor.route.read.v1",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.supervisor.route.read.v2",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.supervisor.turn.cancel",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.supervisor.turn.capabilities",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.supervisor.turn.input",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.supervisor.turn.submit",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.supervisor.turn.watch",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.upsert",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.project.worker.add",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.providers.account.enable",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.providers.account.remove",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.providers.adopt_native",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.providers.list",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.providers.models.exclude",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.providers.models.select",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.providers.observe_native",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.session.create",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.session.list",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.session.transfer.abort",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.session.transfer.pause_ack",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.session.transfer.start",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.session.transfer.status",
        "src/core/business_os/command_plane.rs:1734"
    ),
    core_route!(
        "ctox.workjet.working_copy.upsert",
        "src/core/business_os/command_plane.rs:1734"
    ),
];
pub fn owns_type(method: &str) -> bool {
    method.starts_with("ctox.workjet.") || method == "ctox.plan.step.retry"
}
pub fn timeout_ms(command: &NativeCommand) -> u64 {
    match command.kind.as_str() {
        "ctox.workjet.speech.dictation.v1" | "ctox.workjet.jour_fixe.speech.v1" => {
            if command.payload["op"] == "open" {
                20_000
            } else {
                5_000
            }
        }
        "ctox.workjet.speech.settings.v1" => {
            if command.payload["action"] == "speech.settings.check.transcription" {
                30_000
            } else {
                25_000
            }
        }
        "ctox.workjet.grok.v1" => 24_000,
        "ctox.workjet.device.v1" => 10_000,
        _ => 10_000,
    }
}
pub fn is_command_bus(route: Route) -> bool {
    route.backend == RouteBackend::Core
}
pub fn resolve(method: &str) -> Result<Route, CommandError> {
    ROUTES
        .iter()
        .find(|r| r.method == method)
        .copied()
        .ok_or_else(|| error("not_found", "unknown_command"))
}
/// Before the ordering buffer, document/history or rejection ledger can see
/// an E25 input. Credential bytes belong only to the official secret provider.
pub fn validate_persisted_input(document: &Value) -> Result<(), CommandError> {
    // E49.2 fulfillment carries only an identifier. Values are supplied to the
    // protected SecretStore intake, before any command/outbox write.
    if document["type"] == "ctox.secret.request.fulfill" {
        let valid = document["payload"].as_object().is_some_and(|p|
            p.len()==1 && p.get("request_id").and_then(Value::as_str)
                .is_some_and(|id|!id.is_empty() && id.len()<=180 && !id.chars().any(char::is_control)));
        if !valid { return Err(error("invalid", "credential_ref_required")); }
    }

    if contains_credential(&document["payload"])
        || (document["type"] == "ctox.workjet.calendar.connect.v1"
            && document["payload"].get("callbackUrl").is_some())
    {
        return Err(error("invalid", "credential_ref_required"));
    }
    Ok(())
}
fn contains_credential(v: &Value) -> bool {
    match v {
        Value::Object(o) => o.iter().any(|(k, v)| {
            matches!(
                k.as_str(),
                "secret"
                    | "password"
                    | "access_token"
                    | "refresh_token"
                    | "api_key"
                    | "apiKey"
                    | "capability_token"
            ) && !v.is_null()
                || contains_credential(v)
        }),
        Value::Array(a) => a.iter().any(contains_credential),
        _ => false,
    }
}
/// Preserve already typed native errors; free-text legacy errors are mapped
/// only through fixed public classes. Provider bodies are never reflected.
pub fn native_error(message: &str) -> CommandError {
    if let Ok(typed) = serde_json::from_str::<CommandError>(message) {
        if [
            "invalid",
            "forbidden",
            "not_found",
            "conflict",
            "revoked",
            "expired",
            "quota",
            "unavailable",
            "stale_base",
            "migration_required",
            "locally_modified",
            "token_expired",
            "token_used",
        ]
        .contains(&typed.code.as_str())
            && ["rejected", "failed"].contains(&typed.status.as_str())
            && !typed.reason.is_empty()
            && typed.reason.len() <= 128
            && typed
                .reason
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.:-".contains(&b))
        {
            return typed;
        }
    }
    match message {
        "invalid_request" | "invalid speech request" | "dictation invalid_request" => {
            error("invalid", "native_payload_invalid")
        }
        "denied" | "forbidden" | "workjet device management is not allowed" => {
            error("forbidden", "native_permission_denied")
        }
        "retired" | "revoked" => error("revoked", "native_authority_changed"),
        _ => error("unavailable", "native_handler_failed"),
    }
}
pub fn error(code: &str, reason: &str) -> CommandError {
    CommandError {
        status: "rejected".into(),
        code: code.into(),
        reason: reason.into(),
    }
}
/// The executor loads this from its admitted, committed command document. It
/// must bind actor/app to current policy and hold its actual execution fence.
/// No peer, token, role, capability, root or provider can be supplied in payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeCommand {
    pub command_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub payload: Value,
    pub app_id: String,
    pub actor_id: String,
}
impl NativeCommand {
    pub fn validate(&self) -> Result<Route, CommandError> {
        let route = resolve(&self.kind)?;
        validate_persisted_input(&json!({"type":self.kind,"payload":self.payload}))?;
        if self.kind == "ctox.workjet.device.v1" && self.payload["action"] == "invite.create" {
            return Err(error(
                "unavailable",
                "protected_invite_result_adapter_required",
            ));
        }
        if self.kind == "ctox.workjet.speech.settings.v1"
            && self.payload["action"] == "speech.settings.key"
        {
            return Err(error("unavailable", "credential_provider_adapter_required"));
        }
        if [&self.command_id, &self.app_id, &self.actor_id]
            .iter()
            .any(|s| s.is_empty() || s.len() > 180 || s.chars().any(char::is_control))
            || !self.payload.is_object()
        {
            return Err(error("invalid", "payload_invalid:command"));
        }
        Ok(route)
    }
}
pub use crate::cas::BlobRef as FileRef;
/// Core's files/Transfer-CAS adapter. File metadata and command completion must
/// be applied in the SAME existing writer transaction. CAS bytes may already be
/// staged, but no caller file path or transfer_id may appear in the completion.
pub trait ResultFiles: Send + Sync {
    fn put_json(
        &self,
        tx: &Transaction<'_>,
        command: &NativeCommand,
        bytes: &[u8],
    ) -> Result<FileRef, CommandError>;
}
/// Verified CAS receipt plus the schema owner's normal, policy-checked files
/// insertion. Construct outside the writer; put_json performs metadata only.
pub struct VerifiedResultFiles {
    pub blob: crate::cas::VerifiedBlob,
    pub insert: std::sync::Arc<
        dyn Fn(&Transaction<'_>, &crate::cas::FileRecord) -> Result<(), CommandError> + Send + Sync,
    >,
}
impl ResultFiles for VerifiedResultFiles {
    fn put_json(
        &self,
        tx: &Transaction<'_>,
        command: &NativeCommand,
        bytes: &[u8],
    ) -> Result<FileRef, CommandError> {
        let digest = format!("{:x}", Sha256::digest(bytes));
        if self.blob.reference() != digest || self.blob.size() != bytes.len() as u64 {
            return Err(error("invalid", "files_ref_digest"));
        }
        let reference = FileRef { r#ref: digest };
        let record = crate::cas::FileRecord {
            blob: reference.clone(),
            name: format!("{}-result.json", command.command_id),
            mime: "application/json".into(),
            size: self.blob.size(),
            owner_app: command.app_id.clone(),
        };
        (self.insert)(tx, &record)?;
        Ok(reference)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub command_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CommandError>,
}
impl Completion {
    pub fn rejected(command_id: &str, error: CommandError) -> Self {
        Self {
            command_id: command_id.into(),
            status: "failed".into(),
            result: None,
            error: Some(error),
        }
    }
}
/// Pure preparation before CAS staging. Never expose protected native DTOs
/// to files; the credential/result adapter must resolve them first.
pub fn prepare_completion(
    command: &NativeCommand,
    native: Value,
) -> Result<Completion, CommandError> {
    let route = command.validate()?;
    if contains_credential(&native)
        || native.get("invite").is_some()
        || native.get("pairingUri").is_some()
        || native.get("qrSvg").is_some()
        || native
            .pointer("/login/userCode")
            .is_some_and(|v| !v.is_null())
    {
        return Err(error("unavailable", "protected_result_adapter_required"));
    }
    // Only the opening command owns the ongoing stream. Chunk/read/finish/
    // cancel commands acknowledge their own operation; their snapshots must
    // not leave a separate permanently running command for every audio chunk.
    let owns_stream = route.streaming
        && matches!(
            command.payload.get("op").and_then(Value::as_str),
            None | Some("open")
        );
    let (status, value) = if owns_stream {
        match native["state"].as_str() {
            Some("finished" | "committed") => ("done", native),
            Some("canceled" | "cancelled") => ("cancelled", Value::Null),
            Some("failed") => {
                return Ok(Completion::rejected(
                    &command.command_id,
                    error("unavailable", "native_stream_failed"),
                ))
            }
            Some("open" | "finishing" | "running") => ("running", json!({"partial":native})),
            _ => return Err(error("invalid", "native_stream_state")),
        }
    } else {
        ("done", native)
    };
    Ok(Completion {
        command_id: command.command_id.clone(),
        status: status.into(),
        result: Some(value),
        error: None,
    })
}
fn file_payload<'a>(status: &str, value: &'a Value) -> &'a Value {
    if status == "running" {
        value.get("partial").unwrap_or(value)
    } else {
        value
    }
}
/// Stage these exact JSON DTO bytes in Transfer-CAS OUTSIDE the writer. The
/// returned ResultFiles adapter only attaches the verified files row inside it.
pub fn result_file_bytes(
    command: &NativeCommand,
    native: Value,
) -> Result<Option<Vec<u8>>, CommandError> {
    let prepared = prepare_completion(command, native)?;
    let Some(value) = prepared.result else {
        return Ok(None);
    };
    let bytes = serde_json::to_vec(file_payload(&prepared.status, &value))
        .map_err(|_| error("invalid", "native_result_encoding"))?;
    Ok((bytes.len() > INLINE_RESULT_LIMIT || contains_inline_audio(&value)).then_some(bytes))
}
/// Run under the retained native publication guard AND the P9 writer fence.
/// A missing files sink never permits an inline fallback for a large result.
pub fn completion(
    tx: &Transaction<'_>,
    command: &NativeCommand,
    native: Value,
    files: Option<&dyn ResultFiles>,
) -> Result<Completion, CommandError> {
    let mut prepared = prepare_completion(command, native)?;
    let Some(value) = prepared.result.take() else {
        return Ok(prepared);
    };
    let bytes = serde_json::to_vec(file_payload(&prepared.status, &value))
        .map_err(|_| error("invalid", "native_result_encoding"))?;
    let value = if bytes.len() > INLINE_RESULT_LIMIT || contains_inline_audio(&value) {
        let sink = files.ok_or_else(|| error("unavailable", "files_sink_missing"))?;
        let reference = sink.put_json(tx, command, &bytes)?;
        let expected = format!("{:x}", Sha256::digest(&bytes));
        if reference.r#ref != expected {
            return Err(error("invalid", "files_ref_digest"));
        }
        let reference =
            serde_json::to_value(reference).map_err(|_| error("invalid", "files_ref_encoding"))?;
        if prepared.status == "running" {
            json!({"partial":reference})
        } else {
            reference
        }
    } else {
        value
    };
    prepared.result = Some(value);
    Ok(prepared)
}
fn contains_inline_audio(v: &Value) -> bool {
    match v {
        Value::Object(o) => o.iter().any(|(k, v)| {
            matches!(
                k.as_str(),
                "audio" | "audioBase64" | "audio_base64" | "pcmBase64" | "pcm_base64"
            ) && !v.is_null()
                || contains_inline_audio(v)
        }),
        Value::Array(a) => a.iter().any(contains_inline_audio),
        _ => false,
    }
}
#[cfg(test)]
#[path = "command_routes_tests.rs"]
mod tests;

/// Derive terminal replay equality from the ONE canonical writer snapshot.
/// This does not grant a new effect or publication. A producer/finish race may
/// observe an already committed identical opening result after its lease was
/// retired; that existing terminal update is a no-op, not a second write.
pub fn is_canonical_completion(
    tx: &Transaction<'_>,
    command: &NativeCommand,
    native: Value,
) -> rusqlite::Result<bool> {
    let registered: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='_collections')",
        [],
        |r| r.get(0),
    )?;
    if !registered {
        return Ok(false);
    }
    let Some(doc) = super::command::command_document(tx, &command.command_id)? else {
        return Ok(false);
    };
    let Ok(completion) = prepare_completion(command, native.clone()) else {
        return Ok(false);
    };
    if !["done", "failed", "cancelled"].contains(&completion.status.as_str()) {
        return Ok(false);
    }
    let mut result = completion.result;
    if let Ok(Some(bytes)) = result_file_bytes(command, native) {
        result = Some(json!({"ref":format!("{:x}",Sha256::digest(bytes))}));
    }
    Ok(doc["id"] == command.command_id
        && doc["type"] == command.kind
        && doc["actor_id"] == command.actor_id
        && doc["app_id"] == command.app_id
        && doc["payload"] == command.payload
        && doc["status"] == completion.status
        && doc.get("result") == result.as_ref()
        && doc.get("error")
            == completion
                .error
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                .as_ref())
}
