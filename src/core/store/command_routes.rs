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
pub struct Route {
    pub method: &'static str,
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
            handler: $handler,
            streaming: $streaming,
            lane: CONTROL_LANE,
        }
    };
}
/// Exact method names, not the action discriminants inside their native payloads.
pub const ROUTES: &[Route] = &[
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
];
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
    if contains_credential(&document["payload"]) {
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
    let owns_stream=route.streaming && matches!(command.payload.get("op").and_then(Value::as_str),None|Some("open"));
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
fn file_payload<'a>(status:&str,value:&'a Value)->&'a Value {
    if status=="running" { value.get("partial").unwrap_or(value) } else { value }
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
    let bytes =
        serde_json::to_vec(file_payload(&prepared.status,&value)).map_err(|_| error("invalid", "native_result_encoding"))?;
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
    let bytes =
        serde_json::to_vec(file_payload(&prepared.status,&value)).map_err(|_| error("invalid", "native_result_encoding"))?;
    let value = if bytes.len() > INLINE_RESULT_LIMIT || contains_inline_audio(&value) {
        let sink = files.ok_or_else(|| error("unavailable", "files_sink_missing"))?;
        let reference = sink.put_json(tx, command, &bytes)?;
        let expected = format!("{:x}", Sha256::digest(&bytes));
        if reference.r#ref != expected {
            return Err(error("invalid", "files_ref_digest"));
        }
        let reference=serde_json::to_value(reference).map_err(|_| error("invalid", "files_ref_encoding"))?;
        if prepared.status=="running" { json!({"partial":reference}) } else { reference }
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
