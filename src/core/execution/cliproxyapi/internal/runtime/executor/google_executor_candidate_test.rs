// Origin: CTOX
// License: AGPL-3.0-only
// ref: gemini_executor.go:134-177,282,424-443,686,939-944;
// gemini_vertex_executor.go:334-367,471-508,599-628,928,1022 @ d7914afd
// Scripted selected-client consumers; these do not contact a real provider.

use super::{
    gemini_executor::{GeminiExecutor, GeminiExecutorConfig},
    gemini_vertex_executor::{GeminiVertexExecutor, VertexAccessTokenProvider},
    helps::{PayloadApplyConfig, PayloadModelRule, PayloadRule},
};
use crate::internal::{
    modelconfig,
    registry::ModelInfo,
    thinking::{ErrorCode, ModelInfoResolver, ThinkingEngine, ThinkingError},
};
use crate::sdk::{
    pluginapi::{
        ExecutorRequest, Headers, HostHttpClient, HttpRequest, HttpResponse, HttpStreamResponse,
        PluginFuture, ProviderExecutor,
    },
    translator::{Format, Registry, ResponseTransform},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Gemini,
    Interactions,
    VertexApiKey,
    VertexBearer,
}
const GENERATE_KINDS: [Kind; 3] = [Kind::Gemini, Kind::VertexApiKey, Kind::VertexBearer];

#[derive(Default)]
struct CaptureHttp {
    requests: Mutex<Vec<HttpRequest>>,
}
impl HostHttpClient for CaptureHttp {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            Ok(HttpResponse {
                status_code: 200,
                headers: Headers::default(),
                body: br#"{"totalTokens":7,"candidates":[],"output":[]}"#.to_vec(),
            })
        })
    }
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            let (sender, chunks) = tokio::sync::mpsc::channel(1);
            drop(sender);
            Ok(HttpStreamResponse {
                status_code: 200,
                headers: Headers::default(),
                chunks,
            })
        })
    }
}
struct FixtureToken;
impl VertexAccessTokenProvider for FixtureToken {
    fn access_token<'a>(&'a self, _: &'a Value) -> PluginFuture<'a, String> {
        Box::pin(async { Ok("fixture-access-token".into()) })
    }
}
fn executor(
    kind: Kind,
    registry: Arc<Registry>,
    config: Arc<PayloadApplyConfig>,
    engine: Option<Arc<ThinkingEngine>>,
) -> Box<dyn ProviderExecutor> {
    match kind {
        Kind::Gemini | Kind::Interactions => {
            let mut executor = if kind == Kind::Interactions {
                GeminiExecutor::interactions(Arc::new(GeminiExecutorConfig::default()), registry)
            } else {
                GeminiExecutor::new(Arc::new(GeminiExecutorConfig::default()), registry)
            }
            .with_payload_config(config);
            if let Some(engine) = engine {
                executor = executor.with_canonical_thinking(engine);
            }
            Box::new(executor)
        }
        Kind::VertexApiKey | Kind::VertexBearer => {
            let mut executor = GeminiVertexExecutor::new(registry, Some(Arc::new(FixtureToken)))
                .with_payload_config(config);
            if let Some(engine) = engine {
                executor = executor.with_canonical_thinking(engine);
            }
            Box::new(executor)
        }
    }
}
fn selected(levels: &[&str], min: i64, max: i64) -> Arc<modelconfig::ModelInfo> {
    Arc::new(modelconfig::ModelInfo {
        id: "runtime-google".into(),
        provider_type: "gemini".into(),
        thinking: Some(modelconfig::ThinkingSupport {
            levels: levels.iter().map(|x| (*x).to_owned()).collect(),
            min,
            max,
            zero_allowed: true,
            dynamic_allowed: true,
        }),
        ..Default::default()
    })
}
fn request(kind: Kind, http: Arc<CaptureHttp>, model: &str, payload: &[u8]) -> ExecutorRequest {
    let mut request = ExecutorRequest {
        model: model.into(),
        source_format: if kind == Kind::Interactions {
            "interactions".into()
        } else {
            "gemini".into()
        },
        auth_provider: if kind == Kind::Interactions {
            "gemini-interactions".into()
        } else if kind == Kind::Gemini {
            "gemini".into()
        } else {
            "vertex".into()
        },
        auth_attributes: BTreeMap::from([("base_url".into(), "https://unit.invalid".into())]),
        payload: payload.to_vec(),
        http_client: Some(http),
        ..Default::default()
    };
    if kind == Kind::VertexBearer {
        request.auth_metadata = BTreeMap::from([
            ("service_account".into(), json!({})),
            ("project_id".into(), json!("fixture-project")),
            ("location".into(), json!("fixture-zone")),
        ]);
    } else {
        request
            .auth_attributes
            .insert("api_key".into(), "fixture-api-key".into());
    }
    request
}
async fn invoke(
    executor: &dyn ProviderExecutor,
    request: ExecutorRequest,
    stream: bool,
    count: bool,
) -> Result<(), crate::sdk::pluginapi::PluginExecutionError> {
    if count {
        executor.count_tokens(request).await?;
    } else if stream {
        let mut response = executor.execute_stream(request).await?;
        tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(chunk) = response.chunks.recv().await {
                assert!(chunk.error.is_none());
            }
        })
        .await
        .expect("owned fixture stream must terminate");
    } else {
        executor.execute(request).await?;
    }
    Ok(())
}
fn body(http: &CaptureHttp) -> Value {
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    serde_json::from_slice(&requests[0].body).unwrap()
}
fn override_config(path: &str, value: Value) -> Arc<PayloadApplyConfig> {
    let mut config = PayloadApplyConfig::default();
    config.rules.override_values.push(PayloadRule {
        models: vec![PayloadModelRule {
            name: "*".into(),
            protocol: "gemini".into(),
            ..Default::default()
        }],
        params: BTreeMap::from([(path.into(), value)]),
    });
    Arc::new(config)
}

struct OwnedResolver(modelconfig::ModelInfo);
impl ModelInfoResolver for OwnedResolver {
    fn lookup_model_info(&self, _: &str, _: &str) -> Option<ModelInfo> {
        None
    }
    fn lookup_owned_model_info(&self, model: &str, _: &str) -> Option<modelconfig::ModelInfo> {
        (model == self.0.id).then(|| self.0.clone())
    }
}
async fn operation_contract(kind: Kind) {
    for (stream, count, metadata_count, alt, expected_action, expected_suffix, expected_turns) in [
        (
            false,
            false,
            false,
            "json",
            "generateContent",
            "?$alt=json",
            3,
        ),
        (false, false, true, "json", "countTokens", "", 2),
        (false, true, false, "json", "countTokens", "", 2),
        (
            true,
            false,
            true,
            "json",
            "streamGenerateContent",
            "?$alt=json",
            3,
        ),
        (
            true,
            false,
            true,
            "",
            "streamGenerateContent",
            "?alt=sse",
            3,
        ),
    ] {
        let http = Arc::new(CaptureHttp::default());
        let executor = executor(
            kind,
            Arc::new(Registry::new()),
            override_config("central_rule", json!(true)),
            None,
        );
        let mut request = request(kind, http.clone(), "runtime-google", br#"{"contents":[{"role":"model","parts":[{"text":"answer"}]}],"tools":[],"generationConfig":{"temperature":0.7},"safetySettings":[]}"#);
        request.alt = alt.into();
        if metadata_count {
            request
                .metadata
                .insert("action".into(), json!("countTokens"));
        }
        invoke(executor.as_ref(), request, stream, count)
            .await
            .unwrap();
        let body = body(&http);
        assert_eq!(
            body["contents"].as_array().unwrap().len(),
            expected_turns,
            "{kind:?}, stream={stream}, count={count}, metadata={metadata_count}"
        );
        assert_eq!(body.get("generationConfig").is_some(), !count);
        assert_eq!(body.get("tools").is_some(), !count);
        assert_eq!(body.get("safetySettings").is_some(), !count);
        assert_eq!(body.get("central_rule").is_some(), !count);
        let requests = http.requests.lock().unwrap();
        let outgoing = &requests[0];
        assert!(
            outgoing.url.ends_with(&format!(
                "/models/runtime-google:{expected_action}{expected_suffix}"
            )),
            "{}",
            outgoing.url
        );
        if kind == Kind::VertexBearer {
            assert!(outgoing
                .url
                .contains("/projects/fixture-project/locations/fixture-zone/publishers/google/"));
            assert_eq!(
                outgoing.headers["Authorization"],
                ["Bearer fixture-access-token"]
            );
        } else {
            assert_eq!(outgoing.headers["x-goog-api-key"], ["fixture-api-key"]);
        }
    }
}
