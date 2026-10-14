// ref: sdk/pluginapi/types_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::{collections::BTreeMap, sync::Arc};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;

use super::*;

#[derive(Debug)]
struct TestError;

impl std::fmt::Display for TestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("test error")
    }
}

impl std::error::Error for TestError {}

struct CompileTimeExecutor;

impl ProviderExecutor for CompileTimeExecutor {
    fn identifier(&self) -> &str {
        "compile-time"
    }

    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move {
            Ok(ExecutorResponse {
                payload: request.payload,
                ..ExecutorResponse::default()
            })
        })
    }

    fn execute_stream<'a>(
        &'a self,
        _request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(async move {
            let (_sender, receiver) = tokio::sync::mpsc::channel(1);
            Ok(ExecutorStreamResponse {
                headers: Headers::new(),
                chunks: receiver,
            })
        })
    }

    fn count_tokens<'a>(&'a self, _request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move { Err(Arc::new(TestError) as PluginExecutionError) })
    }

    fn http_request<'a>(
        &'a self,
        request: ExecutorHttpRequest,
    ) -> PluginFuture<'a, ExecutorHttpResponse> {
        Box::pin(async move {
            Ok(ExecutorHttpResponse {
                status_code: 200,
                body: request.body,
                ..ExecutorHttpResponse::default()
            })
        })
    }
}

impl HostHttpClient for CompileTimeExecutor {
    fn execute<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async { Ok(HttpResponse::default()) })
    }

    fn execute_stream<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async {
            let (_sender, receiver) = tokio::sync::mpsc::channel(1);
            Ok(HttpStreamResponse {
                status_code: 200,
                headers: Headers::new(),
                chunks: receiver,
            })
        })
    }
}

#[test]
fn all_async_contracts_are_object_safe_send_and_sync() {
    fn contract<T: ?Sized + Send + Sync>() {}
    contract::<dyn ModelRegistrar>();
    contract::<dyn ModelProvider>();
    contract::<dyn AuthProvider>();
    contract::<dyn FrontendAuthProvider>();
    contract::<dyn Scheduler>();
    contract::<dyn ModelRouter>();
    contract::<dyn ProviderExecutor>();
    contract::<dyn HostHttpClient>();
    contract::<dyn RequestTranslator>();
    contract::<dyn RequestNormalizer>();
    contract::<dyn ResponseTranslator>();
    contract::<dyn ResponseNormalizer>();
    contract::<dyn RequestInterceptor>();
    contract::<dyn RequestLifecyclePlugin>();
    contract::<dyn ResponseInterceptor>();
    contract::<dyn StreamChunkInterceptor>();
    contract::<dyn ThinkingApplier>();
    contract::<dyn UsagePlugin>();
    contract::<dyn CommandLinePlugin>();
    contract::<dyn ManagementApi>();
    contract::<dyn ManagementHandler>();
}

#[test]
fn metadata_config_defaults_match_upstream_go_wire_cases() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/differential/plugin_config_default_fixtures.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let metadata: Metadata =
            serde_json::from_value(serde_json::json!({"ConfigFields":[case["input"].clone()]}))
                .unwrap();
        let encoded = serde_json::to_value(metadata).unwrap();
        let field = &encoded["ConfigFields"][0];
        assert!(
            field.as_object().unwrap().contains_key("Default"),
            "{}",
            case["name"]
        );
        assert_eq!(field["Default"], case["expected"], "{}", case["name"]);
    }
}

#[test]
fn metadata_config_fields_expose_plugin_schema() {
    let metadata = Metadata {
        name: "example".into(),
        version: "1.0.0".into(),
        author: "test".into(),
        github_repository: "https://github.com/router-for-me/CLIProxyAPI".into(),
        logo: "https://example.com/logo.svg".into(),
        config_fields: vec![ConfigField {
            name: "mode".into(),
            field_type: ConfigFieldType(ConfigFieldType::ENUM.into()),
            enum_values: vec!["safe".into(), "fast".into()],
            description: "Execution mode.".into(),
            default_value: None,
        }],
    };
    assert!(!metadata.logo.is_empty());
    assert_eq!(metadata.config_fields.len(), 1);
}

#[test]
fn auth_parse_and_login_poll_support_multiple_auths() {
    let auth = AuthData {
        provider: "gemini-cli".into(),
        id: "primary.json".into(),
        ..AuthData::default()
    };
    let parsed = AuthParseResponse {
        handled: true,
        auth: auth.clone(),
        auths: vec![
            auth.clone(),
            AuthData {
                id: "primary-project-a.json".into(),
                ..auth.clone()
            },
        ],
    };
    let decoded: AuthParseResponse =
        serde_json::from_value(serde_json::to_value(parsed).unwrap()).unwrap();
    assert!(decoded.handled);
    assert_eq!(decoded.auths[1].id, "primary-project-a.json");
    let polled = AuthLoginPollResponse {
        status: AuthLoginStatus(AuthLoginStatus::SUCCESS.into()),
        auth: auth.clone(),
        auths: vec![auth.clone(), auth],
        ..Default::default()
    };
    let decoded: AuthLoginPollResponse =
        serde_json::from_value(serde_json::to_value(polled).unwrap()).unwrap();
    assert_eq!(decoded.status.0, AuthLoginStatus::SUCCESS);
    assert_eq!(decoded.auths.len(), 2);
}

#[test]
fn host_injected_http_clients_are_never_encoded() {
    fn legacy_round_trip<T>(request: &T)
    where
        T: Serialize + DeserializeOwned,
    {
        let mut value = serde_json::to_value(request).unwrap();
        assert!(value.get("HTTPClient").is_none());
        value
            .as_object_mut()
            .unwrap()
            .insert("HTTPClient".into(), json!({}));
        let _: T = serde_json::from_value(value).unwrap();
    }
    let client: Arc<dyn HostHttpClient> = Arc::new(CompileTimeExecutor);
    legacy_round_trip(&AuthLoginStartRequest {
        provider: "p".into(),
        http_client: Some(client.clone()),
        ..Default::default()
    });
    legacy_round_trip(&AuthLoginPollRequest {
        provider: "p".into(),
        http_client: Some(client.clone()),
        ..Default::default()
    });
    legacy_round_trip(&AuthRefreshRequest {
        auth_id: "a".into(),
        http_client: Some(client.clone()),
        ..Default::default()
    });
    legacy_round_trip(&AuthModelRequest {
        auth_id: "a".into(),
        http_client: Some(client.clone()),
        ..Default::default()
    });
    legacy_round_trip(&ExecutorRequest {
        model: "m".into(),
        http_client: Some(client.clone()),
        ..Default::default()
    });
    legacy_round_trip(&ExecutorHttpRequest {
        auth_id: "a".into(),
        http_client: Some(client),
        ..Default::default()
    });
}

#[test]
fn host_model_types_preserve_snake_case_wire_fields() {
    let request = HostModelExecutionRequest {
        entry_protocol: "openai".into(),
        exit_protocol: "claude".into(),
        model: "gpt-test".into(),
        stream: true,
        body: br#"{"input":"hello"}"#.to_vec(),
        headers: Headers::from([("X-Test".into(), vec!["one".into(), "two".into()])]),
        query: QueryValues::from([("alt".into(), vec!["beta".into()])]),
        alt: "chat".into(),
    };
    let value = serde_json::to_value(&request).unwrap();
    for field in [
        "entry_protocol",
        "exit_protocol",
        "model",
        "stream",
        "body",
        "headers",
        "query",
        "alt",
    ] {
        assert!(value.get(field).is_some(), "missing {field}");
    }
    let decoded: HostModelExecutionRequest = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, request);
    let stream = HostModelStreamResponse {
        status_code: 200,
        headers: Headers::new(),
        stream_id: "stream-1".into(),
    };
    assert_eq!(
        serde_json::to_value(stream).unwrap()["stream_id"],
        "stream-1"
    );
    let read = HostModelStreamReadResponse {
        payload: b"data\n".to_vec(),
        error: "temporary".into(),
        done: true,
    };
    let decoded: HostModelStreamReadResponse =
        serde_json::from_value(serde_json::to_value(&read).unwrap()).unwrap();
    assert_eq!(decoded, read);
}

#[test]
fn scheduler_and_router_types_expose_all_routing_fields() {
    let scheduler = SchedulerPickRequest {
        plugin: Metadata {
            name: "scheduler-plugin".into(),
            ..Default::default()
        },
        provider: "openai".into(),
        providers: vec!["openai".into(), "gemini".into()],
        model: "gpt-test".into(),
        stream: true,
        options: SchedulerOptions {
            headers: Headers::from([("X-Test".into(), vec!["1".into()])]),
            metadata: JsonMetadata::from([("tenant".into(), json!("demo"))]),
        },
        candidates: vec![SchedulerAuthCandidate {
            id: "auth-1".into(),
            provider: "openai".into(),
            priority: 10,
            status: "ready".into(),
            attributes: BTreeMap::from([("region".into(), "us".into())]),
            metadata: JsonMetadata::from([("load".into(), json!(0.5))]),
        }],
    };
    assert_eq!(scheduler.providers[1], "gemini");
    assert_eq!(scheduler.candidates[0].priority, 10);
    let response = SchedulerPickResponse {
        auth_id: "auth-1".into(),
        delegate_builtin: SCHEDULER_BUILTIN_ROUND_ROBIN.into(),
        handled: true,
    };
    assert!(response.handled);
    let route = ModelRouteRequest {
        plugin: Metadata {
            name: "router-plugin".into(),
            ..Default::default()
        },
        plugin_id: "router-plugin-id".into(),
        source_format: "anthropic".into(),
        requested_model: "claude-sonnet".into(),
        stream: true,
        headers: Headers::from([("X-Test".into(), vec!["1".into()])]),
        query: QueryValues::from([("beta".into(), vec!["true".into()])]),
        body: br#"{"model":"claude-sonnet"}"#.to_vec(),
        metadata: JsonMetadata::from([("tenant".into(), json!("demo"))]),
        available_providers: vec!["claude".into()],
    };
    assert_eq!(route.plugin_id, "router-plugin-id");
    let routed = ModelRouteResponse {
        handled: true,
        target_kind: ModelRouteTargetKind(ModelRouteTargetKind::EXECUTOR.into()),
        target: "executor".into(),
        reason: "typed websearch".into(),
        ..Default::default()
    };
    assert_eq!(routed.reason, "typed websearch");
}

struct Management;
impl ManagementHandler for Management {
    fn handle_management<'a>(
        &'a self,
        _request: ManagementRequest,
    ) -> PluginFuture<'a, ManagementResponse> {
        Box::pin(async { Ok(ManagementResponse::default()) })
    }
}

#[test]
fn resource_route_exposes_management_ui_hints() {
    let route = ResourceRoute {
        path: "/status".into(),
        menu: "Example Status".into(),
        description: "Shows status.".into(),
        handler: Arc::new(Management),
    };
    assert!(!route.menu.is_empty());
    assert!(!route.description.is_empty());
}

#[test]
fn executor_request_round_trips_go_field_names_without_host_client() {
    let request = ExecutorRequest {
        auth_id: "auth-1".into(),
        auth_provider: "claude".into(),
        model: "sonnet".into(),
        format: "claude".into(),
        stream: true,
        headers: Headers::from([("X-Test".into(), vec!["one".into(), "two".into()])]),
        query: QueryValues::from([("alt".into(), vec!["beta".into()])]),
        payload: br#"{"input":"hello"}"#.to_vec(),
        metadata: JsonMetadata::from([("extension".into(), json!(true))]),
        http_client: None,
        ..ExecutorRequest::default()
    };
    let encoded = serde_json::to_value(&request).expect("serialize");
    assert_eq!(encoded["AuthID"], "auth-1");
    assert_eq!(encoded["Payload"], "eyJpbnB1dCI6ImhlbGxvIn0=");
    assert!(encoded.get("HTTPClient").is_none());
    let decoded: ExecutorRequest = serde_json::from_value(encoded).expect("deserialize");
    assert_eq!(decoded.headers["X-Test"], ["one", "two"]);
    assert!(decoded.http_client.is_none());
}

#[tokio::test]
async fn provider_executor_trait_is_object_safe_and_preserves_payloads() {
    let executor: Arc<dyn ProviderExecutor> = Arc::new(CompileTimeExecutor);
    assert_eq!(executor.identifier(), "compile-time");
    let response = executor
        .execute(ExecutorRequest {
            payload: b"payload".to_vec(),
            ..ExecutorRequest::default()
        })
        .await
        .expect("execute");
    assert_eq!(response.payload, b"payload");
    assert!(executor
        .count_tokens(ExecutorRequest::default())
        .await
        .is_err());
    let response = executor
        .http_request(ExecutorHttpRequest {
            body: b"body".to_vec(),
            ..ExecutorHttpRequest::default()
        })
        .await
        .expect("http request");
    assert_eq!(response.status_code, 200);
    assert_eq!(response.body, b"body");
}
