// ref: internal/thinking/apply_codex_usage_test.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    configuration_update::*, extract_reasoning_effort, extract_translated_reasoning_effort,
    ErrorCode, ThinkingEngine, ThinkingError, ThinkingRequest,
};
use crate::{
    internal::{
        modelconfig,
        runtime::executor::helps::{
            RequestThinkingEngine, RequestThinkingInput, RequestThinkingPipeline,
        },
    },
    sdk::translator::Registry,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn model(updates: bool, thinking: bool) -> modelconfig::ModelInfo {
    modelconfig::ModelInfo {
        id: "configured-responses".into(),
        provider_type: "codex".into(),
        support_configuration_update: updates,
        thinking: thinking.then(|| modelconfig::ThinkingSupport {
            levels: ["low", "medium", "high", "xhigh"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            zero_allowed: true,
            dynamic_allowed: true,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn pipeline() -> RequestThinkingPipeline {
    RequestThinkingPipeline::new(
        Arc::new(ThinkingEngine::default()),
        Arc::new(Registry::new()),
    )
}

#[allow(clippy::too_many_arguments)]
fn apply(
    body: &[u8],
    source: &[u8],
    name: &str,
    from: &str,
    to: &str,
    selected: Option<&modelconfig::ModelInfo>,
    changed: bool,
) -> Result<Vec<u8>, ThinkingError> {
    pipeline().apply_request_thinking(RequestThinkingInput {
        body,
        current_source_payload: source,
        original_source_payload: source,
        model: name,
        from_format: from,
        to_format: to,
        provider: to,
        normalized_updates_changed: changed,
        resolved_model_info: None,
        resolved_config_model_info: selected,
    })
}

fn string(body: &[u8], path: &str) -> String {
    crate::internal::util::get_gjson_bytes_no_copy(body, path)
        .str()
        .to_owned()
}

fn raw(body: &[u8], path: &str) -> String {
    crate::internal::util::get_gjson_bytes_no_copy(body, path)
        .json()
        .to_owned()
}

#[test]
fn candidate_thinking_updates_native_registry_preserves_cache_baseline_and_effective_usage() {
    for (input, expected) in [
        (
            r#"[{"type":"configuration_update","reasoning":{"effort":"low"}}]"#,
            "low",
        ),
        (
            r#"[{"type":"configuration_update","reasoning":{"effort":"low"}},{"type":"configuration_update","reasoning":{"effort":"medium"}}]"#,
            "medium",
        ),
        (
            r#"[{"type":"configuration_update","reasoning":{"effort":"xhigh"}},{"type":"configuration_update","reasoning":{"effort":null}},{"type":"configuration_update","reasoning":{"effort":42}},{"type":"configuration_update","reasoning":{"effort":" "}}]"#,
            "xhigh",
        ),
        (
            r#"[{"type":"configuration_update","reasoning":{"effort":"none"}}]"#,
            "none",
        ),
        (
            r#"[{"type":"configuration_update","reasoning":{"effort":"auto"}}]"#,
            "auto",
        ),
        (r#"[{"role":"user","content":"ok"}]"#, "high"),
    ] {
        let body = format!(
            r#" {{ "model":"gpt-6-astra","reasoning":{{"effort":"high","summary":"auto"}},"input":{input},"number":1.2300,"number":1e400 }} "#
        );
        for provider in ["codex", "openai-response"] {
            let output = ThinkingEngine::default()
                .apply_thinking(ThinkingRequest {
                    body: body.as_bytes(),
                    model: "gpt-6-astra",
                    from_format: provider,
                    to_format: provider,
                    provider_key: "codex",
                })
                .unwrap();
            assert_eq!(output, body.as_bytes(), "{provider}: native bytes changed");
            assert_eq!(string(&output, "reasoning.effort"), "high");
            assert_eq!(
                extract_translated_reasoning_effort(&output, provider),
                expected
            );
        }
    }
}

#[test]
fn candidate_thinking_updates_selected_native_without_thinking_keeps_baseline_until_suffix() {
    let selected = model(true, false);
    let body = br#"{"reasoning":{"effort":"xhigh","summary":"auto","other":1e400},"input":[{"type":"configuration_update","reasoning":{"effort":"low"}},{"role":"user","content":"ok"}]}"#;
    let output = apply(
        body,
        body,
        &selected.id,
        "codex",
        "codex",
        Some(&selected),
        false,
    )
    .unwrap();
    assert_eq!(output, body);
    let output = apply(
        body,
        body,
        "configured-responses(high)",
        "codex",
        "codex",
        Some(&selected),
        false,
    )
    .unwrap();
    assert_eq!(string(&output, "reasoning.effort"), "");
    assert_eq!(string(&output, "reasoning.summary"), "auto");
    assert_eq!(raw(&output, "reasoning.other"), "1e400");
    assert_eq!(raw(&output, "input"), raw(body, "input"));
}

#[test]
fn candidate_thinking_updates_native_suffix_changes_only_baseline_and_keeps_target_summary() {
    let selected = model(true, true);
    let body = br#"{"reasoning":{"effort":"xhigh","summary":"auto","other":1.2300},"input":[{"type":"configuration_update","reasoning":{"effort":"low"}},{"role":"user","content":"ok","duplicate":1,"duplicate":2}],"large":1e400}"#;
    let source = br#"{"reasoning":{"summary":"detailed"},"input":[{"type":"configuration_update","reasoning":{"effort":"medium"}}]}"#;
    for provider in ["codex", "xai", "openai-response"] {
        let output = apply(
            body,
            source,
            "configured-responses(high)",
            "openai-response",
            provider,
            Some(&selected),
            false,
        )
        .unwrap();
        assert_eq!(string(&output, "reasoning.effort"), "high");
        assert_eq!(string(&output, "reasoning.summary"), "auto");
        assert_eq!(raw(&output, "reasoning.other"), "1.2300");
        assert_eq!(raw(&output, "large"), "1e400");
        assert_eq!(raw(&output, "input"), raw(body, "input"));
        assert_eq!(
            extract_translated_reasoning_effort(&output, provider),
            "low"
        );
    }
}

#[test]
fn candidate_thinking_updates_unsupported_routing_uses_last_nonempty_and_retains_input_order() {
    let selected = model(false, true);
    for (body, effort, kept) in [
        (
            r#"{"reasoning":{"effort":"xhigh","summary":"auto","other":7},"input":[{"role":"user","content":"first"},{"type":"configuration_update","reasoning":{"effort":"low"}},{"type":"configuration_update","reasoning":{"effort":" "}},{"role":"assistant","content":"reply"},{"type":"configuration_update","reasoning":{"effort":"medium"}},{"type":"configuration_update","tools":[]},{"role":"user","content":"last"}]}"#,
            "medium",
            r#"[{"role":"user","content":"first"},{"role":"assistant","content":"reply"},{"role":"user","content":"last"}]"#,
        ),
        (
            r#"{"reasoning":{"summary":"auto"},"input":[{"type":"configuration_update","reasoning":{"effort":"low"}},{"type":"configuration_update","reasoning":{"effort":42}},{"type":"configuration_update","reasoning":{"effort":null}},{"type":"configuration_update","reasoning":{"effort":" "}},{"role":"user","content":"ok"}]}"#,
            "low",
            r#"[{"role":"user","content":"ok"}]"#,
        ),
        (
            r#"{"input":[{"type":"configuration_update","reasoning":{"effort":"low"}},{"role":"user","content":"ok"}]}"#,
            "low",
            r#"[{"role":"user","content":"ok"}]"#,
        ),
        (
            r#"{"reasoning":{"summary":"auto"},"input":[{"type":"configuration_update","tools":[]},{"role":"user","content":"ok"}]}"#,
            "",
            r#"[{"role":"user","content":"ok"}]"#,
        ),
    ] {
        for provider in ["codex", "xai", "openai-response"] {
            let output = apply(
                body.as_bytes(),
                body.as_bytes(),
                &selected.id,
                "openai-response",
                provider,
                Some(&selected),
                false,
            )
            .unwrap();
            assert_eq!(string(&output, "reasoning.effort"), effort, "{body}");
            assert_eq!(raw(&output, "input"), kept);
            if raw(body.as_bytes(), "reasoning.summary") != "" {
                assert_eq!(string(&output, "reasoning.summary"), "auto");
            }
        }
    }
    let body = br#"{"reasoning":{"effort":"xhigh","summary":"auto"},"input":[{"type":"configuration_update","reasoning":{"effort":"low"}}]}"#;
    let output = apply(
        body,
        body,
        "configured-responses(high)",
        "codex",
        "codex",
        Some(&selected),
        false,
    )
    .unwrap();
    assert_eq!(string(&output, "reasoning.effort"), "high");
    assert_eq!(raw(&output, "input"), "[]");
}

#[test]
fn candidate_thinking_updates_no_thinking_strips_only_effort_and_empty_container() {
    let selected = model(false, false);
    for (reasoning, expected) in [
        (
            r#"{"effort":"xhigh","summary":"auto","other":7}"#,
            r#"{"summary":"auto","other":7}"#,
        ),
        (r#"{"effort":"xhigh"}"#, ""),
        (r#"{"effort":null}"#, ""),
        (r#"{"effort":""}"#, ""),
        (
            r#"{"effort":"xhigh","effort":"low","summary":"auto"}"#,
            r#"{"effort":"low","summary":"auto"}"#,
        ),
    ] {
        let body = format!(
            r#"{{"reasoning":{reasoning},"input":[{{"type":"configuration_update","reasoning":{{"effort":"low"}}}}],"large":1e400}}"#
        );
        let output = apply(
            body.as_bytes(),
            body.as_bytes(),
            &selected.id,
            "codex",
            "codex",
            Some(&selected),
            false,
        )
        .unwrap();
        assert_eq!(raw(&output, "reasoning"), expected);
        assert_eq!(raw(&output, "input"), "[]");
        assert_eq!(raw(&output, "large"), "1e400");
    }
}

#[test]
fn candidate_thinking_updates_usage_precedence_and_strict_string_normalization() {
    for (effort, expected) in [
        (" LOW ", "low"),
        ("none", "none"),
        (" AUTO ", "auto"),
        ("İ", "i"),
    ] {
        let body = format!(
            r#"{{"reasoning":{{"effort":"xhigh"}},"input":[{{"type":"configuration_update","reasoning":{{"effort":"{effort}"}}}},{{"type":"configuration_update","reasoning":{{"effort":42}}}}]}}"#
        );
        for provider in ["codex", "openai-response"] {
            assert_eq!(
                extract_reasoning_effort(body.as_bytes(), provider, "opaque-route(high)"),
                expected
            );
            assert_eq!(
                extract_translated_reasoning_effort(body.as_bytes(), provider),
                expected
            );
        }
        assert_eq!(
            extract_reasoning_effort(body.as_bytes(), "xai", "opaque-route(high)"),
            "high"
        );
        assert_eq!(
            extract_translated_reasoning_effort(body.as_bytes(), "xai"),
            expected
        );
        assert_eq!(
            extract_reasoning_effort(body.as_bytes(), "openai", "opaque-route(high)"),
            "high"
        );
    }
    let body =
        br#"{"reasoning":{"effort":"xhigh"},"input":[{"type":"configuration_update","tools":[]}]}"#;
    assert_eq!(
        extract_reasoning_effort(body, "codex", "opaque-route"),
        "xhigh"
    );
    assert_eq!(extract_translated_reasoning_effort(body, "codex"), "xhigh");
}

#[test]
fn candidate_thinking_updates_cross_provider_normalized_intent_cannot_restore_removed_amount() {
    let mut selected = model(false, true);
    selected.provider_type = "gemini".into();
    let source = br#"{"reasoning":{"effort":"high"},"input":[{"type":"configuration_update","reasoning":{"effort":"low"}}]}"#;
    let target = br#"{"generationConfig":{"thinkingConfig":{"thinkingLevel":"medium"}}}"#;
    let unnormalized = apply(
        target,
        source,
        &selected.id,
        "openai-response",
        "gemini",
        Some(&selected),
        false,
    )
    .unwrap();
    assert_eq!(
        string(
            &unnormalized,
            "generationConfig.thinkingConfig.thinkingLevel"
        ),
        "low"
    );
    let changed = apply(
        target,
        source,
        &selected.id,
        "openai-response",
        "gemini",
        Some(&selected),
        true,
    )
    .unwrap();
    assert_eq!(
        string(&changed, "generationConfig.thinkingConfig.thinkingLevel"),
        "medium"
    );
    let removed = apply(
        b"{}",
        source,
        &selected.id,
        "openai-response",
        "gemini",
        Some(&selected),
        true,
    )
    .unwrap();
    assert_eq!(removed, b"{}");
}

#[test]
fn candidate_thinking_updates_responses_normalized_working_body_wins_over_stale_source() {
    let selected = model(false, true);
    let source = br#"{"reasoning":{"effort":"high"},"input":[{"type":"configuration_update","reasoning":{"effort":"high"}}]}"#;
    let target = br#"{"input":[{"type":"configuration_update","reasoning":{"effort":"low"}},{"role":"user","content":"kept"}]}"#;
    let output = apply(
        target,
        source,
        &selected.id,
        "codex",
        "codex",
        Some(&selected),
        true,
    )
    .unwrap();
    assert_eq!(string(&output, "reasoning.effort"), "low");
    assert_eq!(
        raw(&output, "input"),
        r#"[{"role":"user","content":"kept"}]"#
    );
    let removed = br#"{"input":[{"role":"user","content":"kept"}]}"#;
    assert_eq!(
        apply(
            removed,
            source,
            &selected.id,
            "codex",
            "codex",
            Some(&selected),
            true
        )
        .unwrap(),
        removed
    );
    let native = model(true, true);
    assert_eq!(
        apply(
            target,
            source,
            &native.id,
            "codex",
            "codex",
            Some(&native),
            true
        )
        .unwrap(),
        target
    );
}

#[test]
fn candidate_thinking_updates_malformed_targets_and_nonarray_inputs_are_not_rebuilt() {
    let selected = model(false, true);
    let source = br#"{"input":[{"type":"configuration_update","reasoning":{"effort":"low"}}]}"#;
    for target in [&b"{\"input\":["[..], &b"\xff"[..]] {
        assert_eq!(
            apply(
                target,
                source,
                &selected.id,
                "codex",
                "codex",
                Some(&selected),
                false
            )
            .unwrap(),
            target
        );
        assert_eq!(strip_configuration_updates(target), target);
        assert_eq!(extract_translated_reasoning_effort(target, "codex"), "");
    }
    for body in [
        &br#"{"reasoning":{"summary":"auto"},"input":{"type":"configuration_update","reasoning":{"effort":"low"}}}"#[..],
        &br#"{"reasoning":{"summary":"auto"}}"#[..],
    ] {
        assert_eq!(apply(body, body, &selected.id, "codex", "codex", Some(&selected), false).unwrap(), body);
    }
    let native = model(true, true);
    let body = br#"{"reasoning":{"generate_summary":"auto"},"input":[{"type":"configuration_update","reasoning":{"effort":"low"}}]}"#;
    assert_eq!(
        apply(
            body,
            body,
            "configured-responses(invalid)",
            "codex",
            "codex",
            Some(&native),
            false
        )
        .unwrap(),
        body
    );
}

#[test]
fn candidate_thinking_updates_raw_items_and_first_duplicate_members_are_preserved() {
    let body = br#" {"input":[{"type":"configuration_update","type":"message","reasoning":{"effort":"low","effort":"high"}},{"type":"message","type":"configuration_update","n":1.2300,"n":1e400},{"type":"configuration_update","reasoning":{"effort":null,"effort":"high"}},{"type":"configuration_update","reasoning":{"effort":"medium"},"reasoning":{"effort":"high"}}],"input":[],"outside":900719925474099312345} "#;
    assert_eq!(extract_translated_reasoning_effort(body, "codex"), "medium");
    let output = strip_configuration_updates(body);
    assert_eq!(
        raw(&output, "input"),
        r#"[{"type":"message","type":"configuration_update","n":1.2300,"n":1e400}]"#
    );
    assert!(std::str::from_utf8(&output)
        .unwrap()
        .contains(r#""input":[],"outside":900719925474099312345"#));
    for untouched in [
        br#"{"input":[]}"#.as_slice(),
        br#"{"input":"value"}"#.as_slice(),
        br#"{"outside":1e400}"#.as_slice(),
    ] {
        assert_eq!(strip_configuration_updates(untouched), untouched);
    }
}

#[test]
fn candidate_thinking_updates_failed_validation_retains_cleaned_target_without_debug_payload() {
    let selected = model(false, true);
    let body = br#"{"input":[{"type":"configuration_update","reasoning":{"effort":"not-supported"}},{"role":"user","content":"private-prompt-canary"}],"outside":1.2300}"#;
    let error = apply(
        body,
        body,
        &selected.id,
        "codex",
        "codex",
        Some(&selected),
        false,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::LevelNotSupported);
    let target = error.target_body().expect("cleaned target retained");
    assert_eq!(
        raw(target, "input"),
        r#"[{"role":"user","content":"private-prompt-canary"}]"#
    );
    assert_eq!(raw(target, "outside"), "1.2300");
    assert!(!format!("{error:?}").contains("private-prompt-canary"));
    assert!(!error.to_string().contains("private-prompt-canary"));
}

struct ClearUpdatesProcessor(Arc<AtomicUsize>);
impl crate::internal::runtime::executor::helps::CodexMultiAgentV2Processor
    for ClearUpdatesProcessor
{
    fn rewrite_spawn_agent_description(
        &self,
        _: &crate::sdk::cliproxy::executor::Headers,
        payload: &[u8],
    ) -> Vec<u8> {
        payload.to_vec()
    }
    fn rewrite_input(
        &self,
        _: &crate::sdk::cliproxy::executor::Headers,
        payload: &[u8],
    ) -> Vec<u8> {
        payload.to_vec()
    }
    fn translate_request(
        &self,
        _: &crate::sdk::cliproxy::executor::Headers,
        _: &crate::sdk::translator::Format,
        _: &crate::sdk::translator::Format,
        _: &str,
        _: &[u8],
        _: bool,
    ) -> Vec<u8> {
        panic!("Meta must retain the envelope update decision")
    }
    fn optimize_request(
        &self,
        _: &crate::sdk::cliproxy::executor::Headers,
        payload: &[u8],
    ) -> (Vec<u8>, bool) {
        (payload.to_vec(), false)
    }
    fn restore_response(&self, payload: &[u8], _: bool) -> Vec<u8> {
        payload.to_vec()
    }
    fn translate_request_envelope(
        &self,
        _: &crate::sdk::cliproxy::executor::Headers,
        _: &crate::sdk::translator::Format,
        to: &crate::sdk::translator::Format,
        mut request: crate::sdk::translator::RequestEnvelope,
    ) -> crate::sdk::translator::RequestEnvelope {
        assert!(!request.configuration_updates_changed);
        let call = self.0.fetch_add(1, Ordering::SeqCst);
        request.body = if call == 0 {
            br#"{"reasoning":{"effort":"medium"}}"#.to_vec()
        } else {
            b"{}".to_vec()
        };
        request.format = to.clone();
        request.configuration_updates_changed = call != 0;
        request
    }
}

#[test]
fn candidate_thinking_updates_meta_preparation_forwards_working_normalizer_decision() {
    use crate::internal::runtime::executor::{
        helps::PayloadApplyConfig, meta_executor_request::MetaRequestOwner,
    };
    use crate::sdk::pluginapi::ExecutorRequest;
    let calls = Arc::new(AtomicUsize::new(0));
    let owner = MetaRequestOwner {
        processor: Arc::new(ClearUpdatesProcessor(Arc::clone(&calls))),
        thinking: Arc::new(pipeline()),
        config: Arc::new(PayloadApplyConfig::default()),
    };
    let request = ExecutorRequest {
        model: "configured-responses".into(), source_format: "codex".into(), format: "codex".into(),
        payload: br#"{"reasoning":{"effort":"high"},"input":[{"type":"configuration_update","reasoning":{"effort":"low"}}]}"#.to_vec(),
        original_request: br#"{"reasoning":{"effort":"medium"}}"#.to_vec(),
        resolved_model_info: Some(Arc::new(model(false, true))),
        ..Default::default()
    };
    let prepared = owner.prepare(&request, false).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(string(&prepared.body, "reasoning.effort"), "");
    assert_eq!(string(&prepared.body, "model"), "configured-responses");
}
