// ref: internal/runtime/executor/gemini_executor.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::super::helps::{PayloadApplyConfig, PayloadModelRule, PayloadRule};
#[test]
fn candidate_google_preflight_gemini_repairs_signatures_and_count_boundaries() {
    let executor = GeminiExecutor::new(
        Arc::new(GeminiExecutorConfig::default()),
        Arc::new(Registry::new()),
    );
    let request = ExecutorRequest {
        model:"gemini-test".into(),
        source_format:"gemini".into(),
        payload:br#"{"contents":[{"role":"model","parts":[{"functionCall":{"name":"run"},"thoughtSignature":"claude#invalid"}]}],"tools":[],"generationConfig":{"temperature":0.7},"safetySettings":[]}"#.to_vec(),
        ..Default::default()
    };
    for (stream, action, len) in [
        (false, "generateContent", 3),
        (true, "streamGenerateContent", 3),
        (false, "countTokens", 2),
    ] {
        let (body, format) = executor
            .prepare_body(&request, stream, action == "countTokens")
            .unwrap();
        assert_eq!(format.as_str(), "gemini");
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["contents"].as_array().unwrap().len(), len);
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(
            body["contents"][1]["parts"][0]["thoughtSignature"],
            crate::internal::signature::GEMINI_SKIP_THOUGHT_SIGNATURE_VALIDATOR
        );
        assert_eq!(
            body["contents"][1]["parts"][0]["functionCall"]["name"],
            "run"
        );
        if action == "countTokens" {
            assert_eq!(body["contents"][1]["role"], "model");
            assert!(body.get("tools").is_none());
            assert!(body.get("generationConfig").is_none());
        } else {
            assert_eq!(body["contents"][2]["role"], "user");
        }
    }
}

#[test]
fn candidate_google_preflight_native_interactions_keeps_its_own_history_contract() {
    for source in ["", "interactions"] {
        let executor = GeminiExecutor::interactions(
            Arc::new(GeminiExecutorConfig::default()),
            Arc::new(Registry::new()),
        );
        let request = ExecutorRequest {
            auth_provider:"gemini-interactions".into(),
            model:"gemini-test".into(),
            source_format:source.into(),
            payload:br#"{"input":"hello","contents":[{"role":"model","parts":[{"functionCall":{"name":"run"},"thoughtSignature":"claude#invalid"}]}]}"#.to_vec(),
            ..Default::default()
        };
        for stream in [false, true] {
            let (body, format) = executor.prepare_body(&request, stream, false).unwrap();
            assert_eq!(format.as_str(), "interactions");
            let body: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["contents"].as_array().unwrap().len(), 1);
            assert_eq!(
                body["contents"][0]["parts"][0]["thoughtSignature"],
                "claude#invalid"
            );
            assert_eq!(body["contents"][0]["role"], "model");
        }
    }
}

use crate::sdk::translator::RequestTransform;

#[test]
fn candidate_google_payload_gemini_defaults_use_original_alias_and_headers() {
    let model = PayloadModelRule {
        name: "public-alias".into(),
        protocol: "gemini".into(),
        from_protocol: "gemini".into(),
        headers: BTreeMap::from([("X-Scope".into(), "blue".into())]),
        ..Default::default()
    };
    let mut config = PayloadApplyConfig::default();
    config.rules.default.push(PayloadRule {
        models: vec![model.clone()],
        params: BTreeMap::from([
            (
                "generationConfig.temperature".into(),
                serde_json::json!(0.3),
            ),
            ("central_default".into(), Value::Bool(true)),
        ]),
    });
    config.rules.override_values.push(PayloadRule {
        models: vec![model],
        params: BTreeMap::from([("generationConfig.topP".into(), serde_json::json!(0.2))]),
    });
    let executor = GeminiExecutor::new(
        Arc::new(GeminiExecutorConfig::default()),
        Arc::new(Registry::new()),
    )
    .with_payload_config(Arc::new(config));
    for scope in ["blue", "red"] {
        let request = ExecutorRequest {
            model: "upstream-name".into(),
            source_format: "gemini".into(),
            payload: br#"{"contents":[]}"#.to_vec(),
            original_request: br#"{"contents":[],"generationConfig":{"temperature":0.9}}"#.to_vec(),
            metadata: BTreeMap::from([(
                "requested_model".into(),
                Value::String(" public-alias ".into()),
            )]),
            headers: BTreeMap::from([("x-scope".into(), vec![scope.into()])]),
            ..Default::default()
        };
        let (body, _) = executor.prepare_body(&request, false, false).unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert!(body.pointer("/generationConfig/temperature").is_none(), "an original explicit value prevents a default from filling a field removed from working input");
        assert_eq!(body.get("central_default").is_some(), scope == "blue");
        assert_eq!(
            body.pointer("/generationConfig/topP").is_some(),
            scope == "blue"
        );
        assert_eq!(body["model"], "upstream-name");
        let (count, _) = executor.prepare_body(&request, false, true).unwrap();
        let count: Value = serde_json::from_slice(&count).unwrap();
        assert!(count.get("central_default").is_none());
        assert!(count.get("generationConfig").is_none());
    }
}

#[test]
fn candidate_google_payload_gemini_pairs_preserve_upstream_order_and_slice_identity() {
    for interactions in [false, true] {
        for original_kind in ["shared", "equal-copy", "distinct"] {
            let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
            let registry = Arc::new(Registry::new());
            let target = if interactions {
                "interactions"
            } else {
                "gemini"
            };
            let observed = calls.clone();
            let transform: RequestTransform = Arc::new(move |_, payload, _| {
                let body: Value = serde_json::from_slice(payload).unwrap();
                observed
                    .lock()
                    .unwrap()
                    .push(body["marker"].as_str().unwrap().to_owned());
                payload.to_vec()
            });
            registry.register(
                Format::from("claude"),
                Format::from(target),
                Some(transform),
                ResponseTransform::default(),
            );
            let executor = GeminiExecutor::new(Arc::new(GeminiExecutorConfig::default()), registry);
            let payload = br#"{"messages":[],"marker":"working"}"#.to_vec();
            let original = match original_kind {
                "shared" => Vec::new(),
                "equal-copy" => payload.clone(),
                _ => br#"{"messages":[],"marker":"original"}"#.to_vec(),
            };
            let request = ExecutorRequest {
                model: "gemini-test".into(),
                source_format: "claude".into(),
                payload,
                original_request: original,
                ..Default::default()
            };
            let (baseline, mut working) = executor.translate_pair(
                &request,
                &Format::from("claude"),
                &Format::from(target),
                "gemini-test",
                false,
            );
            let expected: &[&str] = match (original_kind, interactions) {
                ("shared", _) => &["working"],
                ("equal-copy", _) => &["working", "working"],
                (_, false) => &["original", "working"],
                (_, true) => &["working", "original"],
            };
            assert_eq!(calls.lock().unwrap().as_slice(), expected);
            let before = baseline.clone();
            working.fill(b'x');
            assert_eq!(
                baseline, before,
                "working mutations cannot change the default-rule baseline"
            );
        }
    }
}

#[test]
fn candidate_google_payload_legacy_rules_use_the_complete_pipeline_for_both_protocols() {
    for interactions in [false, true] {
        let target = if interactions {
            "interactions"
        } else {
            "gemini"
        };
        let parameter = if interactions {
            "generation_config.thinking_level"
        } else {
            "generationConfig.topP"
        };
        let expected = if interactions {
            Value::String("low".into())
        } else {
            serde_json::json!(0.2)
        };
        let config = Arc::new(GeminiExecutorConfig {
            payload_rules: vec![GeminiPayloadRule {
                protocol: target.into(),
                from_protocol: target.into(),
                overrides: Map::from_iter([(parameter.into(), expected.clone())]),
                ..Default::default()
            }],
            ..Default::default()
        });
        let executor = if interactions {
            GeminiExecutor::interactions(config, Arc::new(Registry::new()))
        } else {
            GeminiExecutor::new(config, Arc::new(Registry::new()))
        };
        let request = ExecutorRequest {
            model: if interactions {
                "gemini-test(high)".into()
            } else {
                "gemini-test".into()
            },
            source_format: target.into(),
            auth_provider: if interactions {
                "gemini-interactions".into()
            } else {
                "gemini".into()
            },
            payload: if interactions {
                br#"{"input":"hello"}"#.to_vec()
            } else {
                br#"{"contents":[]}"#.to_vec()
            },
            ..Default::default()
        };
        let (body, _) = executor.prepare_body(&request, false, false).unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        let path = if interactions {
            "/generation_config/thinking_level"
        } else {
            "/generationConfig/topP"
        };
        assert_eq!(body.pointer(path), Some(&expected));
    }
}
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
            for (selected, home, compatible) in [
                (true, None, true),
                (true, Some(false), false),
                (false, Some(true), true),
                (false, None, false),
            ] {
                let calls = Arc::new(AtomicUsize::new(0));
                let registry = Arc::new(Registry::new());
                // ref: gemini_executor.go:134,834-835 @ d7914afd
                // Claude input on either account kind uses GenerateContent.
                let target = "gemini";
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
                let (body, format) = executor
                    .prepare_body(&request, stream, action == "countTokens")
                    .unwrap();
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
            let (body, format) = executor.prepare_body(&request, stream, false).unwrap();
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
        let (body, _) = executor
            .prepare_body(&request, stream, action == "countTokens")
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["owner_translation"], true);
    }
    assert_eq!(owner.0.load(Ordering::SeqCst), 3);
}
