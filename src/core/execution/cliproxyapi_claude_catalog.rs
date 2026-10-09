// Origin: CTOX
// License: AGPL-3.0-only
//! Exact-account, bounded Claude OAuth live catalog discovery.
//! Metadata only: no inference authority, token refresh, cooldown or defaults.
use super::cliproxyapi_host::{load_instance_proxy_config, CtoxClaudeSecretStore};
use crate::coding_agents::pi_sidecar::NativeModelCatalogObservation;
use ctox_cliproxyapi::internal::auth::claude::{ClaudeSecretStore, ClaudeStoredCredentials};
use ctox_cliproxyapi::internal::config::ClaudeSubscriptionAccountConfig;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};

const ENDPOINT: &str = "https://api.anthropic.com/v1/models?limit=1000";
const DEADLINE: Duration = Duration::from_secs(8);
const MAX_BODY: u64 = 131_072;
const MAX_MODELS: usize = 1024;

// No Debug/Serialize: credentials and local selectors remain holder-private.
#[derive(PartialEq, Eq)]
struct Captured {
    account: ClaudeSubscriptionAccountConfig,
    credentials: ClaudeStoredCredentials,
}

fn capture(root: &Path, id: &str) -> anyhow::Result<Option<Captured>> {
    let Some(config) = load_instance_proxy_config(root)? else {
        return Ok(None);
    };
    let Some(account) = config
        .runtime
        .claude_accounts
        .into_iter()
        .find(|a| a.id == id)
    else {
        return Ok(None);
    };
    let handles = account
        .credential_handles()
        .map_err(|_| anyhow::anyhow!("Claude credential reference is unavailable"))?;
    let credentials = match CtoxClaudeSecretStore::new(root).load_credentials(&handles) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    Ok(Some(Captured {
        account,
        credentials,
    }))
}

fn fingerprint(value: &Captured) -> anyhow::Result<String> {
    let mut digest = Sha256::new();
    digest.update(b"ctox/native-claude-catalog-binding/v1");
    let config = serde_json::to_vec(&value.account)?;
    digest.update((config.len() as u64).to_be_bytes());
    digest.update(config);
    for secret in [
        value.credentials.access_token(),
        value.credentials.refresh_token(),
    ] {
        let secret = secret.expose_secret();
        digest.update((secret.len() as u64).to_be_bytes());
        digest.update(secret.as_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Holder-private adoption binding; neither model health nor an execution permit.
pub(crate) fn account_binding(root: &Path, id: &str) -> anyhow::Result<Option<String>> {
    let Some(first) = capture(root, id)? else {
        return Ok(None);
    };
    if capture(root, id)?.as_ref() != Some(&first) {
        return Ok(None);
    }
    fingerprint(&first).map(Some)
}

fn failed(reason: &str) -> NativeModelCatalogObservation {
    NativeModelCatalogObservation {
        provider: "claude".into(),
        checked_at_ms: chrono::Utc::now().timestamp_millis(),
        models: None,
        http_status: None,
        elapsed_ms: 0,
        retry_after_seconds: None,
        failure: Some(reason.into()),
        private_binding: None,
        inherited_selected_model: None,
    }
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelEntry>,
    #[serde(default)]
    has_more: bool,
}
#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

fn decode(bytes: &[u8], access: &str, refresh: &str) -> Option<Vec<String>> {
    let list: ModelList = serde_json::from_slice(bytes).ok()?;
    if list.has_more || list.data.is_empty() || list.data.len() > MAX_MODELS {
        return None;
    }
    let mut models = BTreeSet::new();
    for entry in list.data {
        let id = entry.id;
        if id.is_empty()
            || id.len() > 160
            || id.trim() != id
            || id.chars().any(char::is_control)
            || (!access.is_empty() && id.contains(access))
            || (!refresh.is_empty() && id.contains(refresh))
        {
            return None;
        }
        models.insert(id);
    }
    Some(models.into_iter().collect())
}

fn fetch(value: &Captured) -> NativeModelCatalogObservation {
    if value.account.disabled {
        return failed("account_disabled");
    }
    // Never disclose OAuth tokens to an override/proxy or a redirect. Discovery
    // is supported only for the official Anthropic account configured here.
    if value.account.upstream_scheme != "https"
        || value.account.upstream_authority != "api.anthropic.com"
        || value.account.proxy_url_secret.is_some()
    {
        return failed("unsupported_model_list_endpoint");
    }
    let access = value.credentials.access_token().expose_secret();
    if access.trim().is_empty() || access.len() > 8192 || access.chars().any(char::is_control) {
        return failed("credential_unavailable");
    }
    let start = Instant::now();
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .try_proxy_from_env(false)
        .timeout_connect(DEADLINE)
        .timeout(DEADLINE)
        .build();
    let authorization = zeroize::Zeroizing::new(format!("Bearer {access}"));
    let response = agent
        .get(ENDPOINT)
        .set("Accept", "application/json")
        .set("Authorization", &authorization)
        .set("anthropic-version", "2023-06-01")
        .set("anthropic-beta", "oauth-2025-04-20")
        .set("User-Agent", "CTOX-Native-Model-Catalog")
        .call();
    let response = match response {
        Ok(value) | Err(ureq::Error::Status(_, value)) => value,
        Err(ureq::Error::Transport(_)) => {
            let mut result = failed("transport_failed");
            result.elapsed_ms = start.elapsed().as_millis() as u64;
            return result;
        }
    };
    let status = response.status();
    let mut result = failed(match status {
        200 => "invalid_model_list",
        401 | 403 => "upstream_rejected",
        402 => "quota_unavailable",
        429 => "rate_limited",
        _ => "model_list_unavailable",
    });
    result.http_status = Some(status);
    result.retry_after_seconds = response
        .header("Retry-After")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value <= 604_800);
    if status == 200 {
        let mut bytes = Vec::new();
        match response
            .into_reader()
            .take(MAX_BODY + 1)
            .read_to_end(&mut bytes)
        {
            Ok(_) if bytes.len() as u64 <= MAX_BODY => {
                result.models = decode(
                    &bytes,
                    access,
                    value.credentials.refresh_token().expose_secret(),
                );
                if result.models.is_some() {
                    result.failure = None;
                }
            }
            Ok(_) => {}
            Err(_) => result.failure = Some("transport_failed".into()),
        }
    }
    result.elapsed_ms = start.elapsed().as_millis() as u64;
    result
}

fn observe_with(
    mut read: impl FnMut() -> anyhow::Result<Option<Captured>>,
    fetch: impl FnOnce(&Captured) -> NativeModelCatalogObservation,
) -> anyhow::Result<NativeModelCatalogObservation> {
    let Some(first) = read()? else {
        return Ok(failed("account_unavailable"));
    };
    if first.account.disabled {
        return Ok(failed("account_disabled"));
    }
    if read()?.as_ref() != Some(&first) {
        return Ok(failed("account_changed"));
    }
    let mut result = fetch(&first);
    if read()?.as_ref() != Some(&first) {
        return Ok(failed("account_changed"));
    }
    result.private_binding = Some(fingerprint(&first)?);
    Ok(result)
}

/// Called after Owner/Admin domain admission; all network work is outside the
/// policy transaction. Re-login/config changes invalidate in-flight completion.
pub(crate) fn observe(root: &Path, id: &str) -> anyhow::Result<NativeModelCatalogObservation> {
    observe_with(|| capture(root, id), fetch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_cliproxyapi::internal::auth::claude::SecretString;
    fn account(secret: &str) -> Captured {
        Captured {
            account: serde_json::from_value(serde_json::json!({
                "id":"claude-test","access_token_secret":{"scope":"test","name":"access"},
                "refresh_token_secret":{"scope":"test","name":"refresh"}
            }))
            .unwrap(),
            credentials: ClaudeStoredCredentials::new(
                SecretString::new(secret).unwrap(),
                SecretString::new("test-refresh").unwrap(),
            ),
        }
    }
    fn observed(_: &Captured) -> NativeModelCatalogObservation {
        let mut result = failed("unused");
        result.failure = None;
        result.http_status = Some(200);
        result.models = Some(vec!["claude-opus-5-5".into()]);
        result
    }
    #[test]
    fn changed_credentials_before_request_never_disclose_captured_token() {
        let mut reads = 0;
        let result = observe_with(
            || {
                reads += 1;
                Ok(Some(account(if reads == 1 { "first" } else { "second" })))
            },
            |_| panic!("must not fetch changed account"),
        )
        .unwrap();
        assert_eq!(result.failure.as_deref(), Some("account_changed"));
        assert!(result.models.is_none());
    }
    #[test]
    fn relogin_during_request_discards_old_catalog_and_binding() {
        let mut reads = 0;
        let result = observe_with(
            || {
                reads += 1;
                Ok(Some(account(if reads <= 2 { "first" } else { "second" })))
            },
            observed,
        )
        .unwrap();
        assert_eq!(result.failure.as_deref(), Some("account_changed"));
        assert!(result.models.is_none());
        assert!(result.private_binding.is_none());
        assert!(result.http_status.is_none());
    }
    #[test]
    fn current_account_produces_only_private_binding_and_live_models() {
        let result = observe_with(|| Ok(Some(account("test-access"))), observed).unwrap();
        assert_eq!(result.models.as_ref().unwrap(), &["claude-opus-5-5"]);
        assert!(result.private_binding.is_some());
        let public = serde_json::to_string(&result).unwrap();
        assert!(!public.contains("test-access"));
        assert!(!public.contains("test-refresh"));
        assert!(!public.contains("claude-test"));
        assert!(!public.contains("private_binding"));
    }
    #[test]
    fn disabled_or_missing_account_never_starts_request() {
        let result = observe_with(
            || {
                let mut value = account("test-access");
                value.account.disabled = true;
                Ok(Some(value))
            },
            |_| panic!("disabled account"),
        )
        .unwrap();
        assert_eq!(result.failure.as_deref(), Some("account_disabled"));
        let result = observe_with(|| Ok(None), |_| panic!("missing account")).unwrap();
        assert_eq!(result.failure.as_deref(), Some("account_unavailable"));
    }
    #[test]
    fn paginated_or_secret_bearing_lists_are_not_retained() {
        assert!(decode(
            br#"{"data":[{"id":"claude-opus-5-5"}],"has_more":true}"#,
            "",
            ""
        )
        .is_none());
        assert!(decode(br#"{"data":[{"id":"claude-opus-5-5"}]}"#, "opus", "").is_none());
        assert_eq!(
            decode(
                br#"{"data":[{"id":"claude-opus-5-5"},{"id":"claude-opus-5-5"}]}"#,
                "",
                ""
            )
            .unwrap(),
            ["claude-opus-5-5"]
        );
    }
    #[test]
    fn custom_endpoint_is_rejected_before_any_network_or_auth_error() {
        let mut value = account("test-access");
        value.account.upstream_authority = "example.invalid".into();
        let result = fetch(&value);
        assert_eq!(
            result.failure.as_deref(),
            Some("unsupported_model_list_endpoint")
        );
        assert!(result.http_status.is_none());
    }
}
