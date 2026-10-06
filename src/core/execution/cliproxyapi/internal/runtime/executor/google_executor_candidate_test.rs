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

#[tokio::test]
async fn candidate_google_thinking_selected_levels_precede_payload_overrides() {
    for kind in GENERATE_KINDS {
        for stream in [false, true] {
            for overridden in [false, true] {
                let http = Arc::new(CaptureHttp::default());
                let config = if overridden {
                    override_config(
                        "generationConfig.thinkingConfig.thinkingLevel",
                        json!("low"),
                    )
                } else {
                    Arc::new(PayloadApplyConfig::default())
                };
                let executor = executor(kind, Arc::new(Registry::new()), config, None);
                let mut request = request(
                    kind,
                    http.clone(),
                    "runtime-google(high)",
                    br#"{"contents":[]}"#,
                );
                request.resolved_model_info = Some(selected(&["low", "high"], 0, 0));
                invoke(executor.as_ref(), request, stream, false)
                    .await
                    .unwrap();
                let body = body(&http);
                assert_eq!(body["model"], "runtime-google", "{kind:?}");
                assert_eq!(
                    body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
                    if overridden { "low" } else { "high" },
                    "{kind:?}, stream={stream}"
                );
            }
        }
    }
}

#[tokio::test]
async fn candidate_google_thinking_invalid_selected_budget_stops_before_http() {
    for kind in GENERATE_KINDS {
        for (stream, count) in [(false, false), (true, false), (false, true)] {
            let http = Arc::new(CaptureHttp::default());
            let executor = executor(
                kind,
                Arc::new(Registry::new()),
                Arc::new(PayloadApplyConfig::default()),
                None,
            );
            let mut request = request(
                kind,
                http.clone(),
                "runtime-google",
                br#"{"contents":[],"generationConfig":{"thinkingConfig":{"thinkingBudget":900}}}"#,
            );
            request.resolved_model_info = Some(selected(&[], 200, 400));
            request
                .metadata
                .insert("action".into(), json!("countTokens"));
            let error = invoke(executor.as_ref(), request, stream, count)
                .await
                .unwrap_err();
            assert_eq!(
                error.as_ref().downcast_ref::<ThinkingError>().unwrap().code,
                ErrorCode::BudgetOutOfRange,
                "{kind:?}, count={count}"
            );
            assert!(
                http.requests.lock().unwrap().is_empty(),
                "validation must precede counting-field removal and HTTP"
            );
        }
    }
}

#[tokio::test]
async fn candidate_google_thinking_current_effort_and_original_summary_reach_consumers() {
    for kind in GENERATE_KINDS {
        for stream in [false, true] {
            let registry = Arc::new(Registry::new());
            registry.register(
                Format::from("openai-response"),
                Format::from("gemini"),
                Some(Arc::new(|_, _, _| br#"{"contents":[]}"#.to_vec())),
                ResponseTransform::default(),
            );
            let http = Arc::new(CaptureHttp::default());
            let executor = executor(
                kind,
                registry,
                Arc::new(PayloadApplyConfig::default()),
                None,
            );
            let mut request = request(
                kind,
                http.clone(),
                "runtime-google",
                br#"{"reasoning":{"effort":"high"}}"#,
            );
            request.source_format = "openai-response".into();
            request.original_request =
                br#"{"reasoning":{"effort":"low","summary":"detailed"}}"#.to_vec();
            request.resolved_model_info = Some(selected(&["low", "high"], 0, 0));
            invoke(executor.as_ref(), request, stream, false)
                .await
                .unwrap();
            let body = body(&http);
            assert_eq!(
                body["generationConfig"]["thinkingConfig"]["thinkingLevel"], "high",
                "{kind:?}"
            );
            assert_eq!(
                body["generationConfig"]["thinkingConfig"]["includeThoughts"], true,
                "{kind:?}"
            );
        }
    }
}

#[tokio::test]
async fn candidate_google_thinking_native_interactions_uses_canonical_validation() {
    for source in ["", "interactions"] {
        for stream in [false, true] {
            for valid in [false, true] {
                let http = Arc::new(CaptureHttp::default());
                let executor = executor(
                    Kind::Interactions,
                    Arc::new(Registry::new()),
                    Arc::new(PayloadApplyConfig::default()),
                    None,
                );
                let mut request = request(Kind::Interactions, http.clone(), if valid { "runtime-google(high)" } else { "runtime-google(xhigh)" }, br#"{"input":"hello","contents":[{"role":"model","parts":[{"text":"native-history"}]}]}"#);
                request.source_format = source.into();
                let mut capability = selected(&["low", "high"], 0, 0);
                // A native Interactions capability exercises strict same-family validation.
                // A Gemini capability is cross-family here and correctly clamps xhigh.
                Arc::make_mut(&mut capability).provider_type = "interactions".into();
                request.resolved_model_info = Some(capability);
                let result = invoke(executor.as_ref(), request, stream, false).await;
                if valid {
                    result.unwrap();
                    let body = body(&http);
                    assert_eq!(body["generation_config"]["thinking_level"], "high");
                    assert_eq!(
                        body["contents"].as_array().unwrap().len(),
                        1,
                        "native history must bypass GenerateContent repair"
                    );
                    assert!(http.requests.lock().unwrap()[0]
                        .url
                        .ends_with("/interactions"));
                } else {
                    assert!(result
                        .unwrap_err()
                        .as_ref()
                        .downcast_ref::<ThinkingError>()
                        .is_some());
                    assert!(http.requests.lock().unwrap().is_empty());
                }
            }
        }
    }
}

#[tokio::test]
async fn candidate_google_thinking_native_account_dedicated_count_uses_generatecontent() {
    let registry = Arc::new(Registry::new());
    registry.register(Format::from("interactions"), Format::from("gemini"), Some(Arc::new(|_, _, _| br#"{"contents":[],"generationConfig":{"thinkingConfig":{"thinkingLevel":"high"}},"from_interactions":true}"#.to_vec())), ResponseTransform::default());
    let http = Arc::new(CaptureHttp::default());
    let executor = executor(
        Kind::Interactions,
        registry,
        Arc::new(PayloadApplyConfig::default()),
        None,
    );
    let mut request = request(
        Kind::Interactions,
        http.clone(),
        "runtime-google",
        br#"{"input":"hello","generation_config":{"thinking_level":"high"}}"#,
    );
    request.resolved_model_info = Some(selected(&["low", "high"], 0, 0));
    invoke(executor.as_ref(), request, false, true)
        .await
        .unwrap();
    let body = body(&http);
    assert_eq!(body["from_interactions"], true);
    assert!(body.get("generationConfig").is_none());
    assert!(http.requests.lock().unwrap()[0]
        .url
        .ends_with("/models/runtime-google:countTokens"));
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
#[tokio::test]
async fn candidate_google_thinking_injected_resolvers_stay_instance_scoped() {
    for kind in GENERATE_KINDS {
        let allowed = Arc::new(ThinkingEngine::new(Arc::new(OwnedResolver(
            (*selected(&[], 200, 300)).clone(),
        ))));
        let denied = Arc::new(ThinkingEngine::new(Arc::new(OwnedResolver(
            (*selected(&[], 100, 200)).clone(),
        ))));
        for (engine, valid) in [(allowed.clone(), true), (denied, false), (allowed, true)] {
            let http = Arc::new(CaptureHttp::default());
            let executor = executor(
                kind,
                Arc::new(Registry::new()),
                Arc::new(PayloadApplyConfig::default()),
                Some(engine),
            );
            let request = request(
                kind,
                http.clone(),
                "runtime-google",
                br#"{"contents":[],"generationConfig":{"thinkingConfig":{"thinkingBudget":250}}}"#,
            );
            let result = invoke(executor.as_ref(), request, false, false).await;
            if valid {
                result.unwrap();
                assert_eq!(
                    body(&http)["generationConfig"]["thinkingConfig"]["thinkingBudget"],
                    250
                );
            } else {
                assert_eq!(
                    result
                        .unwrap_err()
                        .as_ref()
                        .downcast_ref::<ThinkingError>()
                        .unwrap()
                        .code,
                    ErrorCode::BudgetOutOfRange
                );
                assert!(http.requests.lock().unwrap().is_empty());
            }
        }
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
#[tokio::test]
async fn candidate_google_operation_gemini_metadata_count_keeps_inference_configuration() {
    operation_contract(Kind::Gemini).await;
}
#[tokio::test]
async fn candidate_google_operation_vertex_metadata_count_and_stream_query_cover_both_credentials()
{
    operation_contract(Kind::VertexApiKey).await;
    operation_contract(Kind::VertexBearer).await;
}
