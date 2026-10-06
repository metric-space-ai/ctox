// ref: sdk/cliproxy/auth/types.go:175-205 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Origin: CTOX — real refresh transitions retain provider observations
// License: AGPL-3.0-only

use super::{clear_aggregated_availability, reset_model_state, update_aggregated_availability};
use crate::sdk::cliproxy::auth::types::go_zero_time;
use crate::sdk::cliproxy::auth::{Auth, AuthStatus, ModelState, QuotaState};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::collections::BTreeMap;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_791_000_000, 0).unwrap()
}
fn observed() -> QuotaState {
    QuotaState {
        exceeded: true,
        reason: "private provider reason".into(),
        next_recover_at: now() + chrono::Duration::minutes(20),
        backoff_level: 3,
        observed_at: now() - chrono::Duration::minutes(1),
        signals: BTreeMap::from([
            ("weekly_quota_remaining_percent".into(), "7%".into()),
            ("daily_quota_reset_at".into(), "2026-10-04T10:00:00Z".into()),
        ]),
    }
}
fn assert_observation(actual: &QuotaState, expected: &QuotaState) {
    assert_eq!(actual.observed_at, expected.observed_at);
    assert_eq!(actual.signals, expected.signals);
}
fn assert_clear(actual: &QuotaState) {
    assert!(!actual.exceeded);
    assert!(actual.reason.is_empty());
    assert_eq!(actual.next_recover_at, go_zero_time());
    assert_eq!(actual.backoff_level, 0);
}

#[test]
fn candidate_quota_observation_wire_keeps_go_zero_time_and_legacy_default() {
    let value = serde_json::to_value(QuotaState::default()).unwrap();
    assert_eq!(
        value,
        json!({
            "exceeded": false,
            "next_recover_at": "0001-01-01T00:00:00Z",
            "observed_at": "0001-01-01T00:00:00Z"
        })
    );
    let legacy: QuotaState = serde_json::from_value(json!({
        "exceeded": true, "reason": "quota", "backoff_level": 2
    }))
    .unwrap();
    assert_eq!(legacy.observed_at, go_zero_time());
    assert!(legacy.signals.is_empty());
    assert!(legacy.exceeded);
    assert_eq!(legacy.backoff_level, 2);
}

#[test]
fn candidate_quota_observation_persists_and_clone_owns_its_snapshot() {
    let initial = observed();
    let encoded = serde_json::to_value(&initial).unwrap();
    assert_eq!(encoded["signals"]["weekly_quota_remaining_percent"], "7%");
    assert_eq!(
        encoded["observed_at"],
        initial
            .observed_at
            .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
    );
    let decoded: QuotaState = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, initial);
    let mut clone = decoded.clone();
    clone
        .signals
        .insert("weekly_quota_remaining_percent".into(), "0%".into());
    clone.signals.remove("daily_quota_reset_at");
    clone.observed_at = now();
    assert_eq!(decoded.signals["weekly_quota_remaining_percent"], "7%");
    assert!(decoded.signals.contains_key("daily_quota_reset_at"));
    assert_observation(&decoded, &initial);
}

#[test]
fn candidate_quota_observation_debug_omits_signal_values_and_provider_reason() {
    let mut state = observed();
    state.signals.insert(
        "secret-labelled-key".into(),
        "private-provider-signal".into(),
    );
    let debug = format!("{state:?}");
    assert!(!debug.contains("private provider reason"));
    assert!(!debug.contains("secret-labelled-key"));
    assert!(!debug.contains("private-provider-signal"));
    assert!(debug.contains("signals_len: 3"));
}

#[test]
fn candidate_quota_observation_model_refresh_clears_only_cooldown() {
    let original = observed();
    let mut state = ModelState {
        status: AuthStatus::Error,
        unavailable: true,
        next_retry_after: now() + chrono::Duration::minutes(1),
        quota: original.clone(),
        ..ModelState::default()
    };
    reset_model_state(&mut state, now());
    assert_eq!(state.status, AuthStatus::Active);
    assert!(!state.unavailable);
    assert_eq!(state.next_retry_after, go_zero_time());
    assert_eq!(state.updated_at, now());
    assert_clear(&state.quota);
    assert_observation(&state.quota, &original);
}

#[test]
fn candidate_quota_observation_aggregate_transitions_preserve_account_snapshot() {
    let original = observed();
    let mut auth = Auth::default();
    auth.quota = original.clone();
    auth.model_states.insert(
        "model".into(),
        ModelState {
            status: AuthStatus::Error,
            unavailable: true,
            next_retry_after: now() + chrono::Duration::minutes(1),
            quota: QuotaState {
                exceeded: true,
                reason: "quota".into(),
                backoff_level: 5,
                next_recover_at: now() + chrono::Duration::minutes(2),
                ..QuotaState::default()
            },
            ..ModelState::default()
        },
    );
    update_aggregated_availability(&mut auth, now());
    assert!(auth.unavailable);
    assert!(auth.quota.exceeded);
    assert_eq!(auth.quota.backoff_level, 5);
    assert_observation(&auth.quota, &original);
    reset_model_state(auth.model_states.get_mut("model").unwrap(), now());
    update_aggregated_availability(&mut auth, now());
    assert!(!auth.unavailable);
    assert_clear(&auth.quota);
    assert_observation(&auth.quota, &original);
}

#[test]
fn candidate_quota_observation_empty_aggregate_and_explicit_clear_keep_snapshot() {
    let original = observed();
    let mut auth = Auth::default();
    auth.unavailable = true;
    auth.quota = original.clone();
    update_aggregated_availability(&mut auth, now());
    assert!(!auth.unavailable);
    assert_clear(&auth.quota);
    assert_observation(&auth.quota, &original);
    auth.quota.exceeded = true;
    auth.quota.backoff_level = 4;
    clear_aggregated_availability(&mut auth);
    assert_clear(&auth.quota);
    assert_observation(&auth.quota, &original);
}
