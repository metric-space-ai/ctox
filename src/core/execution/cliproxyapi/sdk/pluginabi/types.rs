// ref: sdk/pluginabi/types.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

pub const ABI_VERSION: u32 = 1;
pub const SCHEMA_VERSION: u32 = 2;

pub const METHOD_PLUGIN_REGISTER: &str = "plugin.register";
pub const METHOD_PLUGIN_RECONFIGURE: &str = "plugin.reconfigure";
pub const METHOD_PLUGIN_SHUTDOWN: &str = "plugin.shutdown";
pub const METHOD_MODEL_REGISTER: &str = "model.register";
pub const METHOD_MODEL_STATIC: &str = "model.static";
pub const METHOD_MODEL_FOR_AUTH: &str = "model.for_auth";
pub const METHOD_AUTH_IDENTIFIER: &str = "auth.identifier";
pub const METHOD_AUTH_PARSE: &str = "auth.parse";
pub const METHOD_AUTH_LOGIN_START: &str = "auth.login.start";
pub const METHOD_AUTH_LOGIN_POLL: &str = "auth.login.poll";
pub const METHOD_AUTH_REFRESH: &str = "auth.refresh";
pub const METHOD_FRONTEND_AUTH_IDENTIFIER: &str = "frontend_auth.identifier";
pub const METHOD_FRONTEND_AUTH_AUTHENTICATE: &str = "frontend_auth.authenticate";
pub const METHOD_SCHEDULER_PICK: &str = "scheduler.pick";
pub const METHOD_MODEL_ROUTE: &str = "model.route";
pub const METHOD_EXECUTOR_IDENTIFIER: &str = "executor.identifier";
pub const METHOD_EXECUTOR_EXECUTE: &str = "executor.execute";
pub const METHOD_EXECUTOR_EXECUTE_STREAM: &str = "executor.execute_stream";
pub const METHOD_EXECUTOR_COUNT_TOKENS: &str = "executor.count_tokens";
pub const METHOD_EXECUTOR_HTTP_REQUEST: &str = "executor.http_request";
pub const METHOD_REQUEST_TRANSLATE: &str = "request.translate";
pub const METHOD_REQUEST_NORMALIZE: &str = "request.normalize";
pub const METHOD_REQUEST_INTERCEPT_BEFORE: &str = "request.intercept_before";
pub const METHOD_REQUEST_INTERCEPT_AFTER: &str = "request.intercept_after";
pub const METHOD_REQUEST_COMPLETE: &str = "request.complete";
pub const METHOD_RESPONSE_TRANSLATE: &str = "response.translate";
pub const METHOD_RESPONSE_NORMALIZE_BEFORE: &str = "response.normalize_before";
pub const METHOD_RESPONSE_NORMALIZE_AFTER: &str = "response.normalize_after";
pub const METHOD_RESPONSE_INTERCEPT_AFTER: &str = "response.intercept_after";
pub const METHOD_RESPONSE_INTERCEPT_STREAM_CHUNK: &str = "response.intercept_stream_chunk";
pub const METHOD_THINKING_IDENTIFIER: &str = "thinking.identifier";
pub const METHOD_THINKING_APPLY: &str = "thinking.apply";
pub const METHOD_USAGE_HANDLE: &str = "usage.handle";
// ref: sdk/pluginabi/types.go:90-93 @ 5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3
pub const METHOD_QUOTA_IDENTIFIER: &str = "quota.identifier";
pub const METHOD_QUOTA_DESCRIBE: &str = "quota.describe";
pub const METHOD_QUOTA_FETCH: &str = "quota.fetch";
pub const METHOD_QUOTA_RESET: &str = "quota.reset";
pub const METHOD_COMMAND_LINE_REGISTER: &str = "command_line.register";
pub const METHOD_COMMAND_LINE_EXECUTE: &str = "command_line.execute";
pub const METHOD_MANAGEMENT_REGISTER: &str = "management.register";
pub const METHOD_MANAGEMENT_HANDLE: &str = "management.handle";
pub const METHOD_HOST_HTTP_DO: &str = "host.http.do";
pub const METHOD_HOST_HTTP_DO_STREAM: &str = "host.http.do_stream";
pub const METHOD_HOST_HTTP_STREAM_READ: &str = "host.http.stream_read";
pub const METHOD_HOST_HTTP_STREAM_CLOSE: &str = "host.http.stream_close";
pub const METHOD_HOST_MODEL_EXECUTE: &str = "host.model.execute";
pub const METHOD_HOST_MODEL_EXECUTE_STREAM: &str = "host.model.execute_stream";
pub const METHOD_HOST_MODEL_STREAM_READ: &str = "host.model.stream_read";
pub const METHOD_HOST_MODEL_STREAM_CLOSE: &str = "host.model.stream_close";
pub const METHOD_HOST_STREAM_EMIT: &str = "host.stream.emit";
pub const METHOD_HOST_STREAM_CLOSE: &str = "host.stream.close";
pub const METHOD_HOST_LOG: &str = "host.log";
pub const METHOD_HOST_AUTH_LIST: &str = "host.auth.list";
pub const METHOD_HOST_AUTH_GET: &str = "host.auth.get";
pub const METHOD_HOST_AUTH_GET_RUNTIME: &str = "host.auth.get_runtime";
pub const METHOD_HOST_AUTH_SAVE: &str = "host.auth.save";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Envelope {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Box<RawValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
}

impl Envelope {
    pub fn success(result: Option<Box<RawValue>>) -> Self {
        Self {
            ok: true,
            result,
            error: None,
        }
    }

    pub fn failure(error: Error) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Error {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub http_status: i32,
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}
