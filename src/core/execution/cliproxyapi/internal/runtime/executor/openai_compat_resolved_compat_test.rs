// ref: internal/runtime/executor/helps/model_capabilities.go @ 2044a01f
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::internal::modelconfig::ModelInfo;
use crate::sdk::translator::{PluginHooks, ResponseTransform};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct CountingNormalizer(AtomicUsize);
impl PluginHooks for CountingNormalizer {
    fn normalize_request(
        &self,
        _: &TranslationContext,
        _: &Format,
        _: &Format,
        _: &str,
        body: Vec<u8>,
        _: bool,
    ) -> Vec<u8> {
        self.0.fetch_add(1, Ordering::SeqCst);
        body
    }
}

fn registry() -> (Arc<Registry>, Arc<CountingNormalizer>) {
    let registry = Arc::new(Registry::new());
    let hooks = Arc::new(CountingNormalizer::default());
    registry.set_plugin_hooks(Some(hooks.clone()));
    registry.register(
        Format::from("claude"),
        Format::from("openai"),
        Some(Arc::new(|_, _, _| {
            br#"{"normal-translator":true}"#.to_vec()
        })),
        ResponseTransform {
            stream: None,
            non_stream: None,
            token_count: None,
        },
    );
    (registry, hooks)
}

fn snapshot(is_compat: bool) -> Arc<ModelInfo> {
    Arc::new(ModelInfo {
        id: "shared".into(),
        is_compat,
        ..ModelInfo::default()
    })
}

#[test]
fn candidate_live_openai_executor_dispatches_only_from_selected_model_authority() {
    let (registry, hooks) = registry();
    let executor =
        OpenAiCompatExecutor::new("test", Arc::new(OpenAiCompatConfig::default()), registry);
    let payload = br#"{"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"retained-thought","signature":""},{"type":"text","text":"visible"}]}],"max_tokens":512}"#;
    for stream in [false, true] {
        let mut request = ExecutorRequest {
            model: "shared".into(),
            source_format: "claude".into(),
            payload: payload.to_vec(),
            resolved_model_info: Some(snapshot(false)),
            ..ExecutorRequest::default()
        };
        let ordinary = executor.translate_request(&request, &Format::from("openai"), stream);
        assert_eq!(
            serde_json::from_slice::<Value>(&ordinary).unwrap()["normal-translator"],
            true
        );

        request.resolved_model_info = Some(snapshot(true));
        let compatible = executor.translate_request(&request, &Format::from("openai"), stream);
        let value: Value = serde_json::from_slice(&compatible).unwrap();
        assert!(value.get("normal-translator").is_none());
        assert!(value["messages"].is_array());
        assert!(std::str::from_utf8(&compatible)
            .unwrap()
            .contains("retained-thought"));
    }
    assert_eq!(hooks.0.load(Ordering::SeqCst), 4);
}

#[test]
fn candidate_json_metadata_and_auth_attributes_cannot_enable_executor_compatibility() {
    let (registry, hooks) = registry();
    let executor =
        OpenAiCompatExecutor::new("test", Arc::new(OpenAiCompatConfig::default()), registry);
    let request = ExecutorRequest {
        model: "shared".into(),
        source_format: "claude".into(),
        payload: br#"{"messages":[]}"#.to_vec(),
        metadata: BTreeMap::from([("is_compat".into(), Value::Bool(true))]),
        auth_attributes: BTreeMap::from([("is_compat".into(), "true".into())]),
        ..ExecutorRequest::default()
    };
    let ordinary = executor.translate_request(&request, &Format::from("openai"), false);
    assert_eq!(
        serde_json::from_slice::<Value>(&ordinary).unwrap()["normal-translator"],
        true
    );
    assert_eq!(hooks.0.load(Ordering::SeqCst), 1);
}

#[test]
fn candidate_openai_executor_rewrites_portable_agent_input_before_normal_translation() {
    let registry = Arc::new(Registry::new());
    registry.register(
        Format::from("openai-response"),
        Format::from("openai"),
        Some(Arc::new(|_, body, _| {
            let input: Value = serde_json::from_slice(body).unwrap();
            assert_eq!(input["input"][0]["type"], "message");
            assert_eq!(input["input"][0]["role"], "user");
            assert_eq!(input["input"][0]["content"][0]["type"], "input_text");
            assert_eq!(input["input"][0]["content"][0]["text"], "agent-text");
            assert!(input["input"][0].get("author").is_none());
            assert!(input["input"][0].get("recipient").is_none());
            br#"{"messages":[]}"#.to_vec()
        })),
        ResponseTransform {
            stream: None,
            non_stream: None,
            token_count: None,
        },
    );
    let executor =
        OpenAiCompatExecutor::new("test", Arc::new(OpenAiCompatConfig::default()), registry);
    let request = ExecutorRequest {
        model: "shared".into(),
        source_format: "openai-response".into(),
        payload: br#"{"input":[{"type":"agent_message","role":"assistant","author":"worker","recipient":"parent","content":[{"type":"encrypted_text","text":"agent-text"}]}]}"#.to_vec(),
        resolved_model_info: Some(snapshot(true)),
        ..ExecutorRequest::default()
    };
    let body = executor.translate_request(&request, &Format::from("openai"), false);
    assert!(serde_json::from_slice::<Value>(&body).unwrap()["messages"].is_array());
}
