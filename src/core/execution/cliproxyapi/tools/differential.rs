// Origin: CTOX
// License: AGPL-3.0-only

#[cfg(any(unix, windows))]
use base64::Engine as _;
use ctox_cliproxyapi::internal::api::handlers::management::static_model_definitions_payload;
use ctox_cliproxyapi::internal::cache::antigravity_reasoning_replay_cache::AntigravityReasoningReplayCache;
use ctox_cliproxyapi::internal::cache::{
    cache_signature, clear_signature_cache, set_signature_bypass_strict_mode,
    set_signature_cache_enabled,
};
use ctox_cliproxyapi::internal::pluginhost::rpc_schema::{RpcLifecycleRequest, RpcRegistration};
#[cfg(unix)]
use ctox_cliproxyapi::internal::pluginhost::transport_unix::{
    handshake_proof, handshake_response_message, HandshakeRequest,
};
#[cfg(windows)]
use ctox_cliproxyapi::internal::pluginhost::transport_windows::{
    handshake_proof, handshake_response_message, HandshakeRequest,
};
#[cfg(any(unix, windows))]
use ctox_cliproxyapi::internal::pluginhost::{
    process_transport::{read_process_message, write_process_message},
    rpc_schema::{
        decode_upstream_json, encode_upstream_json, ProcessMessage, RpcCapabilities,
        RpcIdentifierResponse, PROCESS_PROTOCOL_VERSION,
    },
};
use ctox_cliproxyapi::internal::runtime::executor::antigravity_reasoning_replay::{
    apply_antigravity_reasoning_replay_items, prepare_antigravity_reasoning_replay,
    AntigravityReplayCommitOutcome,
};
use ctox_cliproxyapi::internal::runtime::executor::{
    count_codex_input_tokens, enforce_claude_cache_control_limit, ensure_claude_cache_control,
    normalize_claude_cache_control_ttl, parse_claude_usage,
    remap_claude_oauth_tool_names_with_secret, sign_anthropic_messages_body,
};
use ctox_cliproxyapi::internal::signature::sanitize_gemini_request_thought_signatures;
use ctox_cliproxyapi::internal::translator::antigravity::claude::{
    convert_antigravity_response_to_claude_non_stream,
    convert_antigravity_response_to_claude_stream,
    convert_antigravity_web_search_response_to_claude_non_stream,
    convert_antigravity_web_search_response_to_claude_stream,
    convert_claude_request_to_antigravity_with_capabilities,
    decode_gemini_claude_carrier_signature, encode_gemini_claude_carrier_signature,
    normalize_claude_bypass_signature, strip_empty_signature_thinking_blocks,
    strip_invalid_bypass_signature_thinking_blocks, strip_invalid_gemini_signature_thinking_blocks,
    validate_claude_bypass_signatures, AntigravityClaudeRequestCapabilities,
    AntigravityClaudeStreamState, AntigravityClaudeWebSearchStreamState,
};
use ctox_cliproxyapi::internal::translator::antigravity::gemini::{
    convert_antigravity_response_to_gemini, convert_antigravity_response_to_gemini_non_stream,
    convert_gemini_request_to_antigravity, gemini_token_count as antigravity_gemini_token_count,
};
use ctox_cliproxyapi::internal::translator::antigravity::openai::chat_completions::{
    convert_antigravity_response_to_openai_chat_non_stream,
    convert_antigravity_response_to_openai_chat_stream, convert_openai_chat_request_to_antigravity,
    AntigravityToChatStreamState,
};
use ctox_cliproxyapi::internal::translator::antigravity::openai::responses::convert_antigravity_response_to_openai_responses_non_stream;
use ctox_cliproxyapi::internal::translator::antigravity::openai::responses::convert_openai_responses_request_to_antigravity;
use ctox_cliproxyapi::internal::translator::antigravity::openai::responses::{
    convert_antigravity_response_to_openai_responses_stream, AntigravityToResponsesState,
};
use ctox_cliproxyapi::internal::translator::claude::openai::chat_completions::{
    convert_claude_response_to_openai_chat_non_stream,
    convert_claude_response_to_openai_chat_stream, convert_openai_chat_request_to_claude,
    ClaudeToChatStreamState,
};
use ctox_cliproxyapi::internal::translator::claude::openai::responses::{
    convert_claude_response_to_openai_responses,
    convert_claude_response_to_openai_responses_non_stream,
    convert_openai_responses_request_to_claude, ClaudeToResponsesState,
};
use ctox_cliproxyapi::internal::translator::codex::openai::chat_completions::{
    convert_codex_response_to_openai_chat_non_stream, convert_codex_response_to_openai_chat_stream,
    convert_openai_chat_request_to_codex, CodexToChatStreamState,
};
use ctox_cliproxyapi::internal::translator::codex::openai::responses::{
    convert_codex_response_to_openai_responses,
    convert_codex_response_to_openai_responses_non_stream,
    convert_openai_responses_request_to_codex,
};
use ctox_cliproxyapi::internal::translator::common::{
    attach_cache_control, attach_message_cache_control, claude_message_system_reminder_text,
    interactions_usage, normalize_openai_file_data,
};
use ctox_cliproxyapi::internal::translator::gemini::common::attach_default_safety_settings;
use ctox_cliproxyapi::internal::translator::gemini::openai::chat_completions::{
    convert_gemini_response_to_openai_chat_non_stream,
    convert_gemini_response_to_openai_chat_stream, convert_openai_chat_request_to_gemini,
    GeminiToChatStreamState,
};
use ctox_cliproxyapi::internal::translator::gemini::openai::responses::{
    convert_gemini_response_to_openai_responses_non_stream,
    convert_gemini_response_to_openai_responses_stream, convert_openai_responses_request_to_gemini,
    GeminiToResponsesState,
};
use ctox_cliproxyapi::internal::translator::gemini::passthrough::{
    convert_gemini_request_to_gemini, gemini_token_count, passthrough_gemini_response_non_stream,
    passthrough_gemini_response_stream,
};
use ctox_cliproxyapi::internal::translator::openai::claude::{
    convert_claude_request_to_openai, convert_openai_response_to_claude,
    convert_openai_response_to_claude_non_stream, OpenAIToClaudeStreamState,
};
use ctox_cliproxyapi::internal::translator::openai::interactions::responses::{
    convert_interactions_request_to_openai_responses,
    convert_interactions_response_to_openai_responses_non_stream,
    convert_interactions_response_to_openai_responses_stream,
    convert_openai_responses_request_to_interactions,
    convert_openai_responses_response_to_interactions_non_stream,
    convert_openai_responses_response_to_interactions_stream,
};
use ctox_cliproxyapi::internal::translator::openai::passthrough::chat_completions::{
    convert_openai_request_to_openai, convert_openai_response_to_openai,
    convert_openai_response_to_openai_non_stream,
};
use ctox_cliproxyapi::internal::translator::openai::passthrough::responses::{
    convert_openai_chat_completions_response_to_openai_responses,
    convert_openai_chat_completions_response_to_openai_responses_non_stream,
    convert_openai_responses_request_to_openai_chat_completions,
};
use ctox_cliproxyapi::internal::util::claude_attribution::is_claude_code_attribution_system_text;
use ctox_cliproxyapi::sdk::cliproxy::auth::{
    AccountCandidate, AuthScheduler, SchedulerPickOptions, SchedulerStrategy,
};
#[cfg(any(unix, windows))]
use ctox_cliproxyapi::sdk::pluginapi::{ExecutorModelScope, Metadata};
use ctox_cliproxyapi::sdk::translator::TranslationContext;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::Value;
use std::fs;
#[cfg(any(unix, windows))]
use std::io::Read;
#[cfg(any(unix, windows))]
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(any(unix, windows))]
use std::time::Duration;
#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
#[cfg(unix)]
use tokio::net::UnixStream;
#[cfg(any(unix, windows))]
use zeroize::Zeroizing;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    operation: String,
    model: Option<String>,
    stream: Option<bool>,
    input: Option<Value>,
    raw_body: Option<Box<RawValue>>,
    raw_sse: Option<String>,
    raw_text: Option<String>,
    alt: Option<String>,
    raw_sse_chunks: Option<Vec<String>>,
    original_request: Option<Value>,
    translated_request: Option<Value>,
    raw_json: Option<Value>,
    raw_json_chunks: Option<Vec<Value>>,
    payload: Option<Value>,
    replay_items: Option<Vec<Value>>,
    response_payloads: Option<Vec<Value>>,
    signature_cache_enabled: Option<bool>,
    signature_cache_seed: Option<Vec<SignatureCacheSeed>>,
    signature_bypass_strict: Option<bool>,
    supports_web_search: Option<bool>,
}

#[derive(Deserialize)]
struct SignatureCacheSeed {
    model: String,
    text: String,
    signature: String,
}

#[derive(Serialize)]
struct ResultRow {
    name: String,
    output: Value,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let fixtures_path = args.next().ok_or("missing fixtures path")?;
    if fixtures_path == "--ctox-plugin-child" {
        return run_plugin_child(args.collect());
    }
    let output_path = args.next().ok_or("missing output path")?;
    let fixtures: Vec<Fixture> = serde_json::from_slice(&fs::read(fixtures_path)?)?;
    let mut rows = Vec::new();
    for fixture in fixtures {
        let normalize_function_calls = fixture.operation.starts_with("antigravity_to_responses_")
            || fixture.operation.starts_with("gemini_to_responses_")
            || fixture.operation.starts_with("gemini_chat_")
            || fixture.operation.starts_with("antigravity_chat_");
        let normalize_dynamic_times = !matches!(
            fixture.operation.as_str(),
            "management_static_models" | "codex_count_tokens"
        ) && !fixture.operation.starts_with("codex_chat_");
        let normalize_dynamic_times =
            normalize_dynamic_times && !fixture.operation.starts_with("openai_chat_to_responses_");
        let normalize_claude_tool_ids = fixture.operation == "antigravity_claude_stream";
        let mut output = match fixture.operation.as_str() {
            "claude_executor_normalize_cache_ttl" => {
                serde_json::from_slice(&normalize_claude_cache_control_ttl(
                    fixture
                        .raw_body
                        .as_ref()
                        .map(|raw| raw.get().as_bytes())
                        .unwrap_or(b"null"),
                ))?
            }
            "claude_executor_enforce_cache_limit" => {
                serde_json::from_slice(&enforce_claude_cache_control_limit(
                    fixture
                        .raw_body
                        .as_ref()
                        .map(|raw| raw.get().as_bytes())
                        .unwrap_or(b"null"),
                    4,
                ))?
            }
            "claude_executor_ensure_cache" => {
                serde_json::from_slice(&ensure_claude_cache_control(
                    fixture
                        .raw_body
                        .as_ref()
                        .map(|raw| raw.get().as_bytes())
                        .unwrap_or(b"null"),
                ))?
            }
            "claude_executor_remap_oauth_tools" => {
                let (body, reverse) = remap_claude_oauth_tool_names_with_secret(
                    fixture
                        .raw_body
                        .as_ref()
                        .map(|raw| raw.get().as_bytes())
                        .unwrap_or(b"null"),
                    "cpa-claude-mcp-default-caller",
                );
                serde_json::json!({"body":serde_json::from_slice::<Value>(&body)?,"reverse":reverse})
            }
            "claude_executor_sign_cch" => {
                Value::String(String::from_utf8(sign_anthropic_messages_body(
                    fixture
                        .raw_body
                        .as_ref()
                        .map(|raw| raw.get().as_bytes())
                        .unwrap_or(b"null"),
                ))?)
            }
            "claude_executor_usage" => serde_json::to_value(
                parse_claude_usage(
                    fixture
                        .raw_body
                        .as_ref()
                        .map(|raw| raw.get().as_bytes())
                        .unwrap_or(b"null"),
                )
                .unwrap_or_default(),
            )?,
            "translator_common" => {
                let input = fixture.input.unwrap_or(Value::Null);
                match input.get("kind").and_then(Value::as_str).unwrap_or("") {
                    "cache" => serde_json::from_slice(&attach_cache_control(
                        input
                            .get("dst")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .as_bytes(),
                        input.get("src").unwrap_or(&Value::Null),
                    ))?,
                    "message_cache" => serde_json::from_slice(&attach_message_cache_control(
                        input
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .as_bytes(),
                        input.get("src").unwrap_or(&Value::Null),
                    ))?,
                    "system" => {
                        let text = claude_message_system_reminder_text(
                            input.get("content").unwrap_or(&Value::Null),
                        );
                        serde_json::json!({"text":text.clone().unwrap_or_default(),"ok":text.is_some()})
                    }
                    "file" => {
                        let value = normalize_openai_file_data(
                            input.get("filename").and_then(Value::as_str).unwrap_or(""),
                            input.get("fallback").and_then(Value::as_str).unwrap_or(""),
                            input.get("data").and_then(Value::as_str).unwrap_or(""),
                        );
                        match value {
                            Some((mime, data)) => {
                                serde_json::json!({"mime":mime,"data":data,"ok":true})
                            }
                            None => serde_json::json!({"mime":"","data":"","ok":false}),
                        }
                    }
                    "usage" => interactions_usage(input.get("root").unwrap_or(&Value::Null))
                        .cloned()
                        .unwrap_or(Value::Null),
                    "safety" => serde_json::from_slice(&attach_default_safety_settings(
                        input
                            .get("raw")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .as_bytes(),
                        input.get("path").and_then(Value::as_str).unwrap_or(""),
                    ))?,
                    "attribution" => Value::Bool(is_claude_code_attribution_system_text(
                        input.get("text").and_then(Value::as_str).unwrap_or(""),
                    )),
                    kind => return Err(format!("unknown translator common kind {kind}").into()),
                }
            }
            "plugin_rpc_schema" => {
                #[derive(Deserialize, Serialize)]
                struct PluginSchemaSnapshot {
                    lifecycle: RpcLifecycleRequest,
                    registration: RpcRegistration,
                }

                serde_json::to_value(serde_json::from_value::<PluginSchemaSnapshot>(
                    fixture.input.unwrap_or(Value::Null),
                )?)?
            }
            "management_static_models" => serde_json::from_slice(
                &static_model_definitions_payload(fixture.model.as_deref().unwrap_or(""))?,
            )?,
            "scheduler_sequence" => run_scheduler_sequence(
                fixture.input.unwrap_or(Value::Null),
                fixture.model.as_deref(),
            )?,
            "codex_count_tokens" => {
                let body = fixture
                    .raw_body
                    .as_deref()
                    .map(RawValue::get)
                    .unwrap_or("{}");
                serde_json::json!({
                    "count": count_codex_input_tokens(
                        fixture.model.as_deref().unwrap_or(""),
                        body.as_bytes(),
                    )?
                })
            }
            "request_to_antigravity" => {
                serde_json::from_slice(&convert_openai_responses_request_to_antigravity(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "responses_request_to_gemini" => {
                serde_json::from_slice(&convert_openai_responses_request_to_gemini(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "gemini_chat_request" => {
                serde_json::from_slice(&convert_openai_chat_request_to_gemini(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "gemini_chat_non_stream" => {
                serde_json::from_slice(&convert_gemini_response_to_openai_chat_non_stream(
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    b"",
                    &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                ))?
            }
            "gemini_chat_stream" => {
                let request = serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let mut state = GeminiToChatStreamState::default();
                let mut chunks = Vec::new();
                for chunk in fixture.raw_sse_chunks.unwrap_or_default() {
                    for output in convert_gemini_response_to_openai_chat_stream(
                        fixture.model.as_deref().unwrap_or(""),
                        &request,
                        b"",
                        chunk.as_bytes(),
                        &mut state,
                    ) {
                        chunks.push(serde_json::from_slice::<Value>(&output)?);
                    }
                }
                Value::Array(chunks)
            }
            "gemini_to_responses_non_stream" => {
                serde_json::from_slice(&convert_gemini_response_to_openai_responses_non_stream(
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    b"",
                    &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                ))?
            }
            "gemini_to_responses_stream" => {
                let request = serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let mut state = GeminiToResponsesState::default();
                let mut events = Vec::new();
                for chunk in fixture.raw_sse_chunks.unwrap_or_default() {
                    for event in convert_gemini_response_to_openai_responses_stream(
                        &request,
                        b"",
                        chunk.as_bytes(),
                        &mut state,
                    ) {
                        events.push(parse_sse_event(&event)?);
                    }
                }
                Value::Array(events)
            }
            "antigravity_to_responses_non_stream" => serde_json::from_slice(
                &convert_antigravity_response_to_openai_responses_non_stream(
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    b"",
                    &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                ),
            )?,
            "antigravity_chat_request" => {
                serde_json::from_slice(&convert_openai_chat_request_to_antigravity(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "antigravity_chat_non_stream" => {
                let output = convert_antigravity_response_to_openai_chat_non_stream(
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    b"",
                    &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                );
                if output.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&output)?
                }
            }
            "antigravity_chat_stream" => {
                let request = serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let mut state = AntigravityToChatStreamState::default();
                let mut chunks = Vec::new();
                for chunk in fixture.raw_sse_chunks.unwrap_or_default() {
                    for output in convert_antigravity_response_to_openai_chat_stream(
                        fixture.model.as_deref().unwrap_or(""),
                        &request,
                        b"",
                        chunk.as_bytes(),
                        &mut state,
                    ) {
                        chunks.push(serde_json::from_slice::<Value>(&output)?);
                    }
                }
                Value::Array(chunks)
            }
            "codex_chat_request" => serde_json::from_slice(&convert_openai_chat_request_to_codex(
                fixture.model.as_deref().unwrap_or(""),
                &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                fixture.stream.unwrap_or(false),
            ))?,
            "codex_chat_non_stream" => {
                let output = convert_codex_response_to_openai_chat_non_stream(
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    b"",
                    &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                );
                if output.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&output)?
                }
            }
            "codex_chat_stream" => {
                let request = serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let mut state = CodexToChatStreamState::default();
                let mut chunks = Vec::new();
                for chunk in fixture.raw_sse_chunks.unwrap_or_default() {
                    for output in convert_codex_response_to_openai_chat_stream(
                        fixture.model.as_deref().unwrap_or(""),
                        &request,
                        b"",
                        chunk.as_bytes(),
                        &mut state,
                    ) {
                        chunks.push(serde_json::from_slice::<Value>(&output)?);
                    }
                }
                Value::Array(chunks)
            }
            "codex_responses_request" => {
                serde_json::from_slice(&convert_openai_responses_request_to_codex(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "codex_responses_stream" => Value::Array(
                convert_codex_response_to_openai_responses(
                    fixture.raw_sse.as_deref().unwrap_or("").as_bytes(),
                )
                .into_iter()
                .map(|value| Value::String(String::from_utf8_lossy(&value).into_owned()))
                .collect(),
            ),
            "codex_responses_non_stream" => {
                let output = convert_codex_response_to_openai_responses_non_stream(
                    &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                );
                if output.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&output)?
                }
            }
            "openai_claude_request" => serde_json::from_slice(&convert_claude_request_to_openai(
                fixture.model.as_deref().unwrap_or(""),
                &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                fixture.stream.unwrap_or(false),
            ))?,
            "openai_claude_non_stream" => {
                let mut state = OpenAIToClaudeStreamState::default();
                serde_json::from_slice(&convert_openai_response_to_claude_non_stream(
                    &TranslationContext::default(),
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    b"",
                    &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                    &mut state,
                ))?
            }
            "openai_claude_stream" => {
                let request = serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let mut state = OpenAIToClaudeStreamState::default();
                let mut events = Vec::new();
                for chunk in fixture.raw_sse_chunks.unwrap_or_default() {
                    for event in convert_openai_response_to_claude(
                        &TranslationContext::default(),
                        fixture.model.as_deref().unwrap_or(""),
                        &request,
                        b"",
                        chunk.as_bytes(),
                        &mut state,
                    ) {
                        events.push(parse_sse_event(&event)?);
                    }
                }
                Value::Array(events)
            }
            "openai_passthrough_request" => {
                serde_json::from_slice(&convert_openai_request_to_openai(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "openai_passthrough_stream" => Value::Array(
                convert_openai_response_to_openai(
                    fixture.raw_sse.as_deref().unwrap_or("").as_bytes(),
                )
                .into_iter()
                .map(|value| Value::String(String::from_utf8_lossy(&value).into_owned()))
                .collect(),
            ),
            "openai_passthrough_non_stream" => Value::String(
                String::from_utf8_lossy(&convert_openai_response_to_openai_non_stream(
                    fixture.raw_text.as_deref().unwrap_or("").as_bytes(),
                ))
                .into_owned(),
            ),
            "openai_responses_to_chat_request" => serde_json::from_slice(
                &convert_openai_responses_request_to_openai_chat_completions(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ),
            )?,
            "openai_chat_to_responses_non_stream" => {
                let mut state = None;
                serde_json::from_slice(
                    &convert_openai_chat_completions_response_to_openai_responses_non_stream(
                        &TranslationContext::default(),
                        fixture.model.as_deref().unwrap_or(""),
                        &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                        &serde_json::to_vec(&fixture.translated_request.unwrap_or(Value::Null))?,
                        fixture.raw_text.as_deref().unwrap_or("").as_bytes(),
                        &mut state,
                    ),
                )?
            }
            "openai_chat_to_responses_stream" => {
                let mut state = None;
                let original =
                    serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let translated =
                    serde_json::to_vec(&fixture.translated_request.unwrap_or(Value::Null))?;
                let mut events = Vec::new();
                for chunk in fixture.raw_sse_chunks.unwrap_or_default() {
                    events.extend(
                        convert_openai_chat_completions_response_to_openai_responses(
                            &TranslationContext::default(),
                            fixture.model.as_deref().unwrap_or(""),
                            &original,
                            &translated,
                            chunk.as_bytes(),
                            &mut state,
                        )
                        .into_iter()
                        .map(|value| parse_sse_event(&value))
                        .collect::<Result<Vec<_>, _>>()?,
                    );
                }
                Value::Array(events)
            }
            "gemini_passthrough_request" => {
                serde_json::from_slice(&convert_gemini_request_to_gemini(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "gemini_signature_sanitize" => {
                serde_json::from_slice(&sanitize_gemini_request_thought_signatures(
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                ))?
            }
            "gemini_passthrough_stream" => Value::Array(
                passthrough_gemini_response_stream(
                    fixture.raw_sse.as_deref().unwrap_or("").as_bytes(),
                )
                .into_iter()
                .map(|value| Value::String(String::from_utf8_lossy(&value).into_owned()))
                .collect(),
            ),
            "gemini_passthrough_non_stream" => Value::String(
                String::from_utf8_lossy(&passthrough_gemini_response_non_stream(
                    fixture.raw_text.as_deref().unwrap_or("").as_bytes(),
                ))
                .into_owned(),
            ),
            "gemini_passthrough_count" => serde_json::from_slice(&gemini_token_count(
                fixture
                    .input
                    .as_ref()
                    .and_then(|input| input.get("count"))
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
            ))?,
            "antigravity_gemini_request" => {
                let output = convert_gemini_request_to_antigravity(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                );
                if output.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&output)?
                }
            }
            "antigravity_gemini_non_stream" => {
                serde_json::from_slice(&convert_antigravity_response_to_gemini_non_stream(
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                ))?
            }
            "antigravity_gemini_stream" => {
                let outputs = convert_antigravity_response_to_gemini(
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    fixture.raw_sse.as_deref().unwrap_or("").as_bytes(),
                    fixture.alt.as_deref(),
                );
                Value::Array(
                    outputs
                        .into_iter()
                        .map(|output| {
                            if output.is_empty() {
                                Value::Null
                            } else {
                                serde_json::from_slice(&output).unwrap_or(Value::Null)
                            }
                        })
                        .collect(),
                )
            }
            "antigravity_gemini_count" => serde_json::from_slice(&antigravity_gemini_token_count(
                fixture
                    .input
                    .as_ref()
                    .and_then(|value| value.get("count"))
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
            ))?,
            "antigravity_claude_carrier_encode" => {
                let input = fixture.input.as_ref().unwrap_or(&Value::Null);
                Value::String(encode_gemini_claude_carrier_signature(
                    input
                        .get("raw_signature")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    input
                        .get("direction")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    input
                        .get("target_kind")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ))
            }
            "antigravity_claude_carrier_decode" => {
                let input = fixture.input.as_ref().unwrap_or(&Value::Null);
                match decode_gemini_claude_carrier_signature(
                    input
                        .get("raw_signature")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ) {
                    Some(carrier) => serde_json::json!({
                        "signature": carrier.signature,
                        "direction": carrier.direction,
                        "target_kind": carrier.target_kind,
                        "marked": carrier.marked,
                        "ok": true
                    }),
                    None => serde_json::json!({
                        "signature": "",
                        "direction": "",
                        "target_kind": "",
                        "marked": true,
                        "ok": false
                    }),
                }
            }
            "antigravity_claude_carrier_filter" => {
                let input = fixture.input.unwrap_or(Value::Null);
                serde_json::from_slice(&strip_invalid_gemini_signature_thinking_blocks(
                    &serde_json::to_vec(&input)?,
                ))?
            }
            "antigravity_claude_request" => {
                clear_signature_cache("");
                let previous =
                    set_signature_cache_enabled(fixture.signature_cache_enabled.unwrap_or(false));
                for seed in fixture.signature_cache_seed.as_deref().unwrap_or_default() {
                    cache_signature(&seed.model, &seed.text, &seed.signature);
                }
                let converted = convert_claude_request_to_antigravity_with_capabilities(
                    fixture.model.as_deref().unwrap_or_default(),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                    AntigravityClaudeRequestCapabilities {
                        native_google_search: fixture.supports_web_search.unwrap_or(false),
                    },
                );
                clear_signature_cache("");
                set_signature_cache_enabled(previous);
                serde_json::from_slice(&converted)?
            }
            "antigravity_claude_signature_policy" => {
                let previous = set_signature_bypass_strict_mode(
                    fixture.signature_bypass_strict.unwrap_or(false),
                );
                let input = fixture.input.unwrap_or(Value::Null);
                let action = input.get("action").and_then(Value::as_str).unwrap_or("");
                let payload = input.get("payload").unwrap_or(&Value::Null);
                let output = match action {
                    "strip_prefix" => serde_json::from_slice(
                        &strip_empty_signature_thinking_blocks(&serde_json::to_vec(payload)?),
                    )?,
                    "strip_bypass" => {
                        serde_json::from_slice(&strip_invalid_bypass_signature_thinking_blocks(
                            &serde_json::to_vec(payload)?,
                        ))?
                    }
                    "validate" => serde_json::json!({
                        "ok": validate_claude_bypass_signatures(&serde_json::to_vec(payload)?).is_ok()
                    }),
                    "normalize" => match normalize_claude_bypass_signature(
                        input.get("signature").and_then(Value::as_str).unwrap_or(""),
                    ) {
                        Some(signature) => serde_json::json!({"signature":signature,"ok":true}),
                        None => serde_json::json!({"signature":"","ok":false}),
                    },
                    _ => return Err(format!("unknown signature policy action {action}").into()),
                };
                set_signature_bypass_strict_mode(previous);
                output
            }
            "antigravity_claude_web_search_response" => {
                let original =
                    serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let translated =
                    serde_json::to_vec(&fixture.translated_request.unwrap_or(Value::Null))?;
                let response = serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?;
                match convert_antigravity_web_search_response_to_claude_non_stream(
                    &original,
                    &translated,
                    &response,
                    "srvtoolu_rust",
                ) {
                    Some(output) => serde_json::from_slice(&output)?,
                    None => Value::Null,
                }
            }
            "antigravity_claude_response" => {
                let previous =
                    set_signature_cache_enabled(fixture.signature_cache_enabled.unwrap_or(false));
                let output = convert_antigravity_response_to_claude_non_stream(
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    &serde_json::to_vec(&fixture.translated_request.unwrap_or(Value::Null))?,
                    &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                    "srvtoolu_rust",
                );
                set_signature_cache_enabled(previous);
                serde_json::from_slice(&output)?
            }
            "antigravity_claude_stream" => {
                let previous =
                    set_signature_cache_enabled(fixture.signature_cache_enabled.unwrap_or(false));
                let original =
                    serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let translated =
                    serde_json::to_vec(&fixture.translated_request.unwrap_or(Value::Null))?;
                let mut state = AntigravityClaudeStreamState::default();
                let mut events = Vec::new();
                for chunk in fixture.raw_json_chunks.unwrap_or_default() {
                    for event in convert_antigravity_response_to_claude_stream(
                        &original,
                        &translated,
                        &serde_json::to_vec(&chunk)?,
                        &mut state,
                        "srvtoolu_rust",
                    ) {
                        events.push(parse_sse_event(&event)?);
                    }
                }
                for event in convert_antigravity_response_to_claude_stream(
                    &original,
                    &translated,
                    b"[DONE]",
                    &mut state,
                    "srvtoolu_rust",
                ) {
                    events.push(parse_sse_event(&event)?);
                }
                set_signature_cache_enabled(previous);
                Value::Array(events)
            }
            "antigravity_claude_web_search_stream" => {
                let original =
                    serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let translated =
                    serde_json::to_vec(&fixture.translated_request.unwrap_or(Value::Null))?;
                let mut state = AntigravityClaudeWebSearchStreamState::default();
                let mut events = Vec::new();
                for chunk in fixture.raw_json_chunks.unwrap_or_default() {
                    for event in convert_antigravity_web_search_response_to_claude_stream(
                        &original,
                        &translated,
                        &serde_json::to_vec(&chunk)?,
                        &mut state,
                        "srvtoolu_rust",
                    ) {
                        events.push(parse_sse_event(&event)?);
                    }
                }
                for event in convert_antigravity_web_search_response_to_claude_stream(
                    &original,
                    &translated,
                    b"[DONE]",
                    &mut state,
                    "srvtoolu_rust",
                ) {
                    events.push(parse_sse_event(&event)?);
                }
                Value::Array(events)
            }
            "antigravity_to_responses_stream" => {
                let request = serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let mut state = AntigravityToResponsesState::default();
                let mut events = Vec::new();
                for chunk in fixture.raw_json_chunks.unwrap_or_default() {
                    for event in convert_antigravity_response_to_openai_responses_stream(
                        &request,
                        b"",
                        &serde_json::to_vec(&chunk)?,
                        &mut state,
                    ) {
                        events.push(parse_sse_event(&event)?);
                    }
                }
                Value::Array(events)
            }
            "antigravity_apply_replay_items" => {
                let payload = serde_json::to_vec(&fixture.payload.unwrap_or(Value::Null))?;
                let items = fixture
                    .replay_items
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| serde_json::to_vec(&item))
                    .collect::<Result<Vec<_>, _>>()?;
                let (output, changed) = apply_antigravity_reasoning_replay_items(&payload, &items)?;
                serde_json::json!({
                    "changed": changed > 0,
                    "output": serde_json::from_slice::<Value>(&output)?
                })
            }
            "antigravity_accumulate_replay" => {
                let cache = Arc::new(AntigravityReasoningReplayCache::new());
                let payload = serde_json::to_vec(&fixture.payload.unwrap_or(Value::Null))?;
                let (_, mut accumulator) = prepare_antigravity_reasoning_replay(
                    cache.clone(),
                    fixture.model.as_deref().unwrap_or("gemini-3"),
                    "differential-session",
                    &payload,
                    1,
                )?;
                for response in fixture.response_payloads.unwrap_or_default() {
                    accumulator.observe_response_payload(&serde_json::to_vec(&response)?);
                }
                let outcome = accumulator.commit(2)?;
                let (items, _, found) = cache.read(
                    fixture.model.as_deref().unwrap_or("gemini-3"),
                    "differential-session",
                    3,
                )?;
                serde_json::json!({
                    "published": outcome == AntigravityReplayCommitOutcome::Published && found,
                    "items": items.into_iter().map(|item| serde_json::from_slice::<Value>(&item)).collect::<Result<Vec<_>, _>>()?
                })
            }
            "request_to_claude" => {
                serde_json::from_slice(&convert_openai_responses_request_to_claude(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "chat_request_to_claude" => {
                serde_json::from_slice(&convert_openai_chat_request_to_claude(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "claude_to_chat_non_stream" => {
                serde_json::from_slice(&convert_claude_response_to_openai_chat_non_stream(
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    b"",
                    fixture.raw_sse.as_deref().unwrap_or("").as_bytes(),
                ))?
            }
            "claude_to_chat_stream" => {
                let model = fixture.model.as_deref().unwrap_or("");
                let request = serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let mut state = ClaudeToChatStreamState::default();
                let mut events = Vec::new();
                for chunk in fixture.raw_sse_chunks.unwrap_or_default() {
                    for event in convert_claude_response_to_openai_chat_stream(
                        model,
                        &request,
                        b"",
                        chunk.as_bytes(),
                        &mut state,
                    ) {
                        events.push(serde_json::from_slice::<Value>(&event)?);
                    }
                }
                Value::Array(events)
            }
            "claude_to_responses_non_stream" => {
                serde_json::from_slice(&convert_claude_response_to_openai_responses_non_stream(
                    &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                    b"",
                    fixture.raw_sse.as_deref().unwrap_or("").as_bytes(),
                ))?
            }
            "claude_to_responses_stream" => {
                let model = fixture.model.as_deref().unwrap_or("");
                let request = serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?;
                let mut state = ClaudeToResponsesState::default();
                let mut events = Vec::new();
                for chunk in fixture.raw_sse_chunks.unwrap_or_default() {
                    for event in convert_claude_response_to_openai_responses(
                        model,
                        &request,
                        b"",
                        chunk.as_bytes(),
                        &mut state,
                    ) {
                        events.push(parse_sse_event(&event)?);
                    }
                }
                Value::Array(events)
            }
            "responses_request_to_interactions" => {
                serde_json::from_slice(&convert_openai_responses_request_to_interactions(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "interactions_request_to_responses" => {
                serde_json::from_slice(&convert_interactions_request_to_openai_responses(
                    fixture.model.as_deref().unwrap_or(""),
                    &serde_json::to_vec(&fixture.input.unwrap_or(Value::Null))?,
                    fixture.stream.unwrap_or(false),
                ))?
            }
            "interactions_to_responses_non_stream" => {
                let mut state = None;
                serde_json::from_slice(
                    &convert_interactions_response_to_openai_responses_non_stream(
                        &TranslationContext::default(),
                        fixture.model.as_deref().unwrap_or(""),
                        &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                        b"",
                        &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                        &mut state,
                    ),
                )?
            }
            "interactions_to_responses_stream" => {
                let context = TranslationContext::default();
                let mut state = None;
                let mut events = Vec::new();
                for chunk in fixture.raw_sse_chunks.unwrap_or_default() {
                    for event in convert_interactions_response_to_openai_responses_stream(
                        &context,
                        fixture.model.as_deref().unwrap_or(""),
                        b"",
                        b"",
                        chunk.as_bytes(),
                        &mut state,
                    ) {
                        events.push(parse_sse_event(&event)?);
                    }
                }
                Value::Array(events)
            }
            "responses_to_interactions_non_stream" => {
                let mut state = None;
                serde_json::from_slice(
                    &convert_openai_responses_response_to_interactions_non_stream(
                        &TranslationContext::default(),
                        fixture.model.as_deref().unwrap_or(""),
                        &serde_json::to_vec(&fixture.original_request.unwrap_or(Value::Null))?,
                        b"",
                        &serde_json::to_vec(&fixture.raw_json.unwrap_or(Value::Null))?,
                        &mut state,
                    ),
                )?
            }
            "responses_to_interactions_stream" => {
                let context = TranslationContext::default();
                let mut state = None;
                let mut events = Vec::new();
                for chunk in fixture.raw_sse_chunks.unwrap_or_default() {
                    for event in convert_openai_responses_response_to_interactions_stream(
                        &context,
                        fixture.model.as_deref().unwrap_or(""),
                        b"",
                        b"",
                        chunk.as_bytes(),
                        &mut state,
                    ) {
                        events.push(parse_sse_event(&event)?);
                    }
                }
                Value::Array(events)
            }
            operation => return Err(format!("unknown operation: {operation}").into()),
        };
        normalize(
            &mut output,
            normalize_function_calls,
            normalize_dynamic_times,
            normalize_claude_tool_ids,
        );
        rows.push(ResultRow {
            name: fixture.name,
            output,
        });
    }
    fs::write(output_path, serde_json::to_vec_pretty(&rows)?)?;
    Ok(())
}

fn run_scheduler_sequence(
    input: Value,
    fallback_model: Option<&str>,
) -> Result<Value, Box<dyn std::error::Error>> {
    #[derive(Deserialize)]
    struct Candidate {
        id: String,
        provider: String,
        #[serde(default)]
        priority: i32,
        #[serde(default = "one")]
        weight: i64,
        #[serde(default)]
        websocket: bool,
    }

    #[derive(Deserialize)]
    struct Input {
        strategy: SchedulerStrategy,
        providers: Vec<String>,
        candidates: Vec<Candidate>,
        picks: usize,
        #[serde(default)]
        mixed: bool,
        #[serde(default)]
        prefer_websocket: bool,
        pinned_auth_id: Option<String>,
        #[serde(default)]
        tried_auth_ids: Vec<String>,
        #[serde(default)]
        pick_models: Vec<String>,
    }

    fn one() -> i64 {
        1
    }

    let input: Input = serde_json::from_value(input)?;
    let candidates = input
        .candidates
        .into_iter()
        .map(|candidate| AccountCandidate {
            auth_id: candidate.id,
            provider: candidate.provider,
            priority: candidate.priority,
            weight: candidate.weight,
            websocket_enabled: candidate.websocket,
            supported_models: Vec::new(),
            disabled: false,
        })
        .collect::<Vec<_>>();
    let options = SchedulerPickOptions {
        pinned_auth_id: input.pinned_auth_id,
        prefer_websocket: input.prefer_websocket,
        tried_auth_ids: input.tried_auth_ids.into_iter().collect(),
    };
    let scheduler = AuthScheduler::new(input.strategy);
    let mut output = Vec::new();
    for index in 0..input.picks {
        let model = input
            .pick_models
            .get(index)
            .map(String::as_str)
            .or(fallback_model);
        let picked = if input.mixed {
            scheduler.pick_mixed(&input.providers, model, 0, &candidates, &[], &options)?
        } else {
            let provider = input.providers.first().map(String::as_str).unwrap_or("");
            let candidate =
                scheduler.pick_single(provider, model, 0, &candidates, &[], &options)?;
            ctox_cliproxyapi::sdk::cliproxy::auth::ScheduledAccount {
                provider: provider.trim().to_ascii_lowercase(),
                candidate,
            }
        };
        output.push(serde_json::json!({
            "provider": picked.provider,
            "auth_id": picked.candidate.auth_id,
        }));
    }
    Ok(Value::Array(output))
}

#[cfg(any(unix, windows))]
fn run_plugin_child(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut socket = None;
    let mut plugin_id = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--socket" => {
                index += 1;
                socket = args.get(index).map(PathBuf::from);
            }
            "--plugin-id" => {
                index += 1;
                plugin_id = args.get(index).cloned();
            }
            _ => return Err("unknown plugin child argument".into()),
        }
        index += 1;
    }
    // `Command::env_clear` is the authority boundary. CoreFoundation creates
    // `__CF_USER_TEXT_ENCODING` inside a freshly started macOS process even
    // when exec receives an empty environment; it is UID/locale metadata, not
    // inherited configuration. Reject every other key on every platform.
    let inherited_environment = std::env::vars_os().any(|(key, _)| {
        #[cfg(target_os = "macos")]
        if key == "__CF_USER_TEXT_ENCODING" {
            return false;
        }
        true
    });
    if inherited_environment {
        return Err("plugin child inherited ambient environment".into());
    }
    let socket = socket.ok_or("missing plugin socket")?;
    let plugin_id = plugin_id.ok_or("missing plugin id")?;
    let mut token = Zeroizing::new(String::new());
    std::io::stdin().take(256).read_to_string(&mut token)?;
    let trimmed = token.trim_end_matches(['\r', '\n']);
    if trimmed.len() < 32 || trimmed.len() > 128 {
        return Err("invalid one-shot token".into());
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut stream = connect_plugin_endpoint(&socket).await?;
        let handshake = read_process_message(&mut stream)
            .await?
            .ok_or("missing handshake")?;
        let ProcessMessage::Request {
            request_id,
            method,
            payload,
            ..
        } = handshake
        else {
            return Err::<(), Box<dyn std::error::Error>>("invalid handshake message".into());
        };
        if method != "ctox.handshake" {
            return Err("invalid handshake method".into());
        }
        let request: HandshakeRequest = decode_upstream_json(&payload)?;
        let nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(request.nonce)?;
        let response = handshake_response_message(
            request_id,
            plugin_id.clone(),
            request.schema_version,
            handshake_proof(
                trimmed.as_bytes(),
                &nonce,
                &plugin_id,
                request.schema_version,
            ),
        )?;
        write_process_message(&mut stream, &response).await?;
        if plugin_id == "fixture-crash" {
            tokio::time::sleep(Duration::from_millis(60)).await;
            std::process::exit(23);
        }

        let mut registered = false;
        loop {
            let Some(message) = read_process_message(&mut stream).await? else {
                return Ok(());
            };
            let ProcessMessage::Request {
                request_id,
                method,
                payload,
                ..
            } = message
            else {
                continue;
            };
            if method == ctox_cliproxyapi::sdk::pluginabi::METHOD_PLUGIN_REGISTER {
                let lifecycle: RpcLifecycleRequest = decode_upstream_json(&payload)?;
                if lifecycle.schema_version != ctox_cliproxyapi::sdk::pluginabi::SCHEMA_VERSION {
                    return Err("invalid lifecycle schema".into());
                }
                let registration = RpcRegistration {
                    schema_version: lifecycle.schema_version,
                    metadata: Metadata {
                        name: "fixture-executor".into(),
                        version: "1.0.0".into(),
                        author: "ctox".into(),
                        ..Metadata::default()
                    },
                    capabilities: RpcCapabilities {
                        executor: plugin_id != "fixture-no-executor" && plugin_id != "fixture-quota",
                        quota_provider: plugin_id == "fixture-quota",
                        executor_model_scope: ExecutorModelScope(ExecutorModelScope::BOTH.into()),
                        executor_input_formats: vec!["openai-responses".into()],
                        executor_output_formats: vec!["openai-responses".into()],
                        ..RpcCapabilities::default()
                    },
                };
                let response = ProcessMessage::Response {
                    protocol_version: PROCESS_PROTOCOL_VERSION,
                    request_id,
                    envelope: ctox_cliproxyapi::sdk::pluginabi::Envelope::success(Some(
                        encode_upstream_json(&registration)?,
                    )),
                };
                write_process_message(&mut stream, &response).await?;
                registered = true;
                continue;
            }
            if plugin_id == "fixture-quota" && method.starts_with("quota.") {
                let result = match method.as_str() {
                    "quota.identifier" => serde_json::json!({"identifier":"fixture-quota"}),
                    "quota.describe" => serde_json::json!({"supported_providers":["fixture-quota"],"supports_reset":false}),
                    "quota.fetch" => {
                        let request: Value = decode_upstream_json(&payload)?;
                        write_process_message(&mut stream, &ProcessMessage::Request {
                            protocol_version: PROCESS_PROTOCOL_VERSION,
                            request_id: "fixture-http".into(), method: "host.http.do".into(), deadline_unix_ms: None,
                            payload: encode_upstream_json(&serde_json::json!({
                                "host_callback_id":request["host_callback_id"],
                                "method":"GET", "url":"https://fixture.invalid/quota"
                            }))?,
                        }).await?;
                        let Some(ProcessMessage::Response { request_id: id, envelope, .. }) = read_process_message(&mut stream).await? else { return Err("missing callback response".into()); };
                        if id != "fixture-http" { return Err("wrong callback response".into()); }
                        if !envelope.ok {
                            write_process_message(&mut stream, &ProcessMessage::Response {
                                protocol_version: PROCESS_PROTOCOL_VERSION, request_id,
                                envelope: ctox_cliproxyapi::sdk::pluginabi::Envelope::failure(ctox_cliproxyapi::sdk::pluginabi::Error {
                                    code:"fixture_callback_denied".into(),message:"fixture HTTP rejected".into(), ..Default::default()
                                }),
                            }).await?;
                            continue;
                        }
                        serde_json::json!({"subscription":{"plan":"fixture-plan"}})
                    }
                    _ => return Err("unsupported fixture quota method".into()),
                };
                write_process_message(&mut stream, &ProcessMessage::Response {
                    protocol_version: PROCESS_PROTOCOL_VERSION, request_id,
                    envelope: ctox_cliproxyapi::sdk::pluginabi::Envelope::success(Some(encode_upstream_json(&result)?)),
                }).await?;
                continue;
            }
            if method == "fixture.stream" {
                let request: Value = decode_upstream_json(&payload)?;
                let mode = request["mode"].as_str().unwrap_or("normal");
                let count = if mode == "flood" { 1024 } else if mode == "normal" { 3 } else { 1 };
                if mode == "truncated" {
                    use tokio::io::AsyncWriteExt;
                    stream.write_all(&8_u32.to_be_bytes()).await?;
                    stream.write_all(b"{").await?;
                    return Ok(());
                }
                for sequence in 0..count {
                    write_process_message(&mut stream, &ProcessMessage::StreamChunk {
                        protocol_version: PROCESS_PROTOCOL_VERSION,
                        request_id: if mode == "wrong_id" { "foreign".into() } else { request_id.clone() },
                        sequence: if mode == "bad_sequence" { sequence + 1 } else { sequence },
                        payload: encode_upstream_json(&serde_json::json!({"index":sequence,"pid":std::process::id()}))?,
                    }).await?;
                }
                if matches!(mode, "stall" | "flood") { std::future::pending::<()>().await; }
                if mode == "missing_end" { return Ok(()); }
                write_process_message(&mut stream, &ProcessMessage::StreamEnd {
                    protocol_version: PROCESS_PROTOCOL_VERSION, request_id,
                    next_sequence: count,
                    error: (mode == "application_error").then(|| ctox_cliproxyapi::sdk::pluginabi::Error {
                        code: "fixture_rate_limit".into(), message: "fixture limit".into(),
                        http_status: 429, retryable: true,
                        ..Default::default()
                    }),
                }).await?;
                continue;
            }
            if method == ctox_cliproxyapi::sdk::pluginabi::METHOD_EXECUTOR_IDENTIFIER {
                if !registered {
                    return Err("executor call before registration".into());
                }
                let response = ProcessMessage::Response {
                    protocol_version: PROCESS_PROTOCOL_VERSION,
                    request_id,
                    envelope: ctox_cliproxyapi::sdk::pluginabi::Envelope::success(Some(
                        encode_upstream_json(&RpcIdentifierResponse {
                            identifier: "fixture-executor".into(),
                        })?,
                    )),
                };
                write_process_message(&mut stream, &response).await?;
                continue;
            }
            if method == crate::sdk_method_plugin_shutdown() {
                let response = ProcessMessage::Response {
                    protocol_version: PROCESS_PROTOCOL_VERSION,
                    request_id,
                    envelope: ctox_cliproxyapi::sdk::pluginabi::Envelope::success(None),
                };
                write_process_message(&mut stream, &response).await?;
                return Ok(());
            }
        }
    })
}

#[cfg(any(unix, windows))]
fn sdk_method_plugin_shutdown() -> &'static str {
    ctox_cliproxyapi::sdk::pluginabi::METHOD_PLUGIN_SHUTDOWN
}

#[cfg(not(any(unix, windows)))]
fn run_plugin_child(_args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    Err("plugin child fixture is unavailable on this platform".into())
}

#[cfg(unix)]
async fn connect_plugin_endpoint(path: &PathBuf) -> std::io::Result<UnixStream> {
    UnixStream::connect(path).await
}

#[cfg(windows)]
async fn connect_plugin_endpoint(path: &PathBuf) -> std::io::Result<NamedPipeClient> {
    ClientOptions::new().open(path)
}

fn parse_sse_event(raw: &[u8]) -> Result<Value, Box<dyn std::error::Error>> {
    let text = std::str::from_utf8(raw)?;
    let mut event_name = "";
    let mut data = String::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event_name = value.trim();
        } else if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.trim_start());
        }
    }
    let data = if data == "[DONE]" {
        Value::String(data)
    } else {
        serde_json::from_str::<Value>(&data)?
    };
    Ok(serde_json::json!({"event": event_name, "data": data}))
}

fn normalize(
    value: &mut Value,
    normalize_function_calls: bool,
    normalize_dynamic_times: bool,
    normalize_claude_tool_ids: bool,
) {
    match value {
        Value::Object(object) => {
            if normalize_dynamic_times {
                object.remove("created_at");
                object.remove("created");
                object.remove("updated");
            }
            if normalize_function_calls
                && object.get("type").and_then(Value::as_str) == Some("function_call")
            {
                object.insert("id".to_owned(), Value::String("fc_<dynamic>".to_owned()));
                object.insert(
                    "call_id".to_owned(),
                    Value::String("call_<dynamic>".to_owned()),
                );
            }
            if normalize_function_calls
                && object.get("type").and_then(Value::as_str) == Some("function")
                && object.get("function").is_some_and(Value::is_object)
            {
                object.insert("id".to_owned(), Value::String("call_<dynamic>".to_owned()));
            }
            if normalize_function_calls
                && object
                    .get("item_id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.starts_with("fc_call_"))
            {
                object.insert(
                    "item_id".to_owned(),
                    Value::String("fc_<dynamic>".to_owned()),
                );
            }
            if object.get("type").and_then(Value::as_str) == Some("server_tool_use") {
                object.insert(
                    "id".to_owned(),
                    Value::String("srvtoolu_<dynamic>".to_owned()),
                );
            }
            if object.get("type").and_then(Value::as_str) == Some("web_search_tool_result") {
                object.insert(
                    "tool_use_id".to_owned(),
                    Value::String("srvtoolu_<dynamic>".to_owned()),
                );
            }
            if normalize_claude_tool_ids
                && object.get("type").and_then(Value::as_str) == Some("tool_use")
                && !object
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.starts_with("cpa_gemini_"))
            {
                object.insert("id".to_owned(), Value::String("toolu_<dynamic>".to_owned()));
            }
            if let Some(metadata) = object.get_mut("metadata").and_then(Value::as_object_mut) {
                metadata.remove("user_id");
                if metadata.is_empty() {
                    object.remove("metadata");
                }
            }
            for value in object.values_mut() {
                normalize(
                    value,
                    normalize_function_calls,
                    normalize_dynamic_times,
                    normalize_claude_tool_ids,
                );
            }
        }
        Value::Array(values) => values.iter_mut().for_each(|value| {
            normalize(
                value,
                normalize_function_calls,
                normalize_dynamic_times,
                normalize_claude_tool_ids,
            )
        }),
        _ => {}
    }
}
