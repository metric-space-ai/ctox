// Origin: CTOX
// License: Apache-2.0

//! Vendored CTOX Responses-IPC wire types.
//!
//! Self-contained per the inference-engine architecture rule: each
//! model crate owns its own copy of the Responses-shaped IPC enum so
//! it never imports another crate. Wire-compatible with the canonical
//! `src/harness/core/src/client.rs::LocalIpcRequest`.
//!
//! Only the request/response *shapes* are defined here. The
//! request-handler logic lives in `server.rs`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalIpcRequest {
    ResponsesCreate(ResponsesCreateRequest),
    RuntimeHealth,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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
    /// Stage-1 hint for the harness: tells it the engine is in skeleton
    /// state so it can route real work to the existing
    /// `qwen36_35b_a3b_ggml` shim until the native Metal path lands.
    pub stage: &'static str,
}

#[derive(Debug, Serialize, Clone)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}
