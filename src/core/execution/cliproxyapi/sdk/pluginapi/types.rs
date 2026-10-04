// ref: sdk/pluginapi/types.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metadata {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Author")]
    pub author: String,
    #[serde(rename = "GitHubRepository")]
    pub github_repository: String,
    #[serde(rename = "Logo")]
    pub logo: String,
    #[serde(rename = "ConfigFields", default)]
    pub config_fields: Vec<ConfigField>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConfigFieldType(pub String);

impl ConfigFieldType {
    pub const STRING: &'static str = "string";
    pub const NUMBER: &'static str = "number";
    pub const INTEGER: &'static str = "integer";
    pub const BOOLEAN: &'static str = "boolean";
    pub const ENUM: &'static str = "enum";
    pub const ARRAY: &'static str = "array";
    pub const OBJECT: &'static str = "object";
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigField {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Type")]
    pub field_type: ConfigFieldType,
    #[serde(rename = "EnumValues", default)]
    pub enum_values: Vec<String>,
    #[serde(rename = "Description")]
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExecutorModelScope(pub String);

impl ExecutorModelScope {
    pub const BOTH: &'static str = "both";
    pub const STATIC: &'static str = "static";
    pub const OAUTH: &'static str = "oauth";
}

pub type Headers = BTreeMap<String, Vec<String>>;
pub type QueryValues = BTreeMap<String, Vec<String>>;
pub type JsonMetadata = BTreeMap<String, Value>;
pub type PluginExecutionError = Arc<dyn Error + Send + Sync + 'static>;
pub type PluginFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, PluginExecutionError>> + Send + 'a>>;

macro_rules! plugin_dto {
    ($(#[$meta:meta])* $name:ident { $($(#[$field_meta:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        #[serde(default, rename_all = "PascalCase")]
        pub struct $name { $($(#[$field_meta])* pub $field: $ty),* }
    };
}

#[derive(Default)]
pub struct Plugin {
    pub metadata: Metadata,
    pub capabilities: Capabilities,
}

#[derive(Default)]
pub struct Capabilities {
    pub model_registrar: Option<Arc<dyn ModelRegistrar>>,
    pub model_provider: Option<Arc<dyn ModelProvider>>,
    pub auth_provider: Option<Arc<dyn AuthProvider>>,
    pub frontend_auth_provider: Option<Arc<dyn FrontendAuthProvider>>,
    pub frontend_auth_provider_exclusive: bool,
    pub scheduler: Option<Arc<dyn Scheduler>>,
    pub model_router: Option<Arc<dyn ModelRouter>>,
    pub executor: Option<Arc<dyn ProviderExecutor>>,
    pub executor_model_scope: ExecutorModelScope,
    pub executor_input_formats: Vec<String>,
    pub executor_output_formats: Vec<String>,
    pub request_translator: Option<Arc<dyn RequestTranslator>>,
    pub request_normalizer: Option<Arc<dyn RequestNormalizer>>,
    pub response_translator: Option<Arc<dyn ResponseTranslator>>,
    pub response_before_translator: Option<Arc<dyn ResponseNormalizer>>,
    pub response_after_translator: Option<Arc<dyn ResponseNormalizer>>,
    pub request_interceptor: Option<Arc<dyn RequestInterceptor>>,
    pub request_lifecycle_plugin: Option<Arc<dyn RequestLifecyclePlugin>>,
    pub response_interceptor: Option<Arc<dyn ResponseInterceptor>>,
    pub stream_chunk_interceptor: Option<Arc<dyn StreamChunkInterceptor>>,
    pub thinking_applier: Option<Arc<dyn ThinkingApplier>>,
    pub usage_plugin: Option<Arc<dyn UsagePlugin>>,
    pub command_line_plugin: Option<Arc<dyn CommandLinePlugin>>,
    pub management_api: Option<Arc<dyn ManagementApi>>,
}

plugin_dto!(ModelInfo {
    #[serde(rename = "ID")] id: String,
    object: String,
    created: i64,
    owned_by: String,
    #[serde(rename = "Type")] kind: String,
    display_name: String,
    name: String,
    version: String,
    description: String,
    input_token_limit: i64,
    output_token_limit: i64,
    supported_generation_methods: Vec<String>,
    context_length: i64,
    max_completion_tokens: i64,
    supported_parameters: Vec<String>,
    supported_input_modalities: Vec<String>,
    supported_output_modalities: Vec<String>,
    thinking: Option<ThinkingSupport>,
    user_defined: bool
});

plugin_dto!(ThinkingSupport {
    min: i32, max: i32, zero_allowed: bool, dynamic_allowed: bool, levels: Vec<String>
});

plugin_dto!(HostConfigSummary {
    auth_dir: String,
    #[serde(rename = "ProxyURL")] proxy_url: String,
    force_model_prefix: bool,
    #[serde(rename = "OAuthModelAlias")] oauth_model_alias: BTreeMap<String, Vec<ModelAlias>>,
    excluded_models: BTreeMap<String, Vec<String>>
});

plugin_dto!(ModelAlias {
    name: String,
    alias: String
});

plugin_dto!(AuthData {
    provider: String,
    #[serde(rename = "ID")] id: String,
    file_name: String,
    label: String,
    prefix: String,
    #[serde(rename = "ProxyURL")] proxy_url: String,
    disabled: bool,
    #[serde(rename = "StorageJSON", with = "go_bytes")] storage_json: Vec<u8>,
    metadata: JsonMetadata,
    attributes: BTreeMap<String, String>,
    next_refresh_after: Option<DateTime<Utc>>
});

plugin_dto!(AuthParseRequest {
    provider: String, path: String, file_name: String,
    #[serde(rename = "RawJSON", with = "go_bytes")] raw_json: Vec<u8>,
    host: HostConfigSummary
});
plugin_dto!(AuthParseResponse { handled: bool, auth: AuthData, auths: Vec<AuthData> });

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthLoginStatus(pub String);
impl AuthLoginStatus {
    pub const PENDING: &'static str = "pending";
    pub const SUCCESS: &'static str = "success";
    pub const ERROR: &'static str = "error";
}

macro_rules! authority_request {
    ($name:ident { $($(#[$field_meta:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Clone, Default, Serialize, Deserialize)]
        #[serde(default, rename_all = "PascalCase")]
        pub struct $name {
            $($(#[$field_meta])* pub $field: $ty,)*
            #[serde(skip)]
            pub http_client: Option<Arc<dyn HostHttpClient>>,
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($name)).field("has_http_client", &self.http_client.is_some()).finish_non_exhaustive()
            }
        }
    };
}

authority_request!(AuthLoginStartRequest {
    provider: String,
    base_url: String,
    host: HostConfigSummary,
    metadata: JsonMetadata
});
plugin_dto!(AuthLoginStartResponse {
    provider: String, #[serde(rename = "URL")] url: String, state: String,
    expires_at: Option<DateTime<Utc>>, metadata: JsonMetadata
});
authority_request!(AuthLoginPollRequest {
    provider: String,
    state: String,
    host: HostConfigSummary,
    metadata: JsonMetadata
});
plugin_dto!(AuthLoginPollResponse {
    status: AuthLoginStatus, message: String, auth: AuthData, auths: Vec<AuthData>
});
authority_request!(AuthRefreshRequest {
    #[serde(rename = "AuthID")] auth_id: String,
    auth_provider: String,
    #[serde(rename = "StorageJSON", with = "go_bytes")] storage_json: Vec<u8>,
    metadata: JsonMetadata,
    attributes: BTreeMap<String, String>,
    host: HostConfigSummary
});
plugin_dto!(AuthRefreshResponse { auth: AuthData, next_refresh_after: Option<DateTime<Utc>> });

pub trait AuthProvider: Send + Sync {
    fn identifier(&self) -> &str;
    fn parse_auth<'a>(&'a self, request: AuthParseRequest) -> PluginFuture<'a, AuthParseResponse>;
    fn start_login<'a>(
        &'a self,
        request: AuthLoginStartRequest,
    ) -> PluginFuture<'a, AuthLoginStartResponse>;
    fn poll_login<'a>(
        &'a self,
        request: AuthLoginPollRequest,
    ) -> PluginFuture<'a, AuthLoginPollResponse>;
    fn refresh_auth<'a>(
        &'a self,
        request: AuthRefreshRequest,
    ) -> PluginFuture<'a, AuthRefreshResponse>;
}

plugin_dto!(ModelRegistrationRequest { plugin: Metadata });
plugin_dto!(ModelRegistrationResponse { provider: String, models: Vec<ModelInfo> });
plugin_dto!(StaticModelRequest {
    plugin: Metadata,
    host: HostConfigSummary
});
authority_request!(AuthModelRequest {
    plugin: Metadata,
    #[serde(rename = "AuthID")] auth_id: String,
    auth_provider: String,
    #[serde(rename = "StorageJSON", with = "go_bytes")] storage_json: Vec<u8>,
    metadata: JsonMetadata,
    attributes: BTreeMap<String, String>,
    host: HostConfigSummary
});
plugin_dto!(ModelResponse { provider: String, models: Vec<ModelInfo>, auth_update: AuthData });

pub trait ModelRegistrar: Send + Sync {
    fn register_models<'a>(
        &'a self,
        request: ModelRegistrationRequest,
    ) -> PluginFuture<'a, ModelRegistrationResponse>;
}
pub trait ModelProvider: Send + Sync {
    fn static_models<'a>(&'a self, request: StaticModelRequest) -> PluginFuture<'a, ModelResponse>;
    fn models_for_auth<'a>(&'a self, request: AuthModelRequest) -> PluginFuture<'a, ModelResponse>;
}

plugin_dto!(FrontendAuthRequest {
    method: String, path: String, headers: Headers, query: QueryValues,
    #[serde(with = "go_bytes")] body: Vec<u8>
});
plugin_dto!(FrontendAuthResponse { authenticated: bool, principal: String, metadata: BTreeMap<String, String> });
pub trait FrontendAuthProvider: Send + Sync {
    fn identifier(&self) -> &str;
    fn authenticate<'a>(
        &'a self,
        request: FrontendAuthRequest,
    ) -> PluginFuture<'a, FrontendAuthResponse>;
}

pub const SCHEDULER_BUILTIN_ROUND_ROBIN: &str = "round-robin";
pub const SCHEDULER_BUILTIN_FILL_FIRST: &str = "fill-first";

plugin_dto!(SchedulerOptions {
    headers: Headers,
    metadata: JsonMetadata
});
plugin_dto!(SchedulerAuthCandidate {
    #[serde(rename = "ID")] id: String, provider: String, priority: i32, status: String,
    attributes: BTreeMap<String, String>, metadata: JsonMetadata
});
plugin_dto!(SchedulerPickRequest {
    plugin: Metadata, provider: String, providers: Vec<String>, model: String, stream: bool,
    options: SchedulerOptions, candidates: Vec<SchedulerAuthCandidate>
});
plugin_dto!(SchedulerPickResponse {
    #[serde(rename = "AuthID")]
    auth_id: String,
    delegate_builtin: String,
    handled: bool
});
pub trait Scheduler: Send + Sync {
    fn pick<'a>(&'a self, request: SchedulerPickRequest)
        -> PluginFuture<'a, SchedulerPickResponse>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelRouteTargetKind(pub String);
impl ModelRouteTargetKind {
    pub const SELF: &'static str = "self";
    pub const EXECUTOR: &'static str = "executor";
    pub const PROVIDER: &'static str = "provider";
}
plugin_dto!(ModelRouteRequest {
    plugin: Metadata,
    #[serde(rename = "PluginID")] plugin_id: String,
    source_format: String, requested_model: String, stream: bool,
    headers: Headers, query: QueryValues, #[serde(with = "go_bytes")] body: Vec<u8>,
    metadata: JsonMetadata, available_providers: Vec<String>
});
plugin_dto!(ModelRouteResponse {
    handled: bool,
    target_kind: ModelRouteTargetKind,
    target: String,
    target_model: String,
    reason: String
});
pub trait ModelRouter: Send + Sync {
    fn route_model<'a>(
        &'a self,
        request: ModelRouteRequest,
    ) -> PluginFuture<'a, ModelRouteResponse>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct HostModelExecutionRequest {
    pub entry_protocol: String,
    pub exit_protocol: String,
    pub model: String,
    pub stream: bool,
    #[serde(with = "go_bytes")]
    pub body: Vec<u8>,
    pub headers: Headers,
    pub query: QueryValues,
    pub alt: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct HostModelExecutionResponse {
    pub status_code: u16,
    pub headers: Headers,
    #[serde(with = "go_bytes")]
    pub body: Vec<u8>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct HostModelStreamResponse {
    pub status_code: u16,
    pub headers: Headers,
    pub stream_id: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct HostModelStreamReadRequest {
    pub stream_id: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct HostModelStreamReadResponse {
    #[serde(with = "go_bytes")]
    pub payload: Vec<u8>,
    pub error: String,
    pub done: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct HostModelStreamCloseRequest {
    pub stream_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRecentRequestEntry {
    pub time: String,
    pub success: i64,
    pub failed: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostAuthFileEntry {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub auth_index: String,
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub provider: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status_message: String,
    #[serde(skip_serializing_if = "is_false")]
    pub disabled: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub unavailable: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub runtime_only: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modtime: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_after: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub project_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub account_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub account: String,
    #[serde(skip_serializing_if = "is_zero_i32")]
    pub priority: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(skip_serializing_if = "is_false")]
    pub websockets: bool,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub success: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub failed: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_requests: Vec<HostRecentRequestEntry>,
}
fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}
fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HostAuthGetRequest {
    pub auth_index: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HostAuthGetResponse {
    pub auth_index: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    pub json: Value,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HostAuthGetRuntimeResponse {
    pub auth: HostAuthFileEntry,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HostAuthSaveRequest {
    pub name: String,
    pub json: Value,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HostAuthSaveResponse {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct HttpRequest {
    pub method: String,
    #[serde(rename = "URL")]
    pub url: String,
    pub headers: Headers,
    #[serde(with = "go_bytes")]
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct HttpResponse {
    pub status_code: u16,
    pub headers: Headers,
    #[serde(with = "go_bytes")]
    pub body: Vec<u8>,
}

pub struct HttpStreamChunk {
    pub payload: Vec<u8>,
    pub error: Option<PluginExecutionError>,
}

pub struct HttpStreamResponse {
    pub status_code: u16,
    pub headers: Headers,
    pub chunks: mpsc::Receiver<HttpStreamChunk>,
}

pub trait HostHttpClient: Send + Sync {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse>;
    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse>;
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ExecutorHttpRequest {
    #[serde(rename = "AuthID")]
    pub auth_id: String,
    pub auth_provider: String,
    pub method: String,
    #[serde(rename = "URL")]
    pub url: String,
    pub headers: Headers,
    #[serde(with = "go_bytes")]
    pub body: Vec<u8>,
    #[serde(rename = "StorageJSON", with = "go_bytes")]
    pub storage_json: Vec<u8>,
    pub metadata: JsonMetadata,
    pub attributes: BTreeMap<String, String>,
    #[serde(skip)]
    pub http_client: Option<Arc<dyn HostHttpClient>>,
}

impl fmt::Debug for ExecutorHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutorHttpRequest")
            .field("auth_id", &self.auth_id)
            .field("auth_provider", &self.auth_provider)
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("body_len", &self.body.len())
            .field("storage_json_len", &self.storage_json.len())
            .field("metadata_keys", &self.metadata.keys().collect::<Vec<_>>())
            .field(
                "attribute_keys",
                &self.attributes.keys().collect::<Vec<_>>(),
            )
            .field("has_http_client", &self.http_client.is_some())
            .finish()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ExecutorHttpResponse {
    pub status_code: u16,
    pub headers: Headers,
    #[serde(with = "go_bytes")]
    pub body: Vec<u8>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ExecutorRequest {
    #[serde(rename = "AuthID")]
    pub auth_id: String,
    pub auth_provider: String,
    pub model: String,
    pub format: String,
    pub stream: bool,
    pub alt: String,
    pub headers: Headers,
    pub query: QueryValues,
    #[serde(with = "go_bytes")]
    pub original_request: Vec<u8>,
    pub source_format: String,
    #[serde(with = "go_bytes")]
    pub payload: Vec<u8>,
    pub metadata: JsonMetadata,
    #[serde(rename = "StorageJSON", with = "go_bytes")]
    pub storage_json: Vec<u8>,
    pub auth_metadata: JsonMetadata,
    pub auth_attributes: BTreeMap<String, String>,
    /// Auth-manager-owned selected-model snapshot. It is intentionally absent
    /// from the public plugin/client JSON wire and cannot be supplied in metadata.
    #[serde(skip)]
    pub resolved_model_info: Option<Arc<crate::internal::modelconfig::ModelInfo>>,
    /// Central Home credential options, including false/default compatibility.
    /// Caller JSON and ordinary auth-manager paths cannot supply this authority.
    #[serde(skip)]
    pub resolved_home_model_options: Option<crate::internal::modelconfig::HomeModelOptions>,
    #[serde(skip)]
    pub http_client: Option<Arc<dyn HostHttpClient>>,
}

impl fmt::Debug for ExecutorRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutorRequest")
            .field("auth_id", &self.auth_id)
            .field("auth_provider", &self.auth_provider)
            .field("model", &self.model)
            .field("format", &self.format)
            .field("stream", &self.stream)
            .field("alt", &self.alt)
            .field("headers", &self.headers)
            .field("query", &self.query)
            .field("original_request_len", &self.original_request.len())
            .field("source_format", &self.source_format)
            .field("payload_len", &self.payload.len())
            .field("metadata_keys", &self.metadata.keys().collect::<Vec<_>>())
            .field("storage_json_len", &self.storage_json.len())
            .field(
                "auth_metadata_keys",
                &self.auth_metadata.keys().collect::<Vec<_>>(),
            )
            .field(
                "auth_attribute_keys",
                &self.auth_attributes.keys().collect::<Vec<_>>(),
            )
            .field("has_http_client", &self.http_client.is_some())
            .finish()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ExecutorResponse {
    #[serde(with = "go_bytes")]
    pub payload: Vec<u8>,
    pub headers: Headers,
    pub metadata: JsonMetadata,
}

pub struct ExecutorStreamChunk {
    pub payload: Vec<u8>,
    pub error: Option<PluginExecutionError>,
}

pub struct ExecutorStreamResponse {
    pub headers: Headers,
    pub chunks: mpsc::Receiver<ExecutorStreamChunk>,
}

/// Object-safe async equivalent of the pinned Go plugin executor contract.
pub trait ProviderExecutor: Send + Sync {
    fn identifier(&self) -> &str;
    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse>;
    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse>;
    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse>;
    fn http_request<'a>(
        &'a self,
        request: ExecutorHttpRequest,
    ) -> PluginFuture<'a, ExecutorHttpResponse>;
}

plugin_dto!(RequestTransformRequest {
    from_format: String, to_format: String, model: String, stream: bool,
    #[serde(with = "go_bytes")] body: Vec<u8>
});
plugin_dto!(ResponseTransformRequest {
    from_format: String, to_format: String, model: String, stream: bool,
    #[serde(with = "go_bytes")] original_request: Vec<u8>,
    #[serde(with = "go_bytes")] translated_request: Vec<u8>,
    #[serde(with = "go_bytes")] body: Vec<u8>
});
plugin_dto!(PayloadResponse { #[serde(with = "go_bytes")] body: Vec<u8> });

pub trait RequestTranslator: Send + Sync {
    fn translate_request<'a>(
        &'a self,
        request: RequestTransformRequest,
    ) -> PluginFuture<'a, PayloadResponse>;
}
pub trait RequestNormalizer: Send + Sync {
    fn normalize_request<'a>(
        &'a self,
        request: RequestTransformRequest,
    ) -> PluginFuture<'a, PayloadResponse>;
}
pub trait ResponseTranslator: Send + Sync {
    fn translate_response<'a>(
        &'a self,
        request: ResponseTransformRequest,
    ) -> PluginFuture<'a, PayloadResponse>;
}
pub trait ResponseNormalizer: Send + Sync {
    fn normalize_response<'a>(
        &'a self,
        request: ResponseTransformRequest,
    ) -> PluginFuture<'a, PayloadResponse>;
}

plugin_dto!(RequestInterceptRequest {
    #[serde(rename = "RequestID")] request_id: String,
    #[serde(rename = "TraceID")] trace_id: String,
    source_format: String, to_format: String, model: String, requested_model: String,
    stream: bool, headers: Headers, #[serde(with = "go_bytes")] body: Vec<u8>, metadata: JsonMetadata
});
plugin_dto!(RequestInterceptResponse {
    headers: Headers, #[serde(with = "go_bytes")] body: Vec<u8>, clear_headers: Vec<String>,
    terminate: bool, status_code: u16, response_headers: Headers,
    #[serde(with = "go_bytes")] response_body: Vec<u8>
});

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestCompletionOutcome(pub String);
impl RequestCompletionOutcome {
    pub const SUCCEEDED: &'static str = "succeeded";
    pub const FAILED: &'static str = "failed";
    pub const REJECTED: &'static str = "rejected";
    pub const CANCELED: &'static str = "canceled";
}
plugin_dto!(RequestCompletion {
    #[serde(rename = "RequestID")] request_id: String,
    #[serde(rename = "TraceID")] trace_id: String,
    source_format: String, model: String, requested_model: String, stream: bool,
    outcome: RequestCompletionOutcome, status_code: u16, error: String,
    started_at: Option<DateTime<Utc>>, completed_at: Option<DateTime<Utc>>, metadata: JsonMetadata
});

plugin_dto!(ResponseInterceptRequest {
    #[serde(rename = "RequestID")] request_id: String,
    source_format: String, model: String, requested_model: String, stream: bool,
    request_headers: Headers, response_headers: Headers,
    #[serde(with = "go_bytes")] original_request: Vec<u8>,
    #[serde(with = "go_bytes")] request_body: Vec<u8>,
    #[serde(with = "go_bytes")] body: Vec<u8>, status_code: u16, metadata: JsonMetadata
});
plugin_dto!(ResponseInterceptResponse {
    headers: Headers, #[serde(with = "go_bytes")] body: Vec<u8>, clear_headers: Vec<String>
});
plugin_dto!(StreamChunkInterceptRequest {
    #[serde(rename = "RequestID")] request_id: String,
    source_format: String, model: String, requested_model: String,
    request_headers: Headers, response_headers: Headers,
    #[serde(with = "go_bytes")] original_request: Vec<u8>,
    #[serde(with = "go_bytes")] request_body: Vec<u8>,
    #[serde(with = "go_bytes")] body: Vec<u8>,
    #[serde(with = "go_bytes_vec")] history_chunks: Vec<Vec<u8>>, chunk_index: i64, metadata: JsonMetadata
});
plugin_dto!(StreamChunkInterceptResponse {
    headers: Headers, #[serde(with = "go_bytes")] body: Vec<u8>, clear_headers: Vec<String>, drop_chunk: bool
});

pub const STREAM_CHUNK_HEADER_INIT_INDEX: i64 = -1;

pub trait RequestInterceptor: Send + Sync {
    fn intercept_request_before_auth<'a>(
        &'a self,
        request: RequestInterceptRequest,
    ) -> PluginFuture<'a, RequestInterceptResponse>;
    fn intercept_request_after_auth<'a>(
        &'a self,
        request: RequestInterceptRequest,
    ) -> PluginFuture<'a, RequestInterceptResponse>;
}
pub trait RequestLifecyclePlugin: Send + Sync {
    fn handle_request_complete<'a>(&'a self, completion: RequestCompletion)
        -> PluginFuture<'a, ()>;
}
pub trait ResponseInterceptor: Send + Sync {
    fn intercept_response<'a>(
        &'a self,
        request: ResponseInterceptRequest,
    ) -> PluginFuture<'a, ResponseInterceptResponse>;
}
pub trait StreamChunkInterceptor: Send + Sync {
    fn intercept_stream_chunk<'a>(
        &'a self,
        request: StreamChunkInterceptRequest,
    ) -> PluginFuture<'a, StreamChunkInterceptResponse>;
}

plugin_dto!(ThinkingConfig {
    mode: String,
    budget: i32,
    level: String
});
plugin_dto!(ThinkingApplyRequest {
    provider: String, model: ModelInfo, config: ThinkingConfig,
    #[serde(with = "go_bytes")] body: Vec<u8>
});
pub trait ThinkingApplier: Send + Sync {
    fn identifier(&self) -> &str;
    fn apply_thinking<'a>(
        &'a self,
        request: ThinkingApplyRequest,
    ) -> PluginFuture<'a, PayloadResponse>;
}

plugin_dto!(CommandLineRegistrationRequest { plugin: Metadata });
plugin_dto!(CommandLineRegistrationResponse { flags: Vec<CommandLineFlag> });
plugin_dto!(CommandLineFlag {
    name: String,
    usage: String,
    #[serde(rename = "Type")]
    kind: String,
    default_value: String
});
plugin_dto!(CommandLineFlagValue {
    name: String,
    #[serde(rename = "Type")]
    kind: String,
    value: String,
    set: bool
});
plugin_dto!(CommandLineExecutionRequest {
    plugin: Metadata, program: String, args: Vec<String>, config_path: String, host: HostConfigSummary,
    flags: BTreeMap<String, CommandLineFlagValue>, triggered_flags: BTreeMap<String, CommandLineFlagValue>
});
plugin_dto!(CommandLineExecutionResponse {
    #[serde(with = "go_bytes")] stdout: Vec<u8>, #[serde(with = "go_bytes")] stderr: Vec<u8>,
    auths: Vec<AuthData>, exit_code: i32
});
pub trait CommandLinePlugin: Send + Sync {
    fn register_command_line<'a>(
        &'a self,
        request: CommandLineRegistrationRequest,
    ) -> PluginFuture<'a, CommandLineRegistrationResponse>;
    fn execute_command_line<'a>(
        &'a self,
        request: CommandLineExecutionRequest,
    ) -> PluginFuture<'a, CommandLineExecutionResponse>;
}

plugin_dto!(ManagementRegistrationRequest {
    plugin: Metadata,
    base_path: String,
    resource_base_path: String
});

#[derive(Clone)]
pub struct ManagementRoute {
    pub method: String,
    pub path: String,
    pub menu: String,
    pub description: String,
    pub handler: Arc<dyn ManagementHandler>,
}
impl fmt::Debug for ManagementRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagementRoute")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("menu", &self.menu)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}
#[derive(Clone)]
pub struct ResourceRoute {
    pub path: String,
    pub menu: String,
    pub description: String,
    pub handler: Arc<dyn ManagementHandler>,
}
impl fmt::Debug for ResourceRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceRoute")
            .field("path", &self.path)
            .field("menu", &self.menu)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, Default)]
pub struct ManagementRegistrationResponse {
    pub routes: Vec<ManagementRoute>,
    pub resources: Vec<ResourceRoute>,
}
plugin_dto!(ManagementRequest {
    method: String, path: String, headers: Headers, query: QueryValues,
    #[serde(with = "go_bytes")] body: Vec<u8>
});
plugin_dto!(ManagementResponse { status_code: u16, headers: Headers, #[serde(with = "go_bytes")] body: Vec<u8> });
pub trait ManagementApi: Send + Sync {
    fn register_management<'a>(
        &'a self,
        request: ManagementRegistrationRequest,
    ) -> PluginFuture<'a, ManagementRegistrationResponse>;
}
pub trait ManagementHandler: Send + Sync {
    fn handle_management<'a>(
        &'a self,
        request: ManagementRequest,
    ) -> PluginFuture<'a, ManagementResponse>;
}

plugin_dto!(UsageFailure {
    status_code: u16,
    body: String
});
plugin_dto!(UsageDetail {
    input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    cached_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
    total_tokens: i64
});
plugin_dto!(UsageRecord {
    provider: String, executor_type: String, model: String, alias: String,
    #[serde(rename = "APIKey")] api_key: String,
    #[serde(rename = "AuthID")] auth_id: String,
    auth_index: String, auth_type: String, source: String, reasoning_effort: String,
    service_tier: String, generate: bool, requested_at: Option<DateTime<Utc>>,
    #[serde(with = "duration_nanos")] latency: std::time::Duration,
    #[serde(rename = "TTFT", with = "duration_nanos")] ttft: std::time::Duration,
    failed: bool, failure: UsageFailure, detail: UsageDetail, response_headers: Headers
});
pub trait UsagePlugin: Send + Sync {
    fn handle_usage<'a>(&'a self, record: UsageRecord) -> PluginFuture<'a, ()>;
}

mod go_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(value))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(serde::de::Error::custom)
    }
}

mod go_bytes_vec {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &[Vec<u8>], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value
            .iter()
            .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes))
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<String>::deserialize(deserializer)?
            .into_iter()
            .map(|encoded| {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(serde::de::Error::custom)
            })
            .collect()
    }
}

mod duration_nanos {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let nanos = value.as_nanos().min(i64::MAX as u128) as i64;
        serializer.serialize_i64(nanos)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let nanos = i64::deserialize(deserializer)?;
        if nanos < 0 {
            return Err(serde::de::Error::custom("negative duration is unsupported"));
        }
        Ok(Duration::from_nanos(nanos as u64))
    }
}
