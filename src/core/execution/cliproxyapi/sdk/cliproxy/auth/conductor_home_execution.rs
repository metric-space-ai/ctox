// ref: sdk/cliproxy/auth/conductor_home_execution.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: bounded retry execution uses the selected instance-owned executor and scope
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeSet;
use std::fmt;

use tokio::sync::mpsc;

use crate::sdk::pluginapi::{
    ExecutorRequest, ExecutorResponse, ExecutorStreamChunk, ExecutorStreamResponse,
    PluginExecutionError,
};

use super::{
    access_token_sha256, is_claude_oauth_request_cancellation, is_request_scoped_plugin_error,
    is_unauthorized_plugin_error, stream_tail_is_availability_neutral, Auth, AuthPreparationError,
    HomeDispatchSelection, HomeRefreshError,
};
use super::{HomeAuthRuntime, HomeDispatchError, HomeSelectionRequest};

const MAX_HOME_ATTEMPTS: i32 = 64;

impl HomeAuthRuntime {
    pub async fn execute_home(
        &self,
        request: ExecutorRequest,
        session_id: &str,
        count_tokens: bool,
    ) -> Result<ExecutorResponse, HomeExecutionError> {
        let route_model = request.model.trim().to_owned();
        let mut tried = BTreeSet::new();
        let mut last_provider_error = None;
        for count in 1..=MAX_HOME_ATTEMPTS {
            let selection =
                match self.pick_selection(selection_request(&request, session_id, count)) {
                    Ok(selection) => selection,
                    Err(error) => {
                        if last_provider_error.is_some() && should_preserve_last_error(&error) {
                            return Err(last_provider_error.take().expect("checked above"));
                        }
                        return Err(HomeExecutionError::Dispatch(error));
                    }
                };
            let mut auth = selection.clone_auth_for_route(&route_model);
            if let Err(error) = prepare_selected_auth(&selection, &mut auth).await {
                selection.end("auth_prepare_failed");
                return Err(HomeExecutionError::Preparation(error));
            }
            if !tried.insert(auth.id.clone()) {
                selection.end("repeated_auth");
                return Err(last_provider_error.unwrap_or(HomeExecutionError::RepeatedAuth));
            }
            self.publish_selected_auth(&auth);
            let attempt = selection
                .attempt()
                .map_err(|_| HomeExecutionError::AttemptUnavailable)?;
            let execute = |auth: &Auth| {
                let execution = prepare_executor_request(&request, auth, selection.provider());
                let executor = selection.executor();
                async move {
                    if count_tokens {
                        executor.count_tokens(execution).await
                    } else {
                        executor.execute(execution).await
                    }
                }
            };
            let observed_fingerprint = access_token_sha256(&auth);
            let mut result = execute(&auth).await;
            if let Err(error) = &result {
                if is_claude_oauth_request_cancellation(&auth, error)
                    || is_request_scoped_plugin_error(error)
                {
                    attempt.release();
                    selection.end("request_scoped_error");
                    return Err(HomeExecutionError::Provider(error.clone()));
                }
                if is_unauthorized_plugin_error(error) {
                    self.report_home_unauthorized(
                        usage_context(&request),
                        &auth,
                        selection.provider(),
                        &route_model,
                        Some(&observed_fingerprint),
                    );
                    let refresh = self.refresh_home_selection_after_unauthorized(&selection, &auth);
                    let refreshed = match refresh {
                        Ok(refreshed) => refreshed,
                        Err(error) => {
                            attempt.release();
                            selection.end("refresh_failed");
                            return Err(HomeExecutionError::Refresh(error));
                        }
                    };
                    if let Some(refreshed) = refreshed {
                        auth = refreshed;
                        if let Err(error) = prepare_selected_auth(&selection, &mut auth).await {
                            attempt.release();
                            selection.end("auth_prepare_failed");
                            return Err(HomeExecutionError::Preparation(error));
                        }
                        self.publish_selected_auth(&auth);
                        result = execute(&auth).await;
                    }
                }
            }
            attempt.release();
            match result {
                Ok(response) => {
                    if session_id.trim().is_empty()
                        || !self.retain_selection(session_id, &route_model, selection.clone())
                    {
                        selection.end("completed");
                    }
                    return Ok(response);
                }
                Err(error) => {
                    selection.end("execution_failed");
                    last_provider_error = Some(HomeExecutionError::Provider(error));
                }
            }
        }
        Err(last_provider_error.unwrap_or(HomeExecutionError::RetryExceeded))
    }

    pub async fn execute_home_stream(
        &self,
        request: ExecutorRequest,
        session_id: &str,
    ) -> Result<ExecutorStreamResponse, HomeExecutionError> {
        let route_model = request.model.trim().to_owned();
        let selection = self
            .pick_selection(selection_request(&request, session_id, 1))
            .map_err(HomeExecutionError::Dispatch)?;
        let mut auth = selection.clone_auth_for_route(&route_model);
        if let Err(error) = prepare_selected_auth(&selection, &mut auth).await {
            selection.end("auth_prepare_failed");
            return Err(HomeExecutionError::Preparation(error));
        }
        self.publish_selected_auth(&auth);
        let attempt = selection
            .attempt()
            .map_err(|_| HomeExecutionError::AttemptUnavailable)?;
        let executor = selection.executor();
        let mut refreshed = false;
        let result = loop {
            let execution = prepare_executor_request(&request, &auth, selection.provider());
            match executor.execute_stream(execution).await {
                Ok(mut stream) => match stream.chunks.recv().await {
                    Some(first)
                        if first
                            .error
                            .as_ref()
                            .is_some_and(is_unauthorized_plugin_error)
                            && !refreshed =>
                    {
                        self.report_home_unauthorized(
                            usage_context(&request),
                            &auth,
                            selection.provider(),
                            &route_model,
                            Some(&access_token_sha256(&auth)),
                        );
                        let refresh =
                            self.refresh_home_selection_after_unauthorized(&selection, &auth);
                        let updated = match refresh {
                            Ok(updated) => updated,
                            Err(error) => {
                                attempt.release();
                                selection.end("refresh_failed");
                                return Err(HomeExecutionError::Refresh(error));
                            }
                        };
                        if let Some(updated) = updated {
                            auth = updated;
                            if let Err(error) = prepare_selected_auth(&selection, &mut auth).await {
                                attempt.release();
                                selection.end("auth_prepare_failed");
                                return Err(HomeExecutionError::Preparation(error));
                            }
                            refreshed = true;
                            self.publish_selected_auth(&auth);
                            continue;
                        }
                        stream.chunks = prefixed_stream(first, stream.chunks);
                        break stream;
                    }
                    Some(first) => {
                        stream.chunks = prefixed_stream(first, stream.chunks);
                        break stream;
                    }
                    None => break stream,
                },
                Err(error) => {
                    if is_claude_oauth_request_cancellation(&auth, &error)
                        || is_request_scoped_plugin_error(&error)
                        || refreshed
                    {
                        attempt.release();
                        selection.end("stream_start_failed");
                        return Err(HomeExecutionError::Provider(error));
                    }
                    if is_unauthorized_plugin_error(&error) {
                        self.report_home_unauthorized(
                            usage_context(&request),
                            &auth,
                            selection.provider(),
                            &route_model,
                            Some(&access_token_sha256(&auth)),
                        );
                        let refresh =
                            self.refresh_home_selection_after_unauthorized(&selection, &auth);
                        let updated = match refresh {
                            Ok(updated) => updated,
                            Err(error) => {
                                attempt.release();
                                selection.end("refresh_failed");
                                return Err(HomeExecutionError::Refresh(error));
                            }
                        };
                        if let Some(updated) = updated {
                            auth = updated;
                            if let Err(error) = prepare_selected_auth(&selection, &mut auth).await {
                                attempt.release();
                                selection.end("auth_prepare_failed");
                                return Err(HomeExecutionError::Preparation(error));
                            }
                            refreshed = true;
                            self.publish_selected_auth(&auth);
                            continue;
                        }
                    }
                    attempt.release();
                    selection.end("stream_start_failed");
                    return Err(HomeExecutionError::Provider(error));
                }
            }
        };
        let retained = !session_id.trim().is_empty()
            && self.retain_selection(session_id, &route_model, selection.clone());
        let (sender, receiver) = mpsc::channel(32);
        let ExecutorStreamResponse {
            headers,
            mut chunks,
        } = result;
        tokio::spawn(async move {
            let mut forward = true;
            while let Some(chunk) = chunks.recv().await {
                let terminal = chunk.error.is_some();
                if let Some(error) = &chunk.error {
                    let _availability_neutral = stream_tail_is_availability_neutral(error);
                }
                if forward && sender.send(chunk).await.is_err() {
                    break;
                }
                if terminal {
                    forward = false;
                }
            }
            attempt.release();
            if !retained {
                selection.end("stream_closed");
            }
        });
        Ok(ExecutorStreamResponse {
            headers,
            chunks: receiver,
        })
    }
}

async fn prepare_selected_auth(
    selection: &HomeDispatchSelection,
    auth: &mut Auth,
) -> Result<(), AuthPreparationError> {
    let Some(preparer) = selection.auth_preparer() else {
        return Ok(());
    };
    if !preparer.should_prepare(auth) {
        return Ok(());
    }
    preparer.prepare(auth).await?;
    selection.replace_auth(auth.clone());
    Ok(())
}

fn prefixed_stream(
    first: ExecutorStreamChunk,
    mut rest: mpsc::Receiver<ExecutorStreamChunk>,
) -> mpsc::Receiver<ExecutorStreamChunk> {
    let (sender, receiver) = mpsc::channel(32);
    tokio::spawn(async move {
        if sender.send(first).await.is_err() {
            return;
        }
        while let Some(chunk) = rest.recv().await {
            if sender.send(chunk).await.is_err() {
                return;
            }
        }
    });
    receiver
}

fn should_preserve_last_error(error: &HomeDispatchError) -> bool {
    matches!(
        error,
        HomeDispatchError::Status(status)
            if matches!(status.code.as_str(), "auth_not_found" | "auth_unavailable")
    )
}

fn usage_context(request: &ExecutorRequest) -> crate::sdk::cliproxy::usage::UsageContext {
    let mut context = crate::sdk::cliproxy::usage::UsageContext::default()
        .with_requested_model_alias(&request.model);
    if let Some(effort) = request
        .metadata
        .get("reasoning_effort")
        .and_then(serde_json::Value::as_str)
    {
        context = context.with_reasoning_effort(effort);
    }
    if let Some(tier) = request
        .metadata
        .get("service_tier")
        .and_then(serde_json::Value::as_str)
    {
        context = context.with_service_tier(tier);
    }
    context
}

fn selection_request(
    request: &ExecutorRequest,
    session_id: &str,
    count: i32,
) -> HomeSelectionRequest {
    HomeSelectionRequest {
        model: request.model.clone(),
        session_id: session_id.trim().to_owned(),
        request_id: request
            .metadata
            .get("request_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("home-request")
            .to_owned(),
        kind: if request.stream { "stream" } else { "request" }.to_owned(),
        headers: request
            .headers
            .iter()
            .filter_map(|(key, values)| values.first().map(|value| (key.clone(), value.clone())))
            .collect(),
        count,
        credential_policy: request
            .metadata
            .get("credential_policy")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    }
}

/// Builds one execution-local request snapshot. Auth maps and storage bytes
/// are owned clones, so provider mutation cannot alter manager or Home state.
pub fn prepare_executor_request(
    request: &ExecutorRequest,
    auth: &super::Auth,
    provider: &str,
) -> ExecutorRequest {
    let mut request = request.clone();
    // A new credential selection must never inherit a previous attempt's
    // private capability. The generic conductor rebinds from its own snapshot.
    request.resolved_model_info = None;
    request.auth_id.clone_from(&auth.id);
    request.auth_provider = provider.to_owned();
    request.auth_metadata.clone_from(&auth.metadata);
    request.auth_attributes.clone_from(&auth.attributes);
    request.storage_json = serde_json::to_vec(&auth.metadata).unwrap_or_default();
    if let Some(model) = auth.attributes.get("home_upstream_model") {
        if !model.trim().is_empty() {
            request.model = model.trim().to_owned();
        }
    }
    request
}

pub enum HomeExecutionError {
    Dispatch(HomeDispatchError),
    Preparation(AuthPreparationError),
    Provider(PluginExecutionError),
    RepeatedAuth,
    RetryExceeded,
    AttemptUnavailable,
    Refresh(HomeRefreshError),
}

impl fmt::Debug for HomeExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Dispatch(_) => "HomeExecutionError::Dispatch([REDACTED])",
            Self::Preparation(_) => "HomeExecutionError::Preparation([REDACTED])",
            Self::Provider(_) => "HomeExecutionError::Provider([REDACTED])",
            Self::RepeatedAuth => "HomeExecutionError::RepeatedAuth",
            Self::RetryExceeded => "HomeExecutionError::RetryExceeded",
            Self::AttemptUnavailable => "HomeExecutionError::AttemptUnavailable",
            Self::Refresh(_) => "HomeExecutionError::Refresh([REDACTED])",
        })
    }
}

impl fmt::Display for HomeExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Dispatch(_) => "Home auth dispatch failed",
            Self::Preparation(_) => "Home-selected auth preparation failed",
            Self::Provider(_) => "Home-selected provider execution failed",
            Self::RepeatedAuth => "Home returned a previously tried auth",
            Self::RetryExceeded => "Home execution retry budget was exhausted",
            Self::AttemptUnavailable => "Home execution attempt is unavailable",
            Self::Refresh(_) => "Home credential refresh failed",
        })
    }
}

impl std::error::Error for HomeExecutionError {}

#[allow(dead_code)]
fn _stream_chunk_is_send(_: ExecutorStreamChunk) {}
