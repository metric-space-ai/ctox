// Origin: CTOX
// License: AGPL-3.0-only
//! Holder-private preparation and commit. The caller supplies its retained
//! native Owner/Admin authority, never an identity supplied by the renderer.
//! Credentials never enter a returned receipt or a durable command payload.
use super::{ApiKeyAccountConfig, ApiKeyEndpoint, ProviderSecretRef};
use ctox_cliproxyapi::internal::runtime::executor::api_key_transport::ApiKeyHttpClient;
use ctox_cliproxyapi::sdk::pluginapi::{Headers, HostHttpClient, HttpRequest};
use serde::Serialize;
use std::{fmt, path::Path, sync::Arc, time::Duration};
use zeroize::Zeroizing;

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApiKeyProvider {
    Zai,
    #[serde(rename = "minimax")]
    MiniMax,
    Xai,
    Kimi,
}
impl ApiKeyProvider {
    fn provider(self) -> &'static str {
        match self {
            Self::Zai => "zai",
            Self::MiniMax => "minimax",
            Self::Xai => "xai",
            Self::Kimi => "kimi",
        }
    }
    fn endpoints(self) -> &'static [ApiKeyEndpoint] {
        use ApiKeyEndpoint::*;
        match self {
            Self::Zai => &[ZaiCoding, ZaiPlatform],
            Self::MiniMax => &[MiniMax],
            Self::Xai => &[Xai],
            Self::Kimi => &[
                KimiCoding,
                KimiCodingGlobal,
                KimiPlatformGlobal,
                KimiPlatformChina,
            ],
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CatalogFailure {
    CredentialFormat,
    AuthorityRetired,
    Transport,
    Timeout,
    CredentialsRejected,
    AccessDenied,
    RateLimited,
    CatalogUnavailable,
    ProviderUnavailable,
    InvalidCatalog,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EndpointAttempt {
    pub endpoint: ApiKeyEndpoint,
    pub http_status: Option<u16>,
    pub failure: Option<CatalogFailure>,
}
#[derive(Debug, Serialize)]
pub(crate) struct DiscoveryError {
    pub reason: CatalogFailure,
    pub attempts: Vec<EndpointAttempt>,
}
impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "API account discovery failed ({:?}); checked",
            self.reason
        )?;
        for a in &self.attempts {
            write!(
                f,
                " {} /models: {:?} {:?};",
                a.endpoint.base_url(),
                a.http_status,
                a.failure
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for DiscoveryError {}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoveredAccount {
    pub endpoint: ApiKeyEndpoint,
    pub models: Vec<String>,
    pub attempts: Vec<EndpointAttempt>,
}
fn failure(reason: CatalogFailure) -> DiscoveryError {
    DiscoveryError {
        reason,
        attempts: Vec::new(),
    }
}
fn parse_models(body: &[u8], key: &str) -> Option<Vec<String>> {
    if body.len() > 1024 * 1024 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let data = value.get("data")?.as_array()?;
    if data.len() > 256 {
        return None;
    }
    let mut seen = std::collections::HashSet::new();
    let mut models = Vec::new();
    for entry in data {
        let id = entry.get("id")?.as_str()?;
        if id.is_empty()
            || id.len() > 256
            || id.trim() != id
            || id.chars().any(char::is_control)
            || id.contains(key)
        {
            return None;
        }
        if seen.insert(id.to_owned()) {
            models.push(id.to_owned());
        }
    }
    Some(models)
}
fn http_failure(status: u16) -> CatalogFailure {
    match status {
        401 => CatalogFailure::CredentialsRejected,
        403 => CatalogFailure::AccessDenied,
        429 => CatalogFailure::RateLimited,
        404 => CatalogFailure::CatalogUnavailable,
        500..=599 => CatalogFailure::ProviderUnavailable,
        _ => CatalogFailure::CatalogUnavailable,
    }
}
pub(crate) async fn discover_api_key_account(
    provider: ApiKeyProvider,
    key: &str,
    http: Arc<dyn HostHttpClient>,
    current: impl Fn() -> anyhow::Result<()> + Sync,
) -> Result<DiscoveredAccount, DiscoveryError> {
    current().map_err(|_| failure(CatalogFailure::AuthorityRetired))?;
    if key.is_empty() || key.len() > 8192 || key.trim() != key || key.chars().any(char::is_control)
    {
        return Err(failure(CatalogFailure::CredentialFormat));
    }
    let calls = provider.endpoints().iter().copied().map(|endpoint| {
        let http = http.clone();
        async move {
            let mut headers = Headers::new();
            headers.insert("Authorization".into(), vec![format!("Bearer {key}")]);
            headers.insert("Accept".into(), vec!["application/json".into()]);
            // Real client identity, not a fabricated Kimi/Claude/Codex identity.
            headers.insert(
                "User-Agent".into(),
                vec!["CTOX/provider-account-discovery".into()],
            );
            let response = tokio::time::timeout(
                Duration::from_secs(8),
                http.execute(HttpRequest {
                    method: "GET".into(),
                    url: format!("{}/models", endpoint.base_url()),
                    headers,
                    body: Vec::new(),
                }),
            )
            .await;
            let (status, models, error) = match response {
                Err(_) => (None, None, Some(CatalogFailure::Timeout)),
                Ok(Err(_)) => (None, None, Some(CatalogFailure::Transport)),
                Ok(Ok(r)) if r.status_code == 200 => match parse_models(&r.body, key) {
                    Some(ids) => (Some(200), Some(ids), None),
                    None => (Some(200), None, Some(CatalogFailure::InvalidCatalog)),
                },
                Ok(Ok(r)) => (Some(r.status_code), None, Some(http_failure(r.status_code))),
            };
            (
                EndpointAttempt {
                    endpoint,
                    http_status: status,
                    failure: error,
                },
                models,
            )
        }
    });
    let watch = async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if current().is_err() {
                return failure(CatalogFailure::AuthorityRetired);
            }
        }
    };
    let rows = tokio::select! {
        rows=futures_util::future::join_all(calls)=>rows,
        retired=watch=>return Err(retired),
    };
    current().map_err(|_| failure(CatalogFailure::AuthorityRetired))?;
    // Every fixed endpoint is tested. A successful coding plan takes precedence;
    // there is no prefix guess, user plan choice or endpoint fallback at inference.
    let selected = rows
        .iter()
        .find_map(|(a, ids)| ids.as_ref().map(|ids| (a.endpoint, ids.clone())));
    let attempts = rows.into_iter().map(|(a, _)| a).collect();
    match selected {
        Some((endpoint, models)) => Ok(DiscoveredAccount {
            endpoint,
            models,
            attempts,
        }),
        None => Err(DiscoveryError {
            reason: CatalogFailure::CatalogUnavailable,
            attempts,
        }),
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledApiAccount {
    pub id: String,
    pub endpoint: ApiKeyEndpoint,
    pub models: Vec<String>,
    pub revision: u64,
}
/// The owner is resolved from current native authority by the caller. Operation
/// IDs are scoped to that owner; replay never rotates a key or creates a duplicate.
pub(crate) async fn install_api_key_account_owned_authorized(
    root: &Path,
    owner: &str,
    operation: uuid::Uuid,
    provider: ApiKeyProvider,
    key: Zeroizing<String>,
    current: impl Fn() -> anyhow::Result<()> + Sync,
) -> anyhow::Result<InstalledApiAccount> {
    use sha2::{Digest, Sha256};
    current()?;
    anyhow::ensure!(
        !owner.is_empty()
            && owner.len() <= 512
            && owner.trim() == owner
            && !owner.chars().any(char::is_control),
        "native management owner unavailable"
    );
    let operation = operation.to_string();
    let mut digest = Sha256::new();
    digest.update(owner.as_bytes());
    digest.update([0]);
    digest.update(operation.as_bytes());
    let id = format!("api-{:x}", digest.finalize());
    {
        let _lifecycle = crate::secrets::credential_lifecycle_guard();
        current()?;
        let stored = super::load_provider_integration_config(root)?;
        if let Some(existing) = stored.config.api_key_accounts.iter().find(|a| a.id == id) {
            anyhow::ensure!(
                existing.owner_user_id.as_deref() == Some(owner)
                    && existing.creation_operation.as_deref() == Some(operation.as_str())
                    && existing.endpoint.provider() == provider.provider(),
                "API creation operation conflicts"
            );
            let version = crate::secrets::secret_record_content_version(
                root,
                &existing.api_key_secret.scope,
                &existing.api_key_secret.name,
            )?;
            let existing_key = Zeroizing::new(crate::secrets::read_secret_value(
                root,
                &existing.api_key_secret.scope,
                &existing.api_key_secret.name,
            )?);
            anyhow::ensure!(
                existing_key.as_str() == key.as_str(),
                "API creation operation changed its credential"
            );
            anyhow::ensure!(
                version.is_some()
                    && crate::secrets::secret_record_content_version(
                        root,
                        &existing.api_key_secret.scope,
                        &existing.api_key_secret.name
                    )? == version
                    && super::load_provider_integration_config(root)?
                        .config
                        .api_key_accounts
                        .iter()
                        .any(|a| a == existing),
                "API account changed during operation replay"
            );
            current()?;
            return Ok(InstalledApiAccount {
                id: existing.id.clone(),
                endpoint: existing.endpoint,
                models: existing.models.clone(),
                revision: stored.revision,
            });
        }
    }
    let http = ApiKeyHttpClient::new(None, Duration::from_secs(8))
        .map_err(|_| anyhow::anyhow!("API discovery transport unavailable"))?;
    let found = discover_api_key_account(provider, &key, Arc::new(http), &current).await?;
    commit_discovered_record(root, &key, found, &current, id, Some((owner, &operation)))
}
/// No Policy transaction or lifecycle mutex may be held by the caller across

/// discovery. The current-authority callback must be bounded and read-only.
/// This private seam is not yet a renderer RPC or a public account projection.
pub(crate) async fn install_api_key_account_authorized(
    root: &Path,
    provider: ApiKeyProvider,
    key: Zeroizing<String>,
    current: impl Fn() -> anyhow::Result<()> + Sync,
) -> anyhow::Result<InstalledApiAccount> {
    let http = ApiKeyHttpClient::new(None, Duration::from_secs(8))
        .map_err(|_| anyhow::anyhow!("API discovery transport is unavailable"))?;
    let found = discover_api_key_account(provider, &key, Arc::new(http), &current).await?;
    commit_discovered(root, &key, found, &current)
}
fn commit_discovered(
    root: &Path,
    key: &str,
    found: DiscoveredAccount,
    current: impl Fn() -> anyhow::Result<()>,
) -> anyhow::Result<InstalledApiAccount> {
    commit_discovered_with_id(root, key, found, current, uuid::Uuid::new_v4())
}
fn commit_discovered_with_id(
    root: &Path,
    key: &str,
    found: DiscoveredAccount,
    current: impl Fn() -> anyhow::Result<()>,
    id: uuid::Uuid,
) -> anyhow::Result<InstalledApiAccount> {
    commit_discovered_record(root, key, found, current, id.to_string(), None)
}
fn commit_discovered_record(
    root: &Path,
    key: &str,
    found: DiscoveredAccount,
    current: impl Fn() -> anyhow::Result<()>,
    id: String,
    creation: Option<(&str, &str)>,
) -> anyhow::Result<InstalledApiAccount> {
    let _lifecycle = crate::secrets::credential_lifecycle_guard();
    current()?;
    let mut stored = super::load_provider_integration_config(root)?;
    if let Some(existing) = stored.config.api_key_accounts.iter().find(|a| a.id == id) {
        anyhow::ensure!(
            creation.is_some_and(
                |(owner, op)| existing.owner_user_id.as_deref() == Some(owner)
                    && existing.creation_operation.as_deref() == Some(op)
            ) && existing.endpoint == found.endpoint,
            "API creation operation conflicts with an existing account"
        );
        let existing_key = Zeroizing::new(crate::secrets::read_secret_value(
            root,
            &existing.api_key_secret.scope,
            &existing.api_key_secret.name,
        )?);
        anyhow::ensure!(
            existing_key.as_str() == key,
            "API creation operation changed its credential"
        );
        current()?;
        return Ok(InstalledApiAccount {
            id: existing.id.clone(),
            endpoint: existing.endpoint,
            models: existing.models.clone(),
            revision: stored.revision,
        });
    }
    let secret = ProviderSecretRef::new("provider-api-keys", format!("api-key-{id}"));
    anyhow::ensure!(
        !crate::secrets::secret_exists(root, &secret.scope, &secret.name)?,
        "new API credential name already exists"
    );
    let account = ApiKeyAccountConfig {
        id: id.clone(),
        endpoint: found.endpoint,
        api_key_secret: secret.clone(),
        owner_user_id: creation.map(|(owner, _)| owner.to_owned()),
        creation_operation: creation.map(|(_, op)| op.to_owned()),
        disabled: false,
        priority: 0,
        models: found.models.clone(),
    };
    account
        .validate()
        .map_err(|_| anyhow::anyhow!("discovered API account is invalid"))?;
    current()?;
    crate::secrets::write_secret_record(
        root,
        &secret.scope,
        &secret.name,
        key,
        None,
        serde_json::json!({"purpose":"native provider API account"}),
    )?;
    let version = crate::secrets::secret_record_content_version(root, &secret.scope, &secret.name)?
        .ok_or_else(|| anyhow::anyhow!("new API credential could not be confirmed"))?;
    stored.config.api_key_accounts.push(account);
    let result = current().and_then(|()| {
        super::save_provider_integration_config(root, stored.revision, &stored.config)
    });
    match result {
        Ok(revision) => Ok(InstalledApiAccount {
            id,
            endpoint: found.endpoint,
            models: found.models,
            revision,
        }),
        Err(_) => {
            crate::secrets::delete_secret_records_if_versions(root,&[(secret.scope,secret.name,version)])
                .map_err(|_|anyhow::anyhow!("API account creation is uncertain; owned credential cleanup requires reconciliation"))?;
            anyhow::bail!("API account creation was rejected before configuration commit")
        }
    }
}
#[cfg(test)]
#[path = "api_key_ingress_tests.rs"]
mod tests;
