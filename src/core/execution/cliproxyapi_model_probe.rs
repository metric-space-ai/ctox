// Origin: CTOX
// License: AGPL-3.0-only
//! Bounded, exact-account Hi probes on the instance's existing Responses gateway.
//! The native receiver supplies real admitted consumer authority. No project,
//! meeting, browser token, caller endpoint, or inferred account identity is used.
use super::{
    cliproxyapi_claude_sdk::NativeClaudeSdkAccountReservation,
    cliproxyapi_host::{
        instance_codex_proxy_base_url, instance_codex_proxy_status, InstanceCodexProxyPhase,
    },
};
use crate::business_os::consumer_authority::AdmittedConsumerAuthority;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    future::Future,
    sync::LazyLock,
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

const DEADLINE: Duration = Duration::from_secs(20);
const CURRENT_POLL: Duration = Duration::from_millis(250);
const MAX_BODY: usize = 131_072;
static SLOTS: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProbeStatus {
    Ok,
    Failed,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProbeSource {
    Gateway,
    Upstream,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProbeFailure {
    AccountUnavailable,
    AuthorityUnavailable,
    GatewayCooldown,
    GatewayStateUnavailable,
    Auth,
    ModelNotFound,
    QuotaRateLimit,
    Provider,
    InvalidResponse,
    UnverifiedFailure,
    Transport,
    Timeout,
    ResponseTooLarge,
}
/// Only allowlisted metadata can be persisted or sent to Workjet. Never a
/// credential, private account selector, provider error body or generated text.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeModelProbe {
    pub(crate) model_id: String,
    pub(crate) checked_at_ms: i64,
    pub(crate) elapsed_ms: u64,
    pub(crate) status: ProbeStatus,
    pub(crate) source: ProbeSource,
    pub(crate) failure: Option<ProbeFailure>,
    pub(crate) http_status: Option<u16>,
    pub(crate) retry_at_ms: Option<i64>,
}
impl NativeModelProbe {
    fn unavailable(model: &str, failure: ProbeFailure, started: Instant) -> Self {
        Self {
            model_id: model.into(),
            checked_at_ms: chrono::Utc::now().timestamp_millis(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            status: ProbeStatus::Unavailable,
            source: ProbeSource::Gateway,
            failure: Some(failure),
            http_status: None,
            retry_at_ms: None,
        }
    }
}
// Holder-private transport response. It cannot cross the public result seam.
struct Reply {
    status: u16,
    selected_account: Option<String>,
    upstream_class: Option<String>,
    upstream_status: Option<u16>,
    body: Vec<u8>,
}
enum IoFailure {
    Transport,
    Authority,
    ResponseTooLarge,
}

fn classify(model: &str, account: &str, reply: Reply, started: Instant) -> NativeModelProbe {
    let mut result = NativeModelProbe::unavailable(model, ProbeFailure::UnverifiedFailure, started);
    result.http_status = Some(reply.status);
    // A valid public model name or an HTTP200 cannot acknowledge another account.
    if reply.selected_account.as_deref() != Some(account) {
        return result;
    }
    let body: Option<Value> = serde_json::from_slice(&reply.body).ok();
    if body
        .as_ref()
        .is_some_and(|body| body.pointer("/error/source") == Some(&json!("gateway")))
    {
        result.failure = Some(
            match body
                .as_ref()
                .and_then(|body| body.pointer("/error/code"))
                .and_then(Value::as_str)
            {
                Some("gateway_account_cooldown") => ProbeFailure::GatewayCooldown,
                Some("gateway_account_state_unavailable") => ProbeFailure::GatewayStateUnavailable,
                _ => ProbeFailure::AccountUnavailable,
            },
        );
        result.retry_at_ms = body
            .as_ref()
            .and_then(|body| body.pointer("/error/retry_at_ms"))
            .and_then(Value::as_i64)
            .filter(|value| (1..=9_007_199_254_740_991).contains(value));
        return result;
    }
    let Some(upstream_status) = reply.upstream_status else {
        return result;
    };
    if (200..300).contains(&reply.status) {
        if upstream_status != reply.status {
            return result;
        }
        let valid = body.as_ref().is_some_and(|body| {
            body["status"] == "completed"
                && body["model"].as_str() == Some(model)
                && body["output"].as_array().is_some_and(|items| {
                    items.iter().any(|item| {
                        item["type"] == "message"
                            && item["role"] == "assistant"
                            && item["content"].as_array().is_some_and(|parts| {
                                parts.iter().any(|part| {
                                    part["type"] == "output_text"
                                        && part["text"]
                                            .as_str()
                                            .is_some_and(|text| !text.trim().is_empty())
                                })
                            })
                    })
                })
        });
        result.source = ProbeSource::Upstream;
        result.status = if valid {
            ProbeStatus::Ok
        } else {
            ProbeStatus::Failed
        };
        result.failure = if valid {
            None
        } else {
            Some(ProbeFailure::InvalidResponse)
        };
        return result;
    }
    // Only the gateway's request-scoped recorder of a REAL upstream response
    // can classify authentication/model/quota failures. A local HTTP401/404,
    // scheduler rejection or transport502 alone is never a provider verdict.
    let failure = match (upstream_status, reply.upstream_class.as_deref()) {
        (401 | 403, Some("auth")) => ProbeFailure::Auth,
        (400 | 404, Some("unknown-model")) => ProbeFailure::ModelNotFound,
        (402 | 429, Some("quota-rate-limit")) => ProbeFailure::QuotaRateLimit,
        (400..=599, Some("network-provider")) => ProbeFailure::Provider,
        _ => return result,
    };
    result.http_status = Some(upstream_status);
    result.status = ProbeStatus::Failed;
    result.source = ProbeSource::Upstream;
    result.failure = Some(failure);
    result
}

async fn bounded_probe(
    model: &str,
    account: &str,
    started: Instant,
    deadline: Duration,
    current_poll: Duration,
    current: impl Fn() -> anyhow::Result<()>,
    operation: impl Future<Output = Result<Reply, IoFailure>>,
) -> NativeModelProbe {
    if current().is_err() {
        return NativeModelProbe::unavailable(model, ProbeFailure::AuthorityUnavailable, started);
    }
    let limit = tokio::time::sleep(deadline);
    tokio::pin!(limit, operation);
    let mut poll = tokio::time::interval(current_poll);
    loop {
        tokio::select! {
            biased;
            _ = &mut limit => return NativeModelProbe::unavailable(model, ProbeFailure::Timeout, started),
            _ = poll.tick() => {
                if current().is_err() {
                    return NativeModelProbe::unavailable(model, ProbeFailure::AuthorityUnavailable, started);
                }
            }
            reply = &mut operation => {
                if current().is_err() {
                    return NativeModelProbe::unavailable(model, ProbeFailure::AuthorityUnavailable, started);
                }
                return match reply {
                    Ok(reply) => classify(model, account, reply, started),
                    Err(IoFailure::Authority) => NativeModelProbe::unavailable(model, ProbeFailure::AuthorityUnavailable, started),
                    Err(IoFailure::Transport) => NativeModelProbe::unavailable(model, ProbeFailure::Transport, started),
                    Err(IoFailure::ResponseTooLarge) => NativeModelProbe::unavailable(model, ProbeFailure::ResponseTooLarge, started),
                };
            }
        }
    }
}

/// A native controller must capture authority from its admitted connection.
/// This function does not persist a result or manufacture transport authority.
/// It changes no default, account cooldown, affinity, credential or model list.
pub(crate) async fn check_claude_model(
    authority: &AdmittedConsumerAuthority,
    account_id: &str,
    account_revision: i64,
    model: &str,
) -> NativeModelProbe {
    let started = Instant::now();
    let reservation = match NativeClaudeSdkAccountReservation::prepare_model_check(
        authority,
        account_id,
        account_revision,
        model,
    ) {
        Ok(reservation) => reservation,
        Err(_) => {
            return NativeModelProbe::unavailable(model, ProbeFailure::AccountUnavailable, started)
        }
    };
    if instance_codex_proxy_status(authority.native_host_root()).phase
        != InstanceCodexProxyPhase::Ready
    {
        return NativeModelProbe::unavailable(model, ProbeFailure::AccountUnavailable, started);
    }
    let client = match native_http::Client::builder()
        .redirect(native_http::redirect::Policy::none())
        .no_proxy()
        .timeout(DEADLINE)
        .build()
    {
        Ok(client) => client,
        Err(_) => return NativeModelProbe::unavailable(model, ProbeFailure::Transport, started),
    };
    let account = reservation
        .selected
        .account()
        .private_local_account_id
        .clone();
    let body = json!({
        "model": model,
        "input": [{"role":"user","content":[{"type":"input_text","text":"Reply with Hi only."}]}],
        "max_output_tokens":32, "stream":false, "store":false,
        "prompt_cache_key":format!("ctox-model-check:{}", uuid::Uuid::new_v4()),
    });
    let operation = async {
        let _slot = SLOTS.acquire().await.map_err(|_| IoFailure::Transport)?;
        reservation
            .with_current_configuration(authority, |_| Ok(()))
            .map_err(|_| IoFailure::Authority)?;
        let mut response = client
            .post(format!("{}/responses", instance_codex_proxy_base_url()))
            .header("X-CTOX-Account", &account)
            .header("X-CTOX-Provider", "claude")
            .header("X-CTOX-Purpose", "model-check")
            .json(&body)
            .send()
            .await
            .map_err(|_| IoFailure::Transport)?;
        let status = response.status().as_u16();
        let selected_account = response
            .headers()
            .get("X-CTOX-Account-Selected")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let upstream_class = response
            .headers()
            .get("X-CTOX-Error-Class")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let upstream_status = response
            .headers()
            .get("X-CTOX-Upstream-Status")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|value| (100..600).contains(value));
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| IoFailure::Transport)? {
            if body.len().saturating_add(chunk.len()) > MAX_BODY {
                return Err(IoFailure::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Reply {
            status,
            selected_account,
            upstream_class,
            upstream_status,
            body,
        })
    };
    let result = bounded_probe(
        model,
        &account,
        started,
        DEADLINE.saturating_sub(started.elapsed()),
        CURRENT_POLL,
        || reservation.with_current_configuration(authority, |_| Ok(())),
        operation,
    )
    .await;
    reservation.release();
    result
}

#[cfg(test)]
#[path = "cliproxyapi_model_probe_tests.rs"]
mod tests;
