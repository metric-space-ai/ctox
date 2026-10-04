// ref: sdk/cliproxy/auth/metadata_merge_test.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox — deterministic conflict and owner-cycle guards
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use crate::sdk::cliproxy::auth::{AuthError, ModelState};
use serde_json::json;

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}
fn auth() -> Auth {
    let mut auth = Auth::default();
    auth.id = "meta-one".into();
    auth.provider = "meta".into();
    auth.registration_epoch = 7;
    auth.status = AuthStatus::Active;
    auth.metadata.insert("access_token".into(), json!("old"));
    auth.metadata.insert("notes".into(), json!("original"));
    auth
}
fn now() -> DateTime<Utc> {
    at("2026-10-04T12:00:00Z")
}
fn failure(message: &str, status: u16) -> AuthError {
    AuthError {
        code: "upstream".into(),
        message: message.into(),
        retryable: true,
        http_status: status,
    }
}

#[test]
fn candidate_merge_keeps_concurrent_settings_and_applies_rotated_token() {
    let base = auth();
    let mut current = base.clone();
    let mut updated = base.clone();
    current.metadata.insert("notes".into(), json!("user"));
    current
        .metadata
        .insert("access_token".into(), json!("concurrent-token"));
    current.label = "User label".into();
    current.attributes.insert("priority".into(), "9".into());
    updated.metadata.insert("notes".into(), json!("executor"));
    updated
        .metadata
        .insert("access_token".into(), json!("rotated"));
    updated.attributes.insert("priority".into(), "2".into());
    let merged = merge_prepared_auth(&base, &current, &updated);
    assert_eq!(merged.metadata["notes"], "user");
    assert_eq!(merged.metadata["access_token"], "rotated");
    assert_eq!(merged.label, "User label");
    assert_eq!(merged.attributes["priority"], "9");
    assert_eq!(base.metadata["access_token"], "old");
}

#[test]
fn candidate_merge_deletions_preserve_concurrently_edited_fields() {
    let mut base = auth();
    base.metadata.insert("delete".into(), json!(1));
    base.attributes.insert("keep".into(), "old".into());
    base.attributes.insert("delete".into(), "old".into());
    let mut current = base.clone();
    current.metadata.insert("notes".into(), json!("user"));
    current.attributes.insert("keep".into(), "user".into());
    let mut updated = base.clone();
    updated.metadata.clear();
    updated.attributes.clear();
    let merged = merge_prepared_auth(&base, &current, &updated);
    assert_eq!(merged.metadata["notes"], "user");
    assert!(!merged.metadata.contains_key("delete"));
    assert!(!merged.metadata.contains_key("access_token"));
    assert_eq!(merged.attributes["keep"], "user");
    assert!(!merged.attributes.contains_key("delete"));
}

#[test]
fn candidate_prepare_preserves_refresh_failures_disabling_and_cooldowns() {
    let base = auth();
    let mut current = base.clone();
    let mut updated = base.clone();
    current.disabled = true;
    current.status = AuthStatus::Disabled;
    current.last_error = Some(failure("new failure", 503));
    current.next_retry_after = at("2026-10-04T12:10:00Z");
    current.last_refreshed_at = now();
    current
        .model_states
        .insert("muse".into(), ModelState::default());
    updated.last_error = None;
    updated.last_refreshed_at = at("2026-10-04T12:05:00Z");
    updated.metadata.insert("api_key".into(), json!("minted"));
    let merged = merge_prepared_auth(&base, &current, &updated);
    assert!(merged.disabled);
    assert_eq!(merged.status, AuthStatus::Disabled);
    assert_eq!(merged.last_error.unwrap().message, "new failure");
    assert_eq!(merged.last_refreshed_at, now());
    assert_eq!(merged.next_retry_after, current.next_retry_after);
    assert!(merged.model_states.contains_key("muse"));
    assert_eq!(merged.metadata["api_key"], "minted");
}

#[test]
fn candidate_proxy_merge_honors_user_edit_and_explicit_clear_in_either_representation() {
    // base struct/meta, current struct/meta, executor struct/meta, expected
    for (bs, bm, cs, cm, us, um, expected) in [
        ("old", "old", "user", "old", "executor", "executor", "user"),
        ("old", "old", "old", "user", "executor", "executor", "user"),
        ("old", "old", "", "old", "executor", "executor", ""),
        ("old", "old", "old", "", "executor", "executor", ""),
        ("old", "old", "old", "old", "old", "executor", "executor"),
        ("old", "old", "old", "old", "executor", "old", "executor"),
        ("", "existing", "", "existing", "", "existing", "existing"),
    ] {
        let make = |structure: &str, metadata: &str| {
            let mut value = auth();
            value.proxy_url = structure.into();
            if !metadata.is_empty() {
                value.metadata.insert("proxy_url".into(), json!(metadata));
            }
            value
        };
        let merged = merge_prepared_auth(&make(bs, bm), &make(cs, cm), &make(us, um));
        assert_eq!(merged.proxy_url, expected);
        let expected_metadata = (!expected.is_empty()).then(|| json!(expected));
        assert_eq!(merged.metadata.get("proxy_url"), expected_metadata.as_ref());
    }
}

#[test]
fn candidate_prefix_merge_preserves_concurrent_user_changes() {
    let mut base = auth();
    base.prefix = " original ".into();
    let mut updated = base.clone();
    updated.prefix = " executor ".into();
    assert_eq!(
        merge_prepared_auth(&base, &base, &updated).prefix,
        "executor"
    );
    let mut current = base.clone();
    current.prefix = " user ".into();
    assert_eq!(
        merge_prepared_auth(&base, &current, &updated).prefix,
        "user"
    );
}

#[test]
fn candidate_login_metadata_never_restores_old_meta_credentials() {
    let mut target = auth();
    target.metadata.clear();
    target.metadata.insert("api_key".into(), json!("new-key"));
    let existing = BTreeMap::from([
        ("access_token".into(), json!("old")),
        ("api-key".into(), json!("old")),
        ("dca_token".into(), json!("old")),
        ("dca_expired".into(), json!("old")),
        ("dca_expires_at".into(), json!(12)),
        ("expired".into(), json!("old")),
        ("notes".into(), json!("user")),
        ("disabled".into(), json!(true)),
    ]);
    merge_existing_auth_metadata(&mut target, &existing);
    assert_eq!(target.metadata["api_key"], "new-key");
    assert_eq!(target.metadata["notes"], "user");
    assert!(target.disabled);
    for key in [
        "access_token",
        "api-key",
        "dca_token",
        "dca_expired",
        "dca_expires_at",
        "expired",
    ] {
        assert!(!target.metadata.contains_key(key));
    }
    target.disabled = false;
    target.metadata.insert("disabled".into(), json!(false));
    merge_existing_auth_metadata(&mut target, &existing);
    assert!(!target.disabled);
    assert!(is_auth_token_payload_key(" ACCESS_TOKEN "));
    assert!(!is_auth_token_payload_key("notes"));
}

#[test]
fn candidate_stale_merge_cannot_modify_reloaded_credentials() {
    let base = auth();
    let mut current = base.clone();
    current.registration_epoch += 1;
    current
        .metadata
        .insert("api_key".into(), json!("replacement"));
    let mut updated = base.clone();
    updated.metadata.insert("api_key".into(), json!("obsolete"));
    updated.last_refreshed_at = now();
    for merged in [
        merge_prepared_auth(&base, &current, &updated),
        merge_refreshed_auth(&base, &current, &updated, now()),
    ] {
        assert_eq!(merged.metadata["api_key"], "replacement");
        assert_eq!(merged.last_refreshed_at, current.last_refreshed_at);
        assert_eq!(merged.registration_epoch, current.registration_epoch);
    }
}

#[test]
fn candidate_refresh_preserves_new_concurrent_error() {
    let base = auth();
    let mut current = base.clone();
    current.last_error = Some(failure("new503", 503));
    current.status = AuthStatus::Error;
    current.unavailable = true;
    current.status_message = "new503".into();
    let mut updated = base.clone();
    updated.last_refreshed_at = now();
    updated
        .metadata
        .insert("access_token".into(), json!("fresh"));
    let merged = merge_refreshed_auth(&base, &current, &updated, now());
    assert_eq!(merged.last_error.unwrap().message, "new503");
    assert_eq!(merged.status, AuthStatus::Error);
    assert!(merged.unavailable);
    assert_eq!(merged.last_refreshed_at, now());
    assert_eq!(merged.metadata["access_token"], "fresh");
}

#[test]
fn candidate_refresh_preserves_active_quota_and_cooldown_but_recovers_expired_failures() {
    for quota in [true, false] {
        for active in [true, false] {
            let mut base = auth();
            base.last_error = Some(failure("previous401", 401));
            base.status = AuthStatus::Error;
            base.unavailable = true;
            let mut current = base.clone();
            let boundary = if active {
                now() + chrono::Duration::minutes(1)
            } else {
                now()
            };
            if quota {
                current.quota.exceeded = true;
                current.quota.reason = "credential_quota".into();
                current.quota.next_recover_at = boundary;
            } else {
                current.next_retry_after = boundary;
            }
            let mut updated = base.clone();
            updated.status = AuthStatus::Active;
            updated.last_error = None;
            let merged = merge_refreshed_auth(&base, &current, &updated, now());
            assert_eq!(merged.unavailable, active);
            assert_eq!(merged.last_error.is_some(), active);
            assert_eq!(
                merged.status,
                if active {
                    AuthStatus::Error
                } else {
                    AuthStatus::Active
                }
            );
        }
    }
}

#[test]
fn candidate_refresh_disabled_conflicts_prefer_user_and_allow_uncontested_executor_change() {
    let base = auth();
    let mut updated = base.clone();
    updated.disabled = true;
    updated.status = AuthStatus::Disabled;
    let merged = merge_refreshed_auth(&base, &base, &updated, now());
    assert!(merged.disabled);
    assert_eq!(merged.metadata["disabled"], true);
    let mut current = base.clone();
    current.disabled = true;
    current.status = AuthStatus::Disabled;
    let merged = merge_refreshed_auth(&base, &current, &base, now());
    assert!(merged.disabled);
    let mut disabled_base = base.clone();
    disabled_base.disabled = true;
    disabled_base.status = AuthStatus::Disabled;
    let merged = merge_refreshed_auth(&disabled_base, &disabled_base, &base, now());
    assert!(!merged.disabled);
    assert_eq!(merged.status, AuthStatus::Active);
}

#[test]
fn candidate_refresh_model_state_merge_preserves_new_user_cooldown() {
    let mut base = auth();
    base.model_states
        .insert("changed".into(), ModelState::default());
    base.model_states
        .insert("delete".into(), ModelState::default());
    base.model_states
        .insert("keep".into(), ModelState::default());
    let mut current = base.clone();
    current.model_states.get_mut("changed").unwrap().unavailable = true;
    current.model_states.get_mut("keep").unwrap().unavailable = true;
    let mut updated = base.clone();
    updated
        .model_states
        .get_mut("changed")
        .unwrap()
        .status_message = "executor".into();
    updated.model_states.remove("delete");
    updated.model_states.remove("keep");
    updated
        .model_states
        .insert("new".into(), ModelState::default());
    let merged = merge_refreshed_auth(&base, &current, &updated, now());
    assert!(merged.model_states["changed"].unavailable);
    assert_eq!(merged.model_states["changed"].status_message, "");
    assert!(!merged.model_states.contains_key("delete"));
    assert!(merged.model_states["keep"].unavailable);
    assert!(merged.model_states.contains_key("new"));
}

#[test]
fn candidate_refresh_can_clear_next_refresh_while_preparation_preserves_it() {
    let mut base = auth();
    base.next_refresh_after = now() + chrono::Duration::hours(1);
    base.last_refreshed_at = now();
    let mut updated = base.clone();
    updated.next_refresh_after = super::super::types::go_zero_time();
    updated.last_refreshed_at = now() + chrono::Duration::minutes(1);
    let refreshed = merge_refreshed_auth(&base, &base, &updated, now());
    assert_eq!(refreshed.next_refresh_after, updated.next_refresh_after);
    let prepared = merge_prepared_auth(&base, &base, &updated);
    assert_eq!(prepared.next_refresh_after, base.next_refresh_after);
    assert_eq!(prepared.last_refreshed_at, base.last_refreshed_at);
}

#[test]
fn candidate_normalized_legacy_settings_keep_canonical_zero_false_and_null() {
    let mut metadata = BTreeMap::from([
        ("api-key".into(), json!("legacy-secret")),
        ("base-url".into(), json!("https://regional.example")),
        ("request-retry".into(), json!(3)),
        ("request_retry".into(), json!(0)),
        ("disable-cooling".into(), json!(true)),
        ("disable_cooling".into(), json!(false)),
        (
            "model-aliases".into(),
            json!([{"name":"upstream","alias":"public"}]),
        ),
        ("model_aliases".into(), Value::Null),
        ("provider-specific-key".into(), json!({"nested":[1,2]})),
    ]);
    normalize_credential_metadata(&mut metadata);
    assert_eq!(metadata["api_key"], "legacy-secret");
    assert_eq!(metadata["base_url"], "https://regional.example");
    assert_eq!(metadata["request_retry"], 0);
    assert_eq!(metadata["disable_cooling"], false);
    assert_eq!(metadata["model_aliases"], Value::Null);
    assert_eq!(metadata["provider-specific-key"], json!({"nested":[1,2]}));
    for legacy in [
        "api-key",
        "base-url",
        "request-retry",
        "disable-cooling",
        "model-aliases",
    ] {
        assert!(!metadata.contains_key(legacy));
    }
    let snapshot = metadata.clone();
    normalize_credential_metadata(&mut metadata);
    assert_eq!(metadata, snapshot);
}

#[test]
fn candidate_canonical_keys_preserve_unknown_provider_key_spelling() {
    for key in ["API_KEY", " api-key ", "provider-specific-key"] {
        assert_eq!(canonical_credential_metadata_key(key), key);
    }
    let mut metadata = BTreeMap::from([
        ("API_KEY".into(), json!("provider data")),
        (" api-key ".into(), json!(false)),
    ]);
    let snapshot = metadata.clone();
    normalize_credential_metadata(&mut metadata);
    assert_eq!(metadata, snapshot);
}

#[test]
fn candidate_registration_epoch_is_not_a_persisted_or_client_json_authority() {
    let base = auth();
    let value = serde_json::to_value(&base).unwrap();
    assert!(value.get("registration_epoch").is_none());
    let mut value = value;
    value["registration_epoch"] = json!(999);
    let restored: Auth = serde_json::from_value(value).unwrap();
    assert_eq!(restored.registration_epoch, 0);
    assert_eq!(restored.metadata["access_token"], "old");
}
