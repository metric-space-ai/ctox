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

struct Management;
impl ManagementHandler for Management {
    fn handle_management<'a>(
        &'a self,
        _request: ManagementRequest,
    ) -> PluginFuture<'a, ManagementResponse> {
        Box::pin(async { Ok(ManagementResponse::default()) })
    }
}
