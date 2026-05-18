//! Line-delimited JSON IPC wire types — the contract between CTOX
//! (client) and this crate's server binary over a Unix domain socket.
//!
//! # Framing
//!
//! Each request is a single line of JSON (UTF-8) terminated by `\n`.
//! Each response frame is likewise a single line of JSON terminated by
//! `\n`. For non-streaming requests the server emits one response
//! frame and closes. For streaming requests the server emits many
//! frames (OpenAI Responses API stream events) and closes after
//! `response.completed` (or on error).
//!
//! # Transport
//!
//! Unix domain socket only. On Linux the server performs a
//! `SO_PEERCRED` check at accept time and rejects connections whose
//! peer UID does not match the server UID. No TCP, no HTTP, no TLS —
//! IPC between processes owned by the same user.
//!
//! # Request shape
//!
//! Matches `src/harness/core/src/client.rs::LocalIpcRequest` on the
//! CTOX side exactly. Any drift breaks the gateway.
//!
//! ```text
//! {"kind":"responses_create", "model":"…", "instructions":"…", "input":[…], …}
//! {"kind":"runtime_health"}
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── Request envelope ───────────────────────────────────────────

/// Top-level IPC request — tagged by `kind`. Wire-compatible with
/// the CTOX-side `LocalIpcRequest` enum in
/// `src/harness/core/src/client.rs`.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalIpcRequest {
    /// OpenAI Responses API create call.
    ResponsesCreate(ResponsesCreateRequest),
    /// Health/liveness probe.
    RuntimeHealth,
}

/// Responses API create request body. Field names + types match
/// `src/harness/core/src/client.rs::LocalIpcResponsesRequest`
/// exactly; `serde_json::Value` stands in where CTOX uses
/// OpenAI-schema types we don't need to parse structurally
/// server-side (input items, tool schemas, reasoning, text
/// controls).
#[derive(Debug, Deserialize)]
pub struct ResponsesCreateRequest {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub input: Vec<Value>,
    #[serde(default)]
    pub tools: Vec<Value>,
    #[serde(default)]
    pub tool_choice: String,
    #[serde(default)]
    pub parallel_tool_calls: bool,
    #[serde(default)]
    pub reasoning: Option<Value>,
    #[serde(default)]
    pub max_output_tokens: Option<usize>,
    #[serde(default)]
    pub store: bool,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub prompt_cache_key: Option<String>,
    #[serde(default)]
    pub text: Option<Value>,
}

// ─── Response envelope (runtime_health) ─────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalIpcResponse {
    RuntimeHealth(RuntimeHealth),
    Error(IpcError),
}

#[derive(Debug, Serialize)]
pub struct RuntimeHealth {
    pub healthy: bool,
    pub default_model: Option<String>,
    pub loaded_models: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

// ─── OpenAI Responses API stream events ─────────────────────────
//
// Emitted one-per-line from the server to the client during a
// streaming ResponsesCreate. Field names are OpenAI-stable — they
// match what the CTOX client deserializes as `ResponsesStreamEvent`
// in the harness crate.

/// Responses stream event. Serialized with `{"type": "...", ...}`
/// — OpenAI wire convention.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ResponsesStreamEvent {
    #[serde(rename = "response.created")]
    Created {
        response: ResponseEnvelope,
        sequence_number: u64,
    },
    #[serde(rename = "response.in_progress")]
    InProgress {
        response: ResponseEnvelope,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        output_index: u32,
        item: ResponseOutputItem,
        sequence_number: u64,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: ResponseContentPart,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        item_id: String,
        output_index: u32,
        content_index: u32,
        delta: String,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
        text: String,
        sequence_number: u64,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: ResponseContentPart,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        output_index: u32,
        item: ResponseOutputItem,
        sequence_number: u64,
    },
    #[serde(rename = "response.completed")]
    Completed {
        response: ResponseEnvelope,
        sequence_number: u64,
    },
    #[serde(rename = "response.failed")]
    Failed {
        response: ResponseEnvelope,
        sequence_number: u64,
    },
}

/// The `response` field inside stream events (and the final
/// non-streaming response body).
#[derive(Debug, Serialize, Clone)]
pub struct ResponseEnvelope {
    pub id: String,
    pub object: &'static str, // always "response"
    pub created_at: i64,
    pub status: ResponseStatus,
    pub model: String,
    pub output: Vec<ResponseOutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponseUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcError>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    InProgress,
    Completed,
    Failed,
}

/// One item in `response.output` — for chat, this is always a
/// single "message" item with an assistant role and one or more
/// content parts.
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseOutputItem {
    Message {
        id: String,
        status: ResponseStatus,
        role: &'static str, // always "assistant"
        content: Vec<ResponseContentPart>,
    },
}

/// A single content part within a message item.
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseContentPart {
    OutputText {
        text: String,
        annotations: Vec<Value>,
    },
}

#[derive(Debug, Serialize, Clone)]
pub struct ResponseUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_output_tokens: Option<u32>,
}
