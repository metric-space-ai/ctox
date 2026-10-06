// ref: sdk/api/handlers/openai/openai_handlers_stream_truncation_test.go
// @ a4acc9f752bd46571f737a10c04bf413656ab06b
// Port-Status: candidate — real CTOX HTTP route/owner guards
// License: MIT (upstream); modifications AGPL-3.0-only

#[derive(Default)]
struct CandidateOpenAiCalls(Mutex<Vec<(Option<String>, BTreeMap<String, Vec<String>>, Value)>>);

struct CandidateOpenAiResponse {
    response: OpenAiResponsesHttpResponse,
    calls: CandidateOpenAiCalls,
}

impl CandidateOpenAiResponse {
    fn stream(events: &[Value]) -> Self {
        let body = events
            .iter()
            .flat_map(|event| {
                let mut chunk = b"data: ".to_vec();
                chunk.extend(serde_json::to_vec(event).unwrap());
                chunk.extend_from_slice(b"\n\n");
                chunk
            })
            .collect();
        Self {
            response: OpenAiResponsesHttpResponse::event_stream(200, body),
            calls: CandidateOpenAiCalls::default(),
        }
    }
}

impl OpenAiResponsesRouteHandler for CandidateOpenAiResponse {
    fn handle_provider_route<'a>(
        &'a self,
        provider: Option<&'a str>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            let headers = BTreeMap::new();
            self.handle_provider_route_with_headers(provider, &headers, body)
                .await
        })
    }

    fn handle_provider_route_with_headers<'a>(
        &'a self,
        provider: Option<&'a str>,
        headers: &'a BTreeMap<String, Vec<String>>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        self.calls.0.lock().unwrap().push((
            provider.map(str::to_owned),
            headers.clone(),
            serde_json::from_slice(body).unwrap(),
        ));
        Box::pin(async { OpenAiResponsesRouteResponse::Buffered(self.response.clone()) })
    }
}

/// Both TCP peers and the real provider dispatch are awaited inside one bounded
/// owner. No detached server, background producer or credential mutation.
async fn candidate_openai_wire<H: OpenAiResponsesRouteHandler + ?Sized>(
    handler: &H,
    path: &str,
    body: &[u8],
    method: &str,
) -> String {
    candidate_openai_wire_for_provider(handler, path, body, method, "antigravity").await
}

async fn candidate_openai_wire_for_provider<H: OpenAiResponsesRouteHandler + ?Sized>(
    handler: &H,
    path: &str,
    body: &[u8],
    method: &str,
    provider: &str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let logs = std::env::temp_dir().join(format!("openai-route-{}", uuid::Uuid::new_v4()));
    let policy = RequestLoggingPolicy::error_only(&logs, 2);
    let models = ClaudeMessagesHttpResponse::error(404, "unused models");
    let server = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        serve_provider_connection_with_logging(
            &mut socket,
            handler,
            None::<&ClaudeMessagesAntigravityHandler>,
            &models,
            &policy,
        )
        .await
        .unwrap();
    };
    let client = async {
        let mut socket = TcpStream::connect(address).await.unwrap();
        socket.write_all(format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nX-CTOX-Provider: {provider}\r\nX-Session-Id: affinity-a\r\nContent-Length: {}\r\n\r\n",
            body.len(),
        ).as_bytes()).await.unwrap();
        socket.write_all(body).await.unwrap();
        let mut bytes = Vec::new();
        socket.read_to_end(&mut bytes).await.unwrap();
        String::from_utf8(bytes).unwrap()
    };
    let (_, result) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(server, client)
    })
    .await
    .expect("public OpenAI route must finish within bounded fixture deadline");
    if logs.exists() {
        fs::remove_dir_all(logs).unwrap();
    }
    result
}

fn candidate_openai_chunk(text: &str, finish: Option<Value>) -> Value {
    let mut choice = serde_json::json!({"index":0,"delta":{"content":text}});
    if let Some(finish) = finish {
        choice["finish_reason"] = finish;
    }
    serde_json::json!({"id":"chat-a","object":"chat.completion.chunk","model":"gemini-3-flash-agent","choices":[choice]})
}

fn candidate_openai_body(completions: bool) -> Vec<u8> {
    serde_json::to_vec(&if completions {
        serde_json::json!({"model":"gemini-3-flash-agent","stream":true,"prompt":"hello"})
    } else {
        serde_json::json!({"model":"gemini-3-flash-agent","stream":true,"messages":[{"role":"user","content":"hello"}]})
    }).unwrap()
}

fn candidate_openai_assert_truncated(wire: &str) {
    assert!(wire.starts_with("HTTP/1.1 200 OK\r\n"), "{wire}");
    assert!(
        wire.contains("upstream stream closed before any chunk carried finish_reason"),
        "{wire}"
    );
    assert!(!wire.contains("data: [DONE]"), "{wire}");
    assert_eq!(wire.matches("\"code\":502").count(), 1, "{wire}");
}

#[tokio::test]
async fn candidate_openai_chat_without_finish_reason_is_error() {
    let handler = CandidateOpenAiResponse::stream(&[
        candidate_openai_chunk("hello", None),
        candidate_openai_chunk(" world", None),
    ]);
    let wire = candidate_openai_wire(
        &handler,
        "/v1/chat/completions",
        &candidate_openai_body(false),
        "POST",
    )
    .await;
    candidate_openai_assert_truncated(&wire);
    assert!(wire.find("hello").unwrap() < wire.find("world").unwrap());
}

#[tokio::test]
async fn candidate_openai_chat_null_or_empty_finish_reason_is_error() {
    for finish in [Value::Null, Value::String(String::new())] {
        let handler =
            CandidateOpenAiResponse::stream(&[candidate_openai_chunk("partial", Some(finish))]);
        let wire = candidate_openai_wire(
            &handler,
            "/v1/chat/completions",
            &candidate_openai_body(false),
            "POST",
        )
        .await;
        candidate_openai_assert_truncated(&wire);
    }
}

#[tokio::test]
async fn candidate_openai_chat_single_chunk_without_finish_reason_is_error() {
    let handler = CandidateOpenAiResponse::stream(&[candidate_openai_chunk("partial", None)]);
    let wire = candidate_openai_wire(
        &handler,
        "/v1/chat/completions",
        &candidate_openai_body(false),
        "POST",
    )
    .await;
    candidate_openai_assert_truncated(&wire);
}

#[tokio::test]
async fn candidate_openai_chat_later_finish_reason_completes() {
    let handler = CandidateOpenAiResponse::stream(&[
        candidate_openai_chunk("hello", None),
        candidate_openai_chunk("", Some(serde_json::json!("stop"))),
    ]);
    let wire = candidate_openai_wire(
        &handler,
        "/v1/chat/completions",
        &candidate_openai_body(false),
        "POST",
    )
    .await;
    assert_eq!(wire.matches("data: [DONE]").count(), 1, "{wire}");
    assert!(!wire.contains("\"error\""), "{wire}");
}

#[tokio::test]
async fn candidate_openai_chat_first_finish_reason_completes() {
    let handler = CandidateOpenAiResponse::stream(&[candidate_openai_chunk(
        "hello",
        Some(serde_json::json!("stop")),
    )]);
    let wire = candidate_openai_wire(
        &handler,
        "/v1/chat/completions",
        &candidate_openai_body(false),
        "POST",
    )
    .await;
    assert_eq!(wire.matches("data: [DONE]").count(), 1, "{wire}");
    assert!(!wire.contains("\"error\""), "{wire}");
}

#[tokio::test]
async fn candidate_openai_chat_completions_without_finish_reason_is_error() {
    let handler = CandidateOpenAiResponse::stream(&[candidate_openai_chunk("partial", None)]);
    let wire = candidate_openai_wire(
        &handler,
        "/v1/completions",
        &candidate_openai_body(true),
        "POST",
    )
    .await;
    candidate_openai_assert_truncated(&wire);
    assert!(wire.contains("\"text\":\"partial\""), "{wire}");
    assert!(!wire.contains("\"delta\""), "{wire}");
}

#[tokio::test]
async fn candidate_openai_chat_completions_later_finish_reason_completes() {
    let handler = CandidateOpenAiResponse::stream(&[
        candidate_openai_chunk("hello", None),
        candidate_openai_chunk("", Some(serde_json::json!("stop"))),
    ]);
    let wire = candidate_openai_wire(
        &handler,
        "/v1/completions",
        &candidate_openai_body(true),
        "POST",
    )
    .await;
    assert_eq!(wire.matches("data: [DONE]").count(), 1, "{wire}");
    assert!(wire.contains("\"object\":\"text_completion\""), "{wire}");
    assert!(!wire.contains("\"error\""), "{wire}");
}

#[tokio::test]
async fn candidate_openai_chat_usage_after_finish_reason_completes() {
    for completions in [false, true] {
        let handler = CandidateOpenAiResponse::stream(&[
            candidate_openai_chunk("hello", Some(serde_json::json!("stop"))),
            serde_json::json!({"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}),
        ]);
        let path = if completions {
            "/v1/completions"
        } else {
            "/v1/chat/completions"
        };
        let wire =
            candidate_openai_wire(&handler, path, &candidate_openai_body(completions), "POST")
                .await;
        assert_eq!(wire.matches("data: [DONE]").count(), 1, "{wire}");
        assert!(wire.find("\"usage\"").unwrap() < wire.find("data: [DONE]").unwrap());
        assert!(wire.contains("\"total_tokens\":15"), "{wire}");
        assert!(!wire.contains("\"error\""), "{wire}");
    }
}

#[tokio::test]
async fn candidate_openai_chat_completions_first_finish_reason_completes() {
    let handler = CandidateOpenAiResponse::stream(&[candidate_openai_chunk(
        "hello",
        Some(serde_json::json!("stop")),
    )]);
    let wire = candidate_openai_wire(
        &handler,
        "/v1/completions",
        &candidate_openai_body(true),
        "POST",
    )
    .await;
    assert_eq!(wire.matches("data: [DONE]").count(), 1, "{wire}");
    assert!(!wire.contains("\"error\""), "{wire}");
}

#[tokio::test]
async fn candidate_openai_chat_eof_drains_undelimited_completion() {
    let mut handler = CandidateOpenAiResponse::stream(&[candidate_openai_chunk(
        "hello",
        Some(serde_json::json!("stop")),
    )]);
    let mut bytes = handler.response.body().to_vec();
    bytes.truncate(bytes.len() - 2);
    handler.response = OpenAiResponsesHttpResponse::event_stream(200, bytes);
    let wire = candidate_openai_wire(
        &handler,
        "/v1/chat/completions",
        &candidate_openai_body(false),
        "POST",
    )
    .await;
    assert_eq!(wire.matches("data: [DONE]").count(), 1, "{wire}");
    assert!(!wire.contains("\"error\""), "{wire}");
}

#[tokio::test]
async fn candidate_openai_chat_errors_stay_json_before_commit_or_terminal_after_output() {
    let error = serde_json::json!({"type":"response.failed","response":{"error":{"message":"private upstream body"}}});
    for events in [
        vec![error.clone()],
        vec![candidate_openai_chunk("partial", None), error],
    ] {
        let handler = CandidateOpenAiResponse::stream(&events);
        let wire = candidate_openai_wire(
            &handler,
            "/v1/chat/completions",
            &candidate_openai_body(false),
            "POST",
        )
        .await;
        assert!(
            wire.starts_with(if events.len() == 1 {
                "HTTP/1.1 502 Bad Gateway"
            } else {
                "HTTP/1.1 200 OK"
            }),
            "{wire}"
        );
        assert_eq!(wire.matches("\"error\"").count(), 1, "{wire}");
        assert!(
            !wire.contains("private upstream body") && !wire.contains("data: [DONE]"),
            "{wire}"
        );
    }
}

#[tokio::test]
async fn candidate_openai_chat_bad_method_and_body_do_not_select_accounts() {
    let handler = CandidateOpenAiResponse::stream(&[]);
    for (method, body, status) in [
        ("GET", b"{}".as_slice(), 405),
        ("POST", b"[]".as_slice(), 400),
        ("POST", b"{\"messages\":[]}".as_slice(), 400),
    ] {
        let wire = candidate_openai_wire(&handler, "/v1/chat/completions", body, method).await;
        assert!(wire.starts_with(&format!("HTTP/1.1 {status}")), "{wire}");
    }
    assert!(handler.calls.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn candidate_openai_chat_request_preserves_affinity_sampling_tools_and_headers() {
    let handler = CandidateOpenAiResponse::stream(&[candidate_openai_chunk(
        "done",
        Some(serde_json::json!("stop")),
    )]);
    let body = br#"{"model":"gemini-3-flash-agent","stream":true,"messages":[{"role":"user","content":"hello"}],"session_id":"sticky-a","metadata":{"trace":"private-trace"},"prompt_cache_key":"cache-a","max_completion_tokens":123,"temperature":0.25,"top_p":0.8,"parallel_tool_calls":false,"tools":[{"type":"function","function":{"name":"weather","parameters":{"type":"object"}}}],"stream_options":{"include_usage":true}}"#;
    let wire = candidate_openai_wire(&handler, "/v1/chat/completions", body, "POST").await;
    assert_eq!(wire.matches("data: [DONE]").count(), 1, "{wire}");
    let calls = handler.calls.0.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0.as_deref(), Some("antigravity"));
    assert_eq!(calls[0].1.get("X-Session-Id").unwrap(), &["affinity-a"]);
    let request = &calls[0].2;
    assert_eq!(request["session_id"], "sticky-a");
    assert_eq!(request["prompt_cache_key"], "cache-a");
    assert_eq!(request["max_output_tokens"], 123);
    assert_eq!(request["metadata"]["trace"], "private-trace");
    assert_eq!(request["temperature"], 0.25);
    assert_eq!(request["top_p"], 0.8);
    assert_eq!(request["parallel_tool_calls"], false);
    assert_eq!(request["tools"][0]["name"], "weather");
    assert_eq!(request["input"][0]["content"][0]["text"], "hello");
    assert!(!wire.contains("private-trace"));
}

#[tokio::test]
async fn candidate_openai_chat_canonical_responses_and_unary_completions_keep_usage() {
    for completions in [false, true] {
        let response = serde_json::json!({"id":"response-a","status":"completed","model":"gemini-3-flash-agent","output":[{"type":"message","content":[{"type":"output_text","text":"hello"}]}],"usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}});
        let handler = CandidateOpenAiResponse {
            response: OpenAiResponsesHttpResponse::json(
                200,
                serde_json::to_vec(&response).unwrap(),
            ),
            calls: CandidateOpenAiCalls::default(),
        };
        let body = if completions {
            br#"{"model":"gemini-3-flash-agent","prompt":"hello"}"#.as_slice()
        } else {
            br#"{"model":"gemini-3-flash-agent","input":"hello","metadata":{"trace":"kept"}}"#
                .as_slice()
        };
        let path = if completions {
            "/v1/completions"
        } else {
            "/v1/chat/completions"
        };
        let wire = candidate_openai_wire(&handler, path, body, "POST").await;
        assert!(wire.starts_with("HTTP/1.1 200 OK"), "{wire}");
        let payload: Value = serde_json::from_str(wire.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(payload["usage"]["total_tokens"], 15);
        assert_eq!(payload["choices"][0]["finish_reason"], "stop");
        assert_eq!(
            payload["object"],
            if completions {
                "text_completion"
            } else {
                "chat.completion"
            }
        );
        if !completions {
            assert_eq!(
                handler.calls.0.lock().unwrap()[0].2["metadata"]["trace"],
                "kept"
            );
        }
    }
}

#[tokio::test]
async fn candidate_openai_chat_antigravity_real_pool_translates_terminal_and_usage() {
    let (handler, transport) = antigravity_handler();
    let wire = candidate_openai_wire(
        handler.as_ref(),
        "/v1/chat/completions",
        &candidate_openai_body(false),
        "POST",
    )
    .await;
    assert!(wire.starts_with("HTTP/1.1 200 OK"), "{wire}");
    assert!(wire.contains("\"finish_reason\":\"stop\""), "{wire}");
    assert!(wire.contains("\"total_tokens\":5"), "{wire}");
    assert_eq!(wire.matches("data: [DONE]").count(), 1, "{wire}");
    assert!(!wire.contains("antigravity-access-secret"));
    assert_eq!(transport.0.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn candidate_openai_chat_codex_and_claude_real_pools_use_same_live_writer() {
    let (codex, _) = codex_handler();
    let (claude, _) = handler();
    let providers: [(&str, &str, &dyn OpenAiResponsesRouteHandler); 2] = [
        ("codex", "gpt-5", codex.as_ref()),
        ("claude", "claude-sonnet-4-5", claude.as_ref()),
    ];
    for (provider, model, handler) in providers {
        for completions in [false, true] {
            let body = serde_json::to_vec(&if completions {
                serde_json::json!({"model":model,"prompt":"hello","stream":true})
            } else {
                serde_json::json!({"model":model,"messages":[{"role":"user","content":"hello"}],"stream":true})
            }).unwrap();
            let path = if completions {
                "/v1/completions"
            } else {
                "/v1/chat/completions"
            };
            let wire =
                candidate_openai_wire_for_provider(handler, path, &body, "POST", provider).await;
            assert!(wire.starts_with("HTTP/1.1 200 OK"), "{provider}: {wire}");
            assert_eq!(
                wire.matches("data: [DONE]").count(),
                1,
                "{provider}: {wire}"
            );
            assert!(
                wire.contains("\"finish_reason\":\"stop\""),
                "{provider}: {wire}"
            );
            assert!(
                !wire.contains("\"error\"") && !wire.contains("access-secret"),
                "{provider}: {wire}"
            );
        }
        let rejected = candidate_openai_wire(
            handler,
            "/v1/chat/completions",
            &candidate_openai_body(false),
            "POST",
        )
        .await;
        assert!(rejected.starts_with("HTTP/1.1 400"), "{rejected}");
    }
}
#[tokio::test]
async fn candidate_openai_chat_finish_reason_checks_every_choice_and_keeps_whitespace() {
    for reason in ["stop", " "] {
        let handler = CandidateOpenAiResponse::stream(&[serde_json::json!({
            "choices":[{"index":0,"delta":{"content":"hello"},"finish_reason":null},
                {"index":1,"delta":{},"finish_reason":reason}]
        })]);
        let wire = candidate_openai_wire(
            &handler,
            "/v1/chat/completions",
            &candidate_openai_body(false),
            "POST",
        )
        .await;
        assert_eq!(wire.matches("data: [DONE]").count(), 1, "{wire}");
        assert!(!wire.contains("\"error\""), "{wire}");
    }
}

#[tokio::test]
async fn candidate_openai_chat_multiline_payload_and_legacy_usage_are_valid_sse() {
    let event = candidate_openai_chunk("hello", Some(serde_json::json!("stop")));
    let body = serde_json::to_string_pretty(&event)
        .unwrap()
        .lines()
        .map(|line| format!("data: {line}\n"))
        .collect::<String>()
        + "\n";
    let handler = CandidateOpenAiResponse {
        response: OpenAiResponsesHttpResponse::event_stream(200, body.into_bytes()),
        calls: CandidateOpenAiCalls::default(),
    };
    let wire = candidate_openai_wire(
        &handler,
        "/v1/chat/completions",
        &candidate_openai_body(false),
        "POST",
    )
    .await;
    let mut decoder = crate::internal::translator::common::SseDecoder::new();
    let events = decoder.push(wire.split("\r\n\r\n").nth(1).unwrap().as_bytes());
    assert_eq!(events.len(), 2, "{wire}");
    let first: Value = serde_json::from_slice(&events[0].data).unwrap();
    assert_eq!(first["choices"][0]["delta"]["content"], "hello");
    assert_eq!(events[1].data, b"[DONE]");
}
