use ctox_cliproxyapi::internal::translator::claude::openai::chat_completions::register_openai_chat_claude_request;
use ctox_cliproxyapi::internal::translator::claude::openai::responses::register_openai_responses_claude;
use ctox_cliproxyapi::internal::translator::openai::interactions::responses::register_openai_responses_interactions;
use ctox_cliproxyapi::sdk::translator::{
    claude, interactions, openai, openai_response, Format, Pipeline, PluginHooks, Registry,
    RequestEnvelope, RequestTransform, ResponseEnvelope, ResponseTransform, TranslationContext,
    TranslationState,
};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn request_transform(body: &'static [u8]) -> RequestTransform {
    Arc::new(move |_, _, _| body.to_vec())
}

#[test]
fn fallback_normalizes_only_a_different_model() {
    let registry = Registry::new();
    let context = TranslationContext::default();
    let from = Format::from("a");
    let to = Format::from("b");
    let same = br#"{ "model": "gpt-5", "input": "ping" }"#;
    assert_eq!(
        registry.translate_request(&context, &from, &to, "gpt-5", same, false),
        same
    );

    let changed = registry.translate_request(
        &context,
        &from,
        &to,
        "gpt-5",
        br#"{"model":"copilot/gpt-5","input":"ping"}"#,
        false,
    );
    let value: Value = serde_json::from_slice(&changed).unwrap();
    assert_eq!(value["model"], "gpt-5");
    assert_eq!(value["input"], "ping");
}

#[test]
fn native_request_transform_precedes_plugin_translation() {
    struct Hooks;
    impl PluginHooks for Hooks {
        fn translate_request(
            &self,
            _: &TranslationContext,
            _: &Format,
            _: &Format,
            _: &str,
            _: &[u8],
            _: bool,
        ) -> Option<Vec<u8>> {
            Some(br#"{"source":"plugin"}"#.to_vec())
        }
    }
    let registry = Registry::new();
    let from = Format::from("from");
    let to = Format::from("to");
    registry.set_plugin_hooks(Some(Arc::new(Hooks)));
    registry.register(
        from.clone(),
        to.clone(),
        Some(request_transform(br#"{"source":"native"}"#)),
        ResponseTransform::default(),
    );
    assert_eq!(
        registry.translate_request(
            &TranslationContext::default(),
            &from,
            &to,
            "m",
            b"{}",
            false
        ),
        br#"{"source":"native"}"#,
    );
}

#[test]
fn response_capabilities_are_reported_independently() {
    let registry = Registry::new();
    let provider = Format::from("provider");
    let client = Format::from("client");
    registry.register(
        provider.clone(),
        client.clone(),
        None,
        ResponseTransform {
            stream: Some(Arc::new(|_, _, _, _, raw, _| vec![raw.to_vec()])),
            ..ResponseTransform::default()
        },
    );
    assert!(registry.has_response_transformer(&provider, &client));
    assert!(registry.has_stream_response_transformer(&provider, &client));
    assert!(!registry.has_non_stream_response_transformer(&provider, &client));
}

#[test]
fn native_empty_stream_output_suppresses_raw_fallback() {
    let registry = Registry::new();
    let client = Format::from("client");
    let provider = Format::from("provider");
    registry.register(
        provider.clone(),
        client.clone(),
        None,
        ResponseTransform {
            stream: Some(Arc::new(|_, _, _, _, _, _| Vec::new())),
            ..ResponseTransform::default()
        },
    );
    let mut state: TranslationState = None;
    let output = registry.translate_stream(
        &TranslationContext::default(),
        &client,
        &provider,
        "m",
        b"",
        b"",
        br#"data: {"raw":true}"#,
        &mut state,
    );
    assert!(output.is_empty());
}

#[test]
fn cancelled_stream_does_not_invoke_native_transform() {
    let registry = Registry::new();
    let client = Format::from("client");
    let provider = Format::from("provider");
    let calls = Arc::new(AtomicUsize::new(0));
    let transform_calls = calls.clone();
    registry.register(
        provider.clone(),
        client.clone(),
        None,
        ResponseTransform {
            stream: Some(Arc::new(move |_, _, _, _, raw, _| {
                transform_calls.fetch_add(1, Ordering::Relaxed);
                vec![raw.to_vec()]
            })),
            ..ResponseTransform::default()
        },
    );
    let context = TranslationContext::default();
    context.cancel();
    let mut state = None;
    assert!(registry
        .translate_stream(
            &context,
            &client,
            &provider,
            "m",
            b"",
            b"",
            b"data: {}",
            &mut state,
        )
        .is_empty());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn pipeline_middleware_wraps_in_registration_order() {
    let registry = Arc::new(Registry::new());
    let from = Format::from("from");
    let to = Format::from("to");
    registry.register(
        from.clone(),
        to.clone(),
        Some(Arc::new(|_, body, _| body.to_vec())),
        ResponseTransform::default(),
    );
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut pipeline = Pipeline::new(registry);
    for name in ["outer", "inner"] {
        let calls = calls.clone();
        pipeline.use_request(Arc::new(move |context, request, next| {
            calls.lock().unwrap().push(format!("{name}-before"));
            let result = next(context, request);
            calls.lock().unwrap().push(format!("{name}-after"));
            result
        }));
    }
    let request = RequestEnvelope {
        format: from.clone(),
        model: "m".into(),
        stream: false,
        body: b"{}".to_vec(),
        configuration_updates_changed: false,
    };
    let output = pipeline
        .translate_request(&TranslationContext::default(), from, to.clone(), request)
        .unwrap();
    assert_eq!(output.format, to);
    assert_eq!(
        *calls.lock().unwrap(),
        ["outer-before", "inner-before", "inner-after", "outer-after"]
    );
}

#[test]
fn response_pipeline_returns_mutated_state() {
    let registry = Arc::new(Registry::new());
    let client = Format::from("client");
    let provider = Format::from("provider");
    registry.register(
        provider.clone(),
        client.clone(),
        None,
        ResponseTransform {
            non_stream: Some(Arc::new(|_, _, _, _, raw, state| {
                *state = Some(Box::new(7_u32));
                raw.to_vec()
            })),
            ..ResponseTransform::default()
        },
    );
    let pipeline = Pipeline::new(registry);
    let response = ResponseEnvelope {
        format: provider.clone(),
        model: "m".into(),
        stream: false,
        body: br#"{"ok":true}"#.to_vec(),
        chunks: Vec::new(),
    };
    let (_, state) = pipeline
        .translate_response(
            &TranslationContext::default(),
            client,
            provider,
            response,
            Vec::new(),
            Vec::new(),
            None,
        )
        .unwrap();
    assert_eq!(*state.unwrap().downcast::<u32>().unwrap(), 7);
}
