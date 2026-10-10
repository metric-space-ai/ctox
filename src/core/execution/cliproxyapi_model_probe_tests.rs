// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

// Existing authenticated Claude account GET/models, g3-claude-live-models-20261009.json.
const MODEL: &str = "claude-opus-5-5";
const ACCOUNT: &str = "holder-private-account";

fn response(status: u16, class: Option<&str>, observed: Option<u16>, body: Value) -> Reply {
    Reply {
        status,
        selected_account: Some(ACCOUNT.into()),
        upstream_class: class.map(str::to_owned),
        upstream_status: observed,
        body: serde_json::to_vec(&body).unwrap(),
    }
}
fn success() -> Reply {
    response(
        200,
        None,
        Some(200),
        json!({
            "model": MODEL, "status":"completed",
            "output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hi"}]}]
        }),
    )
}
#[test]
fn successful_check_requires_the_exact_account_real_status_model_and_output() {
    let result = classify(MODEL, ACCOUNT, success(), Instant::now());
    assert_eq!(result.status, ProbeStatus::Ok);
    assert_eq!(result.source, ProbeSource::Upstream);
    for malformed in [
        json!({"model":MODEL,"status":"completed","output":[]}),
        json!({"model":MODEL,"status":"in_progress","output":[{"type":"message"}]}),
        json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hi"}]}]}),
    ] {
        let result = classify(
            MODEL,
            ACCOUNT,
            response(200, None, Some(200), malformed),
            Instant::now(),
        );
        assert_eq!(result.status, ProbeStatus::Failed);
        assert_eq!(result.failure, Some(ProbeFailure::InvalidResponse));
    }
    let mut wrong_account = success();
    wrong_account.selected_account = Some("another-holder-account".into());
    assert_eq!(
        classify(MODEL, ACCOUNT, wrong_account, Instant::now()).status,
        ProbeStatus::Unavailable
    );
    let mut missing_witness = success();
    missing_witness.upstream_status = None;
    assert_eq!(
        classify(MODEL, ACCOUNT, missing_witness, Instant::now()).status,
        ProbeStatus::Unavailable
    );
}
#[test]
fn unobserved_http_errors_never_become_provider_or_credential_failures() {
    for status in [401, 403, 404, 429, 502, 503] {
        let result = classify(
            MODEL,
            ACCOUNT,
            response(
                status,
                None,
                None,
                json!({"error":{"message":"private-access-value","code":"model_not_found"}}),
            ),
            Instant::now(),
        );
        assert_eq!(result.source, ProbeSource::Gateway);
        assert_eq!(result.status, ProbeStatus::Unavailable);
        assert_eq!(result.failure, Some(ProbeFailure::UnverifiedFailure));
    }
}
#[test]
fn real_upstream_status_survives_a_gateway_wrapper_without_raw_error_leakage() {
    for (status, class, failure) in [
        (401, "auth", ProbeFailure::Auth),
        (403, "auth", ProbeFailure::Auth),
        (404, "unknown-model", ProbeFailure::ModelNotFound),
        (429, "quota-rate-limit", ProbeFailure::QuotaRateLimit),
        (500, "network-provider", ProbeFailure::Provider),
    ] {
        let result = classify(
            MODEL,
            ACCOUNT,
            response(
                503,
                Some(class),
                Some(status),
                json!({"error":{"message":"private-access-value","account":ACCOUNT}}),
            ),
            Instant::now(),
        );
        assert_eq!(result.status, ProbeStatus::Failed);
        assert_eq!(result.source, ProbeSource::Upstream);
        assert_eq!(result.http_status, Some(status));
        assert_eq!(result.failure, Some(failure));
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("private-access-value"));
        assert!(!serialized.contains(ACCOUNT));
    }
}
#[test]
fn local_state_and_cooldown_failures_override_any_prior_upstream_witness() {
    let deadline = 1_800_000_000_000i64;
    for (code, expected) in [
        ("gateway_account_cooldown", ProbeFailure::GatewayCooldown),
        (
            "gateway_account_state_unavailable",
            ProbeFailure::GatewayStateUnavailable,
        ),
        (
            "gateway_account_unavailable",
            ProbeFailure::AccountUnavailable,
        ),
    ] {
        let mut reply = response(503, Some("auth"), Some(401),
            json!({"error":{"source":"gateway","code":code,"retry_at_ms":deadline}}));
        reply.selected_account = None;
        let result = classify(
            MODEL,
            ACCOUNT,
            reply,
            Instant::now(),
        );
        assert_eq!(result.status, ProbeStatus::Unavailable);
        assert_eq!(result.source, ProbeSource::Gateway);
        assert_eq!(result.failure, Some(expected));
        assert_eq!(result.retry_at_ms, Some(deadline));
    }
}
#[test]
fn a_successful_gateway_response_cannot_relabel_a_different_observed_status() {
    let mut reply = success();
    reply.upstream_status = Some(401);
    assert_eq!(
        classify(MODEL, ACCOUNT, reply, Instant::now()).status,
        ProbeStatus::Unavailable
    );
}
#[tokio::test]
async fn revoked_authority_before_dispatch_never_polls_the_transport() {
    let polled = AtomicBool::new(false);
    let result = bounded_probe(
        MODEL,
        ACCOUNT,
        Instant::now(),
        Duration::from_secs(1),
        Duration::from_millis(1),
        || anyhow::bail!("revoked"),
        async {
            polled.store(true, Ordering::SeqCst);
            Ok(success())
        },
    )
    .await;
    assert_eq!(result.failure, Some(ProbeFailure::AuthorityUnavailable));
    assert!(!polled.load(Ordering::SeqCst));
}
#[tokio::test]
async fn authority_retirement_before_publication_prevents_a_green_result() {
    let checks = AtomicUsize::new(0);
    let result = bounded_probe(
        MODEL,
        ACCOUNT,
        Instant::now(),
        Duration::from_secs(1),
        Duration::from_secs(1),
        || {
            if checks.fetch_add(1, Ordering::SeqCst) < 2 {
                Ok(())
            } else {
                anyhow::bail!("retired")
            }
        },
        async { Ok(success()) },
    )
    .await;
    assert_eq!(result.status, ProbeStatus::Unavailable);
    assert_eq!(result.failure, Some(ProbeFailure::AuthorityUnavailable));
}
struct Retires(Arc<AtomicBool>);
impl Drop for Retires {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn deadline_drops_the_actual_pending_transport_instead_of_detaching_it() {
    let retired = Arc::new(AtomicBool::new(false));
    let owner = Retires(Arc::clone(&retired));
    let result = bounded_probe(
        MODEL,
        ACCOUNT,
        Instant::now(),
        Duration::from_millis(10),
        Duration::from_millis(1),
        || Ok(()),
        async {
            let _owner = owner;
            std::future::pending::<Result<Reply, IoFailure>>().await
        },
    )
    .await;
    assert_eq!(result.failure, Some(ProbeFailure::Timeout));
    assert!(retired.load(Ordering::SeqCst));
}
#[tokio::test]
async fn in_flight_revocation_reaps_the_pending_transport() {
    let retired = Arc::new(AtomicBool::new(false));
    let owner = Retires(Arc::clone(&retired));
    let checks = AtomicUsize::new(0);
    let result = bounded_probe(
        MODEL,
        ACCOUNT,
        Instant::now(),
        Duration::from_secs(1),
        Duration::from_millis(1),
        || {
            if checks.fetch_add(1, Ordering::SeqCst) < 2 {
                Ok(())
            } else {
                anyhow::bail!("revoked")
            }
        },
        async {
            let _owner = owner;
            std::future::pending::<Result<Reply, IoFailure>>().await
        },
    )
    .await;
    assert_eq!(result.failure, Some(ProbeFailure::AuthorityUnavailable));
    assert!(retired.load(Ordering::SeqCst));
}
