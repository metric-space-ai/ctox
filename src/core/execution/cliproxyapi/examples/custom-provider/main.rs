// ref: examples/custom-provider/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use ctox_cliproxyapi::internal::registry::{embedded_models_catalog, ModelRegistry as Registry};
use ctox_cliproxyapi::sdk::cliproxy::auth::{
    Auth, AuthRefresher, ProviderExecutorRegistration, ProviderExecutorRegistry,
    RefreshExecutorError,
};
use ctox_cliproxyapi::sdk::cliproxy::model_registry::ModelInfo;
use ctox_cliproxyapi::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, HostHttpClient, HttpRequest, HttpResponse,
    HttpStreamResponse, PluginExecutionError, PluginFuture, ProviderExecutor,
};
use ctox_cliproxyapi::sdk::translator::{
    Format, Registry as TranslatorRegistry, ResponseTransform, TranslationContext, TranslationState,
};
use tokio::sync::mpsc;

const PROVIDER_KEY: &str = "myprov";
const OPENAI_CHAT: &str = "openai.chat";
const MYPROV_CHAT: &str = "myprov.chat";
const DEFAULT_ENDPOINT: &str = "https://httpbin.org/post";

#[derive(Debug)]
struct ExampleError(&'static str);

impl fmt::Display for ExampleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for ExampleError {}

fn example_error(message: &'static str) -> PluginExecutionError {
    Arc::new(ExampleError(message))
}

fn endpoint(request: &ExecutorRequest) -> String {
    request
        .auth_attributes
        .get("endpoint")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ENDPOINT)
        .to_owned()
}

fn inject_api_key(headers: &mut ctox_cliproxyapi::sdk::pluginapi::Headers, api_key: Option<&str>) {
    if let Some(api_key) = api_key.map(str::trim).filter(|value| !value.is_empty()) {
        headers.insert(
            "Authorization".to_owned(),
            vec![format!("Bearer {api_key}")],
        );
    }
}

struct MyExecutor;

impl ProviderExecutor for MyExecutor {
    fn identifier(&self) -> &str {
        PROVIDER_KEY
    }

    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move {
            let client = request
                .http_client
                .clone()
                .ok_or_else(|| example_error("myprov executor: host HTTP client is required"))?;
            let mut headers = request.headers.clone();
            headers.insert(
                "Content-Type".to_owned(),
                vec!["application/json".to_owned()],
            );
            inject_api_key(
                &mut headers,
                request.auth_attributes.get("api_key").map(String::as_str),
            );
            let response = client
                .execute(HttpRequest {
                    method: "POST".to_owned(),
                    url: endpoint(&request),
                    headers,
                    body: request.payload,
                })
                .await?;
            Ok(ExecutorResponse {
                payload: response.body,
                headers: response.headers,
                metadata: Default::default(),
            })
        })
    }

    fn execute_stream<'a>(
        &'a self,
        _request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(async move {
            let (sender, receiver) = mpsc::channel(1);
            sender
                .send(ExecutorStreamChunk {
                    payload: b"data: {\"ok\":true}\n\n".to_vec(),
                    error: None,
                })
                .await
                .map_err(|_| example_error("myprov executor: stream receiver closed"))?;
            drop(sender);
            Ok(ExecutorStreamResponse {
                headers: Default::default(),
                chunks: receiver,
            })
        })
    }

    fn count_tokens<'a>(&'a self, _request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async {
            Err(example_error(
                "myprov executor: CountTokens not implemented",
            ))
        })
    }

    fn http_request<'a>(
        &'a self,
        mut request: ExecutorHttpRequest,
    ) -> PluginFuture<'a, ExecutorHttpResponse> {
        Box::pin(async move {
            inject_api_key(
                &mut request.headers,
                request.attributes.get("api_key").map(String::as_str),
            );
            let client = request
                .http_client
                .ok_or_else(|| example_error("myprov executor: host HTTP client is required"))?;
            let response = client
                .execute(HttpRequest {
                    method: request.method,
                    url: request.url,
                    headers: request.headers,
                    body: request.body,
                })
                .await?;
            Ok(ExecutorHttpResponse {
                status_code: response.status_code,
                headers: response.headers,
                body: response.body,
            })
        })
    }
}

struct MyRefresher;

impl AuthRefresher for MyRefresher {
    fn refresh(&self, _auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        Ok(None)
    }
}

#[derive(Default)]
struct EchoHost;

impl HostHttpClient for EchoHost {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            let body = serde_json::to_vec(&serde_json::json!({
                "method": request.method,
                "url": request.url,
                "headers": request.headers,
                "body": String::from_utf8_lossy(&request.body),
            }))
            .map_err(|_| example_error("myprov executor: response encoding failed"))?;
            Ok(HttpResponse {
                status_code: 200,
                body,
                ..HttpResponse::default()
            })
        })
    }

    fn execute_stream<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async { Err(example_error("myprov host: streaming HTTP not implemented")) })
    }
}

fn executor_registry() -> ProviderExecutorRegistry {
    let registry = ProviderExecutorRegistry::default();
    let registration = ProviderExecutorRegistration::new(PROVIDER_KEY, Arc::new(MyRefresher))
        .expect("static provider key")
        .with_execution(Arc::new(MyExecutor))
        .expect("matching executor identifier");
    registry.register(Arc::new(registration));
    registry
}

fn translator_registry() -> TranslatorRegistry {
    let registry = TranslatorRegistry::new();
    registry.register_pair(
        Format::from(OPENAI_CHAT),
        Format::from(MYPROV_CHAT),
        Arc::new(|_, raw, _| raw.to_vec()),
        ResponseTransform {
            stream: Some(Arc::new(
                |_: &TranslationContext, _, _, _, raw, _: &mut TranslationState| vec![raw.to_vec()],
            )),
            non_stream: Some(Arc::new(
                |_: &TranslationContext, _, _, _, raw, _: &mut TranslationState| raw.to_vec(),
            )),
            token_count: None,
        },
    );
    registry
}

fn model_registry() -> Result<Arc<Registry>, Box<dyn Error + Send + Sync>> {
    let registry = Arc::new(Registry::new(Arc::new(embedded_models_catalog()?)));
    registry.register_client(
        "demo-auth",
        PROVIDER_KEY,
        &[ModelInfo {
            id: "myprov-pro-1".to_owned(),
            object: "model".to_owned(),
            provider_type: PROVIDER_KEY.to_owned(),
            display_name: "MyProv Pro 1".to_owned(),
            ..ModelInfo::default()
        }],
    );
    Ok(registry)
}

async fn run_demo() -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let translators = translator_registry();
    let translated = translators.translate_request(
        &TranslationContext::default(),
        &Format::from(OPENAI_CHAT),
        &Format::from(MYPROV_CHAT),
        "myprov-pro-1",
        br#"{"messages":[]}"#,
        false,
    );
    let response = executor_registry()
        .execute(
            PROVIDER_KEY,
            ExecutorRequest {
                model: "myprov-pro-1".to_owned(),
                payload: translated,
                auth_attributes: [
                    ("api_key".to_owned(), "demo-api-key".to_owned()),
                    ("endpoint".to_owned(), DEFAULT_ENDPOINT.to_owned()),
                ]
                .into_iter()
                .collect(),
                http_client: Some(Arc::new(EchoHost)),
                ..ExecutorRequest::default()
            },
        )
        .await?;
    let models = model_registry()?;
    assert!(models.client_supports_model("demo-auth", "myprov-pro-1"));
    Ok(response.payload)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("{}", String::from_utf8_lossy(&run_demo().await?));
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
}
