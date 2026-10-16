// ref: internal/runtime/executor/kimi_thinking_replay_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::kimi_executor::KimiExecutorError;
use super::kimi_thinking_replay::{
    cache_kimi_thinking_replay_content, cache_kimi_thinking_replay_response,
    clear_kimi_thinking_replay_content, kimi_thinking_replay_model_family,
    kimi_thinking_replay_scope_from_request, prepare_kimi_thinking_replay_request,
    restore_kimi_thinking_replay_content, should_clear_kimi_thinking_replay_after_error,
    wrap_kimi_thinking_replay_stream, KimiThinkingReplayStreamAccumulator,
};
use crate::internal::cache::KimiThinkingReplayCache;
use crate::sdk::pluginapi::{ExecutorRequest, ExecutorStreamChunk, ExecutorStreamResponse};

const NOW: i64 = 1_700_000_000_000;
const CACHED: &[u8] = br#"[
    {"type":"thinking","thinking":"full reasoning","signature":"kimi-signature"},
    {"type":"text","text":"I will inspect the file."},
    {"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"README.md"}}
]"#;

fn request(model: &str, payload: &[u8], session: Option<&str>) -> ExecutorRequest {
    let mut metadata = BTreeMap::new();
    if let Some(session) = session {
        metadata.insert("execution_session_id".into(), json!(session));
    }
    ExecutorRequest {
        model: model.into(),
        source_format: "claude".into(),
        payload: payload.to_vec(),
        original_request: payload.to_vec(),
        metadata,
        ..ExecutorRequest::default()
    }
}

fn content(value: &[u8]) -> Value {
    serde_json::from_slice(value).unwrap()
}

#[test]
fn restore_preserves_complete_assistant_content() {
    let body = br#"{"messages":[
        {"role":"user","content":"inspect"},
        {"role":"assistant","content":[
            {"type":"text","text":"I will inspect the file."},
            {"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"README.md"}}
        ]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"}]}
    ]}"#;
    let (updated, restored) = restore_kimi_thinking_replay_content(body, CACHED);
    assert!(restored);
    assert_eq!(content(&updated)["messages"][1]["content"], content(CACHED));
}

#[test]
fn restore_does_not_replace_existing_thinking() {
    let body = br#"{"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"current","signature":"current-signature"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"README.md"}}]}]}"#;
    let (updated, restored) = restore_kimi_thinking_replay_content(body, CACHED);
    assert!(!restored);
    assert_eq!(content(&updated), content(body));
}

#[test]
fn prepare_shares_only_k3_variants() {
    let cache = KimiThinkingReplayCache::new();
    cache
        .store("k3", "execution:family-switch", CACHED, NOW)
        .unwrap();
    cache
        .store("k2.7-code", "execution:family-switch", CACHED, NOW)
        .unwrap();
    let payload = br#"{"messages":[{"role":"assistant","content":[{"type":"text","text":"I will inspect the file."},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"README.md"}}]}]}"#;
    let (prepared, scope) = prepare_kimi_thinking_replay_request(
        &cache,
        NOW + 1,
        request("kimi-k3-256k", payload, Some("family-switch")),
    );
    assert_eq!(scope.model_family, "k3");
    assert_eq!(
        content(&prepared.payload)["messages"][0]["content"][0]["signature"],
        "kimi-signature"
    );

    let (isolated, scope) = prepare_kimi_thinking_replay_request(
        &cache,
        NOW + 1,
        request("kimi-k2.7-code-highspeed", payload, Some("family-switch")),
    );
    assert_eq!(scope.model_family, "k2.7-code-highspeed");
    assert!(content(&isolated.payload)["messages"][0]["content"][0]
        .get("signature")
        .is_none());
}

#[test]
fn scope_isolates_claude_code_callers() {
    let payload =
        br#"{"metadata":{"user_id":"{\"session_id\":\"claude-session\"}"},"messages":[]}"#;
    let mut caller_a = request("kimi-k3", payload, None);
    caller_a
        .auth_attributes
        .insert("downstream_api_key".into(), "caller-a".into());
    let mut caller_b = caller_a.clone();
    caller_b
        .auth_attributes
        .insert("downstream_api_key".into(), "caller-b".into());
    let a = kimi_thinking_replay_scope_from_request(&caller_a);
    let b = kimi_thinking_replay_scope_from_request(&caller_b);
    assert!(a.valid());
    assert!(a.session_key.contains(":claude:claude-session:agent:main"));
    assert_ne!(a.session_key, b.session_key);
    let unauthenticated =
        kimi_thinking_replay_scope_from_request(&request("kimi-k3", payload, None));
    assert!(!unauthenticated.valid());
}

#[test]
fn claude_non_stream_replays_thinking_across_k3_variant_switch() {
    let cache = KimiThinkingReplayCache::new();
    let first = request(
        "kimi-k3-256k",
        br#"{"messages":[]}"#,
        Some("nonstream-switch"),
    );
    let (_, scope) = prepare_kimi_thinking_replay_request(&cache, NOW, first);
    let response = serde_json::to_vec(&json!({"content":content(CACHED)})).unwrap();
    cache_kimi_thinking_replay_response(&cache, NOW + 1, &scope, &response);

    let second_payload = br#"{"messages":[{"role":"assistant","content":[{"type":"text","text":"I will inspect the file."},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"README.md"}}]}]}"#;
    let (second, scope) = prepare_kimi_thinking_replay_request(
        &cache,
        NOW + 2,
        request("kimi-k3", second_payload, Some("nonstream-switch")),
    );
    assert!(scope.replay_applied);
    assert_eq!(
        content(&second.payload)["messages"][0]["content"],
        content(CACHED)
    );
    cache_kimi_thinking_replay_response(
        &cache,
        NOW + 3,
        &scope,
        br#"{"content":[{"type":"text","text":"done"}]}"#,
    );
    assert!(
        !cache
            .read("k3", "execution:nonstream-switch", NOW + 4)
            .unwrap()
            .2
    );
}

#[test]
fn clear_after_error_only_for_upstream_request_rejection() {
    assert!(!should_clear_kimi_thinking_replay_after_error(
        &std::io::Error::other("transport failed")
    ));
    assert!(!should_clear_kimi_thinking_replay_after_error(
        &KimiExecutorError::UpstreamStatus {
            status: 500,
            message: String::new(),
        }
    ));
    for status in [400, 422] {
        assert!(should_clear_kimi_thinking_replay_after_error(
            &KimiExecutorError::UpstreamStatus {
                status,
                message: String::new(),
            }
        ));
    }
}

#[test]
fn claude_error_clears_applied_replay() {
    let cache = KimiThinkingReplayCache::new();
    cache
        .store("k3", "execution:error-clears-replay", CACHED, NOW)
        .unwrap();
    let payload = br#"{"messages":[{"role":"assistant","content":[{"type":"text","text":"I will inspect the file."},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"README.md"}}]}]}"#;
    let (_, scope) = prepare_kimi_thinking_replay_request(
        &cache,
        NOW + 1,
        request("kimi-k3-256k", payload, Some("error-clears-replay")),
    );
    assert!(scope.replay_applied);
    clear_kimi_thinking_replay_content(&cache, NOW + 2, &scope);
    assert!(
        !cache
            .read("k3", "execution:error-clears-replay", NOW + 3)
            .unwrap()
            .2
    );
}

const FIRST_STREAM: &[u8] = br#"event: message_start
data: {"type":"message_start","message":{"model":"k3","content":[]}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"stream reasoning"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"stream-signature"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_stream","name":"Read","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"README.md\"}"}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: message_stop
data: {"type":"message_stop"}

"#;

#[test]
fn claude_stream_accumulates_replay_across_k3_variant_switch() {
    let mut accumulator = KimiThinkingReplayStreamAccumulator::new();
    accumulator.observe(FIRST_STREAM);
    let replay = accumulator.content().expect("complete replay content");
    let value = content(&replay);
    assert_eq!(value[0]["thinking"], "stream reasoning");
    assert_eq!(value[0]["signature"], "stream-signature");
    assert_eq!(value[1]["input"]["path"], "README.md");
}

#[tokio::test]
async fn unknown_stream_delta_preserves_previous_cache() {
    let cache = Arc::new(KimiThinkingReplayCache::new());
    cache
        .store("k3", "execution:unknown-stream-delta", CACHED, NOW)
        .unwrap();
    let payload = br#"{"messages":[{"role":"assistant","content":[{"type":"text","text":"I will inspect the file."},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"README.md"}}]}]}"#;
    let (_, scope) = prepare_kimi_thinking_replay_request(
        &cache,
        NOW + 1,
        request("kimi-k3-256k", payload, Some("unknown-stream-delta")),
    );
    let unknown = br#"event: message_start
data: {"type":"message_start","message":{"model":"k3"}}
event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}
event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"future_delta","value":"new"}}
event: content_block_stop
data: {"type":"content_block_stop","index":0}
event: message_stop
data: {"type":"message_stop"}
"#;
    let (sender, receiver) = mpsc::channel(1);
    sender
        .send(ExecutorStreamChunk {
            payload: unknown.to_vec(),
            error: None,
        })
        .await
        .unwrap();
    drop(sender);
    let mut wrapped = wrap_kimi_thinking_replay_stream(
        cache.clone(),
        NOW + 2,
        ExecutorStreamResponse {
            headers: BTreeMap::new(),
            chunks: receiver,
        },
        scope,
    );
    while wrapped.chunks.recv().await.is_some() {}
    let (got, _, found) = cache
        .read("k3", "execution:unknown-stream-delta", NOW + 3)
        .unwrap();
    assert!(found);
    assert_eq!(content(&got), content(CACHED));
}

#[test]
fn direct_cache_commit_requires_signed_thinking_and_tool_use() {
    let cache = KimiThinkingReplayCache::new();
    let request = request("kimi-k3", br#"{"messages":[]}"#, Some("replayable"));
    let (_, scope) = prepare_kimi_thinking_replay_request(&cache, NOW, request);
    cache_kimi_thinking_replay_content(&cache, NOW + 1, &scope, CACHED);
    assert!(cache.read("k3", "execution:replayable", NOW + 2).unwrap().2);
}
