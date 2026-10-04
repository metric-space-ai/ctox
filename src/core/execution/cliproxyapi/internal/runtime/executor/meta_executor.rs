// ref: internal/runtime/executor/meta_executor_execute.go:85-259; meta_executor_stream.go:1-186 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — selected HTTP transport and attempt-owned usage
// License: MIT (upstream); modifications AGPL-3.0-only

use super::codex_executor_tokens::count_meta_input_tokens;
use super::helps::{
    ensure_responses_usage_details, parse_codex_usage, ClaudeInputTokenState, StreamUsageBuffer,
    UsageReporter, APPLY_PATCH_UPSTREAM_ERROR_MESSAGE,
};
use super::meta_executor_auth::meta_credentials;
use super::meta_executor_request::{
    meta_error, meta_http_request, prepare_meta_http_headers, MetaPreparedRequest, MetaRequestOwner,
};
use super::meta_executor_response::{
    meta_as_completed_event, meta_stream_event_error, meta_upstream_error, MetaHttpStatusError,
    MetaOutputItems, MetaResponseLines, META_INCOMPLETE_STREAM_MESSAGE,
};
use crate::internal::auth::meta::{MetaClock, SystemMetaClock};
use crate::sdk::cliproxy::auth::{Auth, AuthError};
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, HttpRequest, HttpStreamChunk,
    PluginExecutionError, PluginFuture, ProviderExecutor,
};
use crate::sdk::translator::{Format, Registry, TranslationContext, TranslationState};
use std::{fmt, sync::Arc, time::SystemTime};
use tokio::sync::mpsc;

/// The host owns usage identity and persistence; caller metadata cannot supply it.
pub struct MetaAttemptContext {
    pub usage: Option<Arc<UsageReporter>>,
}
pub trait MetaAttemptContextProvider: Send + Sync {
    fn for_request(
        &self,
        request: &ExecutorRequest,
        base_model: &str,
    ) -> Result<MetaAttemptContext, PluginExecutionError>;
}

pub struct MetaExecutor {
    registry: Arc<Registry>,
    request_owner: Arc<MetaRequestOwner>,
    context: Arc<dyn MetaAttemptContextProvider>,
    clock: Arc<dyn MetaClock>,
}
impl fmt::Debug for MetaExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaExecutor").finish_non_exhaustive()
    }
}
impl MetaExecutor {
    pub fn new(
        registry: Arc<Registry>,
        request_owner: Arc<MetaRequestOwner>,
        context: Arc<dyn MetaAttemptContextProvider>,
    ) -> Self {
        Self {
            registry,
            request_owner,
            context,
            clock: Arc::new(SystemMetaClock),
        }
    }
    pub fn with_clock(mut self, clock: Arc<dyn MetaClock>) -> Self {
        self.clock = clock;
        self
    }
    fn validate_provider(request: &ExecutorRequest) -> Result<(), PluginExecutionError> {
        if !request.auth_provider.eq_ignore_ascii_case("meta") {
            return Err(meta_error(
                400,
                "meta executor: selected account is not a Meta provider",
            ));
        }
        Ok(())
    }
    fn validate(request: &ExecutorRequest) -> Result<(), PluginExecutionError> {
        Self::validate_provider(request)?;
        if request.alt == "responses/compact" {
            return Err(meta_error(501, "/responses/compact not supported"));
        }
        Ok(())
    }
    async fn execute_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        Self::validate(&request)?;
        let prepared = self.request_owner.prepare(&request, true)?;
        let context = self.context.for_request(&request, &prepared.base_model)?;
        let mut publication = CancellationPublication(context.usage.clone());
        let result = self.execute_owned(&request, prepared, &context).await;
        if let Err(error) = &result {
            publish_failure(context.usage.as_deref(), error);
        }
        publication.0 = None;
        result
    }
    async fn execute_owned(
        &self,
        request: &ExecutorRequest,
        mut prepared: MetaPreparedRequest,
        context: &MetaAttemptContext,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        // Upstream unary inference also requests Responses SSE and reads the complete body.
        let outgoing = meta_http_request(request, prepared.body.clone(), true)?;
        let client = request
            .http_client
            .as_ref()
            .ok_or_else(|| meta_error(500, "meta executor: HTTP client is missing"))?;
        if let Some(reporter) = &context.usage {
            reporter.set_translated_reasoning_effort(&prepared.body, "codex");
            reporter.start_response_ttft();
        }
        let response = client.execute(outgoing).await?;
        if let Some(reporter) = &context.usage {
            reporter.observe_response_chunk(&response.body);
        }
        if !(200..300).contains(&response.status_code) {
            return Err(Arc::new(meta_upstream_error(
                response.status_code,
                &response.body,
                self.clock.now().into(),
            )));
        }
        let (mut payload, source) = translate_completed(
            &self.registry,
            request,
            &mut prepared,
            &response.body,
            self.clock.now().into(),
        )?;
        if let Some(reporter) = &context.usage {
            reporter.observe_response_chunk(&source);
            if let Some(detail) = parse_codex_usage(&source) {
                reporter.publish(detail);
            } else {
                reporter.ensure_published();
            }
        }
        if prepared.target == Format::from("openai-response") {
            payload = ensure_responses_usage_details(&payload);
        }
        Ok(ExecutorResponse {
            payload,
            headers: response.headers,
            ..ExecutorResponse::default()
        })
    }
    async fn start_stream(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorStreamResponse, PluginExecutionError> {
        Self::validate(&request)?;
        let prepared = self.request_owner.prepare(&request, true)?;
        let context = self.context.for_request(&request, &prepared.base_model)?;
        let mut publication = CancellationPublication(context.usage.clone());
        let result = self.start_owned_stream(request, prepared, &context).await;
        if let Err(error) = &result {
            publish_failure(context.usage.as_deref(), error);
        }
        // On success the stream task owns cancellation publication.
        publication.0 = None;
        result
    }
    async fn start_owned_stream(
        &self,
        request: ExecutorRequest,
        prepared: MetaPreparedRequest,
        context: &MetaAttemptContext,
    ) -> Result<ExecutorStreamResponse, PluginExecutionError> {
        let outgoing = meta_http_request(&request, prepared.body.clone(), true)?;
        let client = request
            .http_client
            .as_ref()
            .ok_or_else(|| meta_error(500, "meta executor: HTTP client is missing"))?;
        if let Some(reporter) = &context.usage {
            reporter.set_translated_reasoning_effort(&prepared.body, "codex");
            reporter.start_response_ttft();
        }
        let mut response = client.execute_stream(outgoing).await?;
        if !(200..300).contains(&response.status_code) {
            let mut body = Vec::new();
            while let Some(chunk) = response.chunks.recv().await {
                body.extend_from_slice(&chunk.payload);
                if let Some(error) = chunk.error {
                    return Err(error);
                }
            }
            return Err(Arc::new(meta_upstream_error(
                response.status_code,
                &body,
                self.clock.now().into(),
            )));
        }
        let (sender, receiver) = mpsc::channel(16);
        let translation = StreamTranslation::new(
            self.registry.clone(),
            request,
            prepared,
            context.usage.clone(),
            self.clock.clone(),
        );
        // This task owns exactly one upstream receiver; downstream close, EOF,
        // translation failure or a provider error releases it without a retry.
        tokio::spawn(pump_stream(translation, response.chunks, sender));
        Ok(ExecutorStreamResponse {
            headers: response.headers,
            chunks: receiver,
        })
    }
}
impl ProviderExecutor for MetaExecutor {
    fn identifier(&self) -> &str {
        "meta"
    }
    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(self.execute_inner(request))
    }
    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(self.start_stream(request))
    }
    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move {
            Self::validate_provider(&request)?;
            let prepared = self.request_owner.prepare(&request, false)?;
            // Validate the manager's inference credential; counting makes no HTTP request.
            meta_http_request(&request, Vec::new(), false)?;
            let count = count_meta_input_tokens(&prepared.body).map_err(|error| {
                meta_error(
                    500,
                    &format!("meta executor: token counting failed: {error}"),
                )
            })?;
            let usage = format!(
                r#"{{"response":{{"usage":{{"input_tokens":{count},"output_tokens":0,"total_tokens":{count}}}}}}}"#
            );
            let payload = self.registry.translate_token_count(
                &TranslationContext::default(),
                &Format::from("codex"),
                &prepared.target,
                count,
                usage.as_bytes(),
            );
            Ok(ExecutorResponse {
                payload,
                ..ExecutorResponse::default()
            })
        })
    }
    fn http_request<'a>(
        &'a self,
        request: ExecutorHttpRequest,
    ) -> PluginFuture<'a, ExecutorHttpResponse> {
        Box::pin(async move {
            if !request.auth_provider.eq_ignore_ascii_case("meta") {
                return Err(meta_error(
                    400,
                    "meta executor: selected account is not a Meta provider",
                ));
            }
            let client = request
                .http_client
                .as_ref()
                .ok_or_else(|| meta_error(500, "meta executor: HTTP client is missing"))?;
            let auth = Auth {
                attributes: request.attributes.clone(),
                metadata: request.metadata.clone(),
                ..Auth::default()
            };
            let credential = meta_credentials(&auth);
            if credential.api_key().is_empty() {
                return Err(meta_error(401, "meta executor: missing inference API key"));
            }
            let mut outgoing = HttpRequest {
                method: request.method,
                url: request.url,
                headers: request.headers,
                body: request.body,
            };
            prepare_meta_http_headers(&mut outgoing, &request.attributes, credential.api_key());
            let response = client.execute(outgoing).await?;
            Ok(ExecutorHttpResponse {
                status_code: response.status_code,
                headers: response.headers,
                body: response.body,
            })
        })
    }
}

fn translate_completed(
    registry: &Registry,
    request: &ExecutorRequest,
    prepared: &mut MetaPreparedRequest,
    data: &[u8],
    now: SystemTime,
) -> Result<(Vec<u8>, Vec<u8>), PluginExecutionError> {
    let mut items = MetaOutputItems::default();
    for line in data.split(|v| *v == b'\n') {
        let Some(event) = line.strip_prefix(b"data:") else {
            continue;
        };
        let event = trim(event);
        if let Some(error) = meta_stream_event_error(event, now) {
            return Err(Arc::new(error));
        }
        let (events, error) = prepared.patch.transform(event);
        if error.is_some() {
            return Err(meta_error(502, APPLY_PATCH_UPSTREAM_ERROR_MESSAGE));
        }
        for event in events {
            match event_kind(&event).as_str() {
                "response.output_item.done" => items.collect(&event),
                "response.completed" | "response.incomplete" => {
                    let terminal = items.patch_completed(&event);
                    return translate_terminal(registry, request, prepared, terminal);
                }
                _ => {}
            }
        }
    }
    if let Some(terminal) = meta_as_completed_event(data) {
        let terminal = items.patch_completed(&terminal);
        let terminal = prepared
            .patch
            .bridge
            .transform_non_stream(&terminal)
            .map_err(|_| meta_error(502, APPLY_PATCH_UPSTREAM_ERROR_MESSAGE))?;
        return translate_terminal(registry, request, prepared, terminal);
    }
    prepared
        .patch
        .finish()
        .map_err(|_| meta_error(502, APPLY_PATCH_UPSTREAM_ERROR_MESSAGE))?;
    Err(meta_error(408, META_INCOMPLETE_STREAM_MESSAGE))
}
fn translate_terminal(
    registry: &Registry,
    request: &ExecutorRequest,
    prepared: &MetaPreparedRequest,
    terminal: Vec<u8>,
) -> Result<(Vec<u8>, Vec<u8>), PluginExecutionError> {
    let mut state: TranslationState = None;
    let output = registry.translate_non_stream(
        &TranslationContext::default(),
        &Format::from("codex"),
        &prepared.target,
        &request.model,
        &prepared.original,
        &prepared.body,
        &terminal,
        &mut state,
    );
    if output.is_empty() {
        return Err(meta_error(502, APPLY_PATCH_UPSTREAM_ERROR_MESSAGE));
    }
    Ok((output, terminal))
}
fn trim(bytes: &[u8]) -> &[u8] {
    let first = bytes
        .iter()
        .position(|v| !v.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let last = bytes
        .iter()
        .rposition(|v| !v.is_ascii_whitespace())
        .map_or(first, |v| v + 1);
    &bytes[first..last]
}
fn event_kind(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes)
        .ok()
        .map(|v| gjson::get(v, "type").str().to_owned())
        .unwrap_or_default()
}
pub(crate) fn meta_plugin_error_status(error: &(dyn std::error::Error + 'static)) -> Option<u16> {
    let mut current = Some(error);
    while let Some(error) = current {
        if let Some(status) = error.downcast_ref::<MetaHttpStatusError>() {
            return Some(status.status_code());
        }
        if let Some(status) = error.downcast_ref::<AuthError>() {
            return Some(status.http_status);
        }
        current = error.source();
    }
    None
}
fn publish_failure(reporter: Option<&UsageReporter>, error: &PluginExecutionError) {
    if let Some(reporter) = reporter {
        reporter.publish_failure(
            meta_plugin_error_status(error.as_ref()).map(i32::from),
            error.as_ref(),
        );
    }
}
struct CancellationPublication(Option<Arc<UsageReporter>>);
impl Drop for CancellationPublication {
    fn drop(&mut self) {
        if let Some(reporter) = &self.0 {
            reporter.publish_failure(
                Some(499),
                meta_error(499, "Meta downstream consumer closed").as_ref(),
            );
        }
    }
}

struct StreamTranslation {
    registry: Arc<Registry>,
    request: ExecutorRequest,
    prepared: MetaPreparedRequest,
    state: TranslationState,
    claude_tokens: ClaudeInputTokenState,
    usage: Option<Arc<UsageReporter>>,
    stream_usage: StreamUsageBuffer,
    items: MetaOutputItems,
    clock: Arc<dyn MetaClock>,
}
impl StreamTranslation {
    fn new(
        registry: Arc<Registry>,
        request: ExecutorRequest,
        prepared: MetaPreparedRequest,
        usage: Option<Arc<UsageReporter>>,
        clock: Arc<dyn MetaClock>,
    ) -> Self {
        let claude_tokens = ClaudeInputTokenState::new(
            &prepared.from,
            &Format::from("codex"),
            &prepared.target,
            &prepared.original,
        );
        Self {
            registry,
            request,
            prepared,
            state: None,
            claude_tokens,
            usage,
            stream_usage: StreamUsageBuffer::default(),
            items: MetaOutputItems::default(),
            clock,
        }
    }
    async fn translated(
        &mut self,
        sender: &mpsc::Sender<ExecutorStreamChunk>,
        line: &[u8],
    ) -> Result<(), PluginExecutionError> {
        let chunks = self.registry.translate_stream(
            &TranslationContext::default(),
            &Format::from("codex"),
            &self.prepared.target,
            &self.request.model,
            &self.prepared.original,
            &self.prepared.body,
            line,
            &mut self.state,
        );
        for mut payload in self.claude_tokens.apply(chunks) {
            // Scanner removes delimiters; preserve them for an unchanged SDK
            // passthrough line so adjacent SSE fields cannot concatenate.
            if payload == line && !payload.ends_with(b"\n") {
                payload.push(b'\n');
            }
            sender
                .send(ExecutorStreamChunk {
                    payload,
                    error: None,
                })
                .await
                .map_err(|_| meta_error(499, "Meta downstream consumer closed"))?;
        }
        Ok(())
    }
    async fn line(
        &mut self,
        sender: &mpsc::Sender<ExecutorStreamChunk>,
        line: &[u8],
    ) -> Result<(), PluginExecutionError> {
        let mut line = line.to_vec();
        if let Some(event) = line.strip_prefix(b"data:") {
            let event = trim(event);
            if let Some(reporter) = &self.usage {
                reporter.observe_response_chunk(event);
            }
            if let Some(error) = meta_stream_event_error(event, self.clock.now().into()) {
                return Err(Arc::new(error));
            }
            match event_kind(event).as_str() {
                "response.output_item.done" => self.items.collect(event),
                "response.completed" | "response.incomplete" => {
                    if let Some(detail) = parse_codex_usage(event) {
                        self.stream_usage.observe(detail, true);
                    }
                    line = [
                        b"data: ".as_slice(),
                        self.items.patch_completed(event).as_slice(),
                    ]
                    .concat();
                }
                _ => {}
            }
        }
        let (lines, error) = self.prepared.patch.stream(&line);
        // Preserve any bridge-generated failure event before the typed stream error.
        for line in lines {
            self.translated(sender, &line).await?;
        }
        if error.is_some() {
            return Err(meta_error(502, APPLY_PATCH_UPSTREAM_ERROR_MESSAGE));
        }
        Ok(())
    }
    async fn finish(
        &mut self,
        sender: &mpsc::Sender<ExecutorStreamChunk>,
    ) -> Result<(), PluginExecutionError> {
        let (events, error) = self.prepared.patch.finish_stream();
        for line in events {
            self.translated(sender, &line).await?;
        }
        if error.is_some() {
            return Err(meta_error(502, APPLY_PATCH_UPSTREAM_ERROR_MESSAGE));
        }
        Ok(())
    }
    async fn fail(
        &mut self,
        sender: &mpsc::Sender<ExecutorStreamChunk>,
        error: PluginExecutionError,
    ) {
        publish_failure(self.usage.as_deref(), &error);
        let _ = sender
            .send(ExecutorStreamChunk {
                payload: Vec::new(),
                error: Some(error),
            })
            .await;
    }
}
async fn pump_stream(
    mut translation: StreamTranslation,
    mut source: mpsc::Receiver<HttpStreamChunk>,
    sender: mpsc::Sender<ExecutorStreamChunk>,
) {
    let mut publication = CancellationPublication(translation.usage.clone());
    let mut lines = MetaResponseLines::default();
    loop {
        let chunk = tokio::select! { biased; _ = sender.closed() => return, chunk = source.recv() => chunk };
        let Some(chunk) = chunk else {
            if let Some(line) = lines.finish() {
                if let Err(error) = translation.line(&sender, &line).await {
                    translation.fail(&sender, error).await;
                    return;
                }
            }
            if let Err(error) = translation.finish(&sender).await {
                translation.fail(&sender, error).await;
                return;
            }
            if let Some(reporter) = &translation.usage {
                translation.stream_usage.publish(reporter);
            }
            publication.0 = None;
            return;
        };
        for bytes in chunk.payload.chunks(16 * 1024) {
            tokio::task::yield_now().await;
            if sender.is_closed() {
                return;
            }
            let ready = match lines.push(bytes) {
                Ok(lines) => lines,
                Err(error) => {
                    translation.fail(&sender, Arc::new(error)).await;
                    return;
                }
            };
            for line in ready {
                if let Err(error) = translation.line(&sender, &line).await {
                    translation.fail(&sender, error).await;
                    return;
                }
            }
        }
        if let Some(error) = chunk.error {
            if let Some(line) = lines.finish() {
                if let Err(error) = translation.line(&sender, &line).await {
                    translation.fail(&sender, error).await;
                    return;
                }
            }
            // Upstream emits bridge closure errors before a scanner/transport error.
            if let Err(bridge) = translation.finish(&sender).await {
                translation.fail(&sender, bridge).await;
            } else {
                translation.fail(&sender, error).await;
            }
            return;
        }
    }
}

#[cfg(test)]
#[path = "meta_executor_test.rs"]
mod tests;
