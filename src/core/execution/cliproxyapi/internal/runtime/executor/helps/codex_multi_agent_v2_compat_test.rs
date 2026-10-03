// ref: internal/runtime/executor/helps/codex_multi_agent_v2.go @ e2bff010
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    translate_request_pair_with_api_key_model_compatibility_and_update_intent as pair,
    translate_request_with_api_key_model_compatibility_and_update_intent_for_executor as translate,
    RegistryCodexMultiAgentV2Processor,
};
use crate::internal::client::codex::optimize_multi_agent_v2::{
    MultiAgentV2Context, SpawnAgentModelMetadata,
};
use crate::sdk::cliproxy::executor::Headers;
use crate::sdk::translator::{
    Format, PluginHooks, Registry, ResponseTransform, TranslationContext,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Default)]
struct Normalizer {
    calls: AtomicUsize,
    replacement: Option<Vec<u8>>,
}
impl PluginHooks for Normalizer {
    fn normalize_request(
        &self,
        _: &TranslationContext,
        _: &Format,
        _: &Format,
        _: &str,
        body: Vec<u8>,
        _: bool,
    ) -> Vec<u8> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.replacement.clone().unwrap_or(body)
    }
}
fn metadata(_: &str) -> Option<SpawnAgentModelMetadata> {
    None
}
fn processor<'a>(
    registry: &'a Registry,
    context: &'a TranslationContext,
    client: &'a MultiAgentV2Context,
) -> RegistryCodexMultiAgentV2Processor<'a> {
    RegistryCodexMultiAgentV2Processor {
        registry,
        context,
        client,
        model_metadata: &metadata,
        orphan_delegation_compatibility: true,
    }
}

#[test]
fn compatibility_dispatches_six_protocol_families_without_the_registered_normal_transform() {
    let claude = br#"{"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"hidden-thought","signature":""},{"type":"text","text":"visible"}]}],"max_tokens":512}"#;
    let chat = br#"{"messages":[{"role":"assistant","reasoning_content":"hidden-thought","content":"visible"}]}"#;
    let responses = br#"{"input":[{"type":"reasoning","summary":[{"type":"summary_text","text":"hidden-thought"}]},{"type":"message","role":"assistant","content":[{"type":"output_text","text":"visible"}]}]}"#;
    for (from, to, body, field) in [
        ("claude", "codex", claude.as_slice(), "input"),
        ("claude", "gemini", claude.as_slice(), "contents"),
        ("claude", "interactions", claude.as_slice(), "input"),
        ("claude", "openai", claude.as_slice(), "messages"),
        ("openai", "claude", chat.as_slice(), "messages"),
        (
            "openai-response",
            "claude",
            responses.as_slice(),
            "messages",
        ),
    ] {
        let registry = Registry::new();
        let normalizer = Arc::new(Normalizer::default());
        registry.set_plugin_hooks(Some(normalizer.clone()));
        let from = Format::from(from);
        let to = Format::from(to);
        registry.register(
            from.clone(),
            to.clone(),
            Some(Arc::new(|_, _, _| {
                br#"{"normal-translator":true}"#.to_vec()
            })),
            ResponseTransform {
                stream: None,
                non_stream: None,
                token_count: None,
            },
        );
        let context = TranslationContext::default();
        let client = MultiAgentV2Context::default();
        let processor = processor(&registry, &context, &client);
        let headers = Headers::new();
        let (normal, _) = translate(
            &processor, &headers, "", &from, &to, "gpt-test", body, false, false,
        );
        assert!(
            serde_json::from_slice::<serde_json::Value>(&normal).unwrap()["normal-translator"]
                .as_bool()
                .unwrap()
        );
        let (compat, changed) = translate(
            &processor, &headers, "", &from, &to, "gpt-test", body, false, true,
        );
        let value: serde_json::Value = serde_json::from_slice(&compat).unwrap();
        assert!(value.get(field).is_some(), "{from} -> {to}: {value}");
        assert!(value.get("normal-translator").is_none());
        assert!(
            std::str::from_utf8(&compat)
                .unwrap()
                .contains("hidden-thought"),
            "{from} -> {to}: {value}"
        );
        assert!(!changed);
        assert_eq!(normalizer.calls.load(Ordering::SeqCst), 2);
    }
}

#[test]
fn codex_envelope_routes_preserve_update_intent_even_when_compatibility_is_enabled() {
    let registry = Registry::new();
    registry.set_plugin_hooks(Some(Arc::new(Normalizer {
        calls: AtomicUsize::new(0),
        replacement: Some(br#"{"input":[{"type":"configuration_update","tools":[2]}]}"#.to_vec()),
    })));
    let context = TranslationContext::default();
    let client = MultiAgentV2Context::default();
    let processor = processor(&registry, &context, &client);
    let body = br#"{"input":[{"type":"configuration_update","tools":[1]}]}"#;
    let from = Format::from("openai-response");
    for compat in [false, true] {
        let (_, changed) = translate(
            &processor,
            &Headers::new(),
            "",
            &from,
            &Format::from("codex"),
            "m",
            body,
            false,
            compat,
        );
        assert!(changed);
    }
}

#[test]
fn explicit_compatibility_facades_do_not_report_envelope_update_intent() {
    let registry = Registry::new();
    registry.set_plugin_hooks(Some(Arc::new(Normalizer {
        calls: AtomicUsize::new(0),
        replacement: Some(br#"{"input":[{"type":"configuration_update","tools":[2]}]}"#.to_vec()),
    })));
    let context = TranslationContext::default();
    let client = MultiAgentV2Context::default();
    let processor = processor(&registry, &context, &client);
    let (_, changed) = translate(
        &processor,
        &Headers::new(),
        "",
        &Format::from("claude"),
        &Format::from("codex"),
        "m",
        br#"{"messages":[]}"#,
        false,
        true,
    );
    assert!(!changed);
}

#[test]
fn identical_backing_buffers_invoke_normalizers_once_and_keep_independent_working_bytes() {
    let registry = Registry::new();
    let hook = Arc::new(Normalizer {
        calls: AtomicUsize::new(0),
        replacement: Some(br#"{"input":[{"type":"configuration_update","tools":[2]}]}"#.to_vec()),
    });
    registry.set_plugin_hooks(Some(hook.clone()));
    let context = TranslationContext::default();
    let client = MultiAgentV2Context::default();
    let processor = processor(&registry, &context, &client);
    let body = br#"{"input":[{"type":"configuration_update","tools":[1]}]}"#;
    let format = Format::from("openai-response");
    let (original, mut working, changed) = pair(
        &processor,
        &Headers::new(),
        "",
        &format,
        &format,
        "m",
        body,
        body,
        false,
        false,
    );
    assert!(changed);
    assert_eq!(original, working);
    assert_ne!(original.as_ptr(), working.as_ptr());
    working[0] = b' ';
    assert_eq!(original[0], b'{');
    assert_eq!(hook.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn distinct_buffers_return_the_working_update_decision() {
    let registry = Registry::new();
    let after = br#"{"input":[{"type":"configuration_update","tools":[2]}]}"#;
    let hook = Arc::new(Normalizer {
        calls: AtomicUsize::new(0),
        replacement: Some(after.to_vec()),
    });
    registry.set_plugin_hooks(Some(hook.clone()));
    let context = TranslationContext::default();
    let client = MultiAgentV2Context::default();
    let processor = processor(&registry, &context, &client);
    let format = Format::from("openai-response");
    let (_, _, changed) = pair(
        &processor,
        &Headers::new(),
        "",
        &format,
        &format,
        "m",
        br#"{"input":[{"type":"configuration_update","tools":[1]}]}"#,
        after,
        false,
        false,
    );
    assert!(!changed);
    assert_eq!(hook.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn registry_envelopes_apply_the_configured_orphan_rule_before_native_codex_translation() {
    let registry = Registry::new();
    let context = TranslationContext::default();
    let client = MultiAgentV2Context::default();
    let processor = processor(&registry, &context, &client);
    let headers = Headers::from([("x-openai-subagent".into(), vec!["COLLAB_SPAWN".into()])]);
    let body = br#"{"input":[{"type":"function_call_output","namespace":"codex_app","name":"create_thread","output":"x"}]}"#;
    let (out, changed) = translate(
        &processor,
        &headers,
        "codex",
        &Format::from("openai-response"),
        &Format::from("codex"),
        "m",
        body,
        false,
        false,
    );
    let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["input"][0]["type"], "message");
    assert_eq!(value["input"][0]["role"], "user");
    assert!(!changed);
}
