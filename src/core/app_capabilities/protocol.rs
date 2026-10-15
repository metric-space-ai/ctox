use super::policy::CapCallContext;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;

pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Invalid,
    Forbidden,
    NotFound,
    Conflict,
    Revoked,
    Expired,
    Quota,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CtoxError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Effect may have happened; the runtime must request reconciliation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub uncertain: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}
impl CtoxError {
    pub fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
            uncertain: false,
        }
    }
    pub(crate) fn invalid(path: &str) -> Self {
        Self {
            path: Some(path.into()),
            ..Self::new(ErrorCode::Invalid, "invalid capability request")
        }
    }
    pub(crate) fn uncertain() -> Self {
        Self {
            uncertain: true,
            ..Self::new(
                ErrorCode::Unavailable,
                "capability outcome requires reconciliation",
            )
        }
    }
}
impl fmt::Display for CtoxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for CtoxError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum CapMessage {
    #[serde(rename = "cap.invoke")]
    Invoke {
        request_id: String,
        ctx: CapCallContext,
        name: String,
        #[serde(rename = "fn")]
        function: String,
        args: Value,
    },
    #[serde(rename = "cap.result")]
    Result {
        request_id: String,
        outcome: CapOutcome,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CapOutcome {
    Ok { value: Value },
    Error { error: CtoxError },
}

impl CapMessage {
    pub fn decode(bytes: &[u8]) -> Result<Self, CtoxError> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(CtoxError::new(
                ErrorCode::Quota,
                "capability frame exceeds 1 MiB",
            ));
        }
        let message: Self = serde_json::from_slice(bytes).map_err(|_| CtoxError::invalid("$"))?;
        match &message {
            Self::Invoke {
                request_id,
                ctx,
                name,
                function,
                ..
            } => {
                identifier(request_id, "$.request_id")?;
                if request_id != &ctx.call_id {
                    return Err(CtoxError::invalid("$.ctx.call_id"));
                }
                identifier(name, "$.name")?;
                identifier(function, "$.fn")?;
                validate_context(ctx)?;
            }
            Self::Result { request_id, .. } => identifier(request_id, "$.request_id")?,
        }
        Ok(message)
    }
    pub fn encode(&self) -> Result<Vec<u8>, CtoxError> {
        let bytes = serde_json::to_vec(self).map_err(|_| CtoxError::invalid("$"))?;
        // One validator for outgoing and incoming frames.
        Self::decode(&bytes)?;
        Ok(bytes)
    }
}

pub(crate) fn identifier(value: &str, path: &str) -> Result<(), CtoxError> {
    if value.is_empty()
        || value.len() > 256
        || value.chars().any(char::is_control)
        || value.trim() != value
    {
        return Err(CtoxError::invalid(path));
    }
    Ok(())
}
pub(crate) fn validate_context(ctx: &CapCallContext) -> Result<(), CtoxError> {
    for (path, value) in [
        ("$.ctx.app_id", &ctx.app_id),
        ("$.ctx.app_ver", &ctx.app_ver),
        ("$.ctx.actor.id", &ctx.actor.id),
        ("$.ctx.actor.role", &ctx.actor.role),
        ("$.ctx.command_id", &ctx.command_id),
        ("$.ctx.call_id", &ctx.call_id),
        ("$.ctx.fence.run_id", &ctx.fence.run_id),
        ("$.ctx.fence.participant", &ctx.fence.participant),
    ] {
        identifier(value, path)?;
    }
    if ctx.fence.fence == 0 || ctx.fence.expires_at_core_ms == 0 {
        return Err(CtoxError::invalid("$.ctx.fence"));
    }
    Ok(())
}

/// JSON schema for exactly the Rust wire types. LocalTransport supplies framing;
/// the receiver also calls decode (byte bound and request/call binding).
pub fn json_schema() -> Value {
    let id =
        json!({"type":"string","minLength":1,"maxLength":256,"pattern":"^\\S(?:[^\\r\\n]*\\S)?$"});
    let actor = json!({"type":"object","additionalProperties":false,"required":["id","role"],"properties":{"id":id,"role":id}});
    let fence = json!({"type":"object","additionalProperties":false,"required":["run_id","participant","fence","expires_at_core_ms"],
        "properties":{"run_id":id,"participant":id,"fence":{"type":"integer","minimum":1},"expires_at_core_ms":{"type":"integer","minimum":1}}});
    let ctx = json!({"type":"object","additionalProperties":false,"required":["app_id","app_ver","actor","command_id","call_id","fence"],
        "properties":{"app_id":id,"app_ver":id,"actor":actor,"command_id":id,"call_id":id,"fence":fence}});
    let error = json!({"type":"object","additionalProperties":false,"required":["code","message"],
        "properties":{"code":{"enum":["invalid","forbidden","not_found","conflict","revoked","expired","quota","unavailable"]},
        "message":{"type":"string"},"path":{"type":"string"},"uncertain":{"type":"boolean"}}});
    json!({"$schema":"https://json-schema.org/draft/2020-12/schema","oneOf":[
        {"type":"object","additionalProperties":false,"required":["type","request_id","ctx","name","fn","args"],
            "properties":{"type":{"const":"cap.invoke"},"request_id":id,"ctx":ctx,"name":id,"fn":id,"args":{}}},
        {"type":"object","additionalProperties":false,"required":["type","request_id","outcome"],
            "properties":{"type":{"const":"cap.result"},"request_id":id,"outcome":{"oneOf":[
                {"type":"object","additionalProperties":false,"required":["status","value"],"properties":{"status":{"const":"ok"},"value":{}}},
                {"type":"object","additionalProperties":false,"required":["status","error"],"properties":{"status":{"const":"error"},"error":error}}
            ]}}}
    ]})
}
