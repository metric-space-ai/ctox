use super::AuthRequestTelemetryContext;
use super::LastResponse;
use super::ModelClient;
use super::PendingUnauthorizedRetry;
use super::UnauthorizedRecoveryExecution;
use ctox_api::ResponsesApiRequest;
use ctox_otel::SessionTelemetry;
use ctox_protocol::ThreadId;
use ctox_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use ctox_protocol::models::BaseInstructions;
use ctox_protocol::models::ContentItem;
use ctox_protocol::models::ResponseItem;
use ctox_protocol::openai_models::ModelInfo;
use ctox_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use ctox_protocol::protocol::SessionSource;
use ctox_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;
use rusqlite::{Connection, params};
use serde_json::json;
use tokio::sync::oneshot;

fn persist_runtime_state_json(root: &std::path::Path, raw_json: &str) {
    let db_path = root.join("runtime/ctox.sqlite3");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS runtime_state_store (
            state_id INTEGER PRIMARY KEY,
            state_json TEXT NOT NULL
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO runtime_state_store (state_id, state_json) VALUES (1, ?1)
         ON CONFLICT(state_id) DO UPDATE SET state_json = excluded.state_json",
        params![raw_json],
    )
    .unwrap();
}

fn test_model_client(session_source: SessionSource) -> ModelClient {
    let provider = crate::model_provider_info::create_oss_provider_with_base_url(
        "https://example.com/v1",
        crate::model_provider_info::WireApi::Responses,
    );
    ModelClient::new(
        None,
        ThreadId::new(),
        provider,
        session_source,
        None,
        false,
        false,
        false,
        None,
    )
}

fn test_openrouter_model_client(session_source: SessionSource) -> ModelClient {
    let provider = crate::model_provider_info::ModelProviderInfo {
        name: "ctox-core-api".to_string(),
        base_url: Some("https://openrouter.ai/api/v1".to_string()),
        transport_endpoint: None,
        socket_transport_required: false,
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: crate::model_provider_info::WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };
    ModelClient::new(
        None,
        ThreadId::new(),
        provider,
        session_source,
        None,
        false,
        false,
        false,
        None,
    )
}

fn test_local_ipc_model_client(session_source: SessionSource) -> ModelClient {
    let provider = crate::model_provider_info::ModelProviderInfo {
        name: "cto-local".to_string(),
        base_url: None,
        transport_endpoint: Some("/tmp/ctox.sock".to_string()),
        socket_transport_required: true,
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: crate::model_provider_info::WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };
    ModelClient::new(
        None,
        ThreadId::new(),
        provider,
        session_source,
        None,
        false,
        false,
        false,
        None,
    )
}

fn test_model_info() -> ModelInfo {
    serde_json::from_value(json!({
        "slug": "gpt-test",
        "display_name": "gpt-test",
        "description": "desc",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": [
            {"effort": "medium", "description": "medium"}
        ],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 1,
        "upgrade": null,
        "base_instructions": "base instructions",
        "model_messages": null,
        "supports_reasoning_summaries": false,
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "truncation_policy": {"mode": "bytes", "limit": 10000},
        "supports_parallel_tool_calls": false,
        "supports_image_detail_original": false,
        "context_window": 272000,
        "auto_compact_token_limit": null,
        "experimental_supported_tools": []
    }))
    .expect("deserialize test model info")
}

fn test_session_telemetry() -> SessionTelemetry {
    SessionTelemetry::new(
        ThreadId::new(),
        "gpt-test",
        "gpt-test",
        None,
        None,
        None,
        "test-originator".to_string(),
        false,
        "test-terminal".to_string(),
        SessionSource::Cli,
    )
}

fn test_user_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn test_assistant_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: text.to_string(),
        }],
        end_turn: Some(true),
        phase: None,
    }
}

fn test_responses_request(input: Vec<ResponseItem>) -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "gpt-test".to_string(),
        instructions: "test instructions".to_string(),
        previous_response_id: None,
        input,
        tools: Vec::new(),
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning: None,
        max_output_tokens: None,
        store: false,
        stream: true,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: Some("thread-1".to_string()),
        text: None,
    }
}

#[test]
fn http_request_uses_previous_response_id_for_incremental_delta() {
    let client = test_model_client(SessionSource::Cli);
    let mut session = client.new_session();
    let initial_user = test_user_message("initial");
    let assistant = test_assistant_message("done");
    let next_user = test_user_message("next");
    let previous_request = test_responses_request(vec![initial_user.clone()]);
    let next_request =
        test_responses_request(vec![initial_user, assistant.clone(), next_user.clone()]);
    let (tx, rx) = oneshot::channel();
    tx.send(LastResponse {
        response_id: "resp_previous".to_string(),
        items_added: vec![assistant],
    })
    .expect("last response receiver should be open");
    session.websocket_session.last_request = Some(previous_request);
    session.websocket_session.last_response_rx = Some(rx);

    let wire_request = session.prepare_http_request(&next_request);

    assert_eq!(
        wire_request.previous_response_id.as_deref(),
        Some("resp_previous")
    );
    assert_eq!(wire_request.input, vec![next_user]);
}

#[test]
fn http_request_can_reuse_previous_response_id_after_preparation() {
    let client = test_model_client(SessionSource::Cli);
    let mut session = client.new_session();
    let initial_user = test_user_message("initial");
    let assistant = test_assistant_message("done");
    let next_user = test_user_message("next");
    let previous_request = test_responses_request(vec![initial_user.clone()]);
    let next_request =
        test_responses_request(vec![initial_user, assistant.clone(), next_user.clone()]);
    let (tx, rx) = oneshot::channel();
    tx.send(LastResponse {
        response_id: "resp_previous".to_string(),
        items_added: vec![assistant],
    })
    .expect("last response receiver should be open");
    session.websocket_session.last_request = Some(previous_request);
    session.websocket_session.last_response_rx = Some(rx);

    let first_wire_request = session.prepare_http_request(&next_request);
    let second_wire_request = session.prepare_http_request(&next_request);

    assert_eq!(
        first_wire_request.previous_response_id.as_deref(),
        Some("resp_previous")
    );
    assert_eq!(
        second_wire_request.previous_response_id.as_deref(),
        Some("resp_previous")
    );
    assert_eq!(second_wire_request.input, vec![next_user]);
}

#[test]
fn http_request_keeps_full_input_when_request_shape_changes() {
    let client = test_model_client(SessionSource::Cli);
    let mut session = client.new_session();
    let initial_user = test_user_message("initial");
    let assistant = test_assistant_message("done");
    let next_user = test_user_message("next");
    let previous_request = test_responses_request(vec![initial_user.clone()]);
    let mut next_request =
        test_responses_request(vec![initial_user, assistant.clone(), next_user.clone()]);
    next_request.instructions = "changed instructions".to_string();
    let (tx, rx) = oneshot::channel();
    tx.send(LastResponse {
        response_id: "resp_previous".to_string(),
        items_added: vec![assistant],
    })
    .expect("last response receiver should be open");
    session.websocket_session.last_request = Some(previous_request);
    session.websocket_session.last_response_rx = Some(rx);

    let wire_request = session.prepare_http_request(&next_request);

    assert_eq!(wire_request.previous_response_id, None);
    assert_eq!(wire_request.input, next_request.input);
}

#[test]
fn build_subagent_headers_sets_other_subagent_label() {
    let client = test_model_client(SessionSource::SubAgent(SubAgentSource::Other(
        "memory_consolidation".to_string(),
    )));
    let headers = client.build_subagent_headers();
    let value = headers
        .get("x-openai-subagent")
        .and_then(|value| value.to_str().ok());
    assert_eq!(value, Some("memory_consolidation"));
}

#[tokio::test]
async fn summarize_memories_returns_empty_for_empty_input() {
    let client = test_model_client(SessionSource::Cli);
    let model_info = test_model_info();
    let session_telemetry = test_session_telemetry();

    let output = client
        .summarize_memories(Vec::new(), &model_info, None, &session_telemetry)
        .await
        .expect("empty summarize request should succeed");
    assert_eq!(output.len(), 0);
}

#[test]
fn auth_request_telemetry_context_tracks_attached_auth_and_retry_phase() {
    let auth_context = AuthRequestTelemetryContext::new(
        Some(crate::auth::AuthMode::Chatgpt),
        &crate::api_bridge::CoreAuthProvider::for_test(Some("access-token"), Some("workspace-123")),
        PendingUnauthorizedRetry::from_recovery(UnauthorizedRecoveryExecution {
            mode: "managed",
            phase: "refresh_token",
        }),
    );

    assert_eq!(auth_context.auth_mode, Some("Chatgpt"));
    assert!(auth_context.auth_header_attached);
    assert_eq!(auth_context.auth_header_name, Some("authorization"));
    assert!(auth_context.retry_after_unauthorized);
    assert_eq!(auth_context.recovery_mode, Some("managed"));
    assert_eq!(auth_context.recovery_phase, Some("refresh_token"));
}

#[test]
fn local_ipc_request_omits_native_web_search_tools() {
    let request = ResponsesApiRequest {
        model: "gpt-test".to_string(),
        instructions: "test instructions".to_string(),
        previous_response_id: None,
        input: Vec::new(),
        tools: vec![json!({
            "type": "web_search",
            "external_web_access": false
        })],
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning: None,
        max_output_tokens: None,
        store: true,
        stream: true,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: None,
        text: None,
    };

    let normalized =
        super::build_local_ipc_request(&request).expect("web_search should be omitted");
    assert!(normalized.tools.is_empty());
}

#[test]
fn local_ipc_request_preserves_function_tools_including_spawn_agent() {
    let request = ResponsesApiRequest {
        model: "gpt-test".to_string(),
        instructions: "test instructions".to_string(),
        previous_response_id: None,
        input: Vec::new(),
        tools: vec![json!({
            "type": "function",
            "function": {
                "name": "spawn_agent",
                "description": "Spawn a helper agent.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    },
                    "required": ["message"],
                    "additionalProperties": false
                }
            }
        })],
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning: None,
        max_output_tokens: None,
        store: true,
        stream: true,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: None,
        text: None,
    };

    let normalized =
        super::build_local_ipc_request(&request).expect("spawn_agent should stay representable");

    assert_eq!(normalized.tools.len(), 1);
    assert_eq!(
        normalized.tools[0],
        json!({
            "type": "function",
            "function": {
                "name": "spawn_agent",
                "description": "Spawn a helper agent.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    },
                    "required": ["message"],
                    "additionalProperties": false
                }
            }
        })
    );
}

#[test]
fn local_ipc_request_forces_parallel_tool_calls_on() {
    let request = ResponsesApiRequest {
        model: "gpt-test".to_string(),
        instructions: "test instructions".to_string(),
        previous_response_id: None,
        input: Vec::new(),
        tools: Vec::new(),
        tool_choice: "auto".to_string(),
        parallel_tool_calls: false,
        reasoning: None,
        max_output_tokens: Some(64),
        store: true,
        stream: true,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: None,
        text: None,
    };

    let normalized =
        super::build_local_ipc_request(&request).expect("request should stay representable");

    assert!(normalized.parallel_tool_calls);
    assert_eq!(normalized.max_output_tokens, Some(64));
}

#[test]
fn managed_local_responses_max_output_tokens_reads_runtime_state_override() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ctox-client-runtime-state-{unique}"));
    std::fs::create_dir_all(root.join("runtime/sockets")).unwrap();
    persist_runtime_state_json(
        &root,
        r#"{"version":4,"realized_context_tokens":131072,"adapter_tuning":{"max_output_tokens_cap":128}}"#,
    );

    let provider = crate::model_provider_info::ModelProviderInfo {
        name: "cto-local".to_string(),
        base_url: None,
        transport_endpoint: Some(
            root.join("runtime/sockets/primary_generation.sock")
                .display()
                .to_string(),
        ),
        socket_transport_required: true,
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: crate::model_provider_info::WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };
    let model_info = ModelInfo {
        slug: "openai/gpt-oss-120b".to_string(),
        ..Default::default()
    };

    assert_eq!(
        super::managed_local_responses_max_output_tokens(&provider, &model_info),
        Some(128)
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn managed_local_responses_max_output_tokens_does_not_use_process_env_fallback() {
    let previous_cap = std::env::var("CTOX_LOCAL_ADAPTER_MAX_OUTPUT_TOKENS_CAP").ok();
    unsafe {
        std::env::set_var("CTOX_LOCAL_ADAPTER_MAX_OUTPUT_TOKENS_CAP", "16");
    }

    let provider = crate::model_provider_info::ModelProviderInfo {
        name: "cto-local".to_string(),
        base_url: None,
        transport_endpoint: Some("/tmp/ctox.sock".to_string()),
        socket_transport_required: true,
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: crate::model_provider_info::WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };
    let model_info = ModelInfo {
        slug: "openai/gpt-oss-120b".to_string(),
        ..Default::default()
    };

    assert_eq!(
        super::managed_local_responses_max_output_tokens(&provider, &model_info),
        None
    );

    unsafe {
        if let Some(value) = previous_cap {
            std::env::set_var("CTOX_LOCAL_ADAPTER_MAX_OUTPUT_TOKENS_CAP", value);
        } else {
            std::env::remove_var("CTOX_LOCAL_ADAPTER_MAX_OUTPUT_TOKENS_CAP");
        }
    }
}

#[test]
fn managed_local_responses_max_output_tokens_defaults_to_realized_context_budget() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ctox-client-runtime-budget-{unique}"));
    std::fs::create_dir_all(root.join("runtime/sockets")).unwrap();
    persist_runtime_state_json(
        &root,
        r#"{"version":4,"realized_context_tokens":131072,"gpt_oss":{}}"#,
    );

    let provider = crate::model_provider_info::ModelProviderInfo {
        name: "cto-local".to_string(),
        base_url: None,
        transport_endpoint: Some(
            root.join("runtime/sockets/primary_generation.sock")
                .display()
                .to_string(),
        ),
        socket_transport_required: true,
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: crate::model_provider_info::WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };
    let model_info = ModelInfo {
        slug: "openai/gpt-oss-120b".to_string(),
        ..Default::default()
    };

    assert_eq!(
        super::managed_local_responses_max_output_tokens(&provider, &model_info),
        Some(131_072)
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn managed_local_gpt_oss_request_preserves_explicit_none_reasoning() {
    let client = test_local_ipc_model_client(SessionSource::Cli);
    let session = client.new_session();
    let api_provider = client.state.provider.to_api_provider(None).unwrap();
    let model_info = ModelInfo {
        slug: "openai/gpt-oss-120b".to_string(),
        default_reasoning_level: Some(ReasoningEffortConfig::Medium),
        supports_reasoning_summaries: false,
        ..test_model_info()
    };
    let prompt = crate::client_common::Prompt {
        input: Vec::new(),
        tools: Vec::new(),
        parallel_tool_calls: true,
        base_instructions: BaseInstructions::default(),
        personality: None,
        output_schema: None,
    };

    let request = session
        .build_responses_request(
            &api_provider,
            &prompt,
            &model_info,
            Some(ReasoningEffortConfig::None),
            ReasoningSummaryConfig::None,
            None,
        )
        .unwrap();

    assert_eq!(
        request
            .reasoning
            .as_ref()
            .and_then(|reasoning| reasoning.effort),
        Some(ReasoningEffortConfig::None)
    );
}

#[test]
fn openrouter_kimi_responses_request_disables_default_thinking() {
    let client = test_openrouter_model_client(SessionSource::Cli);
    let session = client.new_session();
    let api_provider = client.state.provider.to_api_provider(None).unwrap();
    let model_info = ModelInfo {
        slug: "moonshotai/kimi-k2.6".to_string(),
        default_reasoning_level: None,
        supports_reasoning_summaries: false,
        ..test_model_info()
    };
    let prompt = crate::client_common::Prompt {
        input: Vec::new(),
        tools: Vec::new(),
        parallel_tool_calls: true,
        base_instructions: BaseInstructions::default(),
        personality: None,
        output_schema: None,
    };

    let request = session
        .build_responses_request(
            &api_provider,
            &prompt,
            &model_info,
            None,
            ReasoningSummaryConfig::None,
            None,
        )
        .unwrap();
    let reasoning = request
        .reasoning
        .as_ref()
        .expect("OpenRouter Kimi should opt out of provider default thinking");

    assert_eq!(reasoning.effort, Some(ReasoningEffortConfig::None));
    assert_eq!(reasoning.summary, None);
    assert_eq!(reasoning.exclude, Some(true));
}

#[test]
fn local_ipc_request_collapses_non_user_image_messages_to_text() {
    let request = ResponsesApiRequest {
        model: "gpt-test".to_string(),
        instructions: "test instructions".to_string(),
        previous_response_id: None,
        input: vec![ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputImage {
                image_url: "data:image/png;base64,AAAA".to_string(),
            }],
            end_turn: None,
            phase: None,
        }],
        tools: Vec::new(),
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning: None,
        max_output_tokens: None,
        store: true,
        stream: true,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: None,
        text: None,
    };

    let normalized =
        super::build_local_ipc_request(&request).expect("request should stay representable");

    assert_eq!(normalized.input.len(), 1);
    assert_eq!(normalized.input[0]["role"], "developer");
    assert_eq!(
        normalized.input[0]["content"],
        "[image omitted for non-user message]"
    );
}

#[test]
fn local_ipc_request_collapses_non_user_multipart_messages_to_text() {
    let request = ResponsesApiRequest {
        model: "gpt-test".to_string(),
        instructions: "test instructions".to_string(),
        previous_response_id: None,
        input: vec![ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![
                ContentItem::InputText {
                    text: "part one".to_string(),
                },
                ContentItem::InputText {
                    text: "part two".to_string(),
                },
            ],
            end_turn: None,
            phase: None,
        }],
        tools: Vec::new(),
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning: None,
        max_output_tokens: None,
        store: true,
        stream: true,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: None,
        text: None,
    };

    let normalized =
        super::build_local_ipc_request(&request).expect("request should stay representable");

    assert_eq!(normalized.input.len(), 1);
    assert_eq!(normalized.input[0]["role"], "developer");
    assert_eq!(normalized.input[0]["content"], "part one\npart two");
}
