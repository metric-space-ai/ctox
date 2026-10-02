// ref: internal/runtime/executor/kimi_executor_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::kimi_executor::{
    normalize_kimi_tool_message_links, normalize_kimi_upstream_model, KimiClaudeDelegate,
    KimiClock, KimiDeviceProfile, KimiExecutor, KimiExecutorConfig, KimiExecutorError,
};
use crate::internal::cache::KimiThinkingReplayCache;
use crate::sdk::pluginapi::{
    ExecutorRequest, ExecutorResponse, ExecutorStreamChunk, ExecutorStreamResponse, HostHttpClient,
    HttpRequest, HttpResponse, HttpStreamResponse, PluginExecutionError, PluginFuture,
    ProviderExecutor,
};
use crate::sdk::translator::Registry;

#[derive(Default)]
struct FixedClock;

impl KimiClock for FixedClock {
    fn now_ms(&self) -> i64 {
        1_700_000_000_000
    }
}

struct RecordingClaude {
    requests: Mutex<Vec<ExecutorRequest>>,
    response: Mutex<Result<Vec<u8>, KimiExecutorError>>,
    stream: Mutex<Vec<Vec<u8>>>,
    count_response: Mutex<Result<Vec<u8>, KimiExecutorError>>,
}

impl Default for RecordingClaude {
    fn default() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            response: Mutex::new(Ok(br#"{"model":"k3","content":[]}"#.to_vec())),
            stream: Mutex::new(Vec::new()),
            count_response: Mutex::new(Ok(br#"{"input_tokens":42}"#.to_vec())),
        }
    }
}

impl RecordingClaude {
    fn request(&self) -> ExecutorRequest {
        self.requests.lock().unwrap().last().unwrap().clone()
    }
}

impl KimiClaudeDelegate for RecordingClaude {
    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        self.requests.lock().unwrap().push(request);
        let result = self.response.lock().unwrap().clone();
        Box::pin(async move {
            result
                .map(|payload| ExecutorResponse {
                    payload,
                    ..ExecutorResponse::default()
                })
                .map_err(|error| Arc::new(error) as PluginExecutionError)
        })
    }

    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        self.requests.lock().unwrap().push(request);
        let chunks = self.stream.lock().unwrap().clone();
        Box::pin(async move {
            let (sender, receiver) = mpsc::channel(chunks.len().max(1));
            for payload in chunks {
                sender
                    .send(ExecutorStreamChunk {
                        payload,
                        error: None,
                    })
                    .await
                    .unwrap();
            }
            drop(sender);
            Ok(ExecutorStreamResponse {
                headers: BTreeMap::new(),
                chunks: receiver,
            })
        })
    }

    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        self.requests.lock().unwrap().push(request);
        let result = self.count_response.lock().unwrap().clone();
        Box::pin(async move {
            result
                .map(|payload| ExecutorResponse {
                    payload,
                    ..ExecutorResponse::default()
                })
                .map_err(|error| Arc::new(error) as PluginExecutionError)
        })
    }
}

#[derive(Default)]
struct RecordingHttp {
    requests: Mutex<Vec<HttpRequest>>,
}

impl HostHttpClient for RecordingHttp {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        self.requests.lock().unwrap().push(request);
        Box::pin(async {
            Ok(HttpResponse {
                status_code: 200,
                body: br#"{"id":"chat_1","choices":[]}"#.to_vec(),
                ..HttpResponse::default()
            })
        })
    }

    fn execute_stream<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async { unreachable!("non-stream test") })
    }
}

fn executor(delegate: Arc<RecordingClaude>) -> KimiExecutor {
    KimiExecutor::new(
        Arc::new(KimiExecutorConfig::default()),
        Arc::new(Registry::new()),
        delegate,
        Arc::new(KimiThinkingReplayCache::new()),
        Arc::new(FixedClock),
        None,
        KimiDeviceProfile::default(),
    )
}

fn claude_request(model: &str, payload: &[u8]) -> ExecutorRequest {
    ExecutorRequest {
        auth_provider: "kimi".into(),
        model: model.into(),
        source_format: "claude".into(),
        format: "claude".into(),
        payload: payload.to_vec(),
        original_request: payload.to_vec(),
        auth_metadata: BTreeMap::from([("access_token".into(), json!("test-token"))]),
        ..ExecutorRequest::default()
    }
}

#[test]
fn new_executor_initializes_delegated_claude_config() {
    let delegate = Arc::new(RecordingClaude::default());
    assert!(executor(delegate).delegated_claude_is_configured());
}

#[tokio::test]
async fn claude_request_preserves_internal_model_semantics() {
    let delegate = Arc::new(RecordingClaude::default());
    *delegate.response.lock().unwrap() =
        Ok(br#"{"model":"k2.5","content":[{"type":"text","text":"hello"}]}"#.to_vec());
    let executor = executor(delegate.clone());
    let model = "kimi-k2.5(max)";
    let payload = br#"{"model":"kimi-k2.5(max)","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#;
    let response = executor
        .execute(claude_request(model, payload))
        .await
        .unwrap();
    let upstream: Value = serde_json::from_slice(&delegate.request().payload).unwrap();
    assert_eq!(upstream["model"], "k2.5");
    assert_eq!(
        upstream.pointer("/output_config/effort"),
        Some(&json!("high"))
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&response.payload).unwrap()["model"],
        model
    );
}

#[tokio::test]
async fn count_tokens_uses_canonical_upstream_model() {
    let delegate = Arc::new(RecordingClaude::default());
    let executor = executor(delegate.clone());
    let payload =
        br#"{"model":"kimi-k3[1m](high)","messages":[{"role":"user","content":"hello"}]}"#;
    let response = executor
        .count_tokens(claude_request("kimi-k3[1m](high)", payload))
        .await
        .unwrap();
    assert_eq!(response.payload, br#"{"input_tokens":42}"#);
    let request = delegate.request();
    assert_eq!(
        serde_json::from_slice::<Value>(&request.payload).unwrap()["model"],
        "k3"
    );
    assert_eq!(
        request.auth_attributes["base_url"],
        "https://api.kimi.com/coding"
    );
}

#[tokio::test]
async fn count_tokens_invalid_gzip_error_body_returns_decode_message() {
    let delegate = Arc::new(RecordingClaude::default());
    *delegate.count_response.lock().unwrap() = Err(KimiExecutorError::UpstreamStatus {
        status: 400,
        message: "failed to decode error response body: invalid gzip".into(),
    });
    let error = executor(delegate)
        .count_tokens(claude_request("kimi-k3", br#"{"model":"kimi-k3"}"#))
        .await
        .unwrap_err();
    let error = error.as_ref().downcast_ref::<KimiExecutorError>().unwrap();
    assert_eq!(error.status_code(), Some(400));
    assert!(error
        .to_string()
        .contains("failed to decode error response body"));
}

#[tokio::test]
async fn claude_stream_forwards_anthropic_beta_and_rewrites_model() {
    let delegate = Arc::new(RecordingClaude::default());
    *delegate.stream.lock().unwrap() = vec![br#"event: message_start
data: {"type":"message_start","message":{"model":"k3","content":[]}}

event: message_stop
data: {"type":"message_stop"}

"#
    .to_vec()];
    let executor = executor(delegate.clone());
    let payload = br#"{"model":"kimi-k3","messages":[{"role":"user","content":"hello"}]}"#;
    let mut request = claude_request("kimi-k3", payload);
    request.stream = true;
    request.headers.insert(
        "Anthropic-Beta".into(),
        vec!["client-beta-one".into(), "client-beta-two".into()],
    );
    let mut response = executor.execute_stream(request).await.unwrap();
    let mut output = Vec::new();
    while let Some(chunk) = response.chunks.recv().await {
        assert!(chunk.error.is_none());
        output.extend_from_slice(&chunk.payload);
    }
    assert!(String::from_utf8_lossy(&output).contains(r#""model":"kimi-k3""#));
    let upstream = delegate.request();
    assert_eq!(
        upstream.headers["Anthropic-Beta"],
        ["client-beta-one", "client-beta-two"]
    );
    assert_eq!(
        upstream.auth_attributes["base_url"],
        "https://api.kimi.com/coding"
    );
}

#[tokio::test]
async fn openai_request_applies_kimi_auth_device_and_model_semantics() {
    let delegate = Arc::new(RecordingClaude::default());
    let executor = executor(delegate);
    let http = Arc::new(RecordingHttp::default());
    let payload =
        br#"{"model":"kimi-k3[1m](high)","messages":[{"role":"user","content":"hello"}]}"#;
    let request = ExecutorRequest {
        auth_provider: "kimi".into(),
        model: "kimi-k3[1m](high)".into(),
        source_format: "openai".into(),
        format: "openai".into(),
        payload: payload.to_vec(),
        original_request: payload.to_vec(),
        auth_metadata: BTreeMap::from([
            ("access_token".into(), json!("oauth-token")),
            ("device_id".into(), json!("account-device")),
        ]),
        http_client: Some(http.clone()),
        ..ExecutorRequest::default()
    };
    executor.execute(request).await.unwrap();
    let upstream = http.requests.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        upstream.url,
        "https://api.kimi.com/coding/v1/chat/completions"
    );
    assert_eq!(upstream.headers["Authorization"], ["Bearer oauth-token"]);
    assert_eq!(upstream.headers["X-Msh-Device-Id"], ["account-device"]);
    assert_eq!(upstream.headers["Accept"], ["application/json"]);
    let body: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(body["model"], "k3");
    assert_eq!(body.pointer("/thinking/effort"), Some(&json!("high")));
}

#[tokio::test]
async fn openai_response_reorders_interleaved_tool_outputs_before_chat_post() {
    let delegate = Arc::new(RecordingClaude::default());
    let executor = executor(delegate);
    let http = Arc::new(RecordingHttp::default());
    let payload = br#"{
        "model":"kimi-k3",
        "input":[
            {"type":"function_call","call_id":"view_image:31","name":"view_image","arguments":"{}"},
            {"type":"function_call","call_id":"view_image:32","name":"view_image","arguments":"{}"},
            {"type":"function_call_output","call_id":"view_image:31","output":"ok31"},
            {"type":"message","role":"developer","content":"keep following instructions"},
            {"type":"function_call_output","call_id":"view_image:32","output":"ok32"}
        ]
    }"#;
    let request = ExecutorRequest {
        auth_provider: "kimi".into(),
        model: "kimi-k3".into(),
        source_format: "openai-response".into(),
        format: "openai-response".into(),
        payload: payload.to_vec(),
        original_request: payload.to_vec(),
        auth_metadata: BTreeMap::from([("access_token".into(), json!("oauth-token"))]),
        http_client: Some(http.clone()),
        ..ExecutorRequest::default()
    };
    executor.execute(request).await.unwrap();
    let upstream = http.requests.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        upstream.url,
        "https://api.kimi.com/coding/v1/chat/completions"
    );
    let body: Value = serde_json::from_slice(&upstream.body).unwrap();
    let input = body["input"].as_array().expect("responses input survives");
    assert_eq!(input[2]["type"], "function_call_output");
    assert_eq!(input[2]["call_id"], "view_image:31");
    assert_eq!(input[3]["type"], "function_call_output");
    assert_eq!(input[3]["call_id"], "view_image:32");
    assert_eq!(input[4]["role"], "developer");
}

fn normalize(body: &[u8]) -> Value {
    serde_json::from_slice(&normalize_kimi_tool_message_links(body).unwrap()).unwrap()
}

#[test]
fn tool_links_use_call_id_fallback() {
    let value = normalize(br#"{"messages":[{"role":"assistant","tool_calls":[{"id":"list_directory:1"}]},{"role":"tool","call_id":"list_directory:1","content":"[]"}]}"#);
    assert_eq!(value["messages"][1]["tool_call_id"], "list_directory:1");
}

#[test]
fn tool_links_infer_single_pending_id() {
    let value = normalize(br#"{"messages":[{"role":"assistant","tool_calls":[{"id":"call_123"}]},{"role":"tool","content":"file"}]}"#);
    assert_eq!(value["messages"][1]["tool_call_id"], "call_123");
}

#[test]
fn tool_links_do_not_infer_ambiguous_missing_id() {
    let value = normalize(br#"{"messages":[{"role":"assistant","tool_calls":[{"id":"call_1"},{"id":"call_2"}]},{"role":"tool","content":"file"}]}"#);
    assert!(value["messages"][1].get("tool_call_id").is_none());
}

#[test]
fn tool_links_preserve_existing_tool_call_id() {
    let value = normalize(br#"{"messages":[{"role":"assistant","tool_calls":[{"id":"call_1"}]},{"role":"tool","tool_call_id":"call_1","call_id":"different"}]}"#);
    assert_eq!(value["messages"][1]["tool_call_id"], "call_1");
}

#[test]
fn tool_links_inherit_previous_reasoning() {
    let value = normalize(br#"{"messages":[{"role":"assistant","content":"plan","reasoning_content":"previous reasoning"},{"role":"assistant","tool_calls":[{"id":"call_1"}]}]}"#);
    assert_eq!(
        value["messages"][1]["reasoning_content"],
        "previous reasoning"
    );
}

#[test]
fn tool_links_insert_fallback_reasoning() {
    let value = normalize(br#"{"messages":[{"role":"assistant","tool_calls":[{"id":"call_1"}]}]}"#);
    assert_eq!(
        value["messages"][0]["reasoning_content"],
        "[reasoning unavailable]"
    );
}

#[test]
fn tool_links_use_content_as_reasoning_fallback() {
    let value = normalize(br#"{"messages":[{"role":"assistant","content":[{"type":"text","text":"first line"},{"type":"text","text":"second line"}],"tool_calls":[{"id":"call_1"}]}]}"#);
    assert_eq!(
        value["messages"][0]["reasoning_content"],
        "first line\nsecond line"
    );
}

#[test]
fn tool_links_replace_empty_reasoning_content() {
    let value = normalize(br#"{"messages":[{"role":"assistant","content":"assistant summary","tool_calls":[{"id":"call_1"}],"reasoning_content":""}]}"#);
    assert_eq!(
        value["messages"][0]["reasoning_content"],
        "assistant summary"
    );
}

#[test]
fn tool_links_preserve_existing_reasoning() {
    let value = normalize(br#"{"messages":[{"role":"assistant","tool_calls":[{"id":"call_1"}],"reasoning_content":"keep me"}]}"#);
    assert_eq!(value["messages"][0]["reasoning_content"], "keep me");
}

#[test]
fn tool_links_repair_ids_and_reasoning_together() {
    let value = normalize(br#"{"messages":[{"role":"assistant","tool_calls":[{"id":"call_1"}],"reasoning_content":"r1"},{"role":"tool","call_id":"call_1"},{"role":"assistant","tool_calls":[{"id":"call_2"}]},{"role":"tool","call_id":"call_2"}]}"#);
    assert_eq!(value["messages"][1]["tool_call_id"], "call_1");
    assert_eq!(value["messages"][2]["reasoning_content"], "r1");
    assert_eq!(value["messages"][3]["tool_call_id"], "call_2");
}

#[test]
fn drops_empty_assistant_without_tool_link() {
    let value = normalize(br#"{"messages":[{"role":"user","content":"start"},{"role":"assistant","content":""},{"role":"assistant","content":"   "},{"role":"assistant","content":"","tool_calls":null},{"role":"assistant","content":[{"type":"text","text":"  "}]},{"role":"assistant"},{"role":"assistant","content":"keep"},{"role":"user","content":"next"}]}"#);
    let messages = value["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["content"], "start");
    assert_eq!(messages[1]["content"], "keep");
    assert_eq!(messages[2]["content"], "next");
}

#[test]
fn preserves_assistant_with_tool_link_or_reasoning() {
    let value = normalize(br#"{"messages":[{"role":"assistant","content":"","tool_calls":[{"id":"call_1"}]},{"role":"assistant","content":"","function_call":{"name":"legacy"}},{"role":"assistant","content":"","reasoning_content":"thought"},{"role":"assistant","content":[{"type":"text","text":" visible "}]}]}"#);
    assert_eq!(value["messages"].as_array().unwrap().len(), 4);
}

#[test]
fn normalizes_upstream_model() {
    for (input, expected) in [
        ("kimi-k3[1m]", "k3"),
        ("kimi-k3", "k3"),
        ("Kimi-K3[1M]", "k3"),
        ("k3[1m]", "k3"),
        ("k3", "k3"),
        ("kimi-k2.6", "k2.6"),
        ("kimi-k2.6[1m]", "k2.6"),
        ("kimi-k3(1024)", "k3(1024)"),
        ("kimi-k3[1m](1024)", "k3(1024)"),
        ("kimi-k2.6(high)", "k2.6(high)"),
        ("kimi-k2.6[1m](high)", "k2.6(high)"),
    ] {
        assert_eq!(normalize_kimi_upstream_model(input), expected);
    }
}
