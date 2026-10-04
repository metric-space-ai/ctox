// ref: internal/runtime/executor/gemini_executor.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::internal::modelconfig::{HomeModelOptions, ModelInfo};
use crate::sdk::translator::ResponseTransform;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn candidate_google_request_gemini_selected_compatibility_covers_all_preparation_paths() {
    for interactions in [false, true] {
        for (stream, action) in [
            (false, "generateContent"),
            (true, "generateContent"),
            (false, "countTokens"),
        ] {
            if interactions && action == "countTokens" {
                continue;
            }
            for (selected, home, compatible) in [
                (true, None, true),
                (true, Some(false), false),
                (false, Some(true), true),
                (false, None, false),
            ] {
                let calls = Arc::new(AtomicUsize::new(0));
                let registry = Arc::new(Registry::new());
                let target = if interactions {
                    "interactions"
                } else {
                    "gemini"
                };
                let observed = calls.clone();
                registry.register(
                    Format::from("claude"),
                    Format::from(target),
                    Some(Arc::new(move |_, _, _| {
                        observed.fetch_add(1, Ordering::SeqCst);
                        br#"{"contents":[],"input":"legacy","legacy_translation":true}"#.to_vec()
                    })),
                    ResponseTransform::default(),
                );
                let executor = if interactions {
                    GeminiExecutor::interactions(
                        Arc::new(GeminiExecutorConfig::default()),
                        registry,
                    )
                } else {
                    GeminiExecutor::new(Arc::new(GeminiExecutorConfig::default()), registry)
                };
                let request = ExecutorRequest {
                    auth_provider: if interactions {
                        "gemini-interactions".into()
                    } else {
                        "gemini".into()
                    },
                    model: "gemini-test".into(),
                    source_format: "claude".into(),
                    payload: br#"{"messages":[{"role":"user","content":"hello"}]}"#.to_vec(),
                    // Ordinary caller metadata/attributes do not own compatibility.
                    metadata: BTreeMap::from([("is-compat".into(), Value::Bool(true))]),
                    auth_attributes: BTreeMap::from([("is-compat".into(), "true".into())]),
                    resolved_model_info: Some(Arc::new(ModelInfo {
                        is_compat: selected,
                        ..Default::default()
                    })),
                    resolved_home_model_options: home.map(|is_compat| HomeModelOptions {
                        is_compat,
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                let (body, format) = executor.prepare_body(&request, stream, action).unwrap();
                let body: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(format.as_str(), target);
                assert_eq!(calls.load(Ordering::SeqCst), usize::from(!compatible));
                assert_eq!(body.get("legacy_translation").is_some(), !compatible);
                assert_eq!(
                    body.get("model").and_then(Value::as_str),
                    Some("gemini-test")
                );
            }
        }
    }
}

#[test]
fn candidate_google_request_native_interactions_bypass_translation_with_empty_or_native_source() {
    for source in ["", "interactions"] {
        let registry = Arc::new(Registry::new());
        for from in ["gemini", "interactions"] {
            registry.register(
                Format::from(from),
                Format::from("interactions"),
                Some(Arc::new(|_, _, _| {
                    panic!("native input must bypass translation")
                })),
                ResponseTransform::default(),
            );
        }
        let executor =
            GeminiExecutor::interactions(Arc::new(GeminiExecutorConfig::default()), registry);
        let request = ExecutorRequest {
            auth_provider: "gemini-interactions".into(),
            source_format: source.into(),
            model: "gemini-test".into(),
            payload: br#"{"input":"hello","native_marker":17}"#.to_vec(),
            ..Default::default()
        };
        for stream in [false, true] {
            let (body, format) = executor
                .prepare_body(&request, stream, "generateContent")
                .unwrap();
            let body: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(format.as_str(), "interactions");
            assert_eq!(body["native_marker"], 17);
        }
    }
}

struct Owner(AtomicUsize);
impl super::super::helps::CodexMultiAgentV2Processor for Owner {
    fn rewrite_spawn_agent_description(&self, _: &Headers, payload: &[u8]) -> Vec<u8> {
        payload.to_vec()
    }
    fn rewrite_input(&self, _: &Headers, payload: &[u8]) -> Vec<u8> {
        payload.to_vec()
    }
    fn translate_request(
        &self,
        _: &Headers,
        from: &Format,
        to: &Format,
        model: &str,
        payload: &[u8],
        _: bool,
    ) -> Vec<u8> {
        assert_eq!(from.as_str(), "openai-response");
        assert_eq!(to.as_str(), "gemini");
        assert_eq!(model, "gemini-test");
        let body: Value = serde_json::from_slice(payload).unwrap();
        assert_eq!(
            body.pointer("/tools/0/parameters/properties/limit/type")
                .and_then(Value::as_str),
            Some("integer")
        );
        self.0.fetch_add(1, Ordering::SeqCst);
        br#"{"contents":[],"owner_translation":true}"#.to_vec()
    }
    fn optimize_request(&self, _: &Headers, payload: &[u8]) -> (Vec<u8>, bool) {
        (payload.to_vec(), false)
    }
    fn restore_response(&self, payload: &[u8], _: bool) -> Vec<u8> {
        payload.to_vec()
    }
}

#[test]
fn candidate_google_request_gemini_normalizes_before_the_injected_owner_translates() {
    let owner = Arc::new(Owner(AtomicUsize::new(0)));
    let executor = GeminiExecutor::new(
        Arc::new(GeminiExecutorConfig::default()),
        Arc::new(Registry::new()),
    )
    .with_request_processor(owner.clone());
    let request = ExecutorRequest {
        model: "gemini-test".into(), source_format: "openai-response".into(),
        headers: BTreeMap::from([("uSeR-aGeNt".into(), vec!["codex-test".into()])]),
        payload: br#"{"tools":[{"type":"function","name":"read_thread","parameters":{"properties":{"limit":{"type":"number"}}}}]}"#.to_vec(),
        ..Default::default()
    };
    for (stream, action) in [
        (false, "generateContent"),
        (true, "generateContent"),
        (false, "countTokens"),
    ] {
        let (body, _) = executor.prepare_body(&request, stream, action).unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["owner_translation"], true);
    }
    assert_eq!(owner.0.load(Ordering::SeqCst), 3);
}
