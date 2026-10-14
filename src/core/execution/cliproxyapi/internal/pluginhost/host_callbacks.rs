// ref: internal/pluginhost/host_callbacks.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: all upstream callbacks use identity-bound typed process authorities
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{value::to_raw_value, value::RawValue, Value};

use crate::sdk::pluginabi::{
    Envelope, METHOD_HOST_HTTP_DO, METHOD_HOST_HTTP_DO_STREAM, METHOD_HOST_HTTP_STREAM_CLOSE,
    METHOD_HOST_HTTP_STREAM_READ, METHOD_HOST_LOG, METHOD_HOST_STREAM_CLOSE,
    METHOD_HOST_STREAM_EMIT,
};
use crate::sdk::pluginapi::{Headers, HttpRequest, PluginFuture};

use super::abi::PluginClientError;
use super::callback_contexts::{CallbackAuthority, CallbackContextRegistry};
use super::http_bridge::HostHttpBridge;
use super::http_stream_bridge::HttpStreamBridge;
use super::rpc_schema::RpcEmptyResponse;
use super::stream_bridge::StreamBridge;

pub type HostCallbackFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Envelope, PluginClientError>> + Send + 'a>>;

pub trait HostCallbackHandler: Send + Sync {
    fn call<'a>(
        &'a self,
        authority: &'a CallbackAuthority,
        payload: &'a RawValue,
    ) -> HostCallbackFuture<'a>;
}

pub struct HostCallbackRouter {
    contexts: CallbackContextRegistry,
    handlers: BTreeMap<String, Arc<dyn HostCallbackHandler>>,
}

impl std::fmt::Debug for HostCallbackRouter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostCallbackRouter")
            .field("methods", &self.handlers.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl HostCallbackRouter {
    pub fn new(contexts: CallbackContextRegistry) -> Self {
        Self {
            contexts,
            handlers: BTreeMap::new(),
        }
    }

    pub fn register(
        &mut self,
        method: &str,
        handler: Arc<dyn HostCallbackHandler>,
    ) -> Result<(), HostCallbackRouteError> {
        if !valid_host_method(method) {
            return Err(HostCallbackRouteError::InvalidMethod);
        }
        if self.handlers.insert(method.to_owned(), handler).is_some() {
            return Err(HostCallbackRouteError::DuplicateMethod);
        }
        Ok(())
    }

    pub async fn dispatch(
        &self,
        caller_plugin_id: &str,
        callback_id: &str,
        method: &str,
        payload: &RawValue,
        now_unix_ms: u64,
    ) -> Result<Envelope, HostCallbackRouteError> {
        let authority = self
            .contexts
            .resolve(callback_id)
            .ok_or(HostCallbackRouteError::UnknownContext)?;
        if authority.is_cancelled() {
            return Err(HostCallbackRouteError::Cancelled);
        }
        if authority
            .deadline_unix_ms()
            .is_some_and(|deadline| deadline <= now_unix_ms)
        {
            return Err(HostCallbackRouteError::DeadlineExceeded);
        }
        let caller = caller_plugin_id.trim();
        if caller.is_empty()
            || (!authority.plugin_id().is_empty() && authority.plugin_id() != caller)
        {
            return Err(HostCallbackRouteError::WrongPlugin);
        }
        // Quota leases carry only account HTTP authority, never auth-store,
        // model-execution or other host mutation authority.
        if authority.http_is_scoped()
            && !method.starts_with("host.http.")
            && method != METHOD_HOST_LOG
        {
            return Err(HostCallbackRouteError::UnknownMethod);
        }
        let handler = self
            .handlers
            .get(method)
            .ok_or(HostCallbackRouteError::UnknownMethod)?;
        handler
            .call(&authority, payload)
            .await
            .map_err(HostCallbackRouteError::Handler)
    }

    pub fn contexts(&self) -> &CallbackContextRegistry {
        &self.contexts
    }
}

fn valid_host_method(method: &str) -> bool {
    method.starts_with("host.")
        && method.len() <= 128
        && method
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._".contains(&byte))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostCallbackRouteError {
    InvalidMethod,
    DuplicateMethod,
    UnknownMethod,
    UnknownContext,
    WrongPlugin,
    Cancelled,
    DeadlineExceeded,
    Handler(PluginClientError),
}

impl std::fmt::Display for HostCallbackRouteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMethod => formatter.write_str("host callback method is invalid"),
            Self::DuplicateMethod => formatter.write_str("host callback method is duplicated"),
            Self::UnknownMethod => formatter.write_str("host callback method is unknown"),
            Self::UnknownContext => formatter.write_str("host callback context is unknown"),
            Self::WrongPlugin => formatter.write_str("host callback belongs to another plugin"),
            Self::Cancelled => formatter.write_str("host callback context is cancelled"),
            Self::DeadlineExceeded => formatter.write_str("host callback deadline exceeded"),
            Self::Handler(error) => write!(formatter, "host callback failed: {error}"),
        }
    }
}

impl std::error::Error for HostCallbackRouteError {}

pub trait HostLogAuthority: Send + Sync {
    fn log<'a>(
        &'a self,
        caller_plugin_id: &'a str,
        level: &'a str,
        message: &'a str,
        fields: BTreeMap<String, Value>,
    ) -> PluginFuture<'a, ()>;
}

#[derive(Clone)]
struct StandardCallbackHandler {
    contexts: CallbackContextRegistry,
    http: HostHttpBridge,
    http_streams: Arc<HttpStreamBridge>,
    streams: StreamBridge,
    log: Arc<dyn HostLogAuthority>,
    operation: StandardOperation,
}

#[derive(Clone, Copy)]
enum StandardOperation {
    HttpDo,
    HttpDoStream,
    HttpStreamRead,
    HttpStreamClose,
    StreamEmit,
    StreamClose,
    Log,
}

impl StandardCallbackHandler {
    fn http_for(&self, callback: &CallbackAuthority) -> Result<HostHttpBridge, PluginClientError> {
        if callback.http_is_scoped() {
            callback
                .http_client()
                .map(HostHttpBridge::new)
                .ok_or(PluginClientError::UnsupportedCapability)
        } else {
            Ok(self.http.clone())
        }
    }
}
impl HostCallbackHandler for StandardCallbackHandler {
    fn call<'a>(
        &'a self,
        callback: &'a CallbackAuthority,
        payload: &'a RawValue,
    ) -> HostCallbackFuture<'a> {
        Box::pin(async move {
            let http_owner = if callback.http_is_scoped() {
                format!("{}:{}", callback.plugin_id(), callback.context_id())
            } else {
                callback.plugin_id().to_owned()
            };
            match self.operation {
                StandardOperation::HttpDo => {
                    let request: WireHttpRequest = decode(payload)?;
                    let http = self.http_for(callback)?;
                    let response = tokio::select! {
                        biased;
                        _=callback.cancelled()=>return Err(PluginClientError::Cancelled),
                        result=http.execute(request.into_request())=>result.map_err(plugin_error)?,
                    };
                    envelope(&WireHttpResponse {
                        status_code: response.status_code,
                        headers: response.headers,
                        body: response.body,
                    })
                }
                StandardOperation::HttpDoStream => {
                    let request: WireHttpRequest = decode(payload)?;
                    let http = self.http_for(callback)?;
                    let response = tokio::select! {
                        biased;
                        _=callback.cancelled()=>return Err(PluginClientError::Cancelled),
                        result=http.execute_stream(request.into_request())=>result.map_err(plugin_error)?,
                    };
                    let handle = self.http_streams.open(&http_owner, response);
                    if callback.http_is_scoped() {
                        let streams = self.http_streams.clone();
                        let owner = http_owner.clone();
                        let stream_id = handle.stream_id.clone();
                        if !self.contexts.add_cleanup(
                            callback.context_id(),
                            Box::new(move || {
                                let _ = streams.close(&owner, &stream_id);
                            }),
                        ) {
                            return Err(PluginClientError::Cancelled);
                        }
                    }
                    envelope(&WireHttpStreamHandle {
                        status_code: handle.status_code,
                        headers: handle.headers,
                        stream_id: handle.stream_id,
                    })
                }
                StandardOperation::HttpStreamRead => {
                    let request: WireStreamId = decode(payload)?;
                    let stream_id = required(&request.stream_id)?;
                    let chunk = tokio::select! {
                        biased;
                        _=callback.cancelled()=>return Err(PluginClientError::Cancelled),
                        result=self.http_streams.read(&http_owner,stream_id)=>result.map_err(bridge_error)?,
                    };
                    envelope(&WireStreamRead {
                        payload: chunk.payload,
                        error: chunk.error.unwrap_or_default(),
                        done: chunk.done,
                    })
                }
                StandardOperation::HttpStreamClose => {
                    let request: WireStreamId = decode(payload)?;
                    let stream_id = required(&request.stream_id)?;
                    self.http_streams
                        .close(&http_owner, stream_id)
                        .map_err(bridge_error)?;
                    envelope(&RpcEmptyResponse {})
                }
                StandardOperation::StreamEmit => {
                    let request: WireStreamEmit = decode(payload)?;
                    let stream_id = required(&request.stream_id)?;
                    self.streams
                        .emit_chunk(
                            callback.plugin_id(),
                            stream_id,
                            request.payload,
                            nonempty(request.error),
                        )
                        .await
                        .map_err(bridge_error)?;
                    envelope(&RpcEmptyResponse {})
                }
                StandardOperation::StreamClose => {
                    let request: WireStreamClose = decode(payload)?;
                    let stream_id = required(&request.stream_id)?;
                    self.streams
                        .close(callback.plugin_id(), stream_id, nonempty(request.error))
                        .await
                        .map_err(bridge_error)?;
                    envelope(&RpcEmptyResponse {})
                }
                StandardOperation::Log => {
                    let request: WireLogRequest = decode(payload)?;
                    let level = normalize_log_level(&request.level)?;
                    let message = request.message.trim();
                    if message.len() > 16 * 1024
                        || request.fields.len() > 64
                        || request.fields.keys().any(|key| {
                            key.trim().is_empty()
                                || key.len() > 128
                                || key.chars().any(char::is_control)
                        })
                    {
                        return Err(PluginClientError::InvalidRequest);
                    }
                    self.log
                        .log(
                            callback.plugin_id(),
                            level,
                            if message.is_empty() {
                                "plugin log"
                            } else {
                                message
                            },
                            request.fields,
                        )
                        .await
                        .map_err(plugin_error)?;
                    envelope(&RpcEmptyResponse {})
                }
            }
        })
    }
}

pub fn install_standard_host_callbacks(
    router: &mut HostCallbackRouter,
    http: HostHttpBridge,
    http_streams: Arc<HttpStreamBridge>,
    streams: StreamBridge,
    log: Arc<dyn HostLogAuthority>,
) -> Result<(), HostCallbackRouteError> {
    for (method, operation) in [
        (METHOD_HOST_HTTP_DO, StandardOperation::HttpDo),
        (METHOD_HOST_HTTP_DO_STREAM, StandardOperation::HttpDoStream),
        (
            METHOD_HOST_HTTP_STREAM_READ,
            StandardOperation::HttpStreamRead,
        ),
        (
            METHOD_HOST_HTTP_STREAM_CLOSE,
            StandardOperation::HttpStreamClose,
        ),
        (METHOD_HOST_STREAM_EMIT, StandardOperation::StreamEmit),
        (METHOD_HOST_STREAM_CLOSE, StandardOperation::StreamClose),
        (METHOD_HOST_LOG, StandardOperation::Log),
    ] {
        router.register(
            method,
            Arc::new(StandardCallbackHandler {
                contexts: router.contexts.clone(),
                http: http.clone(),
                http_streams: http_streams.clone(),
                streams: streams.clone(),
                log: log.clone(),
                operation,
            }),
        )?;
    }
    Ok(())
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct WireHttpRequest {
    method: String,
    url: String,
    headers: Headers,
    #[serde(with = "wire_bytes")]
    body: Vec<u8>,
    request: Option<WireNestedHttpRequest>,
}

impl WireHttpRequest {
    fn into_request(self) -> HttpRequest {
        match self.request {
            Some(request) => request.into_request(),
            None => HttpRequest {
                method: self.method,
                url: self.url,
                headers: self.headers,
                body: self.body,
            },
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct WireNestedHttpRequest {
    method: String,
    url: String,
    headers: Headers,
    #[serde(with = "wire_bytes")]
    body: Vec<u8>,
}

impl WireNestedHttpRequest {
    fn into_request(self) -> HttpRequest {
        HttpRequest {
            method: self.method,
            url: self.url,
            headers: self.headers,
            body: self.body,
        }
    }
}

#[derive(Serialize)]
struct WireHttpResponse {
    status_code: u16,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    headers: Headers,
    #[serde(with = "wire_bytes", skip_serializing_if = "Vec::is_empty")]
    body: Vec<u8>,
}

#[derive(Serialize)]
struct WireHttpStreamHandle {
    status_code: u16,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    headers: Headers,
    stream_id: String,
}

#[derive(Default, Deserialize)]
struct WireStreamId {
    stream_id: String,
}

#[derive(Default, Deserialize)]
struct WireStreamEmit {
    stream_id: String,
    #[serde(default, with = "wire_bytes")]
    payload: Vec<u8>,
    #[serde(default)]
    error: String,
}

#[derive(Default, Deserialize)]
struct WireStreamClose {
    stream_id: String,
    #[serde(default)]
    error: String,
}

#[derive(Serialize)]
struct WireStreamRead {
    #[serde(with = "wire_bytes", skip_serializing_if = "Vec::is_empty")]
    payload: Vec<u8>,
    #[serde(skip_serializing_if = "String::is_empty")]
    error: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    done: bool,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct WireLogRequest {
    level: String,
    message: String,
    fields: BTreeMap<String, Value>,
}

fn decode<T: serde::de::DeserializeOwned>(payload: &RawValue) -> Result<T, PluginClientError> {
    serde_json::from_str(payload.get()).map_err(|_| PluginClientError::InvalidRequest)
}

fn envelope(value: &impl Serialize) -> Result<Envelope, PluginClientError> {
    Ok(Envelope::success(Some(
        to_raw_value(value).map_err(|_| PluginClientError::InvalidResponse)?,
    )))
}

fn required(value: &str) -> Result<&str, PluginClientError> {
    let value = value.trim();
    if value.is_empty() {
        Err(PluginClientError::InvalidRequest)
    } else {
        Ok(value)
    }
}

fn nonempty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn normalize_log_level(level: &str) -> Result<&'static str, PluginClientError> {
    match level.trim().to_ascii_lowercase().as_str() {
        "trace" => Ok("trace"),
        "info" => Ok("info"),
        "warn" | "warning" => Ok("warn"),
        "error" => Ok("error"),
        "" | "debug" => Ok("debug"),
        _ => Err(PluginClientError::InvalidRequest),
    }
}

fn plugin_error(error: crate::sdk::pluginapi::PluginExecutionError) -> PluginClientError {
    PluginClientError::Transport(error.to_string())
}

fn bridge_error(error: impl std::fmt::Display) -> PluginClientError {
    PluginClientError::Transport(error.to_string())
}

mod wire_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(value))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        STANDARD.decode(encoded).map_err(serde::de::Error::custom)
    }
}
