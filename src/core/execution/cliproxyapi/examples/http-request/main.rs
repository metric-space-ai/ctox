// ref: examples/http-request/main.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use ctox_cliproxyapi::sdk::cliproxy::auth::{
    Auth, AuthRefresher, ProviderExecutorRegistration, ProviderExecutorRegistry,
    RefreshExecutorError,
};
use ctox_cliproxyapi::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamResponse, HostHttpClient, HttpRequest, HttpResponse, HttpStreamResponse,
    PluginExecutionError, PluginFuture, ProviderExecutor,
};

const PROVIDER_KEY: &str = "echo";

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

struct EchoExecutor;

impl ProviderExecutor for EchoExecutor {
    fn identifier(&self) -> &str {
        PROVIDER_KEY
    }

    fn execute<'a>(&'a self, _request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async { Err(example_error("echo executor: Execute not implemented")) })
    }

    fn execute_stream<'a>(
        &'a self,
        _request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(async {
            Err(example_error(
                "echo executor: ExecuteStream not implemented",
            ))
        })
    }

    fn count_tokens<'a>(&'a self, _request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async { Err(example_error("echo executor: CountTokens not implemented")) })
    }

    fn http_request<'a>(
        &'a self,
        mut request: ExecutorHttpRequest,
    ) -> PluginFuture<'a, ExecutorHttpResponse> {
        Box::pin(async move {
            if let Some(api_key) = request
                .attributes
                .get("api_key")
                .map(String::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                request.headers.insert(
                    "Authorization".to_owned(),
                    vec![format!("Bearer {api_key}")],
                );
            }
            let client = request
                .http_client
                .ok_or_else(|| example_error("echo executor: host HTTP client is required"))?;
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

struct EchoRefresher;

impl AuthRefresher for EchoRefresher {
    fn refresh(&self, _auth: &mut Auth) -> Result<Option<Auth>, RefreshExecutorError> {
        Ok(None)
    }
}

#[derive(Default)]
struct DemoHostHttpClient;

impl HostHttpClient for DemoHostHttpClient {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            let body = serde_json::to_vec(&serde_json::json!({
                "method": request.method,
                "url": request.url,
                "headers": request.headers,
                "body": String::from_utf8_lossy(&request.body),
            }))
            .map_err(|_| example_error("echo executor: response encoding failed"))?;
            Ok(HttpResponse {
                status_code: 200,
                body,
                ..HttpResponse::default()
            })
        })
    }

    fn execute_stream<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async {
            Err(example_error(
                "echo executor: streaming HTTP not implemented",
            ))
        })
    }
}

fn registry() -> ProviderExecutorRegistry {
    let registry = ProviderExecutorRegistry::default();
    let refresher: Arc<dyn AuthRefresher> = Arc::new(EchoRefresher);
    let execution: Arc<dyn ProviderExecutor> = Arc::new(EchoExecutor);
    let registration = ProviderExecutorRegistration::new(PROVIDER_KEY, refresher)
        .expect("static provider key")
        .with_execution(execution)
        .expect("matching executor identifier");
    registry.register(Arc::new(registration));
    registry
}

async fn execute_example_requests() -> Result<(HttpResponse, HttpResponse), PluginExecutionError> {
    let registry = registry();
    let client: Arc<dyn HostHttpClient> = Arc::new(DemoHostHttpClient);
    let request = |method: &str, body: &[u8], marker: &str| ExecutorHttpRequest {
        auth_id: "demo-echo".to_owned(),
        auth_provider: PROVIDER_KEY.to_owned(),
        method: method.to_owned(),
        url: "https://httpbin.org/anything".to_owned(),
        headers: [("X-Example".to_owned(), vec![marker.to_owned()])]
            .into_iter()
            .collect(),
        body: body.to_vec(),
        attributes: [("api_key".to_owned(), "demo-api-key".to_owned())]
            .into_iter()
            .collect(),
        http_client: Some(client.clone()),
        ..ExecutorHttpRequest::default()
    };
    let prepared = registry
        .http_request(PROVIDER_KEY, request("GET", b"", "prepared"))
        .await
        .map_err(|error| Arc::new(error) as PluginExecutionError)?;
    let executed = registry
        .http_request(
            PROVIDER_KEY,
            request("POST", br#"{"hello":"world"}"#, "executed"),
        )
        .await
        .map_err(|error| Arc::new(error) as PluginExecutionError)?;
    Ok((
        HttpResponse {
            status_code: prepared.status_code,
            headers: prepared.headers,
            body: prepared.body,
        },
        HttpResponse {
            status_code: executed.status_code,
            headers: executed.headers,
            body: executed.body,
        },
    ))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (prepared, executed) = execute_example_requests().await?;
    println!(
        "Prepared request status: {}\n{}\n",
        prepared.status_code,
        String::from_utf8_lossy(&prepared.body)
    );
    println!(
        "Manager HttpRequest status: {}\n{}",
        executed.status_code,
        String::from_utf8_lossy(&executed.body)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

}
