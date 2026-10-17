// ref: internal/runtime/executor/helps/model_capabilities.go @ 2044a01f
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::internal::modelconfig::ModelInfo;
use crate::sdk::translator::{PluginHooks, ResponseTransform};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct CountingNormalizer(AtomicUsize);
impl PluginHooks for CountingNormalizer {
    fn normalize_request(
        &self,
        _: &TranslationContext,
        _: &Format,
        _: &Format,
        _: &str,
        body: Vec<u8>,
        _: bool,
    ) -> Vec<u8> {
        self.0.fetch_add(1, Ordering::SeqCst);
        body
    }
}

fn registry() -> (Arc<Registry>, Arc<CountingNormalizer>) {
    let registry = Arc::new(Registry::new());
    let hooks = Arc::new(CountingNormalizer::default());
    registry.set_plugin_hooks(Some(hooks.clone()));
    registry.register(
        Format::from("claude"),
        Format::from("openai"),
        Some(Arc::new(|_, _, _| {
            br#"{"normal-translator":true}"#.to_vec()
        })),
        ResponseTransform {
            stream: None,
            non_stream: None,
            token_count: None,
        },
    );
    (registry, hooks)
}

fn snapshot(is_compat: bool) -> Arc<ModelInfo> {
    Arc::new(ModelInfo {
        id: "shared".into(),
        is_compat,
        ..ModelInfo::default()
    })
}

#[derive(Default)]
struct V16CompactCapture(std::sync::Mutex<Option<HttpRequest>>);
impl HostHttpClient for V16CompactCapture {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
    ) -> PluginFuture<'a, crate::sdk::pluginapi::HttpResponse> {
        *self.0.lock().unwrap() = Some(request);
        Box::pin(async {
            Ok(crate::sdk::pluginapi::HttpResponse {
                status_code: 200,
                body: br#"{"object":"response.compaction","output":[]}"#.to_vec(),
                ..Default::default()
            })
        })
    }

    fn execute_stream<'a>(
        &'a self,
        _: HttpRequest,
    ) -> PluginFuture<'a, crate::sdk::pluginapi::HttpStreamResponse> {
        Box::pin(async { panic!("compact requests must use unary HTTP") })
    }
}

async fn v16_compact_wire_request(model_compat: Option<bool>, home_compat: Option<bool>) -> Value {
    let registry = Arc::new(Registry::new());
    let executor =
        OpenAiCompatExecutor::new("test", Arc::new(OpenAiCompatConfig::default()), registry);
    let client = Arc::new(V16CompactCapture::default());
    let request = ExecutorRequest {
        model:"shared".into(), source_format:"openai-response".into(),
        alt:"responses/compact".into(),
        payload:br#"{"store":false,"stream":false,"input":[{"type":"reasoning","id":"rs_compact","summary":[],"content":[{"type":"reasoning_text","text":"retained thought"}],"encrypted_content":"opaque-encrypted-reasoning-token-xyz"}]}"#.to_vec(),
        resolved_model_info:model_compat.map(snapshot),
        resolved_home_model_options:home_compat.map(|is_compat| {
            crate::internal::modelconfig::HomeModelOptions {
                is_compat, ..Default::default()
            }
        }),
        metadata:BTreeMap::from([("is_compat".into(), Value::Bool(true))]),
        auth_attributes:BTreeMap::from([
            ("base_url".into(), "https://fixture.invalid/v1".into()),
            ("is_compat".into(), "true".into())
        ]),
        http_client:Some(client.clone()),
        ..ExecutorRequest::default()
    };
    executor.execute_non_stream(request).await.unwrap();
    let captured = client.0.lock().unwrap().take().unwrap();
    assert_eq!(captured.method, "POST");
    assert_eq!(captured.url, "https://fixture.invalid/v1/responses/compact");
    let body: Value = serde_json::from_slice(&captured.body).unwrap();
    assert!(body.get("stream").is_none());
    body
}
