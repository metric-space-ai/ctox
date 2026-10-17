// ref: test/usage_logging_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::Arc;

use serde_json::Value;

use crate::internal::redisqueue::{UsageQueue, UsageQueuePlugin, UsageStatisticsSwitch};
use crate::internal::runtime::executor::gemini_executor::{GeminiExecutor, GeminiExecutorConfig};
use crate::internal::translator::register_all;
use crate::sdk::cliproxy::usage::Manager;
use crate::sdk::pluginapi::{
    ExecutorRequest, HostHttpClient, HttpRequest, HttpResponse, HttpStreamResponse, PluginFuture,
    ProviderExecutor,
};
use crate::sdk::translator::Registry;

struct ZeroUsageClient;

impl HostHttpClient for ZeroUsageClient {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            assert_eq!(
                request.url,
                "http://upstream/v1beta/models/gemini-zero-usage:generateContent"
            );
            Ok(HttpResponse {
                status_code: 200,
                body: br#"{"candidates":[{"content":{"role":"model","parts":[{"text":"ok"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":0,"candidatesTokenCount":0,"totalTokenCount":0}}"#.to_vec(),
                ..HttpResponse::default()
            })
        })
    }

    fn execute_stream<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async { panic!("stream not used") })
    }
}
