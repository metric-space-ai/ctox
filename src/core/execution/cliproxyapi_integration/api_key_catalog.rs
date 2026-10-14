// Origin: CTOX
// License: AGPL-3.0-only
//! Exact-account API catalog read. The stored endpoint is fixed; model IDs and
//! error classes come only from the received response, not configuration defaults.
use super::{load_provider_integration_config, ApiKeyAccountConfig};
use crate::coding_agents::pi_sidecar::NativeModelCatalogObservation;
use std::{
    io::Read,
    path::Path,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
struct Captured {
    account: ApiKeyAccountConfig,
    binding: String,
    version: String,
    key: Zeroizing<String>,
}
impl Captured {
    fn same(&self, other: &Self) -> bool {
        self.account == other.account
            && self.binding == other.binding
            && self.version == other.version
            && self.key.as_str() == other.key.as_str()
    }
}
fn failed(provider: &str, reason: &str) -> NativeModelCatalogObservation {
    NativeModelCatalogObservation {
        provider: provider.into(),
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
fn capture(root: &Path, provider: &str, id: &str) -> anyhow::Result<Option<Captured>> {
    let config = load_provider_integration_config(root)?;
    let Some(account) = config.config.api_key_accounts.into_iter().find(|a| {
        a.id == id
            && a.endpoint.provider() == provider
            && a.owner_user_id.is_some()
            && a.creation_operation.is_some()
    }) else {
        return Ok(None);
    };
    account.validate()?;
    let Some(version) = crate::secrets::secret_record_content_version(
        root,
        &account.api_key_secret.scope,
        &account.api_key_secret.name,
    )?
    else {
        return Ok(None);
    };
    let Some(binding) =
        crate::execution::cliproxyapi_host::account_controls::account_binding(root, provider, id)?
    else {
        return Ok(None);
    };
    let key = match crate::secrets::read_secret_value(
        root,
        &account.api_key_secret.scope,
        &account.api_key_secret.name,
    ) {
        Ok(key) => Zeroizing::new(key),
        Err(_) => return Ok(None),
    };
    if key.is_empty()
        || key.len() > 8192
        || key.trim() != key.as_str()
        || key.chars().any(char::is_control)
    {
        return Ok(None);
    }
    if crate::secrets::secret_record_content_version(
        root,
        &account.api_key_secret.scope,
        &account.api_key_secret.name,
    )?
    .as_ref()
        != Some(&version)
        || load_provider_integration_config(root)?
            .config
            .api_key_accounts
            .iter()
            .find(|a| a.id == id)
            != Some(&account)
    {
        return Ok(None);
    }
    Ok(Some(Captured {
        account,
        binding,
        version,
        key,
    }))
}
fn fetch(value: &Captured) -> NativeModelCatalogObservation {
    let provider = value.account.endpoint.provider();
    let start = Instant::now();
    let deadline = Duration::from_secs(8);
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .try_proxy_from_env(false)
        .timeout_connect(deadline)
        .timeout(deadline)
        .build();
    let authorization = Zeroizing::new(format!("Bearer {}", value.key.as_str()));
    let response = agent
        .get(&format!("{}/models", value.account.endpoint.base_url()))
        .set("Authorization", &authorization)
        .set("Accept", "application/json")
        .set("User-Agent", "CTOX/provider-account-discovery")
        .call();
    let response = match response {
        Ok(r) | Err(ureq::Error::Status(_, r)) => r,
        Err(ureq::Error::Transport(_)) => {
            let mut r = failed(provider, "transport_failed");
            r.elapsed_ms = start.elapsed().as_millis() as u64;
            return r;
        }
    };
    let status = response.status();
    let retry = response
        .header("Retry-After")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v <= 604800);
    let mut result = failed(
        provider,
        match status {
            200 => "invalid_model_list",
            401 | 403 => "upstream_rejected",
            402 => "quota_unavailable",
            429 => "rate_limited",
            _ => "model_list_unavailable",
        },
    );
    result.http_status = Some(status);
    result.retry_after_seconds = retry;
    if status == 200 {
        let mut bytes = Vec::new();
        match response
            .into_reader()
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
        {
            Ok(_) if bytes.len() <= 1024 * 1024 => {
                result.models = super::api_key_ingress::parse_models(&bytes, &value.key);
                if serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .is_some_and(|v| v["has_more"] == true)
                {
                    result.models = None;
                }
                if result.models.is_some() {
                    result.failure = None
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
    provider: &str,
    mut read: impl FnMut() -> anyhow::Result<Option<Captured>>,
    fetch: impl FnOnce(&Captured) -> NativeModelCatalogObservation,
) -> anyhow::Result<NativeModelCatalogObservation> {
    let Some(first) = read()? else {
        return Ok(failed(provider, "account_unavailable"));
    };
    if first.account.disabled {
        return Ok(failed(provider, "account_disabled"));
    }
    if !read()?.is_some_and(|next| first.same(&next)) {
        return Ok(failed(provider, "account_changed"));
    }
    let mut result = fetch(&first);
    if !read()?.is_some_and(|next| first.same(&next)) {
        return Ok(failed(provider, "account_changed"));
    }
    result.private_binding = Some(first.binding);
    Ok(result)
}
/// Only after native owner/admission lookup; no Policy connection is held here.
/// Catalog observations are metadata, never an inference permission or Hi result.
pub(crate) fn observe(
    root: &Path,
    provider: &str,
    id: &str,
) -> anyhow::Result<NativeModelCatalogObservation> {
    observe_with(provider, || capture(root, provider, id), fetch)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::cliproxyapi_integration::{
        save_provider_integration_config, ApiKeyEndpoint, ProviderSecretRef,
    };
    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let mut config = load_provider_integration_config(root.path()).unwrap();
        crate::secrets::write_secret_record(
            root.path(),
            "provider-api-keys",
            "catalog-key",
            "synthetic-catalog-key",
            None,
            serde_json::json!({}),
        )
        .unwrap();
        config.config.api_key_accounts.push(ApiKeyAccountConfig {
            id: "api-catalog-fixture".into(),
            endpoint: ApiKeyEndpoint::Xai,
            api_key_secret: ProviderSecretRef::new("provider-api-keys", "catalog-key"),
            owner_user_id: Some("verified-owner".into()),
            creation_operation: Some(uuid::Uuid::new_v4().to_string()),
            disabled: false,
            priority: 0,
            models: Vec::new(),
        });
        save_provider_integration_config(root.path(), config.revision, &config.config).unwrap();
        root
    }
    fn observed(value: &Captured) -> NativeModelCatalogObservation {
        let mut result = failed(value.account.endpoint.provider(), "unused");
        result.failure = None;
        result.http_status = Some(200);
        result.models = Some(Vec::new());
        result
    }
    #[test]
    fn api_catalog_same_bytes_new_generation_invalidates_received_models() {
        let root = fixture();
        let result = observe_with(
            "xai",
            || capture(root.path(), "xai", "api-catalog-fixture"),
            |value| {
                crate::secrets::write_secret_record(
                    root.path(),
                    "provider-api-keys",
                    "catalog-key",
                    "synthetic-catalog-key",
                    None,
                    serde_json::json!({}),
                )
                .unwrap();
                observed(value)
            },
        )
        .unwrap();
        assert_eq!(result.failure.as_deref(), Some("account_changed"));
        assert!(
            result.models.is_none()
                && result.http_status.is_none()
                && result.private_binding.is_none()
        );
    }
    #[test]
    fn api_catalog_changed_before_io_never_invokes_transport() {
        let root = fixture();
        let mut reads = 0;
        let result = observe_with(
            "xai",
            || {
                reads += 1;
                if reads == 2 {
                    crate::secrets::write_secret_record(
                        root.path(),
                        "provider-api-keys",
                        "catalog-key",
                        "synthetic-new-key",
                        None,
                        serde_json::json!({}),
                    )
                    .unwrap();
                }
                capture(root.path(), "xai", "api-catalog-fixture")
            },
            |_| panic!("changed account must not issue request"),
        )
        .unwrap();
        assert_eq!(result.failure.as_deref(), Some("account_changed"));
    }
    #[test]
    fn api_catalog_disabled_missing_or_foreign_provider_never_invokes_transport() {
        let root = fixture();
        assert!(capture(root.path(), "kimi", "api-catalog-fixture")
            .unwrap()
            .is_none());
        let mut config = load_provider_integration_config(root.path()).unwrap();
        config.config.api_key_accounts[0].disabled = true;
        save_provider_integration_config(root.path(), config.revision, &config.config).unwrap();
        let result = observe_with(
            "xai",
            || capture(root.path(), "xai", "api-catalog-fixture"),
            |_| panic!("disabled account must not issue request"),
        )
        .unwrap();
        assert_eq!(result.failure.as_deref(), Some("account_disabled"));
    }
    #[test]
    fn api_catalog_actual_response_retains_binding_without_inference_claim() {
        let root = fixture();
        let before = load_provider_integration_config(root.path()).unwrap();
        let result = observe_with(
            "xai",
            || capture(root.path(), "xai", "api-catalog-fixture"),
            observed,
        )
        .unwrap();
        assert_eq!(result.http_status, Some(200));
        assert!(result.failure.is_none() && result.private_binding.is_some());
        assert_eq!(
            load_provider_integration_config(root.path()).unwrap(),
            before
        );
        let public = serde_json::to_string(&result).unwrap();
        assert!(!public.contains("synthetic-catalog-key"));
    }
}
