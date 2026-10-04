// ref: sdk/cliproxy/auth/conductor_stream.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use crate::sdk::cliproxy::executor::RequestTerminatedError;
use crate::sdk::pluginapi::{
    ExecutorRequest, ExecutorStreamChunk, ExecutorStreamResponse, PluginExecutionError,
};
use tokio::sync::mpsc;

use super::conductor_execution::{
    auth_selection_model, normalize_execution_providers, plugin_error_status, scheduler_options,
    selected_executor_request, GenericAuthRuntime, GenericExecutionError,
};
use super::{is_request_scoped_plugin_error, is_unauthorized_plugin_error, Auth};

// Stream transport ownership lives in the registered provider executor. These
// helpers retain the conductor's retry/fallback classification boundary.

#[must_use]
pub fn is_request_terminated_error(mut error: &(dyn Error + 'static)) -> bool {
    loop {
        if error.is::<RequestTerminatedError>() {
            return true;
        }
        let Some(source) = error.source() else {
            return false;
        };
        error = source;
    }
}

#[must_use]
pub fn should_attempt_antigravity_credits_fallback(
    enabled: bool,
    error: &(dyn Error + 'static),
    providers: &[String],
) -> bool {
    enabled
        && !is_request_terminated_error(error)
        && providers
            .iter()
            .any(|provider| provider.trim().eq_ignore_ascii_case("antigravity"))
}

/// A request-scoped stream tail failure is observable by the caller but must
/// not alter credential availability or trigger a replay after bytes started.
#[must_use]
pub(crate) fn stream_tail_is_availability_neutral(error: &PluginExecutionError) -> bool {
    is_request_scoped_plugin_error(error)
}

impl GenericAuthRuntime {
    /// Starts a non-Home stream and commits the selected credential only after
    /// the first non-empty payload. A 401 observed before that boundary may
    /// refresh and replay once; after commit the same stream is only forwarded
    /// and accounted, never redispatched.
    pub async fn execute_stream(
        self: &Arc<Self>,
        providers: &[String],
        request: ExecutorRequest,
    ) -> Result<ExecutorStreamResponse, GenericExecutionError> {
        let providers = normalize_execution_providers(providers)?;
        let route_model = auth_selection_model(&request);
        let mut options = scheduler_options(&request);
        let mut last_failure = None;

        'credentials: while options.tried_auth_ids.len() < self.max_credentials() {
            let (mut auth, registration) = match self.select(&providers, &route_model, &options) {
                Ok(selected) => selected,
                Err(error) => return finish_stream_failure(last_failure, error),
            };
            options.tried_auth_ids.insert(auth.id.clone());
            if let Err(error) = self.prepare(&registration, &mut auth).await {
                if is_request_scoped_dyn_error(error.as_ref()) {
                    return Err(GenericExecutionError::Preparation(error));
                }
                self.record_outcome(&auth, &route_model, dyn_error_status(error.as_ref()), false)?;
                last_failure = Some(StreamStartFailure::Direct(
                    GenericExecutionError::Preparation(error),
                ));
                continue;
            }
            let executor = registration
                .execution()
                .ok_or(GenericExecutionError::ExecutionUnavailable)?;
            let mut refreshed = false;

            loop {
                let execution = selected_executor_request(
                    &self.manager,
                    &request,
                    &auth,
                    registration.provider(),
                );
                let stream = match executor.execute_stream(execution).await {
                    Ok(stream) => stream,
                    Err(error) if is_unauthorized_plugin_error(&error) && !refreshed => {
                        let Some(updated) =
                            self.refresh_after_unauthorized(&auth, &registration)?
                        else {
                            self.record_outcome(&auth, &route_model, 401, false)?;
                            last_failure = Some(StreamStartFailure::Direct(
                                GenericExecutionError::Provider(error),
                            ));
                            continue 'credentials;
                        };
                        auth = updated;
                        self.prepare(&registration, &mut auth)
                            .await
                            .map_err(GenericExecutionError::Preparation)?;
                        refreshed = true;
                        continue;
                    }
                    Err(error) if is_request_scoped_plugin_error(&error) => {
                        return Err(GenericExecutionError::Provider(error));
                    }
                    Err(error) => {
                        let status = plugin_error_status(&error);
                        self.record_outcome(&auth, &route_model, status, false)?;
                        if matches!(status, 400 | 422) {
                            return Err(GenericExecutionError::Provider(error));
                        }
                        last_failure = Some(StreamStartFailure::Direct(
                            GenericExecutionError::Provider(error),
                        ));
                        continue 'credentials;
                    }
                };

                match read_stream_bootstrap(stream).await {
                    Ok(committed) => {
                        return Ok(self.clone().forward_committed_stream(
                            auth,
                            route_model,
                            committed,
                        ));
                    }
                    Err(failure) => match failure.kind {
                        BootstrapFailureKind::Provider(error)
                            if is_unauthorized_plugin_error(&error) && !refreshed =>
                        {
                            drain_stream(failure.remaining);
                            let Some(updated) =
                                self.refresh_after_unauthorized(&auth, &registration)?
                            else {
                                self.record_outcome(&auth, &route_model, 401, false)?;
                                last_failure =
                                    Some(StreamStartFailure::Bootstrap(failure.headers, error));
                                continue 'credentials;
                            };
                            auth = updated;
                            self.prepare(&registration, &mut auth)
                                .await
                                .map_err(GenericExecutionError::Preparation)?;
                            refreshed = true;
                            continue;
                        }
                        BootstrapFailureKind::Provider(error)
                            if is_request_scoped_plugin_error(&error) =>
                        {
                            drain_stream(failure.remaining);
                            self.record_availability_neutral_outcome(&auth.id, false);
                            return Ok(stream_error_response(failure.headers, error));
                        }
                        BootstrapFailureKind::Provider(error) => {
                            drain_stream(failure.remaining);
                            let status = plugin_error_status(&error);
                            self.record_outcome(&auth, &route_model, status, false)?;
                            if matches!(status, 400 | 422) {
                                return Ok(stream_error_response(failure.headers, error));
                            }
                            last_failure =
                                Some(StreamStartFailure::Bootstrap(failure.headers, error));
                            continue 'credentials;
                        }
                        BootstrapFailureKind::Empty => {
                            self.record_outcome(&auth, &route_model, 0, false)?;
                            last_failure = Some(StreamStartFailure::Bootstrap(
                                failure.headers,
                                Arc::new(EmptyStreamError),
                            ));
                            continue 'credentials;
                        }
                    },
                }
            }
        }

        finish_stream_failure(last_failure, GenericExecutionError::CredentialLimit)
    }

    fn forward_committed_stream(
        self: Arc<Self>,
        auth: Auth,
        route_model: String,
        committed: CommittedStream,
    ) -> ExecutorStreamResponse {
        let (sender, receiver) = mpsc::channel(32);
        let CommittedStream {
            headers,
            buffered,
            mut remaining,
        } = committed;
        tokio::spawn(async move {
            for chunk in buffered {
                if sender.send(chunk).await.is_err() {
                    drain_stream(remaining);
                    return;
                }
            }
            while let Some(chunk) = remaining.recv().await {
                if let Some(error) = &chunk.error {
                    if stream_tail_is_availability_neutral(error) {
                        self.record_availability_neutral_outcome(&auth.id, false);
                    } else {
                        let _ = self.record_outcome(
                            &auth,
                            &route_model,
                            plugin_error_status(error),
                            false,
                        );
                    }
                    let _ = sender.send(chunk).await;
                    drain_stream(remaining);
                    return;
                }
                if sender.send(chunk).await.is_err() {
                    drain_stream(remaining);
                    return;
                }
            }
            if let Err(error) = self.record_outcome(&auth, &route_model, 200, true) {
                let _ = sender
                    .send(ExecutorStreamChunk {
                        payload: Vec::new(),
                        error: Some(Arc::new(error)),
                    })
                    .await;
            }
        });
        ExecutorStreamResponse {
            headers,
            chunks: receiver,
        }
    }
}

struct CommittedStream {
    headers: crate::sdk::pluginapi::Headers,
    buffered: Vec<ExecutorStreamChunk>,
    remaining: mpsc::Receiver<ExecutorStreamChunk>,
}

enum BootstrapFailureKind {
    Provider(PluginExecutionError),
    Empty,
}

struct BootstrapFailure {
    headers: crate::sdk::pluginapi::Headers,
    remaining: mpsc::Receiver<ExecutorStreamChunk>,
    kind: BootstrapFailureKind,
}

async fn read_stream_bootstrap(
    stream: ExecutorStreamResponse,
) -> Result<CommittedStream, BootstrapFailure> {
    let ExecutorStreamResponse {
        headers,
        mut chunks,
    } = stream;
    let mut buffered = Vec::new();
    while let Some(chunk) = chunks.recv().await {
        if let Some(error) = chunk.error {
            return Err(BootstrapFailure {
                headers,
                remaining: chunks,
                kind: BootstrapFailureKind::Provider(error),
            });
        }
        let committed = !chunk.payload.is_empty();
        buffered.push(chunk);
        if committed {
            return Ok(CommittedStream {
                headers,
                buffered,
                remaining: chunks,
            });
        }
    }
    Err(BootstrapFailure {
        headers,
        remaining: chunks,
        kind: BootstrapFailureKind::Empty,
    })
}

enum StreamStartFailure {
    Direct(GenericExecutionError),
    Bootstrap(crate::sdk::pluginapi::Headers, PluginExecutionError),
}

fn finish_stream_failure(
    failure: Option<StreamStartFailure>,
    fallback: GenericExecutionError,
) -> Result<ExecutorStreamResponse, GenericExecutionError> {
    match failure {
        Some(StreamStartFailure::Direct(error)) => Err(error),
        Some(StreamStartFailure::Bootstrap(headers, error)) => {
            Ok(stream_error_response(headers, error))
        }
        None => Err(fallback),
    }
}

fn stream_error_response(
    headers: crate::sdk::pluginapi::Headers,
    error: PluginExecutionError,
) -> ExecutorStreamResponse {
    let (sender, receiver) = mpsc::channel(1);
    sender
        .try_send(ExecutorStreamChunk {
            payload: Vec::new(),
            error: Some(error),
        })
        .expect("fresh one-slot stream error channel accepts one chunk");
    ExecutorStreamResponse {
        headers,
        chunks: receiver,
    }
}

fn drain_stream(mut chunks: mpsc::Receiver<ExecutorStreamChunk>) {
    tokio::spawn(async move { while chunks.recv().await.is_some() {} });
}

fn is_request_scoped_dyn_error(mut error: &(dyn Error + 'static)) -> bool {
    loop {
        if error
            .downcast_ref::<super::AuthError>()
            .is_some_and(super::AuthError::is_request_scoped)
            || error.downcast_ref::<RequestTerminatedError>().is_some()
        {
            return true;
        }
        let Some(source) = error.source() else {
            return false;
        };
        error = source;
    }
}

fn dyn_error_status(mut error: &(dyn Error + 'static)) -> u16 {
    loop {
        if let Some(error) = error.downcast_ref::<super::AuthError>() {
            return error.status_code();
        }
        if let Some(error) = error.downcast_ref::<RequestTerminatedError>() {
            return error.status_code();
        }
        let Some(source) = error.source() else {
            return 0;
        };
        error = source;
    }
}

#[derive(Debug)]
struct EmptyStreamError;

impl fmt::Display for EmptyStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("upstream stream closed before its first payload")
    }
}

impl Error for EmptyStreamError {}
