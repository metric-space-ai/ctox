// ref: internal/runtime/executor/helps/model_capabilities_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// ref: internal/runtime/executor/helps/{thinking_providers,codex_multi_agent_v2}.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::cell::RefCell;

use crate::internal::registry::{lookup_model_info, ModelInfo};
use crate::internal::thinking::ThinkingError;
use crate::sdk::cliproxy::executor::{Headers, Options, Request};
use crate::sdk::translator::Format;

use super::codex_multi_agent_v2::{
    optimize_codex_multi_agent_v2_request, restore_codex_multi_agent_v2_response,
    rewrite_codex_multi_agent_v2_input, rewrite_codex_spawn_agent_description,
    translate_request_pair_with_api_key_model_compatibility_and_update_intent,
    translate_request_with_codex_multi_agent_v2,
    translate_request_with_codex_multi_agent_v2_for_executor, CodexMultiAgentV2Processor,
};
use super::model_capabilities::{
    apply_request_thinking, RequestThinkingEngine, RequestThinkingInput, RequestThinkingRoute,
};
use super::thinking_providers::{is_upstream_thinking_provider, THINKING_PROVIDER_MODULES};

#[test]
fn force_link_provider_manifest_matches_pinned_blank_imports() {
    assert_eq!(
        THINKING_PROVIDER_MODULES,
        [
            "antigravity",
            "claude",
            "codex",
            "gemini",
            "interactions",
            "kimi",
            "openai",
            "xai",
        ]
    );
    for provider in THINKING_PROVIDER_MODULES {
        assert!(is_upstream_thinking_provider(provider));
        assert!(is_upstream_thinking_provider(&provider.to_uppercase()));
    }
    assert!(!is_upstream_thinking_provider("openrouter"));
    assert!(!is_upstream_thinking_provider(""));
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapturedThinkingInput {
    body: Vec<u8>,
    current: Vec<u8>,
    original: Vec<u8>,
    model: String,
    from: String,
    to: String,
    provider: String,
    resolved_model_id: Option<String>,
}

#[derive(Default)]
struct CapturingThinkingEngine {
    captured: RefCell<Option<CapturedThinkingInput>>,
}

impl RequestThinkingEngine for CapturingThinkingEngine {
    fn apply_request_thinking(
        &self,
        input: RequestThinkingInput<'_>,
    ) -> Result<Vec<u8>, ThinkingError> {
        self.captured.replace(Some(CapturedThinkingInput {
            body: input.body.to_vec(),
            current: input.current_source_payload.to_vec(),
            original: input.original_source_payload.to_vec(),
            model: input.model.to_owned(),
            from: input.from_format.to_owned(),
            to: input.to_format.to_owned(),
            provider: input.provider.to_owned(),
            resolved_model_id: input.resolved_model_info.map(|model| model.id.to_owned()),
        }));
        Ok(input.body.to_vec())
    }
}

#[test]
fn request_thinking_forwards_exact_capability_and_original_source_precedence() {
    let engine = CapturingThinkingEngine::default();
    let request = Request {
        model: "claude-sonnet-4-6(high)".to_owned(),
        payload: br#"{"source":1.2300,"reasoning":{"summary":"auto"}}"#.to_vec(),
        ..Request::default()
    };
    let options = Options {
        original_request: br#" { "original" : 900719925474099312345 } "#.to_vec(),
        ..Options::default()
    };
    let model = lookup_model_info("claude-sonnet-4-6", "claude").unwrap();
    let body = br#"{"translated":true}"#;
    let result = apply_request_thinking(
        &engine,
        body,
        &request,
        &options,
        RequestThinkingRoute {
            from_format: "openai-response",
            to_format: "claude",
            provider: "claude-api-key",
            resolved_model_info: Some(&model),
        },
    )
    .unwrap();
    assert_eq!(result, body);
    assert_eq!(
        engine.captured.borrow().as_ref().unwrap(),
        &CapturedThinkingInput {
            body: body.to_vec(),
            current: request.payload.clone(),
            original: options.original_request.clone(),
            model: request.model.clone(),
            from: "openai-response".to_owned(),
            to: "claude".to_owned(),
            provider: "claude-api-key".to_owned(),
            resolved_model_id: Some("claude-sonnet-4-6".to_owned()),
        }
    );
}

#[test]
fn request_thinking_falls_back_to_current_payload_without_fabricating_capability() {
    let engine = CapturingThinkingEngine::default();
    let request = Request {
        model: "unknown-model".to_owned(),
        payload: b" source bytes ".to_vec(),
        ..Request::default()
    };
    apply_request_thinking(
        &engine,
        b"target bytes",
        &request,
        &Options::default(),
        RequestThinkingRoute {
            from_format: "openai",
            to_format: "xai",
            provider: "xai",
            resolved_model_info: None,
        },
    )
    .unwrap();
    let captured = engine.captured.borrow();
    let captured = captured.as_ref().unwrap();
    assert_eq!(captured.current, request.payload);
    assert_eq!(captured.original, request.payload);
    assert_eq!(captured.resolved_model_id, None);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CodexCall {
    Spawn(Headers, Vec<u8>),
    Input(Headers, Vec<u8>),
    Translate(Headers, String, String, String, Vec<u8>, bool),
    Optimize(Headers, Vec<u8>),
    Restore(Vec<u8>, bool),
}

#[derive(Default)]
struct CapturingCodexProcessor {
    calls: RefCell<Vec<CodexCall>>,
}

impl CodexMultiAgentV2Processor for CapturingCodexProcessor {
    fn rewrite_spawn_agent_description(&self, headers: &Headers, payload: &[u8]) -> Vec<u8> {
        self.calls
            .borrow_mut()
            .push(CodexCall::Spawn(headers.clone(), payload.to_vec()));
        payload.to_vec()
    }

    fn rewrite_input(&self, headers: &Headers, payload: &[u8]) -> Vec<u8> {
        self.calls
            .borrow_mut()
            .push(CodexCall::Input(headers.clone(), payload.to_vec()));
        payload.to_vec()
    }

    fn translate_request(
        &self,
        headers: &Headers,
        from: &Format,
        to: &Format,
        model: &str,
        payload: &[u8],
        stream: bool,
    ) -> Vec<u8> {
        self.calls.borrow_mut().push(CodexCall::Translate(
            headers.clone(),
            from.as_str().to_owned(),
            to.as_str().to_owned(),
            model.to_owned(),
            payload.to_vec(),
            stream,
        ));
        payload.to_vec()
    }

    fn optimize_request(&self, headers: &Headers, payload: &[u8]) -> (Vec<u8>, bool) {
        self.calls
            .borrow_mut()
            .push(CodexCall::Optimize(headers.clone(), payload.to_vec()));
        (payload.to_vec(), true)
    }

    fn restore_response(&self, payload: &[u8], optimized: bool) -> Vec<u8> {
        self.calls
            .borrow_mut()
            .push(CodexCall::Restore(payload.to_vec(), optimized));
        payload.to_vec()
    }
}

#[test]
fn codex_multi_agent_wrappers_are_byte_and_field_transparent() {
    let processor = CapturingCodexProcessor::default();
    let headers = Headers::from([(
        "uSeR-aGeNt".to_owned(),
        vec![" Codex Desktop/42.0 ".to_owned()],
    )]);
    let payload = br#" { "input" : [{"type":"agent_message","content":[{"type":"encrypted_content","encrypted_content":"x\\ny"}]}], "n":1.2300, "big":900719925474099312345 } "#;
    let from = Format::from("openai-response");
    let to = Format::from("claude");

    assert_eq!(
        rewrite_codex_spawn_agent_description(&processor, &headers, payload),
        payload
    );
    assert_eq!(
        rewrite_codex_multi_agent_v2_input(&processor, &headers, payload),
        payload
    );
    assert_eq!(
        translate_request_with_codex_multi_agent_v2(
            &processor,
            &headers,
            &from,
            &to,
            "gpt-5.5-codex",
            payload,
            true,
        ),
        payload
    );
    assert_eq!(
        optimize_codex_multi_agent_v2_request(&processor, &headers, payload),
        (payload.to_vec(), true)
    );
    assert_eq!(
        restore_codex_multi_agent_v2_response(&processor, payload, true),
        payload
    );

    assert_eq!(
        processor.calls.into_inner(),
        vec![
            CodexCall::Spawn(headers.clone(), payload.to_vec()),
            CodexCall::Input(headers.clone(), payload.to_vec()),
            CodexCall::Translate(
                headers.clone(),
                "openai-response".to_owned(),
                "claude".to_owned(),
                "gpt-5.5-codex".to_owned(),
                payload.to_vec(),
                true,
            ),
            CodexCall::Optimize(headers, payload.to_vec()),
            CodexCall::Restore(payload.to_vec(), true),
        ]
    );
}

#[test]
fn api_key_compatibility_normalizes_reserved_integers_before_the_processor() {
    let processor = CapturingCodexProcessor::default();
    let headers = Headers::from([(
        "User-Agent".to_owned(),
        vec!["Codex Desktop/1.0".to_owned()],
    )]);
    let payload = br#"{"tools":[{"type":"function","name":"exec_command","parameters":{"type":"object","properties":{"yield_time_ms":{"type":"number"}}}}]}"#;
    let from = Format::from("openai-response");
    let to = Format::from("claude");
    let translated = translate_request_with_codex_multi_agent_v2_for_executor(
        &processor, &headers, "claude", &from, &to, "gpt-5.5", payload, false,
    );
    let seen: serde_json::Value = serde_json::from_slice(&translated).unwrap();
    assert_eq!(
        seen["tools"][0]["parameters"]["properties"]["yield_time_ms"]["type"],
        "integer"
    );
    let codex_target = translate_request_with_codex_multi_agent_v2_for_executor(
        &processor, &headers, "codex", &from, &to, "gpt-5.5", payload, false,
    );
    assert_eq!(codex_target, payload);
    let (original, working, updates_changed) =
        translate_request_pair_with_api_key_model_compatibility_and_update_intent(
            &processor, &headers, "claude", &from, &to, "gpt-5.5", payload, payload, false, true,
        );
    assert!(!updates_changed);
    assert_eq!(original, working);
    assert!(!std::ptr::eq(original.as_ptr(), working.as_ptr()));
    let other = br#"{"tools":[{"type":"function","name":"exec_command","parameters":{"type":"object","properties":{"yield_time_ms":{"type":"number"},"cmd":{"type":"string"}}}}]}"#;
    let (_original, _working, updates_changed) =
        translate_request_pair_with_api_key_model_compatibility_and_update_intent(
            &processor, &headers, "claude", &from, &to, "gpt-5.5", payload, other, false, false,
        );
    assert!(!updates_changed);
    let calls = processor.calls.borrow();
    assert_eq!(calls.len(), 5);
}

#[test]
fn model_info_type_remains_the_canonical_registry_type() {
    fn accepts_registry_model(_: Option<&ModelInfo>) {}
    let model = lookup_model_info("claude-sonnet-4-6", "claude").unwrap();
    accepts_registry_model(Some(&model));
}
