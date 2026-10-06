// ref: sdk/translator/registry.go:140-160 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{Format, PluginHooks, Registry, RequestEnvelope, TranslationContext};
use std::sync::Arc;

fn request(body: &[u8]) -> RequestEnvelope {
    RequestEnvelope {
        format: Format::from("openai-response"),
        model: "selected-model".to_owned(),
        stream: false,
        body: body.to_vec(),
        configuration_updates_changed: false,
    }
}
fn translate(registry: &Registry, body: &[u8]) -> RequestEnvelope {
    let format = Format::from("openai-response");
    registry.translate_request_envelope(
        &TranslationContext::default(),
        &format,
        &format,
        request(body),
    )
}

#[test]
fn candidate_registry_raw_model_preserves_duplicate_numbers_and_deep_updates() {
    let update = r#"{"type":"configuration_update","tools":[{"parameters":{"n":9007199254740993,"n":18446744073709551616,"big":1e400}}]}"#;
    for model in [
        "",
        r#","model":"old-model""#,
        r#","model":"selected-model""#,
    ] {
        let body = format!(r#"{{"input":[{update}]{model},"other":{{"z":1,"z":2}}}}"#);
        let out = translate(&Registry::new(), body.as_bytes());
        let text = std::str::from_utf8(&out.body).unwrap();
        assert_eq!(gjson::get(text, "model").str(), "selected-model");
        assert_eq!(gjson::get(text, "input.0").json(), update);
        assert_eq!(gjson::get(text, "other").json(), r#"{"z":1,"z":2}"#);
        assert!(!out.configuration_updates_changed);
        if model.contains("selected-model") {
            assert_eq!(out.body, body.as_bytes());
        }
    }
    let deep = format!("{}null{}", "[".repeat(20_000), "]".repeat(20_000));
    let update = format!(r#"{{"type":"configuration_update","tools":[{{"parameters":{deep}}}]}}"#);
    let raw = format!(r#"{{"input":[{update}]}}"#);
    let out = translate(&Registry::new(), raw.as_bytes());
    assert_eq!(
        gjson::get(std::str::from_utf8(&out.body).unwrap(), "input.0").json(),
        update
    );
    for invalid in [
        b"not-json".as_slice(),
        b"{",
        b"null",
        b"[]",
        b"\"text\"",
        b"{\"a\":1,}",
        b"{\"a\":\"\xff\"}",
    ] {
        assert_eq!(translate(&Registry::new(), invalid).body, invalid);
    }
}

struct Replacement(Vec<u8>);
impl PluginHooks for Replacement {
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
fn candidate_registry_raw_model_update_intent_tracks_only_actual_plugin_updates() {
    let update = r#"{"type":"configuration_update","tools":[2]}"#;
    let raw = format!(r#"{{"input":[{update}]}}"#).into_bytes();
    let registry = Registry::new();
    registry.set_plugin_hooks(Some(Arc::new(Replacement(raw.clone()))));
    let unchanged = translate(&registry, &raw);
    assert!(!unchanged.configuration_updates_changed);
    assert_eq!(unchanged.body, raw);
    let changed = br#"{"input":[{"type":"configuration_update","tools":[3]}]}"#.to_vec();
    registry.set_plugin_hooks(Some(Arc::new(Replacement(changed.clone()))));
    let out = translate(&registry, &raw);
    assert!(out.configuration_updates_changed);
    assert_eq!(out.body, changed);
    let mut envelope = request(&raw);
    envelope.configuration_updates_changed = true;
    registry.set_plugin_hooks(Some(Arc::new(Replacement(raw.clone()))));
    let format = Format::from("openai-response");
    let out = registry.translate_request_envelope(
        &TranslationContext::default(),
        &format,
        &format,
        envelope,
    );
    assert!(out.configuration_updates_changed);
}
