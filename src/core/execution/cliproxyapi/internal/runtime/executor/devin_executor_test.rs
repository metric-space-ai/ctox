// ref: internal/runtime/executor/devin_executor.go:230-387,464-583,990-1123,2465-2482
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::super::helps::devin_wire::{
    wrap_connect_envelope, wrap_connect_envelope_with_flag, CONNECT_FLAG_END_STREAM,
};
use super::*;
use crate::sdk::pluginapi::{HostHttpClient, HttpResponse, HttpStreamResponse};
use crate::sdk::translator::ResponseTransform;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

struct Context {
    calls: AtomicUsize,
}
impl DevinAttemptContextProvider for Context {
    fn for_request(
        &self,
        _: &ExecutorRequest,
    ) -> Result<DevinAttemptContext, PluginExecutionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(DevinAttemptContext {
            canonical_session_id: "owned-session".into(),
            usage: None,
        })
    }
}
struct Transport {
    status: u16,
    headers: Headers,
    receiver: Mutex<Option<mpsc::Receiver<HttpStreamChunk>>>,
    requests: Mutex<Vec<HttpRequest>>,
    unary_calls: AtomicUsize,
}
impl HostHttpClient for Transport {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.unary_calls.fetch_add(1, Ordering::SeqCst);
            self.requests.lock().unwrap().push(request);
            Ok(HttpResponse {
                status_code: self.status,
                headers: self.headers.clone(),
                body: b"native-http-response".to_vec(),
            })
        })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let chunks = self
                .receiver
                .lock()
                .unwrap()
                .take()
                .expect("one owned attempt");
            Ok(HttpStreamResponse {
                status_code: self.status,
                headers: self.headers.clone(),
                chunks,
            })
        })
    }
}
struct Fixture {
    executor: Arc<DevinExecutor>,
    transport: Arc<Transport>,
    sender: mpsc::Sender<HttpStreamChunk>,
    context: Arc<Context>,
    registry: Arc<Registry>,
}
impl Fixture {
    fn new(status: u16, headers: Headers) -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let registry = Arc::new(Registry::new());
        let context = Arc::new(Context {
            calls: AtomicUsize::new(0),
        });
        let transport = Arc::new(Transport {
            status,
            headers,
            receiver: Mutex::new(Some(receiver)),
            requests: Mutex::new(Vec::new()),
            unary_calls: AtomicUsize::new(0),
        });
        let executor = Arc::new(DevinExecutor::new(
            registry.clone(),
            Arc::new(DevinModelsStore::default()),
            Arc::new(StaticModelsCatalog::default()),
            Arc::new(DevinSessionTurns::default()),
            context.clone(),
        ));
        Self {
            executor,
            transport,
            sender,
            context,
            registry,
        }
    }
    fn request(&self) -> ExecutorRequest {
        ExecutorRequest {
            auth_id: "private-test-only-account".into(),
            model: "swe-2".into(),
            source_format: "interactions".into(),
            format: "interactions".into(),
            auth_attributes: BTreeMap::from([("api_key".into(), "test-only-token".into())]),
            payload: br#"{"input":[{"type":"user_input","content":"test-only prompt"}]}"#.to_vec(),
            http_client: Some(self.transport.clone()),
            ..ExecutorRequest::default()
        }
    }
    async fn send(&self, payload: Vec<u8>) {
        self.sender
            .send(HttpStreamChunk {
                payload,
                error: None,
            })
            .await
            .unwrap();
    }
}
fn text_frame(text: &str) -> Vec<u8> {
    assert!(text.len() < 128);
    let mut proto = vec![26, text.len() as u8];
    proto.extend_from_slice(text.as_bytes());
    wrap_connect_envelope(&proto).unwrap()
}
fn eos() -> Vec<u8> {
    wrap_connect_envelope_with_flag(CONNECT_FLAG_END_STREAM, b"{}").unwrap()
}
fn failed_eos() -> Vec<u8> {
    wrap_connect_envelope_with_flag(
        CONNECT_FLAG_END_STREAM,
        br#"{"error":{"code":"unauthenticated","message":"test-only rejection"}}"#,
    )
    .unwrap()
}
async fn collect(response: &mut ExecutorStreamResponse) -> (Vec<u8>, Vec<PluginExecutionError>) {
    let mut bytes = Vec::new();
    let mut errors = Vec::new();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(1), response.chunks.recv())
        .await
        .expect("owned stream must terminate")
    {
        bytes.extend_from_slice(&chunk.payload);
        if let Some(error) = chunk.error {
            errors.push(error);
        }
    }
    (bytes, errors)
}

#[tokio::test]
async fn candidate_devin_transport_unary_reads_incrementally_and_stops_at_eos() {
    let fixture = Fixture::new(
        200,
        BTreeMap::from([("X-Test-Upstream".into(), vec!["retained".into()])]),
    );
    let mut payload = text_frame("hello");
    payload.extend(eos());
    payload.extend_from_slice(&[0xff, 0, 0, 0, 0]); // ignored after the first valid EOS
    fixture.send(payload[..2].to_vec()).await;
    fixture.send(payload[2..].to_vec()).await;
    let response = tokio::time::timeout(
        Duration::from_secs(1),
        fixture.executor.execute(fixture.request()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&response.payload).unwrap()["steps"][0]["content"][0]
            ["text"],
        "hello"
    );
    assert_eq!(response.headers["X-Test-Upstream"], vec!["retained"]);
    assert_eq!(fixture.context.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.transport.unary_calls.load(Ordering::SeqCst), 0);
    tokio::time::timeout(Duration::from_secs(1), fixture.sender.closed())
        .await
        .unwrap();
    let requests = fixture.transport.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(
        requests[0].headers["Authorization"],
        vec!["Basic test-only-token-test-only-token"]
    );
}

#[tokio::test]
async fn candidate_devin_transport_unary_eof_never_invokes_success_translation() {
    let fixture = Fixture::new(200, Headers::new());
    let translated = Arc::new(AtomicUsize::new(0));
    let calls = translated.clone();
    fixture.registry.register(
        Format::from("interactions"),
        Format::from("interactions"),
        None,
        ResponseTransform {
            non_stream: Some(Arc::new(move |_, _, _, _, body, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                body.to_vec()
            })),
            ..ResponseTransform::default()
        },
    );
    fixture.send(text_frame("partial")).await;
    let request = fixture.request();
    drop(fixture.sender);
    let error = fixture.executor.execute(request).await.unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<DevinExecutorError>()
            .unwrap()
            .status_code(),
        502
    );
    assert_eq!(translated.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn candidate_devin_transport_stream_is_visible_before_eos_and_drop_closes_upstream() {
    let fixture = Fixture::new(200, Headers::new());
    let mut response = fixture
        .executor
        .execute_stream(fixture.request())
        .await
        .unwrap();
    let text = text_frame("visible");
    fixture.send(text[..3].to_vec()).await;
    fixture.send(text[3..].to_vec()).await;
    let mut output = Vec::new();
    for _ in 0..3 {
        output.extend(
            tokio::time::timeout(Duration::from_secs(1), response.chunks.recv())
                .await
                .unwrap()
                .unwrap()
                .payload,
        );
    }
    assert!(String::from_utf8_lossy(&output).contains("visible"));
    assert!(!String::from_utf8_lossy(&output).contains("[DONE]"));
    assert!(!fixture.sender.is_closed());
    drop(response);
    tokio::time::timeout(Duration::from_secs(1), fixture.sender.closed())
        .await
        .unwrap();
}

#[tokio::test]
async fn candidate_devin_transport_trailer_failure_preserves_prefix_without_done() {
    let fixture = Fixture::new(200, Headers::new());
    let mut request = fixture.request();
    request.format = "openai".into();
    // A minimal real Registry hook preserves SSE framing while exposing every
    // intermediate event to this transport regression.
    fixture.registry.register(
        Format::from("openai"),
        Format::from("interactions"),
        None,
        ResponseTransform {
            stream: Some(Arc::new(|_, _, _, _, body, _| {
                vec![format!("data: {}\n\n", String::from_utf8_lossy(body)).into_bytes()]
            })),
            ..ResponseTransform::default()
        },
    );
    let mut response = fixture.executor.execute_stream(request).await.unwrap();
    let mut payload = text_frame("before failure");
    payload.extend(failed_eos());
    fixture.send(payload).await;
    let (bytes, errors) = collect(&mut response).await;
    let output = String::from_utf8_lossy(&bytes);
    assert!(output.contains("before failure") && output.contains("response.failed"));
    assert!(!output.contains("[DONE]") && !output.contains("interaction.completed"));
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0]
            .downcast_ref::<DevinExecutorError>()
            .unwrap()
            .status_code(),
        401
    );
    tokio::time::timeout(Duration::from_secs(1), fixture.sender.closed())
        .await
        .unwrap();
}

#[tokio::test]
async fn candidate_devin_transport_framing_failure_and_eof_do_not_become_success() {
    for malformed in [false, true] {
        let fixture = Fixture::new(200, Headers::new());
        let mut request = fixture.request();
        request.format = "openai".into();
        fixture.registry.register(
            Format::from("openai"),
            Format::from("interactions"),
            None,
            ResponseTransform {
                stream: Some(Arc::new(|_, _, _, _, body, _| {
                    vec![format!("data: {}\n\n", String::from_utf8_lossy(body)).into_bytes()]
                })),
                ..ResponseTransform::default()
            },
        );
        let mut response = fixture.executor.execute_stream(request).await.unwrap();
        let mut payload = text_frame("retained prefix");
        if malformed {
            payload.extend_from_slice(&[0xff, 0, 0, 0, 0]);
        }
        fixture.send(payload).await;
        drop(fixture.sender);
        let (bytes, errors) = collect(&mut response).await;
        let output = String::from_utf8_lossy(&bytes);
        assert!(output.contains("retained prefix") && output.contains("response.failed"));
        assert!(!output.contains("[DONE]") && !output.contains("interaction.completed"));
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0]
                .downcast_ref::<DevinExecutorError>()
                .unwrap()
                .status_code(),
            502
        );
    }
}

#[tokio::test]
async fn candidate_devin_transport_http_status_body_is_bounded_and_retry_is_429_only() {
    for status in [429, 403] {
        let fixture = Fixture::new(
            status,
            BTreeMap::from([("retry-after".into(), vec!["3".into()])]),
        );
        let sender = fixture.sender.clone();
        let producer = tokio::spawn(async move {
            sender
                .send(HttpStreamChunk {
                    payload: vec![b'x'; MAX_ERROR_BODY + 1],
                    error: None,
                })
                .await
                .unwrap();
            sender.closed().await;
        });
        let error = match fixture.executor.execute_stream(fixture.request()).await {
            Ok(_) => panic!("HTTP error must fail before stream construction"),
            Err(error) => error,
        };
        let status_error = error.downcast_ref::<DevinHttpStatusError>().unwrap();
        assert_eq!(status_error.status_code, status);
        assert_eq!(status_error.body.len(), MAX_ERROR_BODY);
        assert_eq!(
            status_error.retry_after,
            (status == 429).then_some(Duration::from_secs(3))
        );
        assert!(format!("{status_error:?}").len() < 200);
        tokio::time::timeout(Duration::from_secs(1), producer)
            .await
            .unwrap()
            .unwrap();
    }
}

#[test]
fn candidate_devin_transport_retry_after_accepts_http_dates_and_integer_seconds() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(784_111_776);
    for date in [
        "Sun, 06 Nov 1994 08:49:37 GMT",
        "Sunday, 06-Nov-94 08:49:37 GMT",
        "Sun Nov  6 08:49:37 1994",
    ] {
        let headers = BTreeMap::from([("Retry-After".into(), vec![date.into()])]);
        assert_eq!(
            devin_retry_after(429, &headers, now),
            Some(Duration::from_secs(1))
        );
        assert_eq!(devin_retry_after(503, &headers, now), None);
    }
    for (raw, expected) in [
        (" 0 ", Some(0)),
        ("+3", Some(3)),
        ("3.5", None),
        ("-1", None),
        ("9223372036854775808", None),
        ("bad", None),
    ] {
        let headers = BTreeMap::from([("Retry-After".into(), vec![raw.into()])]);
        assert_eq!(
            devin_retry_after(429, &headers, now),
            expected.map(Duration::from_secs)
        );
    }
}

#[tokio::test]
async fn candidate_devin_transport_native_http_and_count_tokens_preserve_their_contracts() {
    let fixture = Fixture::new(200, Headers::new());
    let mut request = fixture.request();
    request.payload = vec![b'x'; 15];
    let count = fixture.executor.count_tokens(request).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&count.payload).unwrap(),
        serde_json::json!({"total_tokens":3,"input_tokens":3})
    );
    assert_eq!(fixture.context.calls.load(Ordering::SeqCst), 0);
    let response = fixture
        .executor
        .http_request(ExecutorHttpRequest {
            method: "POST".into(),
            url: "https://test.invalid/GetUserStatus".into(),
            attributes: BTreeMap::from([("api_key".into(), "test-only-token".into())]),
            http_client: Some(fixture.transport.clone()),
            ..ExecutorHttpRequest::default()
        })
        .await
        .unwrap();
    assert_eq!(response.body, b"native-http-response");
    assert_eq!(fixture.transport.unary_calls.load(Ordering::SeqCst), 1);
    let requests = fixture.transport.requests.lock().unwrap();
    assert_eq!(
        requests[0].headers["Authorization"],
        vec!["Basic test-only-token-test-only-token"]
    );
    assert!(!requests[0].headers.contains_key("Sentry-Trace"));
}

#[tokio::test]
async fn candidate_devin_transport_patch_failure_prevents_positive_terminal_and_done() {
    let fixture = Fixture::new(200, Headers::new());
    fixture.registry.register(Format::from("openai-response"), Format::from("interactions"), None,
        ResponseTransform { stream: Some(Arc::new(|_, _, _, _, body, _| {
            if is_interaction_event(body, "interaction.created") {
                return vec![br#"data: {"type":"response.created","response":{"id":"r"}}

"#.to_vec()];
            }
            if is_interaction_event(body, "interaction.completed") {
                return vec![br#"data: {"type":"response.completed","response":{"id":"r","output":[{"type":"function_call","id":"item","call_id":"call","name":"apply_patch","arguments":"invalid"}]}}

"#.to_vec()];
            }
            Vec::new()
        })), ..ResponseTransform::default() });
    let mut request = fixture.request();
    request.format = "openai-response".into();
    request.source_format = "openai-response".into();
    request.original_request = br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#.to_vec();
    let mut response = fixture.executor.execute_stream(request).await.unwrap();
    fixture.send(eos()).await;
    let (bytes, errors) = collect(&mut response).await;
    let output = String::from_utf8_lossy(&bytes);
    assert!(output.contains("response.failed"));
    assert!(!output.contains("response.completed") && !output.contains("[DONE]"));
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0]
            .downcast_ref::<DevinExecutorError>()
            .unwrap()
            .status_code(),
        502
    );
}

#[tokio::test]
async fn candidate_devin_transport_unary_patch_uses_original_declaration_and_rejects_empty_translation(
) {
    for empty in [false, true] {
        let fixture = Fixture::new(200, Headers::new());
        fixture.registry.register(Format::from("openai-response"), Format::from("interactions"), None,
            ResponseTransform { non_stream: Some(Arc::new(move |_, _, original, _, _, _| {
                assert!(String::from_utf8_lossy(original).contains(r#""type":"custom""#));
                if empty { Vec::new() } else {
                    br#"{"id":"r","object":"response","status":"completed","output":[{"type":"function_call","id":"item","call_id":"call","name":"apply_patch","arguments":"{\"input\":\"patch-text\"}"}]}"#.to_vec()
                }
            })), ..ResponseTransform::default() });
        let mut request = fixture.request();
        request.format = "openai-response".into();
        request.source_format = "openai-response".into();
        request.original_request =
            br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#.to_vec();
        fixture.send(eos()).await;
        let result = fixture.executor.execute(request).await;
        if empty {
            assert_eq!(
                result
                    .unwrap_err()
                    .downcast_ref::<DevinExecutorError>()
                    .unwrap()
                    .status_code(),
                502
            );
        } else {
            let body: Value = serde_json::from_slice(&result.unwrap().payload).unwrap();
            assert_eq!(body["output"][0]["type"], "custom_tool_call");
            assert_eq!(body["output"][0]["input"], "patch-text");
        }
    }
}
