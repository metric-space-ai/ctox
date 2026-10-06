// ref: internal/api/server_management.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::io;
use std::net::IpAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::internal::api::handlers::management::{
    api_key_usage_payload, management_support_plugin_header, parse_usage_queue_count,
    static_model_definitions_payload, usage_queue_payload, ManagementApiKeyUsageError,
    ManagementApiKeyUsageSource, ManagementAuthenticator, ManagementQuotaResetError,
    ManagementQuotaResetSource, ManagementQuotaSwitchError, ManagementQuotaSwitchSource,
    ManagementUsageQueue, ManagementUsageQueueError, StaticModelDefinitionsError,
};
use crate::internal::config::CliproxyRuntimeConfig;

use super::server::read_request;

const MANAGEMENT_PREFIX: &str = "/v0/management";
const MODEL_DEFINITIONS_PREFIX: &str = "/v0/management/model-definitions/";
const MODEL_DEFINITIONS_QUERY_PATH: &str = "/v0/management/model-definitions";
const RUNTIME_STATUS_PATH: &str = "/v0/management/runtime-status";
const RUNTIME_CONFIG_PATH: &str = "/v0/management/runtime-config";
const USAGE_QUEUE_PATH: &str = "/v0/management/usage-queue";
const API_KEY_USAGE_PATH: &str = "/v0/management/api-key-usage";
const RESET_QUOTA_PATH: &str = "/v0/management/reset-quota";
const SWITCH_PROJECT_PATH: &str = "/v0/management/quota-exceeded/switch-project";
const SWITCH_PREVIEW_MODEL_PATH: &str = "/v0/management/quota-exceeded/switch-preview-model";
const MAX_RESET_QUOTA_BODY_BYTES: usize = 16 * 1024;
const MAX_RUNTIME_CONFIG_BODY_BYTES: usize = 256 * 1024;
pub const MANAGEMENT_RUNTIME_CONFIG_SCHEMA: &str = "ctox.cliproxyapi.runtime-config.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagementRuntimePhase {
    Stopped,
    WaitingForSubscription,
    WaitingForSecret,
    Starting,
    Ready,
    Faulted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagementRuntimeEndpoint {
    pub phase: ManagementRuntimePhase,
    pub listen_addr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagementRuntimeStatus {
    pub schema: String,
    pub main_responses_gateway: ManagementRuntimeEndpoint,
    pub codex_subscription_gateway: ManagementRuntimeEndpoint,
    pub management_gateway: ManagementRuntimeEndpoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_model: Option<String>,
}

pub trait ManagementRuntimeStatusSource: Send + Sync {
    fn snapshot(&self) -> ManagementRuntimeStatus;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagementProviderConfigSummary {
    pub provider: String,
    pub account_count: usize,
    pub enabled_account_count: usize,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagementRuntimeConfigSummary {
    pub schema: String,
    pub revision: u64,
    pub default_provider: String,
    pub providers: Vec<ManagementProviderConfigSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagementRuntimeConfigMutation {
    pub schema: String,
    pub expected_revision: u64,
    pub default_provider: String,
    pub runtime: CliproxyRuntimeConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagementRuntimeConfigError {
    Invalid,
    RevisionConflict,
    CredentialUnavailable,
    StoreUnavailable,
}

pub trait ManagementRuntimeConfigSource: Send + Sync {
    fn snapshot(
        &self,
    ) -> Result<Option<ManagementRuntimeConfigSummary>, ManagementRuntimeConfigError>;

    fn replace(
        &self,
        mutation: ManagementRuntimeConfigMutation,
    ) -> Result<ManagementRuntimeConfigSummary, ManagementRuntimeConfigError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagementHttpResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

impl ManagementHttpResponse {
    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    fn json(status: u16, body: Vec<u8>) -> Self {
        let mut headers = management_headers();
        headers.insert("Content-Type".to_owned(), "application/json".to_owned());
        headers.insert("Cache-Control".to_owned(), "no-store".to_owned());
        Self {
            status,
            headers,
            body,
        }
    }

    fn error(status: u16, message: &str) -> Self {
        Self::json(
            status,
            serde_json::to_vec(&serde_json::json!({"error": message}))
                .unwrap_or_else(|_| b"{}".to_vec()),
        )
    }
}

pub struct ManagementHandler {
    authenticator: Arc<ManagementAuthenticator>,
    runtime_status: Option<Arc<dyn ManagementRuntimeStatusSource>>,
    runtime_config: Option<Arc<dyn ManagementRuntimeConfigSource>>,
    usage_queue: Option<Arc<dyn ManagementUsageQueue>>,
    api_key_usage: Option<Arc<dyn ManagementApiKeyUsageSource>>,
    quota_reset: Option<Arc<dyn ManagementQuotaResetSource>>,
    quota_switches: Option<Arc<dyn ManagementQuotaSwitchSource>>,
}

impl ManagementHandler {
    pub fn new(authenticator: Arc<ManagementAuthenticator>) -> Self {
        Self {
            authenticator,
            runtime_status: None,
            runtime_config: None,
            usage_queue: None,
            api_key_usage: None,
            quota_reset: None,
            quota_switches: None,
        }
    }

    pub fn with_runtime_status(
        authenticator: Arc<ManagementAuthenticator>,
        runtime_status: Arc<dyn ManagementRuntimeStatusSource>,
    ) -> Self {
        Self {
            authenticator,
            runtime_status: Some(runtime_status),
            runtime_config: None,
            usage_queue: None,
            api_key_usage: None,
            quota_reset: None,
            quota_switches: None,
        }
    }

    pub fn with_runtime_sources(
        authenticator: Arc<ManagementAuthenticator>,
        runtime_status: Arc<dyn ManagementRuntimeStatusSource>,
        runtime_config: Arc<dyn ManagementRuntimeConfigSource>,
    ) -> Self {
        Self {
            authenticator,
            runtime_status: Some(runtime_status),
            runtime_config: Some(runtime_config),
            usage_queue: None,
            api_key_usage: None,
            quota_reset: None,
            quota_switches: None,
        }
    }

    pub fn with_usage_queue(
        authenticator: Arc<ManagementAuthenticator>,
        usage_queue: Arc<dyn ManagementUsageQueue>,
    ) -> Self {
        Self {
            authenticator,
            runtime_status: None,
            runtime_config: None,
            usage_queue: Some(usage_queue),
            api_key_usage: None,
            quota_reset: None,
            quota_switches: None,
        }
    }

    pub fn attach_usage_queue(mut self, usage_queue: Arc<dyn ManagementUsageQueue>) -> Self {
        self.usage_queue = Some(usage_queue);
        self
    }

    pub fn attach_api_key_usage_source(
        mut self,
        api_key_usage: Arc<dyn ManagementApiKeyUsageSource>,
    ) -> Self {
        self.api_key_usage = Some(api_key_usage);
        self
    }

    pub fn attach_quota_reset_source(
        mut self,
        quota_reset: Arc<dyn ManagementQuotaResetSource>,
    ) -> Self {
        self.quota_reset = Some(quota_reset);
        self
    }

    pub fn attach_quota_switch_source(
        mut self,
        quota_switches: Arc<dyn ManagementQuotaSwitchSource>,
    ) -> Self {
        self.quota_switches = Some(quota_switches);
        self
    }

    pub fn handle(
        &self,
        method: &str,
        target: &str,
        headers: &BTreeMap<String, Vec<String>>,
        body: &[u8],
        client_ip: IpAddr,
    ) -> ManagementHttpResponse {
        let path = target.split('?').next().unwrap_or(target);
        if path != MANAGEMENT_PREFIX && !path.starts_with(&format!("{MANAGEMENT_PREFIX}/")) {
            return ManagementHttpResponse::error(404, "route not found");
        }
        let provided = management_key(headers);
        if let Err(error) = self.authenticator.authenticate(
            &client_ip.to_string(),
            client_ip.is_loopback(),
            provided,
        ) {
            return ManagementHttpResponse::error(error.status(), &error.message());
        }
        if path == RUNTIME_CONFIG_PATH {
            let Some(source) = self.runtime_config.as_ref() else {
                return ManagementHttpResponse::error(404, "route not found");
            };
            return match method {
                "GET" => match source.snapshot() {
                    Ok(Some(summary)) => match serde_json::to_vec(&summary) {
                        Ok(payload) => ManagementHttpResponse::json(200, payload),
                        Err(_) => ManagementHttpResponse::error(500, "runtime config unavailable"),
                    },
                    Ok(None) => ManagementHttpResponse::error(404, "runtime config not found"),
                    Err(error) => runtime_config_error_response(error),
                },
                "PUT" => {
                    if body.len() > MAX_RUNTIME_CONFIG_BODY_BYTES {
                        return ManagementHttpResponse::error(413, "runtime config is too large");
                    }
                    if !header_value(headers, "content-type")
                        .and_then(|value| value.split(';').next())
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
                    {
                        return ManagementHttpResponse::error(400, "application/json is required");
                    }
                    let mutation: ManagementRuntimeConfigMutation =
                        match serde_json::from_slice(body) {
                            Ok(mutation) => mutation,
                            Err(_) => {
                                return ManagementHttpResponse::error(
                                    400,
                                    "runtime config is invalid",
                                )
                            }
                        };
                    if mutation.schema != MANAGEMENT_RUNTIME_CONFIG_SCHEMA {
                        return ManagementHttpResponse::error(400, "runtime config is invalid");
                    }
                    match source.replace(mutation) {
                        Ok(summary) => match serde_json::to_vec(&summary) {
                            Ok(payload) => ManagementHttpResponse::json(200, payload),
                            Err(_) => {
                                ManagementHttpResponse::error(500, "runtime config unavailable")
                            }
                        },
                        Err(error) => runtime_config_error_response(error),
                    }
                }
                _ => ManagementHttpResponse::error(405, "method not allowed"),
            };
        }
        if path == RESET_QUOTA_PATH {
            if method != "POST" {
                return ManagementHttpResponse::error(405, "method not allowed");
            }
            let Some(source) = self.quota_reset.as_ref() else {
                return ManagementHttpResponse::error(404, "route not found");
            };
            if body.len() > MAX_RESET_QUOTA_BODY_BYTES {
                return ManagementHttpResponse::error(413, "request body is too large");
            }
            if !header_value(headers, "content-type")
                .and_then(|value| value.split(';').next())
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
            {
                return ManagementHttpResponse::error(400, "invalid request body");
            }
            #[derive(Deserialize)]
            struct ResetQuotaRequest {
                #[serde(default)]
                auth_index: String,
            }
            let request: ResetQuotaRequest = match serde_json::from_slice(body) {
                Ok(request) => request,
                Err(_) => return ManagementHttpResponse::error(400, "invalid request body"),
            };
            let auth_index = request.auth_index.trim();
            if auth_index.is_empty() {
                return ManagementHttpResponse::error(400, "auth_index is required");
            }
            return match source.reset_by_index(auth_index) {
                Ok(Some(result)) => ManagementHttpResponse::json(
                    200,
                    serde_json::to_vec(&serde_json::json!({
                        "status": "ok",
                        "auth_index": result.auth_index,
                        "models": result.models
                    }))
                    .unwrap_or_else(|_| b"{}".to_vec()),
                ),
                Ok(None) => ManagementHttpResponse::error(404, "auth not found"),
                Err(ManagementQuotaResetError::InvalidAccount) => {
                    ManagementHttpResponse::error(400, "invalid quota account")
                }
                Err(ManagementQuotaResetError::StoreUnavailable) => {
                    ManagementHttpResponse::error(500, "failed to reset quota")
                }
            };
        }
        if path == SWITCH_PROJECT_PATH || path == SWITCH_PREVIEW_MODEL_PATH {
            let Some(source) = self.quota_switches.as_ref() else {
                return ManagementHttpResponse::error(404, "route not found");
            };
            if method == "GET" {
                return match source.snapshot() {
                    Ok(switches) if path == SWITCH_PROJECT_PATH => ManagementHttpResponse::json(
                        200,
                        serde_json::to_vec(&serde_json::json!({
                            "switch-project": switches.switch_project
                        }))
                        .unwrap_or_else(|_| b"{}".to_vec()),
                    ),
                    Ok(switches) => ManagementHttpResponse::json(
                        200,
                        serde_json::to_vec(&serde_json::json!({
                            "switch-preview-model": switches.switch_preview_model
                        }))
                        .unwrap_or_else(|_| b"{}".to_vec()),
                    ),
                    Err(ManagementQuotaSwitchError::StoreUnavailable) => {
                        ManagementHttpResponse::error(500, "quota policy unavailable")
                    }
                };
            }
            if method != "PUT" && method != "PATCH" {
                return ManagementHttpResponse::error(405, "method not allowed");
            }
            if body.len() > MAX_RESET_QUOTA_BODY_BYTES {
                return ManagementHttpResponse::error(413, "request body is too large");
            }
            if !header_value(headers, "content-type")
                .and_then(|value| value.split(';').next())
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
            {
                return ManagementHttpResponse::error(400, "invalid body");
            }
            #[derive(Deserialize)]
            struct BoolMutation {
                value: Option<bool>,
            }
            let mutation: BoolMutation = match serde_json::from_slice(body) {
                Ok(mutation) => mutation,
                Err(_) => return ManagementHttpResponse::error(400, "invalid body"),
            };
            let Some(value) = mutation.value else {
                return ManagementHttpResponse::error(400, "invalid body");
            };
            let result = if path == SWITCH_PROJECT_PATH {
                source.set_switch_project(value)
            } else {
                source.set_switch_preview_model(value)
            };
            return match result {
                Ok(()) => ManagementHttpResponse::json(
                    200,
                    serde_json::to_vec(&serde_json::json!({"status": "ok"}))
                        .unwrap_or_else(|_| b"{}".to_vec()),
                ),
                Err(ManagementQuotaSwitchError::StoreUnavailable) => {
                    ManagementHttpResponse::error(500, "quota policy unavailable")
                }
            };
        }
        if path == USAGE_QUEUE_PATH {
            if method != "GET" {
                return ManagementHttpResponse::error(405, "method not allowed");
            }
            let Some(queue) = self.usage_queue.as_ref() else {
                return ManagementHttpResponse::error(404, "route not found");
            };
            let count = match parse_usage_queue_count(query_parameter(target, "count").as_deref()) {
                Ok(count) => count,
                Err(message) => return ManagementHttpResponse::error(400, message),
            };
            return match queue.pop_oldest(count) {
                Ok(items) => ManagementHttpResponse::json(200, usage_queue_payload(&items)),
                Err(ManagementUsageQueueError::StoreUnavailable) => {
                    ManagementHttpResponse::error(500, "usage queue unavailable")
                }
            };
        }
        if path == API_KEY_USAGE_PATH {
            if method != "GET" {
                return ManagementHttpResponse::error(405, "method not allowed");
            }
            let Some(source) = self.api_key_usage.as_ref() else {
                return ManagementHttpResponse::error(404, "route not found");
            };
            return match source.snapshot().and_then(api_key_usage_payload) {
                Ok(payload) => ManagementHttpResponse::json(200, payload),
                Err(ManagementApiKeyUsageError::SourceUnavailable) => {
                    ManagementHttpResponse::error(500, "API key usage unavailable")
                }
            };
        }
        if method != "GET" {
            return ManagementHttpResponse::error(405, "method not allowed");
        }
        if path == RUNTIME_STATUS_PATH {
            let Some(source) = self.runtime_status.as_ref() else {
                return ManagementHttpResponse::error(404, "route not found");
            };
            return match serde_json::to_vec(&source.snapshot()) {
                Ok(payload) => ManagementHttpResponse::json(200, payload),
                Err(_) => ManagementHttpResponse::error(500, "runtime status unavailable"),
            };
        }
        let channel = if let Some(channel) = path.strip_prefix(MODEL_DEFINITIONS_PREFIX) {
            channel.to_owned()
        } else if path == MODEL_DEFINITIONS_QUERY_PATH {
            query_parameter(target, "channel").unwrap_or_default()
        } else {
            return ManagementHttpResponse::error(404, "route not found");
        };
        match static_model_definitions_payload(&channel) {
            Ok(payload) => ManagementHttpResponse::json(200, payload),
            Err(StaticModelDefinitionsError::MissingChannel) => {
                ManagementHttpResponse::error(400, "channel is required")
            }
            Err(StaticModelDefinitionsError::UnknownChannel(channel)) => {
                ManagementHttpResponse::json(
                    400,
                    serde_json::to_vec(&serde_json::json!({
                        "error": "unknown channel",
                        "channel": channel
                    }))
                    .unwrap_or_else(|_| b"{}".to_vec()),
                )
            }
            Err(StaticModelDefinitionsError::InvalidCatalog) => {
                ManagementHttpResponse::error(500, "static model catalog is invalid")
            }
        }
    }
}

impl std::fmt::Debug for ManagementHandler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagementHandler")
            .field("authenticator", &self.authenticator)
            .field("runtime_status", &self.runtime_status.is_some())
            .field("runtime_config", &self.runtime_config.is_some())
            .field("usage_queue", &self.usage_queue.is_some())
            .field("api_key_usage", &self.api_key_usage.is_some())
            .field("quota_reset", &self.quota_reset.is_some())
            .field("quota_switches", &self.quota_switches.is_some())
            .finish()
    }
}

pub async fn serve_one_management_connection(
    listener: &TcpListener,
    handler: &ManagementHandler,
) -> io::Result<()> {
    let (mut stream, peer) = listener.accept().await?;
    serve_management_connection(&mut stream, handler, peer.ip()).await
}

pub async fn serve_management_connection<S>(
    stream: &mut S,
    handler: &ManagementHandler,
    client_ip: IpAddr,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let response = match read_request(stream).await {
        Ok(request) => handler.handle(
            &request.method,
            &request.target,
            &request.headers,
            &request.body,
            client_ip,
        ),
        Err(error) => ManagementHttpResponse::error(error.status, error.message),
    };
    write_management_response(stream, &response).await
}

fn runtime_config_error_response(error: ManagementRuntimeConfigError) -> ManagementHttpResponse {
    match error {
        ManagementRuntimeConfigError::Invalid => {
            ManagementHttpResponse::error(400, "runtime config is invalid")
        }
        ManagementRuntimeConfigError::RevisionConflict => {
            ManagementHttpResponse::error(409, "runtime config revision conflict")
        }
        ManagementRuntimeConfigError::CredentialUnavailable => {
            ManagementHttpResponse::error(400, "runtime credential is unavailable")
        }
        ManagementRuntimeConfigError::StoreUnavailable => {
            ManagementHttpResponse::error(500, "runtime config unavailable")
        }
    }
}

fn management_key(headers: &BTreeMap<String, Vec<String>>) -> Option<&str> {
    let authorization = header_value(headers, "authorization");
    if let Some(authorization) = authorization {
        if let Some((scheme, value)) = authorization.split_once(' ') {
            if scheme.eq_ignore_ascii_case("bearer") {
                return Some(value);
            }
        }
        return Some(authorization);
    }
    header_value(headers, "x-management-key")
}

fn header_value<'a>(headers: &'a BTreeMap<String, Vec<String>>, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
}

fn query_parameter(target: &str, name: &str) -> Option<String> {
    let query = target.split_once('?')?.1;
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

fn management_headers() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "X-CPA-VERSION".to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
        ),
        (
            "X-CPA-COMMIT".to_owned(),
            "ffdb9c9fbc78a6235d59c9ccbdc4243ba35ecdcd".to_owned(),
        ),
        ("X-CPA-BUILD-DATE".to_owned(), String::new()),
        (
            "X-CPA-SUPPORT-PLUGIN".to_owned(),
            management_support_plugin_header().to_owned(),
        ),
    ])
}

async fn write_management_response<S>(
    stream: &mut S,
    response: &ManagementHttpResponse,
) -> io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n",
        response.status,
        reason_phrase(response.status)
    );
    for (name, value) in &response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.body.len()
    ));
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&response.body).await?;
    stream.shutdown().await
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        _ => "Error",
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;
    use crate::internal::api::handlers::management::SystemManagementAuthClock;

    fn handler(allow_remote: bool) -> ManagementHandler {
        ManagementHandler::new(Arc::new(
            ManagementAuthenticator::new(
                "management-secret",
                allow_remote,
                Arc::new(SystemManagementAuthClock),
            )
            .unwrap(),
        ))
    }

    #[derive(Default)]
    struct StaticRuntimeStatusSource(AtomicUsize);

    impl ManagementRuntimeStatusSource for StaticRuntimeStatusSource {
        fn snapshot(&self) -> ManagementRuntimeStatus {
            self.0.fetch_add(1, Ordering::SeqCst);
            ManagementRuntimeStatus {
                schema: "ctox.cliproxyapi.runtime-status.v1".to_owned(),
                main_responses_gateway: ManagementRuntimeEndpoint {
                    phase: ManagementRuntimePhase::Ready,
                    listen_addr: "127.0.0.1:12434".to_owned(),
                },
                codex_subscription_gateway: ManagementRuntimeEndpoint {
                    phase: ManagementRuntimePhase::WaitingForSubscription,
                    listen_addr: "127.0.0.1:12435".to_owned(),
                },
                management_gateway: ManagementRuntimeEndpoint {
                    phase: ManagementRuntimePhase::Ready,
                    listen_addr: "127.0.0.1:12436".to_owned(),
                },
                active_provider: Some("openrouter".to_owned()),
                active_model: Some("openai/gpt-5.4".to_owned()),
            }
        }
    }

    #[derive(Default)]
    struct StaticRuntimeConfigSource {
        calls: AtomicUsize,
        revision: Mutex<u64>,
    }

    fn config_summary(revision: u64) -> ManagementRuntimeConfigSummary {
        ManagementRuntimeConfigSummary {
            schema: MANAGEMENT_RUNTIME_CONFIG_SCHEMA.to_owned(),
            revision,
            default_provider: "claude".to_owned(),
            providers: vec![ManagementProviderConfigSummary {
                provider: "claude".to_owned(),
                account_count: 1,
                enabled_account_count: 1,
                models: vec!["claude-sonnet-4-6".to_owned()],
            }],
        }
    }

    impl ManagementRuntimeConfigSource for StaticRuntimeConfigSource {
        fn snapshot(
            &self,
        ) -> Result<Option<ManagementRuntimeConfigSummary>, ManagementRuntimeConfigError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Some(config_summary(
                *self
                    .revision
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            )))
        }

        fn replace(
            &self,
            mutation: ManagementRuntimeConfigMutation,
        ) -> Result<ManagementRuntimeConfigSummary, ManagementRuntimeConfigError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut revision = self
                .revision
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if mutation.expected_revision != *revision {
                return Err(ManagementRuntimeConfigError::RevisionConflict);
            }
            *revision += 1;
            Ok(config_summary(*revision))
        }
    }

    #[test]
    fn handler_requires_key_and_keeps_unknown_channel_explicit() {
        let handler = handler(false);
        let no_headers = BTreeMap::new();
        let missing = handler.handle(
            "GET",
            "/v0/management/model-definitions/claude",
            &no_headers,
            &[],
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        );
        assert_eq!(missing.status(), 401);
        assert_eq!(
            missing.headers()["X-CPA-SUPPORT-PLUGIN"],
            management_support_plugin_header()
        );

        let headers = BTreeMap::from([(
            "Authorization".to_owned(),
            vec!["Bearer management-secret".to_owned()],
        )]);
        let unknown = handler.handle(
            "GET",
            "/v0/management/model-definitions/unknown",
            &headers,
            &[],
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        );
        assert_eq!(unknown.status(), 400);
        let value: serde_json::Value = serde_json::from_slice(unknown.body()).unwrap();
        assert_eq!(value["channel"], "unknown");
    }

    #[test]
    fn runtime_status_is_authenticated_typed_and_source_driven() {
        let source = Arc::new(StaticRuntimeStatusSource::default());
        let handler = ManagementHandler::with_runtime_status(
            Arc::new(
                ManagementAuthenticator::new(
                    "management-secret",
                    false,
                    Arc::new(SystemManagementAuthClock),
                )
                .unwrap(),
            ),
            source.clone(),
        );
        let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let unauthorized = handler.handle("GET", RUNTIME_STATUS_PATH, &BTreeMap::new(), &[], local);
        assert_eq!(unauthorized.status(), 401);
        assert_eq!(source.0.load(Ordering::SeqCst), 0);

        let headers = BTreeMap::from([(
            "X-Management-Key".to_owned(),
            vec!["management-secret".to_owned()],
        )]);
        let response = handler.handle("GET", RUNTIME_STATUS_PATH, &headers, &[], local);
        assert_eq!(response.status(), 200);
        assert_eq!(source.0.load(Ordering::SeqCst), 1);
        let value: serde_json::Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(value["schema"], "ctox.cliproxyapi.runtime-status.v1");
        assert_eq!(value["main_responses_gateway"]["phase"], "ready");
        assert_eq!(
            value["codex_subscription_gateway"]["phase"],
            "waiting_for_subscription"
        );
        assert_eq!(value["active_provider"], "openrouter");
        assert_eq!(value["active_model"], "openai/gpt-5.4");
        assert!(response.headers().contains_key("Cache-Control"));
    }

    #[test]
    fn runtime_config_mutation_is_auth_first_schema_strict_and_revisioned() {
        let status = Arc::new(StaticRuntimeStatusSource::default());
        let config = Arc::new(StaticRuntimeConfigSource::default());
        let handler = ManagementHandler::with_runtime_sources(
            Arc::new(
                ManagementAuthenticator::new(
                    "management-secret",
                    false,
                    Arc::new(SystemManagementAuthClock),
                )
                .unwrap(),
            ),
            status,
            config.clone(),
        );
        let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let body = serde_json::to_vec(&serde_json::json!({
            "schema": MANAGEMENT_RUNTIME_CONFIG_SCHEMA,
            "expected_revision": 0,
            "default_provider": "claude",
            "runtime": {
                "request_timeout_ms": 30000,
                "routing_strategy": "round-robin",
                "claude_accounts": [{
                    "id": "claude-primary",
                    "models": ["claude-sonnet-4-6"],
                    "access_token_secret": {"scope": "provider-subscriptions", "name": "claude-access"},
                    "refresh_token_secret": {"scope": "provider-subscriptions", "name": "claude-refresh"}
                }]
            }
        }))
        .unwrap();
        let unauthorized =
            handler.handle("PUT", RUNTIME_CONFIG_PATH, &BTreeMap::new(), &body, local);
        assert_eq!(unauthorized.status(), 401);
        assert_eq!(config.calls.load(Ordering::SeqCst), 0);

        let headers = BTreeMap::from([
            (
                "X-Management-Key".to_owned(),
                vec!["management-secret".to_owned()],
            ),
            (
                "Content-Type".to_owned(),
                vec!["application/json".to_owned()],
            ),
        ]);
        let written = handler.handle("PUT", RUNTIME_CONFIG_PATH, &headers, &body, local);
        assert_eq!(written.status(), 200);
        let value: serde_json::Value = serde_json::from_slice(written.body()).unwrap();
        assert_eq!(value["revision"], 1);
        assert_eq!(value["providers"][0]["provider"], "claude");
        assert!(!String::from_utf8_lossy(written.body()).contains("claude-access"));

        let conflict = handler.handle("PUT", RUNTIME_CONFIG_PATH, &headers, &body, local);
        assert_eq!(conflict.status(), 409);
        assert_eq!(config.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn real_loopback_serves_authenticated_pinned_claude_catalog() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            serve_one_management_connection(&listener, &handler(false))
                .await
                .unwrap();
        });
        let mut client = TcpStream::connect(address).await.unwrap();
        client
            .write_all(
                b"GET /v0/management/model-definitions/claude HTTP/1.1\r\nHost: localhost\r\nX-Management-Key: management-secret\r\n\r\n",
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();
        let split = response
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .unwrap();
        let head = std::str::from_utf8(&response[..split]).unwrap();
        assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(head.contains(&format!(
            "X-CPA-SUPPORT-PLUGIN: {}\r\n",
            management_support_plugin_header()
        )));
        let body: serde_json::Value = serde_json::from_slice(&response[split + 4..]).unwrap();
        assert_eq!(body["channel"], "claude");
        assert_eq!(body["models"].as_array().unwrap().len(), 18);
    }
}
