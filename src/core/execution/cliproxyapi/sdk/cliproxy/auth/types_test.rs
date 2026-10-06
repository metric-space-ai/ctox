// ref: sdk/cliproxy/auth/types_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

use crate::internal::auth::empty::EmptyStorage;
use crate::internal::auth::models::shared_token_storage;

use super::{
    register_refresh_lead_provider, Auth, ModelState, PostAuthContext, PostAuthHook, QuotaState,
    RefreshLeadRuntime, RequestInfo,
};

struct FixedRefreshLead(Option<Duration>);

impl RefreshLeadRuntime for FixedRefreshLead {
    fn refresh_lead(&self) -> Option<Duration> {
        self.0
    }
}

#[test]
fn tool_prefix_disabled_matches_bool_string_and_legacy_keys() {
    let mut auth = Auth::default();
    assert!(!auth.tool_prefix_disabled());
    auth.metadata
        .insert("tool_prefix_disabled".into(), json!(true));
    assert!(auth.tool_prefix_disabled());
    auth.metadata
        .insert("tool_prefix_disabled".into(), json!("true"));
    assert!(auth.tool_prefix_disabled());
    auth.metadata.clear();
    auth.metadata
        .insert("tool-prefix-disabled".into(), json!(true));
    assert!(auth.tool_prefix_disabled());
    auth.metadata
        .insert("tool-prefix-disabled".into(), json!(false));
    assert!(!auth.tool_prefix_disabled());
}

#[test]
fn ensure_index_uses_credential_identity_not_config_source() {
    fn api_auth(provider: &str, source: &str, base_url: &str) -> Auth {
        let mut auth = Auth::default();
        auth.provider = provider.into();
        auth.attributes
            .insert("api_key".into(), "shared-key".into());
        auth.attributes.insert("source".into(), source.into());
        if !base_url.is_empty() {
            auth.attributes.insert("base_url".into(), base_url.into());
        }
        auth
    }
    let mut gemini = api_auth("gemini", "config:gemini[a]", "");
    let mut duplicate = api_auth("gemini", "config:gemini[b]", "");
    let mut alternate = api_auth("gemini", "config:gemini[c]", "https://alt.example.com");
    let mut compat = api_auth("bohe", "config:bohe[d]", "");
    compat
        .attributes
        .insert("compat_name".into(), "bohe".into());

    let gemini_index = gemini.ensure_index();
    assert_eq!(gemini_index.len(), 16);
    assert_eq!(gemini_index, duplicate.ensure_index());
    assert_ne!(gemini_index, alternate.ensure_index());
    assert_ne!(gemini_index, compat.ensure_index());
    assert!(!format!("{gemini:?}").contains("shared-key"));
}

#[test]
fn ensure_index_uses_oauth_type_and_absolute_clean_path() {
    let mut auth = Auth::default();
    auth.provider = "antigravity".into();
    auth.attributes
        .insert("path".into(), "nested/../test-oauth.json".into());
    auth.metadata.insert("type".into(), json!("antigravity"));

    let index = auth.ensure_index();
    assert_eq!(index.len(), 16);
    assert_eq!(index, auth.ensure_index());
}

#[test]
fn recent_snapshot_has_twenty_ordered_buckets_and_counts() {
    let now = 1_700_000_000;
    let mut auth = Auth::default();
    let empty = auth.recent_requests_snapshot(now);
    assert_eq!(empty.len(), 20);
    assert!(empty
        .iter()
        .all(|bucket| bucket.success == 0 && bucket.failed == 0));
    assert!(empty.iter().all(|bucket| !bucket.time.trim().is_empty()));

    auth.record_recent_request(now, true);
    auth.record_recent_request(now, false);
    let snapshot = auth.recent_requests_snapshot(now);
    assert_eq!((snapshot[19].success, snapshot[19].failed), (1, 1));
}

#[test]
fn bucket_advance_moves_counts_to_second_newest() {
    let now = 1_700_000_000;
    let mut auth = Auth::default();
    auth.record_recent_request(now, true);
    auth.record_recent_request(now + 600, false);

    let snapshot = auth.recent_requests_snapshot(now + 600);
    assert_eq!((snapshot[18].success, snapshot[18].failed), (1, 0));
    assert_eq!((snapshot[19].success, snapshot[19].failed), (0, 1));
}

#[test]
fn clone_shares_the_injected_storage_implementation() {
    let mut auth = Auth::default();
    auth.storage = Some(shared_token_storage(EmptyStorage::default()));

    let cloned = auth.clone();
    let original_storage = auth.storage.as_ref().expect("original storage");
    let cloned_storage = cloned.storage.as_ref().expect("cloned storage");

    assert!(std::sync::Arc::ptr_eq(original_storage, cloned_storage));
}

#[test]
fn expiration_time_preserves_key_order_formats_nesting_and_unix_units() {
    let mut auth = Auth::default();
    auth.metadata.insert("expired".into(), json!("invalid"));
    auth.metadata
        .insert("expires_at".into(), json!("2026-08-03T12:34:56.789+02:00"));
    auth.metadata
        .insert("expiry".into(), json!("2027-01-01T00:00:00Z"));
    assert_eq!(
        auth.expiration_time()
            .expect("first valid expiration")
            .to_rfc3339(),
        "2026-08-03T10:34:56.789+00:00"
    );

    auth.metadata.clear();
    auth.metadata
        .insert("token".into(), json!({"expiresAt": "2026-08-03 12:34:56"}));
    assert_eq!(
        auth.expiration_time()
            .expect("nested legacy expiration")
            .timestamp(),
        1_785_760_496
    );

    auth.metadata.clear();
    auth.metadata
        .insert("expires".into(), json!(1_700_000_000_123_i64));
    let milliseconds = auth.expiration_time().expect("unix milliseconds");
    assert_eq!(milliseconds.timestamp_millis(), 1_700_000_000_123);

    auth.metadata
        .insert("expires".into(), json!(1_700_000_000.9));
    let seconds = auth.expiration_time().expect("truncated unix seconds");
    assert_eq!(seconds.timestamp(), 1_700_000_000);

    auth.metadata.insert("expires".into(), json!(0));
    assert_eq!(
        auth.expiration_time().expect("Go zero time").to_rfc3339(),
        "0001-01-01T00:00:00+00:00"
    );
}

#[test]
fn runtime_refresh_lead_precedes_normalized_registry_and_zero_falls_back() {
    let provider = " worker-18bg-refresh-provider ";
    register_refresh_lead_provider(provider, || Some(Duration::from_secs(300)));

    let mut auth = Auth::default();
    auth.provider = "WORKER-18BG-REFRESH-PROVIDER".into();
    auth.runtime = Some(Arc::new(FixedRefreshLead(Some(Duration::from_secs(90)))));
    assert_eq!(auth.refresh_lead(), Some(Duration::from_secs(90)));

    auth.runtime = Some(Arc::new(FixedRefreshLead(Some(Duration::ZERO))));
    assert_eq!(auth.refresh_lead(), Some(Duration::from_secs(300)));

    auth.runtime = Some(Arc::new(FixedRefreshLead(None)));
    assert_eq!(auth.refresh_lead(), Some(Duration::from_secs(300)));
}

#[test]
fn clone_shares_refresh_runtime_identity_without_debugging_it() {
    let mut auth = Auth::default();
    auth.runtime = Some(Arc::new(FixedRefreshLead(Some(Duration::from_secs(42)))));
    let cloned = auth.clone();
    assert!(Arc::ptr_eq(
        auth.runtime.as_ref().expect("original runtime"),
        cloned.runtime.as_ref().expect("cloned runtime")
    ));
    let debug = format!("{auth:?}");
    assert!(debug.contains("has_runtime: true"));
    assert!(!debug.contains("42"));
}

#[test]
fn auth_default_and_nested_state_match_the_upstream_json_contract() {
    let default_value = serde_json::to_value(Auth::default()).expect("serialize default auth");
    assert_eq!(
        default_value,
        json!({
            "id": "",
            "provider": "",
            "status": "",
            "disabled": false,
            "unavailable": false,
            "quota": {
                "exceeded": false,
                "next_recover_at": "0001-01-01T00:00:00Z",
                "observed_at": "0001-01-01T00:00:00Z"
            },
            "created_at": "0001-01-01T00:00:00Z",
            "updated_at": "0001-01-01T00:00:00Z",
            "last_refreshed_at": "0001-01-01T00:00:00Z",
            "next_refresh_after": "0001-01-01T00:00:00Z",
            "next_retry_after": "0001-01-01T00:00:00Z"
        })
    );

    let mut auth = Auth::default();
    auth.id = "account-public-id".into();
    auth.provider = "claude".into();
    auth.index = "runtime-index".into();
    auth.file_name = "credential.json".into();
    auth.success = 7;
    auth.failed = 3;
    auth.quota = QuotaState {
        exceeded: true,
        reason: "rate limit".into(),
        next_recover_at: chrono::DateTime::from_timestamp(1_700_000_100, 0).expect("quota time"),
        backoff_level: 2,
        ..QuotaState::default()
    };
    auth.model_states.insert(
        "claude-opus".into(),
        ModelState {
            status: super::AuthStatus::Error,
            status_message: "cooling".into(),
            unavailable: true,
            next_retry_after: chrono::DateTime::from_timestamp(1_700_000_200, 0)
                .expect("retry time"),
            last_error: Some(super::AuthError {
                code: "quota".into(),
                message: "safe summary".into(),
                retryable: true,
                http_status: 429,
            }),
            quota: auth.quota.clone(),
            updated_at: chrono::DateTime::from_timestamp(1_700_000_050, 0).expect("updated time"),
        },
    );

    let value = serde_json::to_value(&auth).expect("serialize populated auth");
    for skipped in [
        "index",
        "file_name",
        "storage",
        "runtime",
        "success",
        "failed",
    ] {
        assert!(value.get(skipped).is_none(), "unexpected {skipped} field");
    }
    assert_eq!(value["quota"]["backoff_level"], 2);
    assert_eq!(
        value["model_states"]["claude-opus"]["last_error"]["http_status"],
        429
    );

    let roundtrip: Auth = serde_json::from_value(value).expect("deserialize auth state");
    assert_eq!(roundtrip.quota, auth.quota);
    assert_eq!(roundtrip.model_states, auth.model_states);
    assert!(roundtrip.index.is_empty());
    assert!(roundtrip.file_name.is_empty());
    assert_eq!((roundtrip.success, roundtrip.failed), (0, 0));
}

#[test]
fn clone_deep_copies_json_and_model_state_but_shares_injected_owners() {
    let mut auth = Auth::default();
    auth.metadata
        .insert("nested".into(), json!({"token": {"expires": 123}}));
    auth.model_states.insert(
        "model".into(),
        ModelState {
            last_error: Some(super::AuthError {
                message: "original".into(),
                ..super::AuthError::default()
            }),
            quota: QuotaState {
                reason: "original quota".into(),
                ..QuotaState::default()
            },
            ..ModelState::default()
        },
    );
    auth.storage = Some(shared_token_storage(EmptyStorage::default()));
    auth.runtime = Some(Arc::new(FixedRefreshLead(Some(Duration::from_secs(42)))));

    let mut cloned = auth.clone();
    cloned
        .metadata
        .get_mut("nested")
        .expect("cloned nested metadata")["token"]["expires"] = json!(999);
    let cloned_model = cloned.model_states.get_mut("model").expect("cloned model");
    cloned_model.quota.reason = "changed quota".into();
    cloned_model
        .last_error
        .as_mut()
        .expect("cloned error")
        .message = "changed".into();

    assert_eq!(auth.metadata["nested"]["token"]["expires"], 123);
    assert_eq!(auth.model_states["model"].quota.reason, "original quota");
    assert_eq!(
        auth.model_states["model"]
            .last_error
            .as_ref()
            .expect("original error")
            .message,
        "original"
    );
    assert!(Arc::ptr_eq(
        auth.storage.as_ref().expect("original storage"),
        cloned.storage.as_ref().expect("cloned storage")
    ));
    assert!(Arc::ptr_eq(
        auth.runtime.as_ref().expect("original runtime"),
        cloned.runtime.as_ref().expect("cloned runtime")
    ));
}

#[test]
fn post_auth_context_is_copy_on_derive_and_hook_receives_typed_request_info() {
    let parent = PostAuthContext::default();
    let child = parent.with_request_info(RequestInfo {
        query: [("tenant".into(), vec!["team-a".into()])].into(),
        headers: [("x-request-id".into(), vec!["request-1".into()])].into(),
    });
    assert!(parent.request_info().is_none());

    let hook: PostAuthHook = Arc::new(|context, auth| {
        let request = context.request_info().expect("typed request info");
        auth.label = format!(
            "{}:{}",
            request.query["tenant"][0], request.headers["x-request-id"][0]
        );
        Ok(())
    });
    let mut auth = Auth::default();
    hook(&child, &mut auth).expect("post-auth hook");
    assert_eq!(auth.label, "team-a:request-1");
}
