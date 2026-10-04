// ref: internal/runtime/executor/devin_executor.go:126-139,230-387,464-583,990-1123,2465-2482
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — owned SDK HTTP, context and usage authorities
// License: MIT (upstream); modifications AGPL-3.0-only

use super::devin_executor_request::{
    prepare_devin_http_headers, prepare_devin_http_request, DevinRequestError, DevinRequestOwner,
};
use super::devin_executor_response::{
    DevinAggregateContext, DevinAggregateError, DevinInteractionAccumulator,
};
use super::devin_executor_stream::{DevinInteractionsStream, DevinStreamBatch};
use super::helps::apply_patch::{
    apply_patch_original_request, apply_patch_requested, APPLY_PATCH_UPSTREAM_ERROR_MESSAGE,
};
use super::helps::apply_patch_responses::ApplyPatchResponsesState;
use super::helps::claude_input_tokens::ClaudeInputTokenState;
use super::helps::cloak_obfuscate::SensitiveWordMatcher;
use super::helps::devin_request::DevinSessionTurns;
use super::helps::devin_wire::{ConnectFrameDecoder, ConnectFrameError};
use super::helps::usage_helpers::{parse_interactions_usage, UsageReporter};
use crate::internal::registry::{DevinModelsStore, StaticModelsCatalog};
use crate::sdk::pluginapi::{
    ExecutorHttpRequest, ExecutorHttpResponse, ExecutorRequest, ExecutorResponse,
    ExecutorStreamChunk, ExecutorStreamResponse, Headers, HttpRequest, HttpStreamChunk,
    PluginExecutionError, PluginFuture, ProviderExecutor,
};
use crate::sdk::translator::{Format, Registry, TranslationContext, TranslationState};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;

const MAX_ERROR_BODY: usize = 1024 * 1024;
const FRAME_INPUT_SLICE: usize = 16 * 1024;

/// Supplied by the selected execution owner, never inferred from account labels
/// or mutable request metadata by this executor.
pub struct DevinAttemptContext {
    pub canonical_session_id: String,
    pub usage: Option<Arc<UsageReporter>>,
}
impl fmt::Debug for DevinAttemptContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinAttemptContext")
            .field("has_session", &!self.canonical_session_id.is_empty())
            .field("has_usage_owner", &self.usage.is_some())
            .finish()
    }
}
pub trait DevinAttemptContextProvider: Send + Sync {
    fn for_request(
        &self,
        request: &ExecutorRequest,
    ) -> Result<DevinAttemptContext, PluginExecutionError>;
}

pub struct DevinExecutor {
    registry: Arc<Registry>,
    models: Arc<DevinModelsStore>,
    catalog: Arc<StaticModelsCatalog>,
    turns: Arc<DevinSessionTurns>,
    context: Arc<dyn DevinAttemptContextProvider>,
    matcher: Option<Arc<SensitiveWordMatcher>>,
}
impl fmt::Debug for DevinExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinExecutor")
            .field("has_prompt_matcher", &self.matcher.is_some())
            .finish_non_exhaustive()
    }
}
impl DevinExecutor {
    pub fn new(
        registry: Arc<Registry>,
        models: Arc<DevinModelsStore>,
        catalog: Arc<StaticModelsCatalog>,
        turns: Arc<DevinSessionTurns>,
        context: Arc<dyn DevinAttemptContextProvider>,
    ) -> Self {
        Self {
            registry,
            models,
            catalog,
            turns,
            context,
            matcher: None,
        }
    }
    pub fn with_matcher(mut self, matcher: Arc<SensitiveWordMatcher>) -> Self {
        self.matcher = Some(matcher);
        self
    }
    fn request_owner<'a>(&'a self, session: &'a str) -> DevinRequestOwner<'a> {
        DevinRequestOwner {
            registry: &self.registry,
            models: &self.models,
            catalog: &self.catalog,
            turns: &self.turns,
            canonical_session_id: session,
            matcher: self.matcher.as_deref(),
        }
    }

    async fn execute_inner(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        let context = self.context.for_request(&request)?;
        // The publication owner also records cancellation when this future is
        // dropped before an outcome can be returned.
        let _publication = CancellationPublication(context.usage.clone());
        let result = self.execute_owned(&request, &context).await;
        if let Err(error) = &result {
            publish_failure(context.usage.as_deref(), error);
        }
        result
    }

    async fn execute_owned(
        &self,
        request: &ExecutorRequest,
        context: &DevinAttemptContext,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        let prepared =
            prepare_devin_http_request(&self.request_owner(&context.canonical_session_id), request)
                .map_err(|error| plugin_error(DevinExecutorError::Request(error)))?;
        let client = request
            .http_client
            .as_ref()
            .ok_or_else(|| plugin_error(DevinExecutorError::MissingHttpClient))?;
        if let Some(reporter) = &context.usage {
            reporter.start_response_ttft();
        }
        // Use the streaming HTTP contract even for unary inference. Full-body
        // buffering must not bypass frame/aggregate bounds or delay a clean EOS.
        let mut upstream = client.execute_stream(prepared.http_request).await?;
        if !(200..300).contains(&upstream.status_code) {
            return Err(status_error(
                upstream.status_code,
                &upstream.headers,
                collect_error_body(&mut upstream.chunks).await,
            ));
        }
        let headers = upstream.headers;
        let mut decoder = ConnectFrameDecoder::default();
        let mut accumulator = DevinInteractionAccumulator::default();
        let mut complete = false;
        while let Some(chunk) = upstream.chunks.recv().await {
            if let Some(reporter) = &context.usage {
                reporter.observe_response_chunk(&chunk.payload);
            }
            for input in chunk.payload.chunks(FRAME_INPUT_SLICE) {
                tokio::task::yield_now().await;

                let mut aggregate_error = None;
                let framing = decoder.feed(input, |frame| {
                    if complete || aggregate_error.is_some() {
                        return;
                    }
                    match accumulator.accept(frame) {
                        Ok(done) => complete = done,
                        Err(error) => aggregate_error = Some(error),
                    }
                });
                if let Some(error) = aggregate_error {
                    return Err(aggregate_error_for_request(request, error));
                }
                // Upstream stops reading at the first EOS; bytes after that EOS
                // cannot turn an already-complete attempt into a framing error.
                if complete {
                    break;
                }
                framing.map_err(|error| {
                    aggregate_error_for_request(request, DevinAggregateError::Connect(error))
                })?;
            }
            if complete {
                break;
            }
            if let Some(error) = chunk.error {
                return Err(error);
            }
        }
        if !complete {
            decoder.finish().map_err(|error| {
                aggregate_error_for_request(request, DevinAggregateError::Connect(error))
            })?;
        }
        let aggregate = accumulator
            .finish(&DevinAggregateContext {
                model: &request.model,
                original_request: apply_patch_original_request(request),
            })
            .map_err(|failure| aggregate_error_for_request(request, failure.error))?;
        let target = response_format(request);
        let mut state: TranslationState = None;
        let mut payload = self.registry.translate_non_stream(
            &TranslationContext::default(),
            &Format::from("interactions"),
            &target,
            &request.model,
            apply_patch_original_request(request),
            &request.payload,
            &aggregate.payload,
            &mut state,
        );
        if target == Format::from("openai-response") {
            let mut patch = ApplyPatchResponsesState::new(
                &Format::from(request.source_format.as_str()),
                apply_patch_original_request(request),
                apply_patch_original_request(request),
            );
            payload = patch
                .bridge
                .transform_non_stream(&payload)
                .map_err(|_| plugin_error(DevinExecutorError::Translation))?;
        }
        if payload.is_empty() {
            return Err(plugin_error(DevinExecutorError::Translation));
        }
        if let Some(reporter) = &context.usage {
            reporter.publish(parse_interactions_usage(&aggregate.payload));
        }
        Ok(ExecutorResponse {
            payload,
            headers,
            ..ExecutorResponse::default()
        })
    }

    async fn execute_stream_inner(
        &self,
        mut request: ExecutorRequest,
    ) -> Result<ExecutorStreamResponse, PluginExecutionError> {
        let context = self.context.for_request(&request)?;
        let mut publication = CancellationPublication(context.usage.clone());
        request.stream = true;
        let result = self.start_stream(request, &context).await;
        if result.is_ok() {
            publication.0 = None;
        }
        if let Err(error) = &result {
            publish_failure(context.usage.as_deref(), error);
        }
        result
    }

    async fn start_stream(
        &self,
        request: ExecutorRequest,
        context: &DevinAttemptContext,
    ) -> Result<ExecutorStreamResponse, PluginExecutionError> {
        let prepared = prepare_devin_http_request(
            &self.request_owner(&context.canonical_session_id),
            &request,
        )
        .map_err(|error| plugin_error(DevinExecutorError::Request(error)))?;
        let client = request
            .http_client
            .as_ref()
            .ok_or_else(|| plugin_error(DevinExecutorError::MissingHttpClient))?;
        if let Some(reporter) = &context.usage {
            reporter.start_response_ttft();
        }
        let mut upstream = client.execute_stream(prepared.http_request).await?;
        if !(200..300).contains(&upstream.status_code) {
            return Err(status_error(
                upstream.status_code,
                &upstream.headers,
                collect_error_body(&mut upstream.chunks).await,
            ));
        }
        let headers = upstream.headers;
        let (sender, receiver) = mpsc::channel(16);
        let registry = Arc::clone(&self.registry);
        let usage = context.usage.clone();
        // This task owns the upstream receiver. Downstream drop, first terminal
        // frame, body failure or EOF ends it and releases that receiver.
        tokio::spawn(async move {
            pump_stream(registry, request, upstream.chunks, sender, usage).await;
        });
        Ok(ExecutorStreamResponse {
            headers,
            chunks: receiver,
        })
    }
}

impl ProviderExecutor for DevinExecutor {
    fn identifier(&self) -> &str {
        "devin"
    }
    fn execute<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(self.execute_inner(request))
    }
    fn execute_stream<'a>(
        &'a self,
        request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(self.execute_stream_inner(request))
    }
    fn count_tokens<'a>(&'a self, request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async move {
            let count = request.payload.len() / 4;
            Ok(ExecutorResponse {
                payload: format!(r#"{{"total_tokens":{count},"input_tokens":{count}}}"#)
                    .into_bytes(),
                ..ExecutorResponse::default()
            })
        })
    }
    fn http_request<'a>(
        &'a self,
        request: ExecutorHttpRequest,
    ) -> PluginFuture<'a, ExecutorHttpResponse> {
        Box::pin(async move {
            let client = request
                .http_client
                .as_ref()
                .ok_or_else(|| plugin_error(DevinExecutorError::MissingHttpClient))?;
            let mut upstream = HttpRequest {
                method: request.method,
                url: request.url,
                headers: request.headers,
                body: request.body,
            };
            prepare_devin_http_headers(&mut upstream, &request.attributes, &request.metadata);
            let response = client.execute(upstream).await?;
            Ok(ExecutorHttpResponse {
                status_code: response.status_code,
                headers: response.headers,
                body: response.body,
            })
        })
    }
}

#[derive(Clone)]
pub struct DevinHttpStatusError {
    pub status_code: u16,
    pub retry_after: Option<Duration>,
    body: Vec<u8>,
}
impl fmt::Debug for DevinHttpStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DevinHttpStatusError")
            .field("status_code", &self.status_code)
            .field("retry_after", &self.retry_after)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}
impl fmt::Display for DevinHttpStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.body.is_empty() {
            write!(f, "Devin upstream HTTP {}", self.status_code)
        } else {
            f.write_str(&String::from_utf8_lossy(&self.body))
        }
    }
}
impl std::error::Error for DevinHttpStatusError {}

#[derive(Debug)]
pub enum DevinExecutorError {
    MissingHttpClient,
    Request(DevinRequestError),
    Aggregate(DevinAggregateError),
    Translation,
    ConsumerClosed,
}
impl DevinExecutorError {
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Aggregate(error) => error.status_code(),
            Self::ConsumerClosed => 499,
            Self::Translation => 502,
            Self::Request(_) => 400,
            Self::MissingHttpClient => 500,
        }
    }
}
impl fmt::Display for DevinExecutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHttpClient => f.write_str("devin executor: HTTP client is missing"),
            Self::Request(error) => write!(f, "{error}"),
            Self::Aggregate(error) => write!(f, "{error}"),
            Self::Translation => f.write_str(APPLY_PATCH_UPSTREAM_ERROR_MESSAGE),
            Self::ConsumerClosed => f.write_str("Devin downstream consumer closed"),
        }
    }
}
impl std::error::Error for DevinExecutorError {}

fn plugin_error(error: DevinExecutorError) -> PluginExecutionError {
    Arc::new(error)
}
fn aggregate_error_for_request(
    request: &ExecutorRequest,
    error: DevinAggregateError,
) -> PluginExecutionError {
    if apply_patch_requested(apply_patch_original_request(request)) {
        plugin_error(DevinExecutorError::Translation)
    } else {
        plugin_error(DevinExecutorError::Aggregate(error))
    }
}
fn response_format(request: &ExecutorRequest) -> Format {
    Format::from(if request.format.is_empty() {
        request.source_format.as_str()
    } else {
        request.format.as_str()
    })
}
fn status_error(status: u16, headers: &Headers, body: Vec<u8>) -> PluginExecutionError {
    Arc::new(DevinHttpStatusError {
        status_code: status,
        retry_after: devin_retry_after(status, headers, SystemTime::now()),
        body,
    })
}
fn devin_retry_after(status: u16, headers: &Headers, now: SystemTime) -> Option<Duration> {
    if status != 429 {
        return None;
    }
    let raw = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("Retry-After"))?
        .1
        .first()?
        .trim();
    if let Ok(seconds) = raw.parse::<i64>() {
        if seconds >= 0 {
            return Some(Duration::from_secs(seconds as u64));
        }
    }
    let delay = httpdate::parse_http_date(raw)
        .ok()?
        .duration_since(now)
        .ok()?;
    (!delay.is_zero()).then_some(delay)
}
async fn collect_error_body(source: &mut mpsc::Receiver<HttpStreamChunk>) -> Vec<u8> {
    let mut body = Vec::new();
    while body.len() < MAX_ERROR_BODY {
        let Some(chunk) = source.recv().await else {
            break;
        };
        let count = chunk.payload.len().min(MAX_ERROR_BODY - body.len());
        body.extend_from_slice(&chunk.payload[..count]);
        if chunk.error.is_some() {
            break;
        }
    }
    body
}
fn publish_failure(reporter: Option<&UsageReporter>, error: &PluginExecutionError) {
    if let Some(reporter) = reporter {
        let status = error
            .downcast_ref::<DevinHttpStatusError>()
            .map(|error| error.status_code)
            .or_else(|| {
                error
                    .downcast_ref::<DevinExecutorError>()
                    .map(DevinExecutorError::status_code)
            });
        reporter.publish_failure(status.map(i32::from), error.as_ref());
    }
}

/// UsageReporter publishes once, so this fallback cannot overwrite a completed
/// success or explicit failure. Ownership transfers to the streaming task.
struct CancellationPublication(Option<Arc<UsageReporter>>);
impl Drop for CancellationPublication {
    fn drop(&mut self) {
        if let Some(reporter) = &self.0 {
            reporter.publish_failure(Some(499), &DevinExecutorError::ConsumerClosed);
        }
    }
}

struct StreamTranslation {
    registry: Arc<Registry>,
    request: ExecutorRequest,
    target: Format,
    state: TranslationState,
    patch: Option<ApplyPatchResponsesState>,
    claude_tokens: ClaudeInputTokenState,
    usage: Option<Arc<UsageReporter>>,
}
impl StreamTranslation {
    fn new(
        registry: Arc<Registry>,
        request: ExecutorRequest,
        usage: Option<Arc<UsageReporter>>,
    ) -> Self {
        let target = response_format(&request);
        let source = Format::from(request.source_format.as_str());
        let patch = (target == Format::from("openai-response")).then(|| {
            ApplyPatchResponsesState::new(
                &source,
                apply_patch_original_request(&request),
                apply_patch_original_request(&request),
            )
        });
        let claude_tokens = ClaudeInputTokenState::new(
            &source,
            &Format::from("interactions"),
            &target,
            &request.original_request,
        );
        Self {
            registry,
            request,
            target,
            state: None,
            patch,
            claude_tokens,
            usage,
        }
    }
    async fn send(
        &mut self,
        sender: &mpsc::Sender<ExecutorStreamChunk>,
        event: &[u8],
    ) -> Result<(), PluginExecutionError> {
        let chunks = if self.target == Format::from("interactions") {
            let mut frame = b"data: ".to_vec();
            frame.extend_from_slice(event);
            frame.extend_from_slice(b"\n\n");
            vec![frame]
        } else {
            self.registry.translate_stream(
                &TranslationContext::default(),
                &Format::from("interactions"),
                &self.target,
                &self.request.model,
                apply_patch_original_request(&self.request),
                &self.request.payload,
                event,
                &mut self.state,
            )
        };
        let chunks = self.claude_tokens.apply(chunks);
        for chunk in chunks {
            if let Some(patch) = &mut self.patch {
                for line in chunk.split_inclusive(|byte| *byte == b'\n') {
                    let (outputs, error) = patch.stream(line);
                    send_outputs(sender, outputs).await?;
                    if error.is_some() {
                        return Err(plugin_error(DevinExecutorError::Translation));
                    }
                }
            } else {
                send_outputs(sender, vec![chunk]).await?;
            }
        }
        Ok(())
    }
    async fn batch(
        &mut self,
        sender: &mpsc::Sender<ExecutorStreamChunk>,
        batch: DevinStreamBatch,
        transport_error: Option<PluginExecutionError>,
    ) -> bool {
        let mut completion_usage = None;
        for event in batch.events {
            if let Some(reporter) = &self.usage {
                reporter.observe_response_chunk(&event);
            }
            if gjson::get_bytes(&event, "event_type").str() == "interaction.completed" {
                completion_usage = Some(parse_interactions_usage(&event));
            }
            if let Err(error) = self.send(sender, &event).await {
                publish_failure(self.usage.as_deref(), &error);
                send_error(sender, error).await;
                return false;
            }
        }
        if let Some(error) = transport_error.or_else(|| {
            batch
                .error
                .map(|error| plugin_error(DevinExecutorError::Aggregate(error)))
        }) {
            publish_failure(self.usage.as_deref(), &error);
            send_error(sender, error).await;
            return false;
        }
        if batch.complete {
            if let Some(patch) = &mut self.patch {
                let (outputs, error) = patch.finish_stream();
                if let Err(error) = send_outputs(sender, outputs).await {
                    publish_failure(self.usage.as_deref(), &error);
                    return false;
                }
                if error.is_some() {
                    let error = plugin_error(DevinExecutorError::Translation);
                    publish_failure(self.usage.as_deref(), &error);
                    send_error(sender, error).await;
                    return false;
                }
            }
            if let Some(reporter) = &self.usage {
                if let Some(detail) = completion_usage {
                    reporter.publish(detail);
                }
            }
            if let Err(error) = self.send(sender, b"[DONE]").await {
                publish_failure(self.usage.as_deref(), &error);
                send_error(sender, error).await;
            }
            return false;
        }
        true
    }
}
async fn send_outputs(
    sender: &mpsc::Sender<ExecutorStreamChunk>,
    outputs: Vec<Vec<u8>>,
) -> Result<(), PluginExecutionError> {
    for mut payload in outputs {
        if payload.starts_with(b"event:") && !payload.ends_with(b"\n") {
            payload.push(b'\n');
        }
        sender
            .send(ExecutorStreamChunk {
                payload,
                error: None,
            })
            .await
            .map_err(|_| plugin_error(DevinExecutorError::ConsumerClosed))?;
    }
    Ok(())
}
async fn send_error(sender: &mpsc::Sender<ExecutorStreamChunk>, error: PluginExecutionError) {
    let _ = sender
        .send(ExecutorStreamChunk {
            payload: Vec::new(),
            error: Some(error),
        })
        .await;
}
async fn pump_stream(
    registry: Arc<Registry>,
    request: ExecutorRequest,
    mut source: mpsc::Receiver<HttpStreamChunk>,
    sender: mpsc::Sender<ExecutorStreamChunk>,
    usage: Option<Arc<UsageReporter>>,
) {
    let _publication = CancellationPublication(usage.clone());
    let mut events =
        DevinInteractionsStream::new(&request.model, response_format(&request).as_str());
    let mut translation = StreamTranslation::new(registry, request, usage);
    let mut decoder = ConnectFrameDecoder::default();
    loop {
        let chunk = tokio::select! {
            biased;
            _ = sender.closed() => {
                let error = plugin_error(DevinExecutorError::ConsumerClosed);
                publish_failure(translation.usage.as_deref(), &error);
                return;
            }
            chunk = source.recv() => chunk,
        };
        let Some(chunk) = chunk else {
            let batch = match decoder.finish() {
                Ok(()) => events.finish(),
                Err(error) => events.fail(DevinAggregateError::Connect(error), "stream_read_error"),
            };
            translation.batch(&sender, batch, None).await;
            return;
        };
        for input in chunk.payload.chunks(FRAME_INPUT_SLICE) {
            tokio::task::yield_now().await;
            if sender.is_closed() {
                let error = plugin_error(DevinExecutorError::ConsumerClosed);
                publish_failure(translation.usage.as_deref(), &error);
                return;
            }
            let mut terminal = false;
            let mut batches = Vec::new();
            let framing = decoder.feed(input, |frame| {
                if terminal {
                    return;
                }
                let batch = events.accept(frame);
                terminal = batch.complete || batch.error.is_some();
                if !batch.events.is_empty() || terminal {
                    batches.push(batch);
                }
            });
            for batch in batches {
                if !translation.batch(&sender, batch, None).await {
                    return;
                }
            }
            if let Err(error) = framing {
                let batch = events.fail(DevinAggregateError::Connect(error), "stream_read_error");
                translation.batch(&sender, batch, None).await;
                return;
            }
        }
        if let Some(error) = chunk.error {
            let batch = events.fail(
                DevinAggregateError::Connect(ConnectFrameError::Read(std::io::ErrorKind::Other)),
                "stream_read_error",
            );
            translation.batch(&sender, batch, Some(error)).await;
            return;
        }
    }
}

#[cfg(test)]
#[path = "devin_executor_test.rs"]
mod tests;
