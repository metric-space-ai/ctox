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
        Route {
            method: $method,
            handler: $handler,
            streaming: false,
            lane: CONTROL_LANE,
        }
    };
}
/// Exact method names, not the action discriminants inside their native payloads.
pub const ROUTES: &[Route] = &[
    route!(
        "ctox.workjet.device.v1",
        "src/core/business_os/rxdb_peer_workjet_devices.rs:122"
    ),
    route!(
        "ctox.workjet.grok.v1",
        "src/core/business_os/rxdb_peer_grok.rs:390"
    ),
    route!(
        "ctox.workjet.calendar.read.v1",
        "src/core/business_os/rxdb_peer.rs:3168"
    ),
    route!(
        "ctox.browser.live.v1",
        "src/core/business_os/rxdb_peer.rs:3105"
    ),
    route!(
        "ctox.outbound.sellify_lookup.v1",
        "src/core/business_os/rxdb_peer.rs:3122"
    ),
];
pub fn resolve(method: &str) -> Result<Route, CommandError> {
    ROUTES
        .iter()
        .find(|r| r.method == method)
        .copied()
        .ok_or_else(|| error("not_found", "unknown_command"))
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRef {
    #[serde(rename = "ref")]
    pub reference: String,
}
/// Core's files/Transfer-CAS adapter. File metadata and command completion must
/// be applied in the SAME existing writer transaction. CAS bytes may already be
/// staged, but no caller file path or transfer_id may appear in the completion.
pub trait ResultFiles {
    fn put_json(
        &self,
        tx: &Transaction<'_>,
        command: &NativeCommand,
        bytes: &[u8],
    ) -> Result<FileRef, CommandError>;
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
/// Run under the retained native publication guard AND the P9 writer fence.
/// A missing files sink never permits an inline fallback for a large result.
pub fn completion(
    tx: &Transaction<'_>,
    command: &NativeCommand,
    native: Value,
    files: Option<&dyn ResultFiles>,
) -> Result<Completion, CommandError> {
    let route = command.validate()?;
    let (status, value) = if route.streaming {
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
    let bytes =
        serde_json::to_vec(&value).map_err(|_| error("invalid", "native_result_encoding"))?;
    let audio = contains_inline_audio(&value);
    let value = if bytes.len() > INLINE_RESULT_LIMIT || audio {
        let sink = files.ok_or_else(|| error("unavailable", "files_sink_missing"))?;
        let reference = sink.put_json(tx, command, &bytes)?;
        let expected = format!("{:x}", Sha256::digest(&bytes));
        if reference.reference != expected {
            return Err(error("invalid", "files_ref_digest"));
        }
        serde_json::to_value(reference).map_err(|_| error("invalid", "files_ref_encoding"))?
    } else {
        value
    };
    Ok(Completion {
        command_id: command.command_id.clone(),
        status: status.into(),
        result: Some(value),
        error: None,
    })
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
