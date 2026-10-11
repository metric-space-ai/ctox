// Origin: CTOX
// License: AGPL-3.0-only
//! Native API-key routes on the existing instance gateway. No inbound credential
//! is forwarded; routing uses the same persistent account/session/quota selector.
#[cfg(test)]
#[path = "cliproxyapi_api_keys_tests.rs"]
mod tests;
use super::*;
use crate::execution::cliproxyapi_integration::{
    ApiKeyAccountConfig, StoredProviderIntegrationConfig,
};
use ctox_cliproxyapi::internal::runtime::executor::{
    api_key_transport::ApiKeyHttpClient,
    openai_compat_executor::{OpenAiCompatConfig, OpenAiCompatError, OpenAiCompatExecutor},
};
use ctox_cliproxyapi::sdk::api::handlers::openai::OpenAiResponsesGenericStream;
use ctox_cliproxyapi::sdk::pluginapi::{HostHttpClient, ProviderExecutor};

pub(super) struct Routes {
    root: PathBuf,
    timeout: Duration,
    http: std::sync::Mutex<Option<Arc<dyn HostHttpClient>>>,
    cached: std::sync::Mutex<BTreeMap<String, Arc<Handler>>>,
}
impl Routes {
    pub(super) fn route_for(&self, provider: &str) -> anyhow::Result<Option<Arc<Handler>>> {
        if !matches!(provider, "zai" | "minimax" | "xai" | "kimi") {
            return Ok(None);
        }
        let mut stored =
            crate::execution::cliproxyapi_integration::load_provider_integration_config(
                &self.root,
            )?;
        stored
            .config
            .api_key_accounts
            .retain(|a| a.endpoint.provider() == provider && !a.disabled && !a.models.is_empty());
        let mut cached = self
            .cached
            .lock()
            .map_err(|_| anyhow::anyhow!("API route cache unavailable"))?;
        if stored.config.api_key_accounts.is_empty() {
            cached.remove(provider);
            return Ok(None);
        }
        if let Some(handler) = cached.get(provider) {
            if handler.accounts.len() == stored.config.api_key_accounts.len()
                && handler
                    .accounts
                    .iter()
                    .zip(&stored.config.api_key_accounts)
                    .all(|(a, config)| {
                        a.config == *config && current(&self.root, config, &a.version)
                    })
            {
                return Ok(Some(handler.clone()));
            }
        }
        let mut transport = self
            .http
            .lock()
            .map_err(|_| anyhow::anyhow!("API transport unavailable"))?;
        let http = match transport.as_ref() {
            Some(http) => http.clone(),
            None => {
                let http: Arc<dyn HostHttpClient> = Arc::new(
                    ApiKeyHttpClient::new(None, self.timeout)
                        .map_err(|_| anyhow::anyhow!("API provider transport unavailable"))?,
                );
                *transport = Some(http.clone());
                http
            }
        };
        let handler = build_with_http(&self.root, &stored, http)?.remove(provider);
        if let Some(handler) = &handler {
            cached.insert(provider.to_owned(), handler.clone());
        }
        Ok(handler)
    }
}

struct Resolved {
    config: ApiKeyAccountConfig,
    key: Zeroizing<String>,
    version: String,
    http: Arc<dyn HostHttpClient>,
}
pub(super) struct Handler {
    root: PathBuf,
    provider: String,
    accounts: Vec<Resolved>,
    router: AccountRouter,
    executor: OpenAiCompatExecutor,
}
fn current(root: &Path, config: &ApiKeyAccountConfig, version: &str) -> bool {
    let _guard = crate::secrets::credential_lifecycle_guard();
    crate::execution::cliproxyapi_integration::load_provider_integration_config(root).is_ok_and(
        |s| {
            s.config
                .api_key_accounts
                .iter()
                .any(|a| a == config && !a.disabled)
        },
    ) && crate::secrets::secret_record_content_version(
        root,
        &config.api_key_secret.scope,
        &config.api_key_secret.name,
    )
    .is_ok_and(|v| v.as_deref() == Some(version))
}
fn resolve(
    root: &Path,
    config: &ApiKeyAccountConfig,
    http: Arc<dyn HostHttpClient>,
) -> anyhow::Result<Resolved> {
    config
        .validate()
        .map_err(|_| anyhow::anyhow!("API account configuration is invalid"))?;
    let r = &config.api_key_secret;
    let before = crate::secrets::secret_record_content_version(root, &r.scope, &r.name)?
        .context("API credential is unavailable")?;
    let key = Zeroizing::new(
        crate::secrets::read_secret_value(root, &r.scope, &r.name)
            .map_err(|_| anyhow::anyhow!("API credential is unavailable"))?,
    );
    anyhow::ensure!(
        !key.trim().is_empty() && key.len() <= 8192 && !key.chars().any(char::is_control),
        "API credential is invalid"
    );
    anyhow::ensure!(current(root, config, &before), "API account changed");
    Ok(Resolved {
        config: config.clone(),
        key,
        version: before,
        http,
    })
}
pub(super) fn build(
    root: &Path,
    _stored: &StoredProviderIntegrationConfig,
    timeout: Duration,
) -> anyhow::Result<Arc<Routes>> {
    // Keep the manager even for OAuth-only startup. A subsequently added API
    // account becomes usable without rebuilding the instance proxy service.
    Ok(Arc::new(Routes {
        root: root.to_path_buf(),
        timeout,
        http: std::sync::Mutex::new(None),
        cached: std::sync::Mutex::new(BTreeMap::new()),
    }))
}
fn build_with_http(
    root: &Path,
    stored: &StoredProviderIntegrationConfig,
    http: Arc<dyn HostHttpClient>,
) -> anyhow::Result<BTreeMap<String, Arc<Handler>>> {
    let mut groups: BTreeMap<String, Vec<Resolved>> = BTreeMap::new();
    for config in &stored.config.api_key_accounts {
        if config.disabled {
            continue;
        }
        // An unobserved empty catalog is not a wildcard/default-model route.
        if config.models.is_empty() {
            continue;
        }
        let account = resolve(root, config, http.clone())?;
        groups
            .entry(config.endpoint.provider().to_owned())
            .or_default()
            .push(account);
    }
    Ok(groups
        .into_iter()
        .map(|(provider, accounts)| {
            let router = AccountRouter::with_account_state_stores(
                Arc::new(CtoxCooldownStateStore::new(root)),
                Arc::new(CtoxAccountSessionStore::new(root)),
                Arc::new(CtoxAccountQuotaStore::new(root)),
            );
            let executor = OpenAiCompatExecutor::new(
                provider.clone(),
                Arc::new(OpenAiCompatConfig::default()),
                ctox_cliproxyapi::sdk::translator::builtin::registry(),
            );
            (
                provider.clone(),
                Arc::new(Handler {
                    root: root.to_path_buf(),
                    provider,
                    accounts,
                    router,
                    executor,
                }),
            )
        })
        .collect())
}
impl Handler {
    pub(super) fn contains(&self, id: &str) -> bool {
        self.accounts.iter().any(|a| a.config.id == id)
    }
    pub(super) async fn handle_route(&self, body: &[u8]) -> OpenAiResponsesRouteResponse {
        let error = |status, message| {
            OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                status, message,
            ))
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
            return error(400, "invalid JSON request body");
        };
        let Some(model) = value
            .get("model")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.trim().is_empty())
        else {
            return error(400, "model is required");
        };
        let candidates = self
            .accounts
            .iter()
            .map(|a| AccountCandidate {
                auth_id: a.config.id.clone(),
                provider: self.provider.clone(),
                disabled: a.config.disabled,
                priority: a.config.priority,
                supported_models: a.config.models.clone(),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(i64::MAX as u128) as i64;
        let Ok(selected) = self
            .router
            .select(&self.provider, Some(model), now, &candidates)
        else {
            return error(503, "no API account available for this model");
        };
        let Some(account) = self
            .accounts
            .iter()
            .find(|a| a.config.id == selected.auth_id)
        else {
            return error(503, "API account unavailable");
        };
        if !current(&self.root, &account.config, &account.version) {
            return error(409, "provider account configuration changed");
        }
        let stream = value.get("stream").and_then(serde_json::Value::as_bool) == Some(true);
        let request = ExecutorRequest {
            auth_id: account.config.id.clone(),
            auth_provider: self.provider.clone(),
            model: model.to_owned(),
            format: "openai-response".into(),
            source_format: "openai-response".into(),
            payload: body.to_vec(),
            original_request: body.to_vec(),
            stream,
            http_client: Some(account.http.clone()),
            auth_attributes: BTreeMap::from([
                ("base_url".into(), account.config.endpoint.base_url().into()),
                ("api_key".into(), account.key.to_string()),
            ]),
            ..Default::default()
        };
        ctox_cliproxyapi::internal::api::account_selection::record_selected(&account.config.id);
        if stream {
            match self.executor.execute_stream(request).await {
                Ok(response) => {
                    if !current(&self.root, &account.config, &account.version) {
                        return error(409, "provider account configuration changed");
                    }
                    let root = self.root.clone();
                    let config = account.config.clone();
                    let version = account.version.clone();
                    OpenAiResponsesRouteResponse::GenericStream(Box::new(
                        OpenAiResponsesGenericStream::new(
                            response.chunks,
                            Arc::new(move || current(&root, &config, &version)),
                        ),
                    ))
                }
                Err(e) => upstream_error(e),
            }
        } else {
            match self.executor.execute(request).await {
                Ok(response) => {
                    if !current(&self.root, &account.config, &account.version) {
                        return error(409, "provider account configuration changed");
                    }
                    OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::json(
                        200,
                        response.payload,
                    ))
                }
                Err(e) => upstream_error(e),
            }
        }
    }
}
fn upstream_error(
    e: ctox_cliproxyapi::sdk::pluginapi::PluginExecutionError,
) -> OpenAiResponsesRouteResponse {
    let status = e
        .downcast_ref::<OpenAiCompatError>()
        .map_or(502, |e| e.status_code);
    OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
        status,
        "API provider request failed",
    ))
}
