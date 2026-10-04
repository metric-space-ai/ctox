// Origin: CTOX native Meta reset/account-scope conductor guards
// License: AGPL-3.0-only
use super::*;
use crate::internal::runtime::executor::meta_executor_response::{
    meta_upstream_error, MetaHttpStatusError,
};
use std::time::{Duration, SystemTime};
const NOW: SystemTime = SystemTime::UNIX_EPOCH;
fn account(provider: &str) -> Auth {
    let mut auth = Auth::default();
    auth.id = "owned-test-account".into();
    auth.provider = provider.into();
    auth
}
fn error(status: u16, body: &[u8]) -> PluginExecutionError {
    Arc::new(meta_upstream_error(status, body, NOW))
}

#[test]
fn candidate_meta_cooldown_subscription_quota_broadens_only_the_selected_meta_account() {
    let failure = error(429, br#"{"error":{"code":"rate_limit_exceeded","message":"Subscription quota exhausted","resets_at":180}}"#);
    assert_eq!(
        provider_error_retry_policy(&account(" Meta "), "muse-test", &failure),
        (None, Some(180_000))
    );
    assert_eq!(
        provider_error_retry_policy(&account("codex"), "muse-test", &failure),
        (Some("muse-test".into()), None)
    );
    assert_eq!(plugin_error_status(&failure), 429);
}

#[test]
fn candidate_meta_cooldown_model_rate_limits_not_found_and_untyped_text_keep_scope() {
    let meta = account("meta");
    let failure = error(
        429,
        br#"{"error":{"message":"requests per minute","resets_at":60}}"#,
    );
    assert_eq!(
        provider_error_retry_policy(&meta, " muse-test ", &failure),
        (Some("muse-test".into()), Some(60_000))
    );
    let failure = error(404, b"unknown model");
    assert_eq!(
        provider_error_retry_policy(&meta, "muse-test", &failure),
        (Some("muse-test".into()), Some(300_000))
    );
    let untyped: PluginExecutionError = Arc::new(AuthError {
        code: String::new(),
        message: "subscription quota exhausted".into(),
        http_status: 429,
        retryable: true,
    });
    assert_eq!(
        provider_error_retry_policy(&meta, "muse-test", &untyped),
        (Some("muse-test".into()), None)
    );
}

#[derive(Debug)]
struct Wrapped(MetaHttpStatusError);
impl fmt::Display for Wrapped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("owned failure wrapper")
    }
}
impl std::error::Error for Wrapped {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}
#[test]
fn candidate_meta_cooldown_nested_status_preserves_provider_reset_without_text_inference() {
    let typed = meta_upstream_error(404, br#"{"error":{"resets_at":90}}"#, NOW);
    assert_eq!(typed.retry_after, Some(Duration::from_secs(90)));
    let wrapped: PluginExecutionError = Arc::new(Wrapped(typed));
    assert_eq!(
        provider_error_retry_policy(&account("meta"), "muse-test", &wrapped),
        (Some("muse-test".into()), Some(90_000))
    );
    assert_eq!(plugin_error_status(&wrapped), 404);
    let permission = error(
        403,
        br#"{"error":{"message":"subscription quota exhausted","resets_at":90}}"#,
    );
    assert_eq!(
        provider_error_retry_policy(&account("meta"), "muse-test", &permission),
        (Some("muse-test".into()), None)
    );
}
