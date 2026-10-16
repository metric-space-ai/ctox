// Origin: CTOX
// License: AGPL-3.0-only
//! Bounded exact-account inference check. Catalog/configuration is not a pass.
use super::*;
use anyhow::Context as _;
use ctox_cliproxyapi::internal::api::account_selection::{self, PinnedObservation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Probe {
    pub model_id: String,
    pub checked_at_ms: i64,
    pub elapsed_ms: u64,
    pub status: String,
    pub source: String,
    pub failure: Option<String>,
    pub http_status: Option<u16>,
    pub retry_at_ms: Option<i64>,
}
impl Probe {
    pub(crate) fn unavailable(model: &str, reason: &str) -> Self {
        Self {
            model_id: model.into(),
            checked_at_ms: chrono::Utc::now().timestamp_millis(),
            elapsed_ms: 0,
            status: "unavailable".into(),
            source: "gateway".into(),
            failure: Some(reason.into()),
            http_status: None,
            retry_at_ms: None,
        }
    }
}

/// Drive provider IO on the already admitted blocking caller, borrowing the
/// existing Tokio reactor. Entering a nested Tokio runtime would forbid the
/// synchronous owner facades; moving to an arbitrary blocking thread loses
/// their private admission. This wake-driven park has one absolute deadline.
pub(super) fn drive_native<T>(future: impl Future<Output=T>) -> Option<T> {
    if crate::persistence::require_sync_owner_context().is_err() { return None; }
    struct WakeThread(std::thread::Thread);
    impl std::task::Wake for WakeThread {
        fn wake(self: Arc<Self>) { self.0.unpark(); }
        fn wake_by_ref(self: &Arc<Self>) { self.0.unpark(); }
    }
    let waker = std::task::Waker::from(Arc::new(WakeThread(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = Box::pin(future);
    let deadline = Instant::now()+Duration::from_secs(15);
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) { return Some(value); }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() { return None; }
        std::thread::park_timeout(remaining);
    }
}

/// Kept generic over the existing Responses handler so controlled provider
/// transports exercise exactly the same pin/response invariant as production.
pub(super) async fn probe_route(
    router: &(impl OpenAiResponsesRouteHandler + ?Sized),
    provider: &str,
    account: &str,
    model: &str,
) -> Probe {
    let start = Instant::now();
    let body = serde_json::to_vec(&json!({"model":model,"stream":false,
        "input":"Reply OK.","max_output_tokens":16,"store":false}))
    .unwrap();
    let call = account_selection::with_pinned_account(
        account.into(),
        router.handle_provider_route(Some(provider), &body),
    );
    let mut probe = match tokio::time::timeout(Duration::from_secs(12), call).await {
        Err(_) => Probe::unavailable(model, "timeout"),
        Ok((response, observation)) => classify(model, account, response, observation),
    };
    probe.elapsed_ms = start.elapsed().as_millis() as u64;
    probe
}
fn classify(
    model: &str,
    account: &str,
    response: OpenAiResponsesRouteResponse,
    observed: PinnedObservation,
) -> Probe {
    let mut probe = Probe::unavailable(model, "unverified_failure");
    // These are executor facts in the exact task-local pin, never response
    // headers or caller metadata. A local/fallback success cannot pass.
    if observed.selected.as_deref() != Some(account) {
        return probe;
    }
    let Some(status) = observed.upstream_status else {
        return probe;
    };
    let OpenAiResponsesRouteResponse::Buffered(response) = response else {
        return probe;
    };
    if response.body().len() > 65_536 {
        return Probe::unavailable(model, "response_too_large");
    }
    probe.http_status = Some(status);
    probe.source = "upstream".into();
    let failure = if (200..300).contains(&status) {
        if !(200..300).contains(&response.status()) {
            return Probe::unavailable(model, "gateway_state_unavailable");
        }
        let valid = serde_json::from_slice::<Value>(response.body()).is_ok_and(|v| {
                v["object"] == "response"
                    && v["output"].is_array()
                    && v["status"]
                        .as_str()
                        .is_some_and(|s| matches!(s, "completed" | "incomplete"))
            });
        if valid {
            None
        } else {
            Some("invalid_response")
        }
    } else if matches!(status, 401 | 403) {
        Some("auth")
    } else if matches!(status, 402 | 429) {
        Some("quota_rate_limit")
    } else if matches!(status, 400 | 404) && observed.error_class == Some("unknown-model") {
        Some("model_not_found")
    } else if (400..600).contains(&status) {
        Some("provider")
    } else {
        return Probe::unavailable(model, "unverified_failure");
    };
    probe.status = if failure.is_none() { "ok" } else { "failed" }.into();
    probe.failure = failure.map(str::to_owned);
    probe
}

/// Only a native caller supplies the root/actor. The public reference resolves
/// to the current local holder; no key, endpoint or account selector is input.
pub(crate) async fn check(
    root: PathBuf,
    actor: String,
    account: String,
    revision: i64,
    model: String,
    fence: Option<crate::business_os::app_platform::ModelEffectFence>,
) -> Probe {
    use crate::business_os::provider_federation::model_probe_target;
    let Some(fence) = fence else {
        return Probe::unavailable(&model, "authority_unavailable");
    };
    let requested_model = model.clone();
    // Existing synchronous credential/cooldown adapters require this bounded
    // Core caller admission. No database transaction spans the network wait.
    crate::persistence::run_core_sync(move || {
        let value = fence(&mut || {
            let target = model_probe_target(&root, &actor, &account, revision)?;
            anyhow::ensure!(
                account_selection::valid_account_id(&target.local_account_id),
                "account has no native pin"
            );
            let config = load_instance_proxy_config(&root)?;
            let router = build_instance_codex_responses_router(&root)?
                .context("native gateway unavailable")?;
            let probe = drive_native(probe_route(
                router.as_ref(), &target.provider, &target.local_account_id, &model,
            )).unwrap_or_else(||Probe::unavailable(&model,"timeout"));
            if model_probe_target(&root, &actor, &account, revision)? != target
                || load_instance_proxy_config(&root)? != config
            {
                return Ok(serde_json::to_value(Probe::unavailable(
                    &model,
                    "account_unavailable",
                ))?);
            }
            Ok(serde_json::to_value(probe)?)
        })?;
        Ok(serde_json::from_value(value)?)
    })
    .await
    .unwrap_or_else(|_| Probe::unavailable(&requested_model, "authority_unavailable"))
}

#[cfg(test)]
#[path = "cliproxyapi_model_probe_tests.rs"]
mod tests;
