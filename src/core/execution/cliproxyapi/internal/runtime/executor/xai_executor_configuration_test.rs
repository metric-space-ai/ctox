// ref: internal/runtime/executor/xai_configuration_update_test.go @ 0d18d3b365be000eeaee7de4615f9da596b2059d
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::sdk::{
    cliproxy::executor::{Headers, Options, Request},
    translator::{Format, PluginHooks, Registry, TranslationContext},
};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

fn capability(supported: bool) -> Arc<crate::internal::modelconfig::ModelInfo> {
    Arc::new(crate::internal::modelconfig::ModelInfo {
        id: "fixture-xai".into(),
        provider_type: "xai".into(),
        support_configuration_update: supported,
        thinking: Some(crate::internal::modelconfig::ThinkingSupport {
            levels: vec!["low".into(), "medium".into(), "high".into()],
            ..Default::default()
        }),
        ..Default::default()
    })
}
fn options(supported: bool) -> Options {
    let mut result = Options::default();
    result.metadata.resolved_api_key_model_info = Some(capability(supported));
    result
}
fn request(model: &str, payload: &[u8]) -> Request {
    Request {
        model: model.into(),
        payload: payload.to_vec(),
        ..Request::default()
    }
}
const SOURCE: &[u8] = br#"{"reasoning":{"effort":"medium","summary":"auto"},"input":[{"role":"user","content":"hi"},{"type":"configuration_update","reasoning":{"effort":"low"}},{"type":"configuration_update","tools":[]},{"type":"configuration_update","reasoning":{"effort":"high"}},{"role":"user","content":"again"}]}"#;
#[test]
fn xai_configuration_unsupported_updates_and_suffix_follow_upstream() {
    for (model, stream, want) in [
        ("fixture-xai", false, "high"),
        ("fixture-xai(low)", true, "low"),
    ] {
        let p = XaiRequestOwner::default()
            .prepare(&request(model, SOURCE), &options(false), stream, false)
            .unwrap();
        let b: Value = serde_json::from_slice(&p.body).unwrap();
        assert_eq!(b["reasoning"]["effort"], want);
        assert_eq!(b["reasoning"]["summary"], "auto");
        assert_eq!(b["stream"], stream);
        assert_eq!(
            b["input"],
            json!([{"role":"user","content":"hi"},{"role":"user","content":"again"}])
        );
    }
}
#[test]
fn xai_configuration_only_private_selected_support_preserves_updates() {
    for (model, baseline) in [("fixture-xai", "medium"), ("fixture-xai(low)", "low")] {
        let p = XaiRequestOwner::default()
            .prepare(&request(model, SOURCE), &options(true), false, false)
            .unwrap();
        let b: Value = serde_json::from_slice(&p.body).unwrap();
        assert_eq!(b["reasoning"]["effort"], baseline);
        assert_eq!(b["input"][3]["reasoning"]["effort"], "high");
    }
    let mut untrusted = options(false);
    untrusted
        .metadata
        .extensions
        .insert("support_configuration_update".into(), json!(true));
    let p = XaiRequestOwner::default()
        .prepare(&request("fixture-xai", SOURCE), &untrusted, false, false)
        .unwrap();
    let b: Value = serde_json::from_slice(&p.body).unwrap();
    assert_eq!(b["input"].as_array().unwrap().len(), 2);
}
struct UpdateHook(Option<&'static str>);
impl PluginHooks for UpdateHook {
    fn normalize_request(
        &self,
        _: &TranslationContext,
        _: &Format,
        _: &Format,
        _: &str,
        body: Vec<u8>,
        _: bool,
    ) -> Vec<u8> {
        let mut b: Value = serde_json::from_slice(&body).unwrap();
        if let Some(effort) = self.0 {
            b["input"][0]["reasoning"]["effort"] = effort.into();
        } else {
            b["input"].as_array_mut().unwrap().remove(0);
        }
        serde_json::to_vec(&b).unwrap()
    }
}
#[test]
fn xai_configuration_plugin_update_decision_does_not_restore_old_source() {
    let payload=br#"{"reasoning":{"effort":"medium"},"input":[{"type":"configuration_update","reasoning":{"effort":"low"}},{"role":"user","content":"hi"}]}"#;
    for (change, want) in [(None, "medium"), (Some("high"), "high")] {
        let registry = Arc::new(Registry::new());
        registry.set_plugin_hooks(Some(Arc::new(UpdateHook(change))));
        let owner = XaiRequestOwner::new(
            registry,
            Arc::new(crate::internal::thinking::ThinkingEngine::default()),
        );
        let p = owner
            .prepare(
                &request("fixture-xai", payload),
                &options(false),
                false,
                false,
            )
            .unwrap();
        let b: Value = serde_json::from_slice(&p.body).unwrap();
        assert_eq!(b["reasoning"]["effort"], want);
        assert_eq!(b["input"].as_array().unwrap().len(), 1);
    }
}
#[test]
fn xai_configuration_original_summary_and_owned_metadata_effort_survive() {
    let mut o = options(false);
    o.original_request = br#"{"reasoning":{"summary":"detailed"}}"#.to_vec();
    o.metadata.reasoning_effort = Some("high".into());
    let p = XaiRequestOwner::default()
        .prepare(
            &request("fixture-xai", br#"{"input":"hi"}"#),
            &o,
            false,
            false,
        )
        .unwrap();
    let b: Value = serde_json::from_slice(&p.body).unwrap();
    assert_eq!(b["reasoning"]["effort"], "high");
    assert_eq!(b["reasoning"]["summary"], "detailed");
}
#[derive(Default)]
struct Wire {
    seen: Mutex<Vec<Value>>,
}
fn complete() -> Vec<u8> {
    b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"fixture-response\",\"output\":[]}}\n\n".to_vec()
}
impl XaiHttpTransport for Wire {
    fn execute<'a>(
        &'a self,
        r: &'a XaiHttpRequest,
        _: Duration,
    ) -> XaiTransportFuture<'a, XaiHttpResponse> {
        self.seen
            .lock()
            .unwrap()
            .push(serde_json::from_slice(&r.body).unwrap());
        Box::pin(async {
            Ok(XaiHttpResponse {
                status: 200,
                headers: Headers::new(),
                body: complete().into(),
            })
        })
    }
}
impl XaiStreamingTransport for Wire {
    fn execute_stream<'a>(
        &'a self,
        r: &'a XaiHttpRequest,
        _: Duration,
    ) -> XaiTransportFuture<'a, XaiStreamResponse> {
        self.seen
            .lock()
            .unwrap()
            .push(serde_json::from_slice(&r.body).unwrap());
        Box::pin(async {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            tx.send(Ok(complete())).await.unwrap();
            drop(tx);
            Ok(XaiStreamResponse {
                status: 200,
                headers: Headers::new(),
                chunks: rx,
            })
        })
    }
}
#[tokio::test]
async fn xai_configuration_real_buffered_and_streamed_transport_share_preparation() {
    let wire = Arc::new(Wire::default());
    let executor = XaiExecutor::new(wire.clone(), Duration::from_secs(3))
        .unwrap()
        .with_stream_transport(wire.clone());
    let r = request("fixture-xai(low)", SOURCE);
    let o = options(false);
    executor.execute(None, &r, &o).await.unwrap();
    let mut stream = executor.execute_stream(None, &r, &o).await.unwrap();
    while let Some(item) = stream.chunks.recv().await {
        item.unwrap();
    }
    let seen = wire.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    for b in seen.iter() {
        assert_eq!(b["reasoning"]["effort"], "low");
        assert_eq!(b["input"].as_array().unwrap().len(), 2);
    }
}
#[tokio::test]
async fn xai_configuration_unrepresentable_selected_level_fails_before_io() {
    let wire = Arc::new(Wire::default());
    let executor = XaiExecutor::new(wire.clone(), Duration::from_secs(3))
        .unwrap()
        .with_stream_transport(wire.clone());
    let r = request(
        "fixture-xai",
        br#"{"input":"hi","reasoning":{"effort":"ultra"}}"#,
    );
    let mut o = options(false);
    o.metadata.resolved_api_key_model_info =
        Some(Arc::new(crate::internal::modelconfig::ModelInfo {
            id: "fixture-xai".into(),
            provider_type: "xai".into(),
            thinking: Some(crate::internal::modelconfig::ThinkingSupport {
                min: 1024,
                max: 4096,
                ..Default::default()
            }),
            ..Default::default()
        }));

    assert!(matches!(
        executor.execute(None, &r, &o).await,
        Err(XaiExecutionError::Request(_))
    ));
    assert!(matches!(
        executor.execute_stream(None, &r, &o).await,
        Err(XaiExecutionError::Request(_))
    ));
    assert!(wire.seen.lock().unwrap().is_empty());
}
