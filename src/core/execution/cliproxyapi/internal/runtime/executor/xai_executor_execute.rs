// ref: internal/runtime/executor/xai_executor_execute.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::sdk::cliproxy::auth::Auth;
use crate::sdk::cliproxy::executor::{Headers, Options, Request, Response};

use super::xai_executor::{
    xai_status_error, XaiHttpRequest, XaiHttpTransport, XaiStatusError, XaiStreamingTransport,
    XaiTransportFailure, XaiUpstreamTarget,
};
use super::xai_executor_media::{xai_image_endpoint_path, xai_video_endpoint_path};
use super::xai_executor_request::{
    apply_xai_chat_headers, apply_xai_headers, normalize_image_refs, xai_chat_base_url,
    xai_compact_base_url, xai_credentials,
};
use super::xai_executor_response::normalize_reasoning_event_data;
use super::xai_reasoning_replay::{
    apply_reasoning_replay, cache_reasoning_replay_from_completed,
    clear_reasoning_replay_after_compaction, XaiReasoningReplayScope, XaiReasoningReplayStore,
};

pub struct XaiExecutor {
    transport: Arc<dyn XaiHttpTransport>,
    stream_transport: Option<Arc<dyn XaiStreamingTransport>>,
    replay_store: Option<Arc<dyn XaiReasoningReplayStore>>,
    timeout: Duration,
    inject_x_search: bool,
    request_owner: Arc<super::xai_executor_prepare::XaiRequestOwner>,
}

impl XaiExecutor {
    pub fn new(
        transport: Arc<dyn XaiHttpTransport>,
        timeout: Duration,
    ) -> Result<Self, XaiExecutionError> {
        if timeout.is_zero() {
            return Err(XaiExecutionError::InvalidTimeout);
        }
        Ok(Self {
            transport,
            stream_transport: None,
            replay_store: None,
            timeout,
            inject_x_search: false,
            request_owner: Arc::new(super::xai_executor_prepare::XaiRequestOwner::default()),
        })
    }
    /// Bind the gateway's existing translator, thinking and client owners.
    #[must_use]
    pub fn with_request_owner(
        mut self,
        owner: Arc<super::xai_executor_prepare::XaiRequestOwner>,
    ) -> Self {
        self.request_owner = owner;
        self
    }
    pub(crate) fn prepare_request(
        &self,
        request: &Request,
        options: &Options,
        stream: bool,
    ) -> Result<super::xai_executor_request::XaiPreparedRequest, XaiExecutionError> {
        self.request_owner
            .prepare(request, options, stream, self.inject_search())
            .map_err(|error| XaiExecutionError::Request(error.to_string()))
    }
    pub fn with_stream_transport(mut self, transport: Arc<dyn XaiStreamingTransport>) -> Self {
        self.stream_transport = Some(transport);
        self
    }
    pub fn with_replay_store(mut self, store: Arc<dyn XaiReasoningReplayStore>) -> Self {
        self.replay_store = Some(store);
        self
    }
    pub fn inject_x_search(mut self, enabled: bool) -> Self {
        self.inject_x_search = enabled;
        self
    }
    #[must_use]
    pub fn identifier(&self) -> &str {
        "xai"
    }
    #[must_use]
    pub fn replay_store(&self) -> Option<&Arc<dyn XaiReasoningReplayStore>> {
        self.replay_store.as_ref()
    }

    pub async fn execute(
        &self,
        auth: Option<&Auth>,
        request: &Request,
        options: &Options,
    ) -> Result<Response, XaiExecutionError> {
        let credentials = xai_credentials(auth);
        let (base_url, path, media) = if let Some(path) = xai_image_endpoint_path(options) {
            (credentials.base_url.clone(), path.to_owned(), true)
        } else if let Some(path) = xai_video_endpoint_path(options) {
            (credentials.base_url.clone(), path, true)
        } else if options.alt == "responses/compact" {
            (
                xai_compact_base_url(auth),
                "/responses/compact".into(),
                false,
            )
        } else {
            (xai_chat_base_url(auth), "/responses".into(), false)
        };
        let target =
            XaiUpstreamTarget::new(&base_url).map_err(|_| XaiExecutionError::InvalidTarget)?;
        let (body, mut headers) = if media {
            (normalize_image_refs(&request.payload), Headers::new())
        } else {
            let mut prepared = self.prepare_request(request, options, path == "/responses")?;
            if let (Some(store), Some(scope)) = (
                self.replay_store.as_deref(),
                self.reasoning_replay_scope(options, &credentials.token),
            ) {
                prepared.body = apply_reasoning_replay(store, Some(&scope), &prepared.body);
            }
            if path == "/responses/compact" {
                sanitize_compact_body(&mut prepared.body);
            }
            (prepared.body, Headers::new())
        };
        if media || path == "/responses/compact" {
            apply_xai_headers(&mut headers, auth, &credentials.token, false, "");
        } else {
            apply_xai_chat_headers(
                &mut headers,
                auth,
                &credentials.token,
                true,
                options
                    .metadata
                    .execution_session_id
                    .as_deref()
                    .unwrap_or_default(),
            );
        }
        let upstream = XaiHttpRequest {
            url: target.url(&path),
            headers,
            body: body.into(),
        };
        let response = self
            .transport
            .execute(&upstream, self.timeout)
            .await
            .map_err(XaiExecutionError::Transport)?;
        if !(200..300).contains(&response.status) {
            return Err(XaiExecutionError::Status(xai_status_error(
                response.status,
                &response.body,
            )));
        }
        let replay_scope = self.reasoning_replay_scope(options, &credentials.token);
        let payload = if media || path == "/responses/compact" {
            if path == "/responses/compact" {
                if let Some(store) = self.replay_store.as_deref() {
                    clear_reasoning_replay_after_compaction(store, replay_scope.as_ref());
                }
            }
            response.body.to_vec()
        } else {
            let completed = aggregate_responses_sse(&response.body)?;
            if let Some(store) = self.replay_store.as_deref() {
                cache_reasoning_replay_from_completed(store, replay_scope.as_ref(), &completed);
            }
            completed
        };
        Ok(Response {
            payload,
            headers: response.headers,
            ..Response::default()
        })
    }

    pub(crate) fn stream_transport(
        &self,
    ) -> Result<&Arc<dyn XaiStreamingTransport>, XaiExecutionError> {
        self.stream_transport
            .as_ref()
            .ok_or(XaiExecutionError::StreamingUnavailable)
    }
    pub(crate) fn timeout(&self) -> Duration {
        self.timeout
    }
    pub(crate) fn inject_search(&self) -> bool {
        self.inject_x_search
    }

    pub(crate) fn reasoning_replay_scope(
        &self,
        options: &Options,
        credential: &str,
    ) -> Option<XaiReasoningReplayScope> {
        self.replay_store.as_ref()?;
        XaiReasoningReplayScope::new(
            "xai",
            options.metadata.execution_session_id.as_deref()?,
            (!credential.trim().is_empty()).then_some(credential),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XaiExecutionError {
    InvalidTimeout,
    InvalidTarget,
    Request(String),
    Transport(XaiTransportFailure),
    Status(XaiStatusError),
    MissingCompleted,
    StreamingUnavailable,
    StreamingCompactUnsupported,
}
impl fmt::Display for XaiExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "xAI execution failed: {self:?}")
    }
}
impl std::error::Error for XaiExecutionError {}

fn aggregate_responses_sse(body: &[u8]) -> Result<Vec<u8>, XaiExecutionError> {
    let mut completed = None;
    for line in body.split(|byte| *byte == b'\n') {
        let Some(data) = line.strip_prefix(b"data:") else {
            continue;
        };
        let normalized = normalize_reasoning_event_data(trim_ascii(data));
        if serde_json::from_slice::<Value>(&normalized)
            .ok()
            .and_then(|event| event.get("type").and_then(Value::as_str).map(str::to_owned))
            .as_deref()
            == Some("response.completed")
        {
            completed = Some(normalized);
        }
    }
    completed.ok_or(XaiExecutionError::MissingCompleted)
}

pub(crate) fn sanitize_compact_body(body: &mut Vec<u8>) {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return;
    };
    if let Some(object) = value.as_object_mut() {
        for field in [
            "stream",
            "tools",
            "max_output_tokens",
            "temperature",
            "top_p",
            "top_k",
            "stop",
        ] {
            object.remove(field);
        }
    }
    if let Some(input) = value.get_mut("input").and_then(Value::as_array_mut) {
        input.retain(|item| item.get("type").and_then(Value::as_str) != Some("compaction_trigger"));
    }
    *body = serde_json::to_vec(&value).unwrap_or_else(|_| body.clone());
}
fn trim_ascii(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}
