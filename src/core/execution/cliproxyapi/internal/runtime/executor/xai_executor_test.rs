// ref: internal/runtime/executor/xai_executor_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

use super::*;
use crate::sdk::cliproxy::auth::Auth;
use crate::sdk::cliproxy::executor::{ExecutionMetadata, Headers, Options, Request};

#[test]
fn token_count_excludes_request_structure() {
    let empty =
        count_xai_input_tokens(br#"{"model":"grok","stream":true,"metadata":{"x":"y"}}"#).unwrap();
    assert_eq!(empty, 0);
    let populated = count_xai_input_tokens(br#"{"instructions":"help","input":[{"type":"message","content":[{"type":"input_text","text":"hello world"}]}],"tools":[{"type":"function","name":"run","description":"Run it","parameters":{"type":"object"}}]}"#).unwrap();
    assert!(populated > 3);
}

#[test]
fn prepare_shapes_responses_and_thinking_suffix() {
    let prepared = prepare_xai_responses_body(
        br#"{"input":"hi","stop":["x"]}"#,
        XaiRequestPolicy {
            model: "grok-4 (high)",
            stream: true,
            reasoning_effort: Some("high"),
            ..Default::default()
        },
    )
    .unwrap();
    let body: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(body["model"], "grok-4");
    assert_eq!(body["stream"], true);
    assert_eq!(body["reasoning"]["effort"], "high");
    assert!(body.get("stop").is_none());
}

#[test]
fn promotes_additional_tools_and_qualifies_namespaces() {
    let payload = br#"{"input":[{"type":"additional_tools","tools":[{"type":"namespace","name":"collaboration","tools":[{"type":"function","name":"spawn_agent","parameters":{"oneOf":[{"properties":{}}]}}]}]}]}"#;
    let prepared = prepare_xai_responses_body(
        payload,
        XaiRequestPolicy {
            model: "grok",
            ..Default::default()
        },
    )
    .unwrap();
    let body: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(body["tools"][0]["name"], "collaboration__spawn_agent");
    assert_eq!(body["tools"][0]["parameters"]["oneOf"][0]["type"], "object");
    assert!(body["input"].as_array().unwrap().is_empty());
    assert_eq!(
        prepared.namespace_tools["collaboration__spawn_agent"].namespace,
        "collaboration"
    );
}

#[test]
fn injects_x_search_once_and_prunes_orphaned_choice() {
    let prepared = prepare_xai_responses_body(
        br#"{"input":[],"tool_choice":{"type":"function","name":"missing"},"tools":[]}"#,
        XaiRequestPolicy {
            model: "grok",
            inject_x_search: true,
            ..Default::default()
        },
    )
    .unwrap();
    let body: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(body["tools"], json!([{"type":"x_search"}]));
    assert!(body.get("tool_choice").is_none());
    assert!(prepared.filter_internal_x_search);
}

#[test]
fn normalizes_custom_tool_history() {
    let prepared = prepare_xai_responses_body(br#"{"input":[{"type":"custom_tool_call","name":"shell","input":"ls"},{"type":"custom_tool_call_output","output":"ok"}]}"#, XaiRequestPolicy { model:"grok", ..Default::default() }).unwrap();
    let body: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(body["input"][0]["type"], "function_call");
    assert_eq!(body["input"][0]["arguments"], "ls");
    assert_eq!(body["input"][1]["type"], "function_call_output");
}

#[test]
fn image_refs_rewrite_recursively() {
    let body =
        normalize_image_refs(br#"{"image_url":"a","nested":[{"image_url":"b","url":"keep"}]}"#);
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["url"], "a");
    assert_eq!(value["nested"][0]["url"], "keep");
    assert!(value["nested"][0].get("image_url").is_none());
}

#[test]
fn endpoint_selection_covers_images_and_video_native_paths() {
    let mut options = Options {
        alt: "images/edits".into(),
        ..Options::default()
    };
    assert_eq!(
        xai_image_endpoint_path(&options),
        Some(XAI_IMAGES_EDITS_PATH)
    );
    options.alt.clear();
    options.metadata.request_path = Some("/v1/videos/job-1".into());
    assert_eq!(
        xai_video_endpoint_path(&options).as_deref(),
        Some("/videos/job-1")
    );
}

#[test]
fn api_and_chat_headers_remain_distinct() {
    let mut auth = Auth::default();
    auth.metadata.insert("auth_kind".into(), json!("oauth"));
    auth.metadata.insert("access_token".into(), json!("secret"));
    let mut api = Headers::new();
    apply_xai_headers(&mut api, Some(&auth), "secret", false, "session");
    assert!(!api.contains_key(XAI_TOKEN_AUTH_HEADER));
    let mut chat = Headers::new();
    apply_xai_chat_headers(&mut chat, Some(&auth), "secret", true, "session");
    assert_eq!(chat[XAI_TOKEN_AUTH_HEADER], vec![XAI_TOKEN_AUTH_VALUE]);
    assert_eq!(chat[XAI_CLIENT_VERSION_HEADER], vec!["1.0.44"]);
    assert_eq!(chat["x-grok-conv-id"], vec!["session"]);
    assert_eq!(chat["User-Agent"], vec!["xai-grok-workspace/1.0.44"]);
    assert_eq!(chat[XAI_CLIENT_IDENTIFIER_HEADER], vec!["grok-shell"]);
    assert_eq!(
        chat[XAI_AUTHENTICATE_RESPONSE_HEADER],
        vec!["authenticate-response"]
    );
    auth.attributes.insert("using_api".into(), "true".into());
    let mut official = Headers::new();
    apply_xai_chat_headers(&mut official, Some(&auth), "secret", true, "");
    assert!(!official.contains_key(XAI_TOKEN_AUTH_HEADER));
}

#[test]
fn xai_auth_mode_matches_upstream_boolean_and_oauth_defaults() {
    assert!(xai_credentials(None).using_api);
    let mut auth = Auth::default();
    assert!(xai_credentials(Some(&auth)).using_api);
    auth.metadata.insert("auth_kind".into(), json!("oauth"));
    assert!(!xai_credentials(Some(&auth)).using_api);
    auth.metadata.insert("using_api".into(), json!(true));
    assert!(xai_credentials(Some(&auth)).using_api);
    for value in ["0", "f", "F", "false", "FALSE", "False"] {
        auth.attributes.insert("using_api".into(), value.into());
        assert!(!xai_credentials(Some(&auth)).using_api, "{value}");
    }
    for value in ["1", "t", "T", "true", "TRUE", "True"] {
        auth.attributes.insert("using_api".into(), value.into());
        assert!(xai_credentials(Some(&auth)).using_api, "{value}");
    }
    auth.attributes.insert("using_api".into(), "invalid".into());
    auth.metadata.insert("using_api".into(), json!(" false "));
    assert!(!xai_credentials(Some(&auth)).using_api);
    auth.metadata.remove("using_api");
    auth.attributes.insert("auth_kind".into(), " API ".into());
    assert!(xai_credentials(Some(&auth)).using_api);
}

#[test]
fn xai_credentials_prioritize_nonempty_attributes_over_metadata() {
    let mut auth = Auth::default();
    auth.metadata
        .insert("access_token".into(), json!("oauth-token"));
    auth.metadata
        .insert("base_url".into(), json!("https://edge.example/v1/"));
    auth.attributes.insert("api_key".into(), " api-key ".into());
    auth.attributes.insert("base_url".into(), "   ".into());
    let credentials = xai_credentials(Some(&auth));
    assert_eq!(credentials.token, "api-key");
    assert_eq!(credentials.base_url, "https://edge.example/v1");
    auth.attributes.insert("api_key".into(), "   ".into());
    assert_eq!(xai_credentials(Some(&auth)).token, "oauth-token");
}

#[test]
fn xai_subscription_chat_and_compact_routes_follow_upstream() {
    assert_eq!(xai_chat_base_url(None), "https://api.x.ai/v1");
    let mut auth = Auth::default();
    auth.metadata.insert("auth_kind".into(), json!("oauth"));
    for base_url in ["", " https://api.x.ai/v1/// "] {
        auth.attributes.insert("base_url".into(), base_url.into());
        assert_eq!(
            xai_chat_base_url(Some(&auth)),
            "https://cli-chat-proxy.grok.com/v1"
        );
        assert_eq!(xai_compact_base_url(Some(&auth)), "https://api.x.ai/v1");
    }
    auth.attributes.insert(
        "base_url".into(),
        " https://cli-chat-proxy.grok.com/v1/ ".into(),
    );
    assert_eq!(xai_compact_base_url(Some(&auth)), "https://api.x.ai/v1");
    auth.attributes
        .insert("base_url".into(), " https://edge.example/v1/ ".into());
    assert_eq!(xai_chat_base_url(Some(&auth)), "https://edge.example/v1");
    assert_eq!(xai_compact_base_url(Some(&auth)), "https://edge.example/v1");
}

#[test]
fn xai_subscription_identity_is_scoped_and_custom_headers_override() {
    let mut auth = Auth::default();
    auth.attributes.insert("using_api".into(), "false".into());
    auth.attributes
        .insert("base_url".into(), "https://edge.example/v1".into());
    let mut custom = Headers::new();
    apply_xai_chat_headers(&mut custom, Some(&auth), "", false, "");
    assert!(!custom.contains_key(XAI_TOKEN_AUTH_HEADER));
    assert!(!custom.contains_key("Authorization"));
    auth.attributes.remove("base_url");
    auth.attributes.insert(
        "header:x-grok-client-version".into(),
        "custom-version".into(),
    );
    let mut official = Headers::new();
    apply_xai_chat_headers(&mut official, Some(&auth), "token", false, "s");
    assert_eq!(official[XAI_CLIENT_VERSION_HEADER], vec!["custom-version"]);
    assert_eq!(official[XAI_TOKEN_AUTH_HEADER], vec!["xai-grok-cli"]);
}

#[test]
fn base_url_source_and_compact_path_are_safe() {
    assert_eq!(xai_base_url_source(DEFAULT_XAI_API_BASE_URL), "default_api");
    assert_eq!(
        xai_base_url_source("https://chat-proxy.example/v1"),
        "chat_proxy"
    );
    assert_eq!(xai_base_url_source("https://edge.example/v1"), "custom_api");
}

#[test]
fn response_filter_drops_only_internal_search_calls() {
    let mut filter = InternalXSearchResponseFilter::new(true, BTreeSet::new());
    let dropped = filter.apply(br#"{"type":"response.output_item.done","item":{"type":"function_call","id":"i1","name":"x_search"}}"#);
    assert!(dropped.is_empty());
    let followup = filter
        .apply(br#"{"type":"response.function_call_arguments.delta","item_id":"i1","delta":"x"}"#);
    assert!(followup.is_empty());
    let kept = filter.apply(br#"{"type":"response.output_item.done","item":{"type":"function_call","id":"i2","name":"client"}}"#);
    assert!(!kept.is_empty());
}

#[test]
fn response_filter_preserves_declared_same_name_tool() {
    let declared = BTreeSet::from([ClientToolKey {
        tool_type: "function".into(),
        name: "x_search".into(),
    }]);
    let mut filter = InternalXSearchResponseFilter::new(true, declared);
    assert!(!filter.apply(br#"{"type":"response.output_item.done","item":{"type":"function_call","name":"x_search"}}"#).is_empty());
}

#[test]
fn restores_namespace_calls() {
    let refs = BTreeMap::from([(
        "collaboration__spawn_agent".into(),
        NamespaceToolRef {
            namespace: "collaboration".into(),
            name: "spawn_agent".into(),
        },
    )]);
    let restored: Value = serde_json::from_slice(&restore_namespace_tool_calls(
        br#"{"type":"function_call","name":"collaboration__spawn_agent","arguments":"{}"}"#,
        &refs,
    ))
    .unwrap();
    assert_eq!(restored["name"], "spawn_agent");
    assert_eq!(restored["namespace"], "collaboration");
}

#[test]
fn encrypted_reasoning_is_sanitized() {
    let body = sanitize_input_encrypted_content(br#"{"input":[{"type":"reasoning","encrypted_content":"bad"},{"type":"reasoning","summary":[{"text":"safe"}],"encrypted_content":"bad"}]}"#);
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["input"].as_array().unwrap().len(), 1);
    assert!(value["input"][0].get("encrypted_content").is_none());
}

#[test]
fn reasoning_text_events_are_normalized() {
    assert_eq!(
        normalize_reasoning_event_name("response.reasoning_text.delta"),
        "response.reasoning_summary_text.delta"
    );
    let data: Value = serde_json::from_slice(&normalize_reasoning_event_data(
        br#"{"type":"response.reasoning_text.done"}"#,
    ))
    .unwrap();
    assert_eq!(data["type"], "response.reasoning_summary_text.done");
}

#[derive(Default)]
struct MemoryReplay(Mutex<BTreeMap<String, Vec<Vec<u8>>>>);
impl XaiReasoningReplayStore for MemoryReplay {
    fn load(&self, key: &str) -> Result<Vec<Vec<u8>>, String> {
        Ok(self.0.lock().unwrap().get(key).cloned().unwrap_or_default())
    }
    fn store(&self, key: &str, items: &[Vec<u8>]) -> Result<(), String> {
        self.0.lock().unwrap().insert(key.into(), items.to_vec());
        Ok(())
    }
    fn clear(&self, key: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

#[test]
fn replay_scope_isolates_credentials_and_replays() {
    let a = XaiReasoningReplayScope::new("xai", "session", Some("key-a")).unwrap();
    let b = XaiReasoningReplayScope::new("xai", "session", Some("key-b")).unwrap();
    assert_ne!(a.key(), b.key());
    let store = MemoryReplay::default();
    store
        .store(
            &a.key(),
            &[br#"{"type":"reasoning","encrypted_content":"cipher"}"#.to_vec()],
        )
        .unwrap();
    let body = apply_reasoning_replay(
        &store,
        Some(&a),
        br#"{"input":[{"type":"message","role":"user","content":"next"}]}"#,
    );
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["input"][0]["type"], "reasoning");
}

#[test]
fn completed_cache_stores_and_compaction_clears() {
    let store = MemoryReplay::default();
    let scope = XaiReasoningReplayScope::new("xai", "s", None).unwrap();
    cache_reasoning_replay_from_completed(&store, Some(&scope), br#"{"response":{"output":[{"type":"reasoning","encrypted_content":"x"},{"type":"message","role":"assistant","content":"done"}]}}"#);
    assert_eq!(store.load(&scope.key()).unwrap().len(), 2);
    clear_reasoning_replay_after_compaction(&store, Some(&scope));
    assert!(store.load(&scope.key()).unwrap().is_empty());
}

struct FixedClock;
impl XaiAuthClock for FixedClock {
    fn now(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(10)
    }
}
struct Refresh;
impl XaiRefreshTransport for Refresh {
    fn refresh<'a>(
        &'a self,
        _: &'a str,
        _: Option<&'a str>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<XaiRefreshTokens, XaiAuthError>> + Send + 'a>,
    > {
        Box::pin(async {
            Ok(XaiRefreshTokens {
                access_token: "new".into(),
                refresh_token: Some("rotated".into()),
                id_token: None,
                token_type: Some("Bearer".into()),
                expires_in: Some(60),
                expires_at: None,
                email: None,
                subject: None,
            })
        })
    }
}

#[tokio::test]
async fn refresh_uses_injected_transport_and_clock() {
    let service = XaiSubscriptionAuth::new(
        Arc::new(Refresh),
        Arc::new(FixedClock),
        DEFAULT_XAI_API_BASE_URL,
    );
    let mut auth = Auth::default();
    auth.metadata.insert("refresh_token".into(), json!("old"));
    let updated = service.refresh(Some(&auth)).await.unwrap();
    assert_eq!(updated.metadata["access_token"], "new");
    assert_eq!(updated.metadata["refresh_token"], "rotated");
    assert_eq!(updated.updated_at.timestamp(), 10);
}

struct HttpFixture {
    response: Mutex<Option<(u16, Vec<u8>)>>,
    seen: Mutex<Option<String>>,
    seen_body: Mutex<Option<Vec<u8>>>,
}
impl XaiHttpTransport for HttpFixture {
    fn execute<'a>(
        &'a self,
        request: &'a XaiHttpRequest,
        _: Duration,
    ) -> XaiTransportFuture<'a, XaiHttpResponse> {
        *self.seen.lock().unwrap() = Some(request.url.clone());
        *self.seen_body.lock().unwrap() = Some(request.body.to_vec());
        let (status, body) = self.response.lock().unwrap().take().unwrap();
        Box::pin(async move {
            Ok(XaiHttpResponse {
                status,
                headers: Headers::new(),
                body: body.into(),
            })
        })
    }
}

#[tokio::test]
async fn execute_aggregates_completed_event() {
    let transport=Arc::new(HttpFixture{response:Mutex::new(Some((200,b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r\"}}\n\n".to_vec()))),seen:Mutex::new(None),seen_body:Mutex::new(None)});
    let executor = XaiExecutor::new(transport.clone(), Duration::from_secs(5)).unwrap();
    let request = Request {
        model: "grok".into(),
        payload: br#"{"input":"hi"}"#.to_vec(),
        ..Request::default()
    };
    let response = executor
        .execute(None, &request, &Options::default())
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&response.payload).unwrap()["type"],
        "response.completed"
    );
    assert!(transport
        .seen
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .ends_with("/responses"));
}

#[tokio::test]
async fn execute_applies_and_updates_injected_reasoning_replay() {
    let store = Arc::new(MemoryReplay::default());
    let scope = XaiReasoningReplayScope::new("xai", "session-1", None).unwrap();
    store
        .store(
            &scope.key(),
            &[br#"{"type":"reasoning","encrypted_content":"YWJjZGVmZ2hpamtsbW5vcA=="}"#.to_vec()],
        )
        .unwrap();
    let transport = Arc::new(HttpFixture {
        response: Mutex::new(Some((
            200,
            b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"reasoning\",\"encrypted_content\":\"cXdlcnR5dWlvcGFzZGZnaA==\"}]}}\n\n".to_vec(),
        ))),
        seen: Mutex::new(None),
        seen_body: Mutex::new(None),
    });
    let executor = XaiExecutor::new(transport.clone(), Duration::from_secs(5))
        .unwrap()
        .with_replay_store(store.clone());
    let request = Request {
        model: "grok".into(),
        payload: br#"{"input":[{"role":"user","content":"next"}]}"#.to_vec(),
        ..Request::default()
    };
    let options = Options {
        metadata: ExecutionMetadata {
            execution_session_id: Some("session-1".into()),
            ..ExecutionMetadata::default()
        },
        ..Options::default()
    };

    executor.execute(None, &request, &options).await.unwrap();

    let outbound: Value =
        serde_json::from_slice(transport.seen_body.lock().unwrap().as_deref().unwrap()).unwrap();
    assert_eq!(outbound["input"][0]["type"], "reasoning");
    let cached = store.load(&scope.key()).unwrap();
    assert_eq!(cached.len(), 1);
    assert!(cached[0]
        .windows(24)
        .any(|window| window == b"cXdlcnR5dWlvcGFzZGZnaA=="));
}

#[tokio::test]
async fn execute_media_uses_native_endpoint_and_status_error() {
    let transport = Arc::new(HttpFixture {
        response: Mutex::new(Some((
            429,
            br#"{"error":{"message":"Free usage quota exhausted"}}"#.to_vec(),
        ))),
        seen: Mutex::new(None),
        seen_body: Mutex::new(None),
    });
    let executor = XaiExecutor::new(transport.clone(), Duration::from_secs(5)).unwrap();
    let request = Request {
        model: "grok-image".into(),
        payload: br#"{"prompt":"x"}"#.to_vec(),
        ..Request::default()
    };
    let options = Options {
        alt: "images/generations".into(),
        ..Options::default()
    };
    let error = executor
        .execute(None, &request, &options)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        XaiExecutionError::Status(XaiStatusError {
            retry_after: Some(_),
            ..
        })
    ));
    assert!(transport
        .seen
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .ends_with(XAI_IMAGES_GENERATIONS_PATH));
}

#[tokio::test]
async fn compaction_trigger_stream_uses_compact_endpoint_and_synthetic_sse() {
    let transport = Arc::new(HttpFixture {
        response: Mutex::new(Some((
            200,
            br#"{"id":"resp_xai_1","model":"grok-4.3","output":[{"type":"compaction","encrypted_content":"opaque"}],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3}}"#.to_vec(),
        ))),
        seen: Mutex::new(None),
        seen_body: Mutex::new(None),
    });
    let executor = XaiExecutor::new(transport.clone(), Duration::from_secs(5)).unwrap();
    let request = Request {
        model: "grok-4.3".into(),
        payload: br#"{"model":"grok-4.3","stream":true,"input":[{"role":"user","content":"hello"},{"type":"compaction_trigger"}]}"#.to_vec(),
        ..Request::default()
    };
    let mut stream = executor
        .execute_stream(None, &request, &Options::default())
        .await
        .unwrap();

    assert_eq!(
        stream.headers["Content-Type"],
        vec!["text/event-stream".to_owned()]
    );
    let mut output = Vec::new();
    while let Some(chunk) = stream.chunks.recv().await {
        output.extend(chunk.unwrap());
    }
    let output = String::from_utf8(output).unwrap();
    for event in [
        "response.created",
        "response.in_progress",
        "response.output_item.added",
        "response.output_item.done",
        "response.completed",
    ] {
        assert!(output.contains(&format!("event: {event}\n")));
    }
    assert!(output.contains(r#""type":"compaction""#));
    assert!(output.contains(r#""encrypted_content":"opaque""#));
    let completed = output
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|event| event["type"] == "response.completed")
        .unwrap();
    assert_eq!(
        completed["response"]["usage"],
        json!({"input_tokens":1,"output_tokens":2,"total_tokens":3})
    );
    assert!(transport
        .seen
        .lock()
        .unwrap()
        .as_deref()
        .unwrap()
        .ends_with("/responses/compact"));
    let body: Value =
        serde_json::from_slice(transport.seen_body.lock().unwrap().as_deref().unwrap()).unwrap();
    assert!(body.get("stream").is_none());
    assert!(!xai_input_has_item_type(
        transport.seen_body.lock().unwrap().as_deref().unwrap(),
        "compaction_trigger"
    ));
}
