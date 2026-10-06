// ref: internal/runtime/executor/issue6381_responses_eof_test.go @ a2976eb8a303f11b4ea5177bce9f9ff752634dfc
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::sdk::pluginapi::{HttpResponse, HttpStreamChunk, HttpStreamResponse};
use serde_json::json;
use std::sync::Mutex;

struct V16EofClient(Mutex<Option<Vec<HttpStreamChunk>>>);
impl HostHttpClient for V16EofClient {
    fn execute<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { panic!("stream fixture must use streaming HTTP") })
    }

    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        let chunks = self.0.lock().unwrap().take().unwrap();
        Box::pin(async move {
            assert_eq!(request.url, "https://fixture.invalid/v1/chat/completions");
            assert!(chunks.len() <= 32);
            let (sender, receiver) = mpsc::channel(32);
            for chunk in chunks {
                sender.send(chunk).await.unwrap();
            }
            drop(sender);
            Ok(HttpStreamResponse {
                status_code: 200,
                headers: Default::default(),
                chunks: receiver,
            })
        })
    }
}

fn v16_eof_data(value: Value) -> HttpStreamChunk {
    HttpStreamChunk {
        payload: format!("data: {value}\n\n").into_bytes(),
        error: None,
    }
}

fn v16_eof_choice(delta: Value, finish_reason: Value) -> HttpStreamChunk {
    v16_eof_data(json!({
        "id":"chatcmpl_eof","object":"chat.completion.chunk","model":"test","created":1773896263,
        "choices":[{"index":0,"delta":delta,"finish_reason":finish_reason}]
    }))
}

async fn v16_eof_run(chunks: Vec<HttpStreamChunk>, tools: Value) -> (Vec<Value>, Vec<String>) {
    let registry = Arc::new(Registry::new());
    crate::internal::translator::openai::openai::responses::register_openai_responses_chat_completions(&registry);
    let executor =
        OpenAiCompatExecutor::new("fixture", Arc::new(OpenAiCompatConfig::default()), registry);
    let payload = serde_json::to_vec(&json!({
        "model":"test","input":"hello","stream":true,"tools":tools
    }))
    .unwrap();
    let request = ExecutorRequest {
        model: "test".into(),
        source_format: "openai-response".into(),
        stream: true,
        payload: payload.clone(),
        original_request: payload,
        auth_attributes: BTreeMap::from([("base_url".into(), "https://fixture.invalid/v1".into())]),
        http_client: Some(Arc::new(V16EofClient(Mutex::new(Some(chunks))))),
        ..ExecutorRequest::default()
    };
    let mut response = executor.execute_stream(request).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut events = Vec::new();
        let mut errors = Vec::new();
        while let Some(chunk) = response.chunks.recv().await {
            if let Some(error) = chunk.error {
                errors.push(error.to_string());
            }
            for line in std::str::from_utf8(&chunk.payload).unwrap().lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    events.push(serde_json::from_str(data).unwrap());
                }
            }
        }
        (events, errors)
    })
    .await
    .expect("owned executor stream must terminate")
}

fn v16_eof_completed(events: &[Value]) -> &Value {
    let completed: Vec<&Value> = events
        .iter()
        .filter(|event| event["type"] == "response.completed")
        .collect();
    assert_eq!(completed.len(), 1);
    completed[0]
}

#[tokio::test]
async fn candidate_v16_openai_eof_finish_reason_preserves_late_usage() {
    for first_chunk_finishes in [false, true] {
        let mut chunks = vec![v16_eof_choice(
            json!({"role":"assistant","content":"done"}),
            if first_chunk_finishes {
                json!("stop")
            } else {
                Value::Null
            },
        )];
        if !first_chunk_finishes {
            chunks.push(v16_eof_choice(json!({}), json!("stop")));
        }
        chunks.push(v16_eof_data(json!({
            "id":"chatcmpl_eof","object":"chat.completion.chunk","choices":[],
            "usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4,
                "prompt_tokens_details":{"cached_tokens":2}}
        })));
        let (events, errors) = v16_eof_run(chunks, json!([])).await;
        assert!(errors.is_empty(), "{errors:?}");
        let completed = v16_eof_completed(&events);
        assert_eq!(completed["response"]["usage"]["input_tokens"], 3);
        assert_eq!(completed["response"]["usage"]["output_tokens"], 1);
        assert_eq!(
            completed["response"]["usage"]["input_tokens_details"]["cached_tokens"],
            2
        );
        assert_eq!(
            completed["response"]["output"][0]["content"][0]["text"],
            "done"
        );
    }
}

#[tokio::test]
async fn candidate_v16_openai_eof_finished_apply_patch_keeps_custom_tool() {
    let patch = "*** Begin Patch\n*** Add File: hello.txt\n+hello\n*** End Patch";
    let arguments = json!({"input":patch}).to_string();
    let chunks = vec![
        v16_eof_choice(
            json!({"tool_calls":[{
                "index":0,"id":"call_patch","type":"function",
                "function":{"name":"apply_patch","arguments":arguments}
            }]}),
            Value::Null,
        ),
        v16_eof_choice(json!({}), json!("tool_calls")),
    ];
    let (events, errors) =
        v16_eof_run(chunks, json!([{"type":"custom","name":"apply_patch"}])).await;
    assert!(errors.is_empty(), "{errors:?}");
    let completed = v16_eof_completed(&events);
    assert_eq!(
        completed["response"]["output"][0]["type"],
        "custom_tool_call"
    );
    assert_eq!(completed["response"]["output"][0]["name"], "apply_patch");
    assert_eq!(completed["response"]["output"][0]["input"], patch);
}

#[tokio::test]
async fn candidate_v16_openai_eof_read_error_after_finish_does_not_complete() {
    let (events, errors) = v16_eof_run(
        vec![
            v16_eof_choice(json!({"role":"assistant","content":"done"}), json!("stop")),
            HttpStreamChunk {
                payload: Vec::new(),
                error: Some(Arc::new(OpenAiCompatError::status(
                    502,
                    "synthetic upstream read error",
                ))),
            },
        ],
        json!([]),
    )
    .await;
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("synthetic upstream read error"));
    assert!(events
        .iter()
        .any(|event| event["type"] == "response.output_text.delta"));
    assert!(!events
        .iter()
        .any(|event| event["type"] == "response.completed"));
}

#[tokio::test]
async fn candidate_v16_openai_eof_partial_without_finish_is_failure() {
    let (events, errors) = v16_eof_run(
        vec![v16_eof_choice(
            json!({"role":"assistant","content":"partial"}),
            Value::Null,
        )],
        json!([]),
    )
    .await;
    assert_eq!(errors.len(), 1);
    assert!(events
        .iter()
        .any(|event| event["type"] == "response.output_text.delta" && event["delta"] == "partial"));
    assert!(!events
        .iter()
        .any(|event| event["type"] == "response.completed"));
}

#[tokio::test]
async fn candidate_v16_openai_eof_finish_without_output_does_not_complete() {
    let (events, errors) =
        v16_eof_run(vec![v16_eof_choice(json!({}), json!("stop"))], json!([])).await;
    assert_eq!(errors.len(), 1);
    assert!(!events
        .iter()
        .any(|event| event["type"] == "response.completed"));
}

#[tokio::test]
async fn candidate_v16_openai_eof_source_done_owns_terminal_delivery() {
    let (events, errors) = v16_eof_run(
        vec![
            v16_eof_choice(json!({"role":"assistant","content":"done"}), json!("stop")),
            HttpStreamChunk {
                payload: b"data: [DONE]\n\n".to_vec(),
                error: None,
            },
            HttpStreamChunk {
                payload: Vec::new(),
                error: Some(Arc::new(OpenAiCompatError::status(
                    502,
                    "error after source terminator",
                ))),
            },
        ],
        json!([]),
    )
    .await;
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        v16_eof_completed(&events)["response"]["output"][0]["content"][0]["text"],
        "done"
    );
}
