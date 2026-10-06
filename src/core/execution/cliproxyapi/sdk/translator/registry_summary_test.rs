// ref: sdk/translator/registry_summary_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use serde_json::Value;

use super::{
    claude, gemini, openai, openai_response, Format, PluginHooks, Registry, ResponseTransform,
    TranslationContext,
};

fn translated_registry(from: Format, to: Format, output: &'static [u8]) -> Registry {
    let registry = Registry::new();
    registry.register(
        from,
        to,
        Some(Arc::new(move |_, _, _| output.to_vec())),
        ResponseTransform::default(),
    );
    registry
}

fn json(body: &[u8]) -> Value {
    serde_json::from_slice(body).unwrap()
}

#[test]
fn native_translation_applies_protocol_specific_summary_intent() {
    let chat = translated_registry(openai(), claude(), br#"{"thinking":{"type":"adaptive"}}"#);
    let output = chat.translate_request(
        &TranslationContext::default(),
        &openai(),
        &claude(),
        "claude-opus-5",
        br#"{"reasoning_effort":"high"}"#,
        false,
    );
    assert!(json(&output)["thinking"].get("display").is_none());

    let responses = translated_registry(
        openai_response(),
        claude(),
        br#"{"thinking":{"type":"adaptive"}}"#,
    );
    let effort_only = responses.translate_request(
        &TranslationContext::default(),
        &openai_response(),
        &claude(),
        "claude-opus-5",
        br#"{"reasoning":{"effort":"high"}}"#,
        false,
    );
    assert!(json(&effort_only)["thinking"].get("display").is_none());
    let enabled = responses.translate_request(
        &TranslationContext::default(),
        &openai_response(),
        &claude(),
        "claude-opus-5",
        br#"{"reasoning":{"effort":"high","summary":"auto"}}"#,
        false,
    );
    assert_eq!(json(&enabled)["thinking"]["display"], "summarized");

    let gemini_registry = translated_registry(
        openai_response(),
        gemini(),
        br#"{"generationConfig":{"thinkingConfig":{"thinkingLevel":"high"}}}"#,
    );
    let disabled = gemini_registry.translate_request(
        &TranslationContext::default(),
        &openai_response(),
        &gemini(),
        "gemini-3.6",
        br#"{"reasoning":{"summary":null}}"#,
        false,
    );
    assert_eq!(
        json(&disabled)["generationConfig"]["thinkingConfig"]["includeThoughts"],
        false
    );

    let google_override = translated_registry(
        openai(),
        gemini(),
        br#"{"generationConfig":{"thinkingConfig":{"thinkingLevel":"high","includeThoughts":true}}}"#,
    );
    let overridden = google_override.translate_request(
        &TranslationContext::default(),
        &openai(),
        &gemini(),
        "gemini-3.6",
        br#"{"reasoning_effort":"high","extra_body":{"google":{"thinking_config":{"include_thoughts":false}}}}"#,
        false,
    );
    assert_eq!(
        json(&overridden)["generationConfig"]["thinkingConfig"]["includeThoughts"],
        false
    );
}

struct RemovingNormalizer;

impl PluginHooks for RemovingNormalizer {
    fn normalize_request(
        &self,
        _: &TranslationContext,
        _: &Format,
        _: &Format,
        _: &str,
        body: Vec<u8>,
        _: bool,
    ) -> Vec<u8> {
        let mut value = json(&body);
        value["generationConfig"]["thinkingConfig"]
            .as_object_mut()
            .unwrap()
            .remove("includeThoughts");
        serde_json::to_vec(&value).unwrap()
    }
}

#[test]
fn native_normalizer_owns_the_final_provider_summary_field() {
    let registry = translated_registry(
        openai_response(),
        gemini(),
        br#"{"generationConfig":{"thinkingConfig":{"thinkingLevel":"high"}}}"#,
    );
    registry.set_plugin_hooks(Some(Arc::new(RemovingNormalizer)));
    let output = registry.translate_request(
        &TranslationContext::default(),
        &openai_response(),
        &gemini(),
        "gemini-3.6",
        br#"{"reasoning":{"summary":"auto"}}"#,
        false,
    );
    assert!(json(&output)["generationConfig"]["thinkingConfig"]
        .get("includeThoughts")
        .is_none());
}

#[test]
fn fallback_never_mixes_target_summary_fields_into_source_payload() {
    let registry = Registry::new();
    let input = br#"{"model":"old","reasoning":{"summary":"auto"},"input":"hi"}"#;
    let output = registry.translate_request(
        &TranslationContext::default(),
        &openai_response(),
        &gemini(),
        "gemini-3.6",
        input,
        false,
    );
    let value = json(&output);
    assert_eq!(value["model"], "gemini-3.6");
    assert!(value.get("generationConfig").is_none());
    assert_eq!(value["reasoning"]["summary"], "auto");
}

struct PluginTranslator {
    source_summary: Option<Value>,
    translate: bool,
}

impl PluginHooks for PluginTranslator {
    fn normalize_request(
        &self,
        _: &TranslationContext,
        _: &Format,
        _: &Format,
        _: &str,
        body: Vec<u8>,
        _: bool,
    ) -> Vec<u8> {
        let Some(summary) = &self.source_summary else {
            return body;
        };
        let mut value = json(&body);
        if summary.is_null() {
            value["reasoning"]["summary"] = Value::Null;
        } else if summary == "remove" {
            value["reasoning"]
                .as_object_mut()
                .unwrap()
                .remove("summary");
        }
        serde_json::to_vec(&value).unwrap()
    }

    fn translate_request(
        &self,
        _: &TranslationContext,
        _: &Format,
        _: &Format,
        _: &str,
        _: &[u8],
        _: bool,
    ) -> Option<Vec<u8>> {
        self.translate.then(|| {
            br#"{"generationConfig":{"thinkingConfig":{"thinkingLevel":"high"}}}"#.to_vec()
        })
    }
}

#[test]
fn plugin_translation_uses_normalized_source_intent_and_miss_stays_source_shaped() {
    for (summary, expected) in [
        (None, Some(true)),
        (Some(Value::String("remove".into())), None),
        (Some(Value::Null), Some(false)),
    ] {
        let registry = Registry::new();
        registry.set_plugin_hooks(Some(Arc::new(PluginTranslator {
            source_summary: summary,
            translate: true,
        })));
        let output = registry.translate_request(
            &TranslationContext::default(),
            &openai_response(),
            &gemini(),
            "gemini-3.6",
            br#"{"reasoning":{"summary":"auto"},"input":"hi"}"#,
            false,
        );
        assert_eq!(
            json(&output)["generationConfig"]["thinkingConfig"]["includeThoughts"].as_bool(),
            expected
        );
    }

    let miss = Registry::new();
    miss.set_plugin_hooks(Some(Arc::new(PluginTranslator {
        source_summary: None,
        translate: false,
    })));
    let output = miss.translate_request(
        &TranslationContext::default(),
        &openai_response(),
        &gemini(),
        "gemini-3.6",
        br#"{"reasoning":{"summary":"auto"},"input":"hi"}"#,
        false,
    );
    let output = json(&output);
    assert!(output.get("generationConfig").is_none());
    assert_eq!(output["reasoning"]["summary"], "auto");
}
