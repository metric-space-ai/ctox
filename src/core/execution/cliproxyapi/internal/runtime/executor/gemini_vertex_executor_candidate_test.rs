// ref: internal/runtime/executor/gemini_vertex_executor.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
#[test]
fn candidate_google_preflight_vertex_repairs_signatures_and_count_boundaries() {
    let executor = GeminiVertexExecutor::new(Arc::new(Registry::new()), None);
    let request = ExecutorRequest {
        model:"gemini-test".into(),
        source_format:"gemini".into(),
        payload:br#"{"contents":[{"role":"model","parts":[{"functionCall":{"name":"run"},"thoughtSignature":"claude#invalid"}]}],"tools":[],"generationConfig":{"temperature":0.7},"safetySettings":[]}"#.to_vec(),
        ..Default::default()
    };
    for (stream, count, len) in [(false, false, 3), (true, false, 3), (false, true, 2)] {
        let (body, format) = executor.prepare_body(&request, stream, count).unwrap();
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
        if count {
            assert_eq!(body["contents"][1]["role"], "model");
            assert!(body.get("tools").is_none());
            assert!(body.get("generationConfig").is_none());
        } else {
            assert_eq!(body["contents"][2]["role"], "user");
        }
    }
}

use super::super::helps::{PayloadApplyConfig, PayloadModelRule, PayloadRule};

#[test]
fn candidate_google_payload_vertex_translates_original_first_and_keeps_count_single_pass() {
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let registry = Arc::new(Registry::new());
    let observed = calls.clone();
    registry.register(
        Format::from("gemini"),
        Format::from("gemini"),
        Some(Arc::new(move |_, payload, _| {
            let body: Value = serde_json::from_slice(payload).unwrap();
            observed
                .lock()
                .unwrap()
                .push(body["marker"].as_str().unwrap().to_owned());
            payload.to_vec()
        })),
        ResponseTransform::default(),
    );
    let mut config = PayloadApplyConfig::default();
    config.rules.default.push(PayloadRule {
        models: vec![PayloadModelRule {
            name: "public-alias".into(),
            protocol: "gemini".into(),
            from_protocol: "gemini".into(),
            headers: BTreeMap::from([("X-Scope".into(), "blue".into())]),
            ..Default::default()
        }],
        params: BTreeMap::from([
            (
                "generationConfig.temperature".into(),
                serde_json::json!(0.3),
            ),
            ("central_default".into(), Value::Bool(true)),
        ]),
    });
    let executor = GeminiVertexExecutor::new(registry, None).with_payload_config(Arc::new(config));
    let request = ExecutorRequest {
        model: "upstream-name".into(),
        source_format: "gemini".into(),
        payload: br#"{"contents":[],"marker":"working"}"#.to_vec(),
        original_request:
            br#"{"contents":[],"marker":"original","generationConfig":{"temperature":0.9}}"#
                .to_vec(),
        metadata: BTreeMap::from([(
            "requested_model".into(),
            Value::String(" public-alias ".into()),
        )]),
        headers: BTreeMap::from([("x-scope".into(), vec!["blue".into()])]),
        ..Default::default()
    };
    let (body, _) = executor.prepare_body(&request, false, false).unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert!(body.pointer("/generationConfig/temperature").is_none());
    assert_eq!(body["central_default"], true);
    let (count, _) = executor.prepare_body(&request, false, true).unwrap();
    let count: Value = serde_json::from_slice(&count).unwrap();
    assert!(count.get("central_default").is_none());
    assert!(count.get("generationConfig").is_none());
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        ["original", "working", "working"]
    );
}
use super::*;
use crate::internal::modelconfig::{HomeModelOptions, ModelInfo};
use crate::sdk::translator::ResponseTransform;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn candidate_google_request_vertex_keeps_ordinary_translation_and_normalizes_before_conversion() {
    let calls = Arc::new(AtomicUsize::new(0));
    let registry = Arc::new(Registry::new());
    let observed = calls.clone();
    registry.register(Format::from("claude"), Format::from("gemini"), Some(Arc::new(move |_, payload, _| {
        let payload: Value = serde_json::from_slice(payload).unwrap();
        assert_eq!(payload.pointer("/tools/0/parameters/properties/limit/type").and_then(Value::as_str), Some("integer"));
        observed.fetch_add(1, Ordering::SeqCst);
        br#"{"contents":[],"ordinary_vertex_translation":true,"tools":[],"generationConfig":{},"safetySettings":[]}"#.to_vec()
    })), ResponseTransform::default());
    let executor = GeminiVertexExecutor::new(registry, None);
    let request = ExecutorRequest {
        model: "gemini-test".into(), source_format: "claude".into(),
        headers: BTreeMap::from([("uSeR-aGeNt".into(), vec!["codex-test".into()])]),
        payload: br#"{"tools":[{"type":"function","name":"read_thread","parameters":{"properties":{"limit":{"type":"number"}}}}]}"#.to_vec(),
        resolved_model_info: Some(Arc::new(ModelInfo { is_compat: true, ..Default::default() })),
        resolved_home_model_options: Some(HomeModelOptions { is_compat: true, ..Default::default() }),
        ..Default::default()
    };
    for (stream, count) in [(false, false), (true, false), (false, true)] {
        let (body, format) = executor.prepare_body(&request, stream, count).unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(format.as_str(), "gemini");
        assert_eq!(body["ordinary_vertex_translation"], true);
        assert_eq!(body.get("tools").is_some(), !count);
        assert_eq!(body.get("generationConfig").is_some(), !count);
        assert_eq!(body.get("safetySettings").is_some(), !count);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 5);
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
        _: &Format,
        to: &Format,
        _: &str,
        _: &[u8],
        _: bool,
    ) -> Vec<u8> {
        assert_eq!(to.as_str(), "gemini");
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
fn candidate_google_request_vertex_uses_the_host_owned_processor() {
    let owner = Arc::new(Owner(AtomicUsize::new(0)));
    let executor = GeminiVertexExecutor::new(Arc::new(Registry::new()), None)
        .with_request_processor(owner.clone());
    let request = ExecutorRequest {
        model: "gemini-test".into(),
        source_format: "gemini".into(),
        payload: br#"{"contents":[]}"#.to_vec(),
        ..Default::default()
    };
    for (stream, count) in [(false, false), (true, false), (false, true)] {
        let (body, _) = executor.prepare_body(&request, stream, count).unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["owner_translation"], true);
    }
    assert_eq!(owner.0.load(Ordering::SeqCst), 5);
}
