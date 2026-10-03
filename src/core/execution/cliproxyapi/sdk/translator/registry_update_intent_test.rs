// Origin: CTOX
// ref: sdk/translator/registry.go:118-195 @ e2bff0107bb307337aaa19018ccddd55f64253d5
// License: AGPL-3.0-only

use std::sync::Arc;

use super::{
    claude, openai_response, Format, Pipeline, PluginHooks, Registry, RequestEnvelope,
    ResponseTransform, TranslationContext,
};

fn response() -> ResponseTransform {
    ResponseTransform {
        stream: None,
        non_stream: None,
        token_count: None,
    }
}

fn envelope(format: &Format, body: &[u8]) -> RequestEnvelope {
    RequestEnvelope {
        format: format.clone(),
        model: "m".into(),
        stream: false,
        body: body.to_vec(),
        configuration_updates_changed: false,
    }
}

struct ReplaceBody(Vec<u8>);

impl PluginHooks for ReplaceBody {
    fn normalize_request(
        &self,
        _: &TranslationContext,
        _: &Format,
        _: &Format,
        _: &str,
        _: Vec<u8>,
        _: bool,
    ) -> Vec<u8> {
        self.0.clone()
    }
}

#[test]
fn normalizer_edits_report_update_intent_for_native_and_fallback_routes() {
    let before = br#"{"input":[{"type":"configuration_update","tools":[1]}]}"#;
    let after = br#"{"input":[{"type":"configuration_update","tools":[2]}]}"#;
    for native in [false, true] {
        let registry = Registry::new();
        let format = openai_response();
        if native {
            registry.register(
                format.clone(),
                format.clone(),
                Some(Arc::new(|_, body, _| body.to_vec())),
                response(),
            );
        }
        registry.set_plugin_hooks(Some(Arc::new(ReplaceBody(after.to_vec()))));
        let result = registry.translate_request_envelope(
            &TranslationContext::default(),
            &format,
            &format,
            envelope(&format, before),
        );
        assert_eq!(result.body, after);
        assert!(result.configuration_updates_changed);
        assert_eq!(result.format, format);
    }
}

#[test]
fn native_cross_protocol_removal_is_not_a_plugin_configuration_edit() {
    let registry = Registry::new();
    let from = openai_response();
    let to = claude();
    let after = br#"{"messages":[]}"#;
    registry.register(
        from.clone(),
        to.clone(),
        Some(Arc::new(move |_, _, _| after.to_vec())),
        response(),
    );
    registry.set_plugin_hooks(Some(Arc::new(ReplaceBody(after.to_vec()))));
    let result = registry.translate_request_envelope(
        &TranslationContext::default(),
        &from,
        &to,
        envelope(
            &from,
            br#"{"input":[{"type":"configuration_update","tools":[1]}]}"#,
        ),
    );
    assert!(!result.configuration_updates_changed);
    assert_eq!(result.body, after);
}

#[test]
fn ordinary_message_normalizer_edits_leave_update_intent_false() {
    let registry = Registry::new();
    let format = openai_response();
    registry.set_plugin_hooks(Some(Arc::new(ReplaceBody(
        br#"{"input":[{"type":"message","content":"after"}]}"#.to_vec(),
    ))));
    let result = registry.translate_request_envelope(
        &TranslationContext::default(),
        &format,
        &format,
        envelope(
            &format,
            br#"{"input":[{"type":"message","content":"before"}]}"#,
        ),
    );
    assert!(!result.configuration_updates_changed);
}

#[test]
fn update_order_and_internal_bytes_follow_upstream_raw_comparison() {
    let format = openai_response();
    for (before, after, changed) in [
        (
            r#"{"input":[{"type":"configuration_update","a":1},{"type":"configuration_update","a":2}]}"#,
            r#"{"input":[{"type":"configuration_update","a":2},{"type":"configuration_update","a":1}]}"#,
            true,
        ),
        (
            r#"{"input":[{"type":"configuration_update","a":1}]}"#,
            r#"{"input":[{ "type": "configuration_update", "a": 1 }]}"#,
            true,
        ),
        (
            r#"{"input":[{"type":"configuration_update","a":1}]}"#,
            r#" { "input" : [  {"type":"configuration_update","a":1}  ] } "#,
            false,
        ),
    ] {
        let registry = Registry::new();
        registry.set_plugin_hooks(Some(Arc::new(ReplaceBody(after.as_bytes().to_vec()))));
        let result = registry.translate_request_envelope(
            &TranslationContext::default(),
            &format,
            &format,
            envelope(&format, before.as_bytes()),
        );
        assert_eq!(result.configuration_updates_changed, changed);
    }
}

#[test]
fn incoming_and_transform_intent_survive_without_hooks() {
    let format = openai_response();
    let registry = Registry::new();
    let mut input = envelope(&format, br#"{"input":[]}"#);
    input.configuration_updates_changed = true;
    let result = registry.translate_request_envelope(
        &TranslationContext::default(),
        &format,
        &format,
        input,
    );
    assert!(result.configuration_updates_changed);

    registry.register_request_envelope(
        format.clone(),
        format.clone(),
        Some(Arc::new(|_, mut request| {
            request.configuration_updates_changed = true;
            request
        })),
        response(),
    );
    let result = registry.translate_request_envelope(
        &TranslationContext::default(),
        &format,
        &format,
        envelope(&format, br#"{"input":[]}"#),
    );
    assert!(result.configuration_updates_changed);
}

#[test]
fn pipeline_preserves_transform_intent_through_middleware() {
    let format = openai_response();
    let registry = Arc::new(Registry::new());
    registry.register_request_envelope(
        format.clone(),
        format.clone(),
        Some(Arc::new(|_, mut request| {
            request.configuration_updates_changed = true;
            request
        })),
        response(),
    );
    let mut pipeline = Pipeline::new(registry);
    pipeline.use_request(Arc::new(|context, request, next| next(context, request)));
    let result = pipeline
        .translate_request(
            &TranslationContext::default(),
            format.clone(),
            format.clone(),
            envelope(&format, br#"{"input":[]}"#),
        )
        .unwrap();
    assert!(result.configuration_updates_changed);
    assert_eq!(result.format, format);
}

#[test]
fn non_object_bodies_do_not_expose_positional_struct_updates() {
    let registry = Registry::new();
    let format = openai_response();
    registry.set_plugin_hooks(Some(Arc::new(ReplaceBody(br#"{"input":[]}"#.to_vec()))));
    let result = registry.translate_request_envelope(
        &TranslationContext::default(),
        &format,
        &format,
        envelope(&format, br#"[[{"type":"configuration_update","a":1}]]"#),
    );
    assert!(!result.configuration_updates_changed);
}

#[test]
fn duplicate_keys_use_the_first_gjson_match_for_update_intent() {
    let format = openai_response();
    for (before, after, changed) in [
        (
            r#"{"input":[{"type":"configuration_update","type":"message","a":1}]}"#,
            r#"{"input":[{"type":"configuration_update","type":"message","a":2}]}"#,
            true,
        ),
        (
            r#"{"input":[{"type":"configuration_update","a":1}],"input":[]}"#,
            r#"{"input":[{"type":"configuration_update","a":2}],"input":[]}"#,
            true,
        ),
        (
            r#"{"input":[],"input":[{"type":"configuration_update","a":1}]}"#,
            r#"{"input":[],"input":[{"type":"configuration_update","a":2}]}"#,
            false,
        ),
    ] {
        let registry = Registry::new();
        registry.register(
            format.clone(),
            format.clone(),
            Some(Arc::new(|_, body, _| body.to_vec())),
            response(),
        );
        registry.set_plugin_hooks(Some(Arc::new(ReplaceBody(after.as_bytes().to_vec()))));
        let result = registry.translate_request_envelope(
            &TranslationContext::default(),
            &format,
            &format,
            envelope(&format, before.as_bytes()),
        );
        assert_eq!(result.configuration_updates_changed, changed);
    }
}

#[test]
fn chat_reasoning_depth_does_not_imply_claude_display_visibility() {
    use crate::internal::thinking::{extract_translated_summary_config, SummaryMode};
    for (body, target, expected) in [
        (
            br#"{"reasoning_effort":"high"}"#.as_slice(),
            " Claude ",
            SummaryMode::Unspecified,
        ),
        (
            br#"{"reasoning_effort":"none"}"#.as_slice(),
            "claude",
            SummaryMode::Unspecified,
        ),
        (
            br#"{"reasoning_effort":"high"}"#.as_slice(),
            "codex",
            SummaryMode::Enabled,
        ),
        (
            br#"{"reasoning_effort":"none"}"#.as_slice(),
            "gemini",
            SummaryMode::Disabled,
        ),
    ] {
        let actual = extract_translated_summary_config(body, " OPENAI ", target);
        assert_eq!(actual.mode, expected);
    }
}
