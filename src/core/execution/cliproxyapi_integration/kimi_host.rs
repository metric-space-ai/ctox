// Origin: CTOX
// License: AGPL-3.0-only

//! Native CTOX host boundary for the portable Kimi device-flow and executor.
//!
//! This module owns CTOX persistence, encrypted secret references and network
//! authority. The portable crate remains free of CTOX SQLite and secret-store
//! dependencies.

use std::fmt;
use std::io::Read as _;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ctox_cliproxyapi::internal::auth::kimi::{
    DeviceFlowClient, KimiAuth, KimiDeviceIdentity, KimiHttpFuture, KimiHttpRequest,
    KimiHttpResponse, KimiHttpTransport, KimiRefreshCoordinator, KimiTokenStorage,
    KimiTransportFailure, SystemKimiClock, KIMI_DEVICE_CODE_URL, KIMI_TOKEN_URL,
};
use ctox_cliproxyapi::internal::buildinfo::BuildInfo;
use ctox_cliproxyapi::internal::cache::KimiThinkingReplayCache;
use ctox_cliproxyapi::internal::runtime::executor::kimi_executor::{
    normalize_kimi_upstream_model, KimiClaudeDelegate, KimiClock, KimiDeviceProfile, KimiExecutor,
    KimiExecutorConfig, KIMI_CHAT_COMPLETIONS_URL, KIMI_MESSAGES_COUNT_TOKENS_URL,
};
use ctox_cliproxyapi::sdk::auth::LoginCancellation;
use ctox_cliproxyapi::sdk::pluginapi::{
    ExecutorRequest, ExecutorResponse, ExecutorStreamResponse, Headers, HostHttpClient,
    HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse, PluginExecutionError,
    PluginFuture, ProviderExecutor,
};
use ctox_cliproxyapi::sdk::translator::Registry;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use super::{
    KimiSubscriptionAccountConfig, ProviderIntegrationConfig, ProviderSecretRef,
    DEFAULT_KIMI_SUBSCRIPTION_MODEL,
};

const INTEGRATION_CONFIG_TABLE: &str = "cliproxyapi_provider_integration_config";
const KIMI_STATE_SCHEMA: &str = "ctox.kimi-subscription-state.v1";
const PROVIDER_SECRET_SCOPE: &str = "provider-subscriptions";
const MAX_KIMI_HTTP_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug)]
struct ProviderIntegrationRevisionConflict;

impl fmt::Display for ProviderIntegrationRevisionConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("provider integration config changed concurrently")
    }
}

impl std::error::Error for ProviderIntegrationRevisionConflict {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProviderIntegrationConfig {
    pub revision: u64,
    pub config: ProviderIntegrationConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredKimiState {
    schema: String,
    device_id: String,
    token_type: String,
    scope: String,
    expires_at_unix: Option<u64>,
}

impl StoredKimiState {
    fn from_storage(storage: &KimiTokenStorage) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !storage.device_id().trim().is_empty(),
            "Kimi device identity is unavailable"
        );
        Ok(Self {
            schema: KIMI_STATE_SCHEMA.to_owned(),
            device_id: storage.device_id().trim().to_owned(),
            token_type: storage.token_type().trim().to_owned(),
            scope: storage.scope().trim().to_owned(),
            expires_at_unix: storage
                .expires_at()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_secs()),
        })
    }

    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema == KIMI_STATE_SCHEMA,
            "Kimi state schema is invalid"
        );
        anyhow::ensure!(
            !self.device_id.trim().is_empty() && !self.device_id.chars().any(char::is_control),
            "Kimi device identity is invalid"
        );
        if let Some(expires_at) = self.expires_at_unix {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            anyhow::ensure!(expires_at > now, "Kimi subscription credential is expired");
        }
        Ok(())
    }
}

fn open_config_db(root: &Path) -> anyhow::Result<Connection> {
    let path = crate::inference::runtime_env::runtime_config_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {INTEGRATION_CONFIG_TABLE} (\
         config_id INTEGER PRIMARY KEY CHECK(config_id = 1), revision INTEGER NOT NULL, \
         config_json TEXT NOT NULL, updated_at_ms INTEGER NOT NULL)"
    ))?;
    Ok(conn)
}

fn open_config_db_read_only(root: &Path) -> anyhow::Result<Option<Connection>> {
    let path = crate::inference::runtime_env::runtime_config_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [INTEGRATION_CONFIG_TABLE],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(exists.then_some(conn))
}

pub fn load_provider_integration_config(
    root: &Path,
) -> anyhow::Result<StoredProviderIntegrationConfig> {
    let Some(conn) = open_config_db_read_only(root)? else {
        return Ok(StoredProviderIntegrationConfig {
            revision: 0,
            config: ProviderIntegrationConfig::default(),
        });
    };
    let row = conn
        .query_row(
            &format!(
                "SELECT revision, config_json FROM {INTEGRATION_CONFIG_TABLE} WHERE config_id = 1"
            ),
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((revision, config_json)) = row else {
        return Ok(StoredProviderIntegrationConfig {
            revision: 0,
            config: ProviderIntegrationConfig::default(),
        });
    };
    let revision = u64::try_from(revision)
        .map_err(|_| anyhow::anyhow!("provider integration revision is invalid"))?;
    let config: ProviderIntegrationConfig = serde_json::from_str(&config_json)
        .map_err(|_| anyhow::anyhow!("provider integration config is invalid"))?;
    config
        .validate()
        .map_err(|_| anyhow::anyhow!("provider integration config is invalid"))?;
    Ok(StoredProviderIntegrationConfig { revision, config })
}

fn save_provider_integration_config(
    root: &Path,
    expected_revision: u64,
    config: &ProviderIntegrationConfig,
) -> anyhow::Result<u64> {
    config
        .validate()
        .map_err(|_| anyhow::anyhow!("provider integration config is invalid"))?;
    let serialized = serde_json::to_string(config)?;
    let mut conn = open_config_db(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = tx
        .query_row(
            &format!("SELECT revision FROM {INTEGRATION_CONFIG_TABLE} WHERE config_id = 1"),
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(u64::try_from)
        .transpose()
        .map_err(|_| anyhow::anyhow!("provider integration revision is invalid"))?
        .unwrap_or(0);
    if current != expected_revision {
        return Err(anyhow::Error::new(ProviderIntegrationRevisionConflict));
    }
    let revision = current
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("provider integration revision overflow"))?;
    let updated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let updated_at_ms = i64::try_from(updated_at_ms).unwrap_or(i64::MAX);
    tx.execute(
        &format!(
            "INSERT INTO {INTEGRATION_CONFIG_TABLE} \
             (config_id, revision, config_json, updated_at_ms) VALUES (1, ?1, ?2, ?3) \
             ON CONFLICT(config_id) DO UPDATE SET revision=excluded.revision, \
             config_json=excluded.config_json, updated_at_ms=excluded.updated_at_ms"
        ),
        params![revision, serialized, updated_at_ms],
    )?;
    tx.commit()?;
    Ok(revision)
}

fn secret_ref(account_id: &str, suffix: &str) -> ProviderSecretRef {
    ProviderSecretRef::new(PROVIDER_SECRET_SCOPE, format!("{account_id}-{suffix}"))
}

fn ensure_owned_secret_reference(secret: &ProviderSecretRef) -> anyhow::Result<()> {
    anyhow::ensure!(
        secret.scope == PROVIDER_SECRET_SCOPE,
        "Kimi credential reference is outside the provider subscription scope"
    );
    Ok(())
}

fn mutate_provider_integration_config(
    root: &Path,
    mut mutation: impl FnMut(&mut ProviderIntegrationConfig),
) -> anyhow::Result<u64> {
    for _ in 0..4 {
        let stored = load_provider_integration_config(root)?;
        let mut next = stored.config;
        mutation(&mut next);
        match save_provider_integration_config(root, stored.revision, &next) {
            Ok(revision) => return Ok(revision),
            Err(error)
                if error
                    .downcast_ref::<ProviderIntegrationRevisionConflict>()
                    .is_some() =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    anyhow::bail!("provider integration config changed concurrently")
}

/// Stores the complete portable Kimi OAuth result as one encrypted tuple and
/// then publishes only its typed secret references into runtime configuration.
pub fn install_kimi_subscription(
    root: &Path,
    account_id: &str,
    storage: &KimiTokenStorage,
) -> anyhow::Result<u64> {
    let _lifecycle = crate::secrets::credential_lifecycle_guard();
    let account_id = account_id.trim();
    let previous = load_provider_integration_config(root)?
        .config
        .kimi_subscription_accounts
        .into_iter()
        .find(|account| account.id == account_id);
    let access = secret_ref(account_id, "access-token");
    let refresh = storage
        .credentials()
        .refresh_token()
        .map(|_| secret_ref(account_id, "refresh-token"));
    let state = secret_ref(account_id, "state");
    // Reauthentication replaces credentials, not the user's account policy.
    // In particular, a disabled account must not re-enter the inference pool.
    let mut config = previous.clone().unwrap_or_else(|| KimiSubscriptionAccountConfig {
        id: account_id.to_owned(),
        disabled: false,
        priority: 100,
        weight: 1,
        models: vec![DEFAULT_KIMI_SUBSCRIPTION_MODEL.to_owned()],
        access_token_secret: access.clone(),
        refresh_token_secret: refresh.clone(),
        state_secret: state.clone(),
        endpoint_profile: Default::default(),
    });
    config.access_token_secret = access.clone();
    config.refresh_token_secret = refresh.clone();
    config.state_secret = state.clone();
    config
        .validate()
        .map_err(|_| anyhow::anyhow!("Kimi account config is invalid"))?;
    let state_value = Zeroizing::new(serde_json::to_string(&StoredKimiState::from_storage(
        storage,
    )?)?);
    let mut writes = vec![
        crate::secrets::SecretRecordWrite {
            scope: &access.scope,
            name: &access.name,
            value: storage.credentials().access_token().expose_secret(),
            description: Some("Kimi subscription access token"),
            metadata: serde_json::json!({"provider": "kimi", "kind": "access_token"}),
        },
        crate::secrets::SecretRecordWrite {
            scope: &state.scope,
            name: &state.name,
            value: state_value.as_str(),
            description: Some("Kimi subscription device and expiry state"),
            metadata: serde_json::json!({"provider": "kimi", "kind": "state"}),
        },
    ];
    if let (Some(reference), Some(token)) = (&refresh, storage.credentials().refresh_token()) {
        writes.push(crate::secrets::SecretRecordWrite {
            scope: &reference.scope,
            name: &reference.name,
            value: token.expose_secret(),
            description: Some("Kimi subscription refresh token"),
            metadata: serde_json::json!({"provider": "kimi", "kind": "refresh_token"}),
        });
    }
    crate::secrets::write_secret_records(root, &writes)?;
    let revision = mutate_provider_integration_config(root, |runtime| {
        if let Some(account) = runtime
            .kimi_subscription_accounts
            .iter_mut()
            .find(|account| account.id == account_id)
        {
            *account = config.clone();
        } else {
            runtime.kimi_subscription_accounts.push(config.clone());
        }
    })?;
    if let Some(previous) = previous {
        let current_refs = [
            Some(&config.access_token_secret),
            config.refresh_token_secret.as_ref(),
            Some(&config.state_secret),
        ]
        .into_iter()
        .flatten()
        .map(|secret| (secret.scope.as_str(), secret.name.as_str()))
        .collect::<Vec<_>>();
        let stale = [
            Some(previous.access_token_secret),
            previous.refresh_token_secret,
            Some(previous.state_secret),
        ]
        .into_iter()
        .flatten()
        .filter(|secret| {
            !current_refs
                .iter()
                .any(|key| key == &(secret.scope.as_str(), secret.name.as_str()))
        })
        .collect::<Vec<_>>();
        let stale_keys = stale
            .iter()
            .map(|secret| (secret.scope.as_str(), secret.name.as_str()))
            .collect::<Vec<_>>();
        crate::secrets::delete_secret_records(root, &stale_keys)?;
    }
    Ok(revision)
}

/// Removes one Kimi account from the revisionsafe topology and returns its
/// already-validated secret references for a subsequent atomic tuple delete.
pub fn remove_kimi_subscription_from_topology(
    root: &Path,
    account_id: &str,
) -> anyhow::Result<(u64, Vec<ProviderSecretRef>)> {
    let account_id = account_id.trim();
    let stored = load_provider_integration_config(root)?;
    let account = stored
        .config
        .kimi_subscription_accounts
        .iter()
        .find(|account| account.id == account_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("provider subscription account was not found"))?;
    let mut secrets = vec![account.access_token_secret, account.state_secret];
    if let Some(refresh) = account.refresh_token_secret {
        secrets.push(refresh);
    }
    for secret in &secrets {
        ensure_owned_secret_reference(secret)?;
    }
    let remaining_refs = stored
        .config
        .kimi_subscription_accounts
        .iter()
        .filter(|account| account.id != account_id)
        .flat_map(|account| {
            [
                Some(&account.access_token_secret),
                account.refresh_token_secret.as_ref(),
                Some(&account.state_secret),
            ]
            .into_iter()
            .flatten()
        })
        .map(|secret| (secret.scope.as_str(), secret.name.as_str()))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        secrets.iter().all(|secret| !remaining_refs
            .iter()
            .any(|key| key == &(secret.scope.as_str(), secret.name.as_str()))),
        "Kimi credential reference is shared by another account"
    );
    let revision = mutate_provider_integration_config(root, |runtime| {
        runtime
            .kimi_subscription_accounts
            .retain(|account| account.id != account_id);
    })?;
    Ok((revision, secrets))
}

#[derive(Clone, Copy, Debug, Default)]
struct SystemExecutorClock;

impl KimiClock for SystemExecutorClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|value| i64::try_from(value.as_millis()).ok())
            .unwrap_or(i64::MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KimiHostError {
    UnsupportedClaudeDelegate,
    UnsupportedFormat,
    ProviderMismatch,
    AccountMismatch,
    ModelUnavailable,
    CredentialExpired,
    HttpTransport,
    ResponseTooLarge,
}

impl fmt::Display for KimiHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedClaudeDelegate => {
                "Kimi Claude delegation is not enabled on this route"
            }
            Self::UnsupportedFormat => {
                "Kimi route accepts only OpenAI Responses or Chat Completions"
            }
            Self::ProviderMismatch => "Kimi route provider does not match",
            Self::AccountMismatch => "Kimi route account does not match",
            Self::ModelUnavailable => "Kimi route model is not enabled for this account",
            Self::CredentialExpired => "Kimi subscription credential is expired",
            Self::HttpTransport => "Kimi upstream transport failed",
            Self::ResponseTooLarge => "Kimi upstream response exceeded the bounded limit",
        })
    }
}

impl std::error::Error for KimiHostError {}

fn plugin_error(error: KimiHostError) -> PluginExecutionError {
    Arc::new(error)
}

#[derive(Debug, Default)]
struct RejectingClaudeDelegate;

impl KimiClaudeDelegate for RejectingClaudeDelegate {
    fn execute<'a>(&'a self, _request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async { Err(plugin_error(KimiHostError::UnsupportedClaudeDelegate)) })
    }

    fn execute_stream<'a>(
        &'a self,
        _request: ExecutorRequest,
    ) -> PluginFuture<'a, ExecutorStreamResponse> {
        Box::pin(async { Err(plugin_error(KimiHostError::UnsupportedClaudeDelegate)) })
    }

    fn count_tokens<'a>(&'a self, _request: ExecutorRequest) -> PluginFuture<'a, ExecutorResponse> {
        Box::pin(async { Err(plugin_error(KimiHostError::UnsupportedClaudeDelegate)) })
    }
}

/// One exact, server-selected Kimi account route. Credentials are resolved
/// when the route is built and are never accepted from the caller.
pub struct KimiSubscriptionRoute {
    account_id: String,
    models: Vec<String>,
    endpoint: &'static str,
    access_token: Zeroizing<String>,
    refresh_token: Option<Zeroizing<String>>,
    state: StoredKimiState,
    executor: Arc<KimiExecutor>,
    http: Arc<dyn HostHttpClient>,
}

impl fmt::Debug for KimiSubscriptionRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KimiSubscriptionRoute")
            .field("account_id", &self.account_id)
            .field("models", &self.models)
            .field("endpoint", &self.endpoint)
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

impl KimiSubscriptionRoute {
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn models(&self) -> &[String] {
        &self.models
    }

    pub fn supports_model(&self, model: &str) -> bool {
        let requested = normalize_kimi_upstream_model(model);
        self.models
            .iter()
            .any(|configured| normalize_kimi_upstream_model(configured) == requested)
    }

    fn prepare(
        &self,
        mut request: ExecutorRequest,
    ) -> Result<ExecutorRequest, PluginExecutionError> {
        if !request.auth_provider.trim().is_empty()
            && !request.auth_provider.eq_ignore_ascii_case("kimi")
        {
            return Err(plugin_error(KimiHostError::ProviderMismatch));
        }
        if !request.auth_id.trim().is_empty() && request.auth_id != self.account_id {
            return Err(plugin_error(KimiHostError::AccountMismatch));
        }
        if !matches!(request.source_format.as_str(), "openai" | "openai-response")
            || !matches!(request.format.as_str(), "openai" | "openai-response")
        {
            return Err(plugin_error(KimiHostError::UnsupportedFormat));
        }
        if self.state.expires_at_unix.is_some_and(|expires_at| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                >= expires_at
        }) {
            return Err(plugin_error(KimiHostError::CredentialExpired));
        }
        let requested = normalize_kimi_upstream_model(&request.model);
        let allowed = self
            .models
            .iter()
            .any(|model| normalize_kimi_upstream_model(model) == requested);
        if !allowed {
            return Err(plugin_error(KimiHostError::ModelUnavailable));
        }
        request.auth_id.clone_from(&self.account_id);
        request.auth_provider = "kimi".to_owned();
        request.auth_metadata.insert(
            "access_token".to_owned(),
            Value::String(self.access_token.as_str().to_owned()),
        );
        if let Some(refresh) = &self.refresh_token {
            request.auth_metadata.insert(
                "refresh_token".to_owned(),
                Value::String(refresh.as_str().to_owned()),
            );
        }
        request.auth_metadata.insert(
            "device_id".to_owned(),
            Value::String(self.state.device_id.clone()),
        );
        request.http_client = Some(Arc::clone(&self.http));
        Ok(request)
    }

    pub async fn execute(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, PluginExecutionError> {
        self.executor.execute(self.prepare(request)?).await
    }

    pub async fn execute_stream(
        &self,
        request: ExecutorRequest,
    ) -> Result<ExecutorStreamResponse, PluginExecutionError> {
        self.executor.execute_stream(self.prepare(request)?).await
    }
}

pub fn build_kimi_subscription_route(
    root: &Path,
    account_id: &str,
) -> anyhow::Result<KimiSubscriptionRoute> {
    build_kimi_subscription_route_with_http(root, account_id, Arc::new(CtoxKimiHostHttpClient))
}

pub fn build_kimi_subscription_route_with_http(
    root: &Path,
    account_id: &str,
    http: Arc<dyn HostHttpClient>,
) -> anyhow::Result<KimiSubscriptionRoute> {
    let stored = load_provider_integration_config(root)?;
    let account = stored
        .config
        .kimi_subscription_accounts
        .iter()
        .find(|account| account.id == account_id.trim())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Kimi subscription account was not found"))?;
    anyhow::ensure!(!account.disabled, "Kimi subscription account is disabled");
    account
        .validate()
        .map_err(|_| anyhow::anyhow!("Kimi subscription account is invalid"))?;
    ensure_owned_secret_reference(&account.access_token_secret)?;
    ensure_owned_secret_reference(&account.state_secret)?;
    if let Some(refresh) = &account.refresh_token_secret {
        ensure_owned_secret_reference(refresh)?;
    }
    let mut keys = vec![
        (
            account.access_token_secret.scope.as_str(),
            account.access_token_secret.name.as_str(),
        ),
        (
            account.state_secret.scope.as_str(),
            account.state_secret.name.as_str(),
        ),
    ];
    if let Some(refresh) = &account.refresh_token_secret {
        keys.push((refresh.scope.as_str(), refresh.name.as_str()));
    }
    let mut values = crate::secrets::read_secret_values(root, &keys)?;
    anyhow::ensure!(
        values.len() == keys.len(),
        "Kimi credential tuple is incomplete"
    );
    let refresh_token = account
        .refresh_token_secret
        .as_ref()
        .map(|_| Zeroizing::new(values.pop().unwrap_or_default()));
    let state_value = Zeroizing::new(values.pop().unwrap_or_default());
    let access_token = Zeroizing::new(values.pop().unwrap_or_default());
    anyhow::ensure!(
        !access_token.trim().is_empty(),
        "Kimi access token is unavailable"
    );
    if let Some(refresh) = &refresh_token {
        anyhow::ensure!(
            !refresh.trim().is_empty(),
            "Kimi refresh token is unavailable"
        );
    }
    let state: StoredKimiState = serde_json::from_str(state_value.as_str())
        .map_err(|_| anyhow::anyhow!("Kimi subscription state is invalid"))?;
    state.validate()?;

    let registry = Arc::new(Registry::new());
    ctox_cliproxyapi::internal::translator::openai::passthrough::responses::register_openai_responses_chat_completions(&registry);
    let executor = Arc::new(KimiExecutor::new(
        Arc::new(KimiExecutorConfig::default()),
        registry,
        Arc::new(RejectingClaudeDelegate),
        Arc::new(KimiThinkingReplayCache::new()),
        Arc::new(SystemExecutorClock),
        None,
        KimiDeviceProfile {
            device_id: state.device_id.clone(),
            build: BuildInfo::default(),
            ..KimiDeviceProfile::default()
        },
    ));
    let models = account.effective_models();
    Ok(KimiSubscriptionRoute {
        account_id: account.id,
        models,
        endpoint: account.endpoint_profile.base_url(),
        access_token,
        refresh_token,
        state,
        executor,
        http,
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct CtoxKimiHostHttpClient;

impl HostHttpClient for CtoxKimiHostHttpClient {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || {
                execute_ureq(
                    request,
                    &[KIMI_CHAT_COMPLETIONS_URL, KIMI_MESSAGES_COUNT_TOKENS_URL],
                )
            })
            .await
            .map_err(|_| plugin_error(KimiHostError::HttpTransport))?;
            result.map_err(plugin_error)
        })
    }

    fn execute_stream<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async move {
            let (sender, receiver) = mpsc::channel(8);
            let (headers_sender, headers_receiver) = std::sync::mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("ctox-kimi-subscription-stream".to_owned())
                .spawn(move || {
                    let response =
                        match open_ureq_response(request, &[KIMI_CHAT_COMPLETIONS_URL], true) {
                            Ok(response) => response,
                            Err(error) => {
                                let _ = headers_sender.send(Err(error));
                                return;
                            }
                        };
                    let status_code = response.status();
                    let headers = response_headers(&response);
                    if headers_sender.send(Ok((status_code, headers))).is_err() {
                        return;
                    }
                    pump_kimi_stream(response.into_reader(), sender);
                })
                .map_err(|_| plugin_error(KimiHostError::HttpTransport))?;
            let (status_code, headers) = tokio::task::spawn_blocking(move || {
                headers_receiver
                    .recv()
                    .map_err(|_| KimiHostError::HttpTransport)?
            })
            .await
            .map_err(|_| plugin_error(KimiHostError::HttpTransport))?
            .map_err(plugin_error)?;
            Ok(HttpStreamResponse {
                status_code,
                headers,
                chunks: receiver,
            })
        })
    }
}

fn response_headers(response: &ureq::Response) -> Headers {
    let mut headers = Headers::new();
    for name in response.headers_names() {
        headers.insert(
            name.clone(),
            response.all(&name).into_iter().map(str::to_owned).collect(),
        );
    }
    headers
}

fn open_ureq_response(
    request: HttpRequest,
    allowed_urls: &[&str],
    stream: bool,
) -> Result<ureq::Response, KimiHostError> {
    if !allowed_urls.iter().any(|allowed| request.url == *allowed) {
        return Err(KimiHostError::HttpTransport);
    }
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_write(Duration::from_secs(30))
        .redirects(0);
    builder = if stream {
        builder
            .timeout_read(Duration::from_secs(300))
            .timeout(Duration::from_secs(600))
    } else {
        builder.timeout(Duration::from_secs(30))
    };
    let agent = builder.build();
    let mut upstream = agent.request(request.method.trim(), &request.url);
    for (name, values) in &request.headers {
        for value in values {
            upstream = upstream.set(name, value);
        }
    }
    match upstream.send_bytes(&request.body) {
        Ok(response) => Ok(response),
        Err(ureq::Error::Status(_, response)) => Ok(response),
        Err(ureq::Error::Transport(_)) => Err(KimiHostError::HttpTransport),
    }
}

fn pump_kimi_stream(mut reader: impl std::io::Read, sender: mpsc::Sender<HttpStreamChunk>) {
    let mut total = 0u64;
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                total = total.saturating_add(read as u64);
                if total > MAX_KIMI_HTTP_RESPONSE_BYTES {
                    let _ = sender.blocking_send(HttpStreamChunk {
                        payload: Vec::new(),
                        error: Some(plugin_error(KimiHostError::ResponseTooLarge)),
                    });
                    break;
                }
                if sender
                    .blocking_send(HttpStreamChunk {
                        payload: buffer[..read].to_vec(),
                        error: None,
                    })
                    .is_err()
                {
                    break;
                }
            }
            Err(_) => {
                let _ = sender.blocking_send(HttpStreamChunk {
                    payload: Vec::new(),
                    error: Some(plugin_error(KimiHostError::HttpTransport)),
                });
                break;
            }
        }
    }
}

fn execute_ureq(
    request: HttpRequest,
    allowed_urls: &[&str],
) -> Result<HttpResponse, KimiHostError> {
    let response = open_ureq_response(request, allowed_urls, false)?;
    let status_code = response.status();
    let headers = response_headers(&response);
    let mut body = Vec::new();
    response
        .into_reader()
        .take(MAX_KIMI_HTTP_RESPONSE_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|_| KimiHostError::HttpTransport)?;
    if body.len() as u64 > MAX_KIMI_HTTP_RESPONSE_BYTES {
        return Err(KimiHostError::ResponseTooLarge);
    }
    Ok(HttpResponse {
        status_code,
        headers,
        body,
    })
}

/// Real, bounded Kimi device-flow transport. The portable auth state machine
/// still owns request formation, polling and refresh singleflight semantics.
#[derive(Debug, Clone, Copy, Default)]
pub struct CtoxKimiAuthHttpTransport;

fn ureq_transport_is_timeout(error: &ureq::Transport) -> bool {
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        if cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
            matches!(
                io.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            )
        }) {
            return true;
        }
        source = cause.source();
    }
    false
}

impl KimiHttpTransport for CtoxKimiAuthHttpTransport {
    fn execute<'a>(
        &'a self,
        request: &'a KimiHttpRequest,
        timeout: Duration,
        cancellation: &'a LoginCancellation,
    ) -> KimiHttpFuture<'a> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(KimiTransportFailure::Cancelled);
            }
            let url = request.url;
            if !matches!(url, KIMI_DEVICE_CODE_URL | KIMI_TOKEN_URL) {
                return Err(KimiTransportFailure::Protocol);
            }
            let headers = request.headers.clone();
            let body = request.body.to_vec();
            let response = tokio::task::spawn_blocking(move || {
                let agent = ureq::AgentBuilder::new()
                    .timeout(timeout)
                    .redirects(0)
                    .build();
                let mut call = agent.post(url);
                for (name, value) in headers {
                    call = call.set(&name, &value);
                }
                match call.send_bytes(&body) {
                    Ok(response) => Ok(response),
                    Err(ureq::Error::Status(_, response)) => Ok(response),
                    Err(ureq::Error::Transport(error)) if ureq_transport_is_timeout(&error) => {
                        Err(KimiTransportFailure::Timeout)
                    }
                    Err(ureq::Error::Transport(_)) => Err(KimiTransportFailure::Connect),
                }
            })
            .await
            .map_err(|_| KimiTransportFailure::Protocol)??;
            if cancellation.is_cancelled() {
                return Err(KimiTransportFailure::Cancelled);
            }
            let status = response.status();
            let mut response_body = Vec::new();
            response
                .into_reader()
                .take(1024 * 1024 + 1)
                .read_to_end(&mut response_body)
                .map_err(|_| KimiTransportFailure::Protocol)?;
            if response_body.len() > 1024 * 1024 {
                return Err(KimiTransportFailure::Protocol);
            }
            Ok(KimiHttpResponse::new(status, response_body))
        })
    }
}

pub fn build_instance_kimi_auth(identity: KimiDeviceIdentity) -> KimiAuth {
    let client = Arc::new(DeviceFlowClient::new(
        Arc::new(CtoxKimiAuthHttpTransport),
        Arc::new(SystemKimiClock),
        identity,
        Arc::new(KimiRefreshCoordinator::default()),
    ));
    KimiAuth::new(client)
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::Mutex;

    use ctox_cliproxyapi::internal::auth::kimi::{KimiAuthBundle, KimiTokenData, SecretString};
    use serde_json::json;

    use super::*;

    #[derive(Default)]
    struct RecordingHttp {
        requests: Mutex<Vec<HttpRequest>>,
    }

    impl HostHttpClient for RecordingHttp {
        fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
            self.requests.lock().unwrap().push(request);
            Box::pin(async {
                Ok(HttpResponse {
                    status_code: 200,
                    body: br#"{"id":"chat_1","object":"chat.completion","created":1,"model":"k3","choices":[{"index":0,"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#.to_vec(),
                    ..HttpResponse::default()
                })
            })
        }

        fn execute_stream<'a>(
            &'a self,
            request: HttpRequest,
        ) -> PluginFuture<'a, HttpStreamResponse> {
            self.requests.lock().unwrap().push(request);
            Box::pin(async {
                let (sender, receiver) = mpsc::channel(1);
                sender
                    .send(HttpStreamChunk {
                        payload: br#"data: {"id":"chat_1","object":"chat.completion.chunk","created":1,"model":"k3","choices":[{"index":0,"delta":{"role":"assistant","content":"hello"},"finish_reason":null}]}

data: {"id":"chat_1","object":"chat.completion.chunk","created":1,"model":"k3","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}

data: [DONE]

"#.to_vec(),
                        error: None,
                    })
                    .await
                    .unwrap();
                drop(sender);
                Ok(HttpStreamResponse {
                    status_code: 200,
                    headers: Headers::new(),
                    chunks: receiver,
                })
            })
        }
    }

    fn token_storage_with(
        access_token: &str,
        refresh_token: &str,
        device_id: &str,
    ) -> KimiTokenStorage {
        let token = KimiTokenData::new(
            SecretString::new(access_token).unwrap(),
            Some(SecretString::new(refresh_token).unwrap()),
            "Bearer",
            Some(SystemTime::now() + Duration::from_secs(3600)),
            "kimi-code",
        );
        KimiTokenStorage::from_bundle(&KimiAuthBundle::new(token, device_id))
    }

    fn token_storage() -> KimiTokenStorage {
        token_storage_with(
            "access-kimi-do-not-leak",
            "refresh-kimi-do-not-leak",
            "device-kimi",
        )
    }

    struct GatedTwoChunkReader {
        state: u8,
        release: std::sync::mpsc::Receiver<()>,
    }

    impl io::Read for GatedTwoChunkReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let bytes = match self.state {
                0 => b"data: first\n\n".as_slice(),
                1 => {
                    self.release
                        .recv()
                        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "release"))?;
                    b"data: second\n\n".as_slice()
                }
                _ => return Ok(0),
            };
            self.state += 1;
            buffer[..bytes.len()].copy_from_slice(bytes);
            Ok(bytes.len())
        }
    }

    #[test]
    fn host_stream_pump_emits_before_upstream_eof() {
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let (sender, mut receiver) = mpsc::channel(2);
        let worker = std::thread::spawn(move || {
            pump_kimi_stream(
                GatedTwoChunkReader {
                    state: 0,
                    release: release_receiver,
                },
                sender,
            );
        });
        let first = receiver.blocking_recv().expect("first stream chunk");
        assert_eq!(first.payload, b"data: first\n\n");
        release_sender.send(()).unwrap();
        let second = receiver.blocking_recv().expect("second stream chunk");
        assert_eq!(second.payload, b"data: second\n\n");
        worker.join().unwrap();
        assert!(receiver.blocking_recv().is_none());
    }

    #[tokio::test]
    async fn installed_route_resolves_secrets_and_executes_exact_kimi_contract() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            install_kimi_subscription(root.path(), "kimi-primary", &token_storage()).unwrap(),
            1
        );
        let http = Arc::new(RecordingHttp::default());
        let route =
            build_kimi_subscription_route_with_http(root.path(), "kimi-primary", http.clone())
                .unwrap();
        let payload = br#"{"model":"kimi-k3[1m]","input":"hello"}"#;
        let response = route
            .execute(ExecutorRequest {
                auth_id: "kimi-primary".to_owned(),
                auth_provider: "kimi".to_owned(),
                model: "k3[1m]".to_owned(),
                source_format: "openai-response".to_owned(),
                format: "openai-response".to_owned(),
                payload: payload.to_vec(),
                original_request: payload.to_vec(),
                ..ExecutorRequest::default()
            })
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&response.payload).unwrap()["object"],
            json!("response")
        );
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, KIMI_CHAT_COMPLETIONS_URL);
        assert_eq!(
            requests[0].headers["Authorization"],
            ["Bearer access-kimi-do-not-leak"]
        );
        assert_eq!(requests[0].headers["X-Msh-Device-Id"], ["device-kimi"]);
        assert_eq!(
            serde_json::from_slice::<Value>(&requests[0].body).unwrap()["model"],
            json!("k3")
        );
        let debug = format!("{route:?}");
        assert!(!debug.contains("access-kimi-do-not-leak"));
        assert!(!debug.contains("refresh-kimi-do-not-leak"));
    }

    #[test]
    fn route_fails_closed_for_missing_or_expired_credentials() {
        let root = tempfile::tempdir().unwrap();
        assert!(build_kimi_subscription_route(root.path(), "missing").is_err());

        let expired = KimiTokenData::new(
            SecretString::new("expired-access").unwrap(),
            None,
            "Bearer",
            Some(SystemTime::now() - Duration::from_secs(1)),
            "kimi-code",
        );
        let storage = KimiTokenStorage::from_bundle(&KimiAuthBundle::new(expired, "device"));
        install_kimi_subscription(root.path(), "expired", &storage).unwrap();
        assert!(build_kimi_subscription_route(root.path(), "expired").is_err());
    }

    #[test]
    fn relogin_preserves_account_policy_while_replacing_credentials() {
        let root = tempfile::tempdir().unwrap();
        install_kimi_subscription(root.path(), "kimi-policy", &token_storage()).unwrap();
        install_kimi_subscription(root.path(), "kimi-sibling", &token_storage()).unwrap();
        mutate_provider_integration_config(root.path(), |runtime| {
            let account = runtime.kimi_subscription_accounts.iter_mut()
                .find(|account| account.id == "kimi-policy").unwrap();
            account.disabled = true;
            account.priority = -10;
            account.weight = 3;
            account.models.clear();
        }).unwrap();
        let before = load_provider_integration_config(root.path()).unwrap();
        let mut expected = before.config.clone();
        let account = expected.kimi_subscription_accounts.iter_mut()
            .find(|account| account.id == "kimi-policy").unwrap();
        // The new tuple deliberately has no refresh token. Only that private
        // reference changes; both accounts' routing policy must stay intact.
        account.refresh_token_secret = None;
        let old_version = crate::secrets::secret_record_content_version(
            root.path(), PROVIDER_SECRET_SCOPE, "kimi-policy-access-token",
        ).unwrap();
        let rotated = KimiTokenData::new(
            SecretString::new("rotated-access-kimi-do-not-leak").unwrap(),
            None, "Bearer", Some(SystemTime::now() + Duration::from_secs(3600)),
            "kimi-code",
        );
        let storage = KimiTokenStorage::from_bundle(&KimiAuthBundle::new(rotated, "rotated-device"));
        install_kimi_subscription(root.path(), "kimi-policy", &storage).unwrap();
        let after = load_provider_integration_config(root.path()).unwrap();
        assert_eq!(after.revision, before.revision + 1);
        assert_eq!(after.config, expected);
        assert_ne!(crate::secrets::secret_record_content_version(
            root.path(), PROVIDER_SECRET_SCOPE, "kimi-policy-access-token",
        ).unwrap(), old_version);
        assert!(!crate::secrets::secret_exists(
            root.path(), PROVIDER_SECRET_SCOPE, "kimi-policy-refresh-token",
        ).unwrap());
        assert!(build_kimi_subscription_route(root.path(), "kimi-policy").is_err());
    }

    #[test]
    fn provider_neutral_disconnect_removes_kimi_topology_and_secret_tuple() {
        let root = tempfile::tempdir().unwrap();
        install_kimi_subscription(root.path(), "kimi-remove", &token_storage()).unwrap();
        let result = crate::execution::cliproxyapi_host::disconnect_provider_subscription(
            root.path(),
            "kimi",
            "kimi-remove",
        )
        .unwrap();
        assert_eq!(result["revision"], 2);
        assert_eq!(result["deleted_secret_records"], 3);
        assert!(load_provider_integration_config(root.path())
            .unwrap()
            .config
            .kimi_subscription_accounts
            .is_empty());
        for suffix in ["access-token", "refresh-token", "state"] {
            assert!(!crate::secrets::secret_exists(
                root.path(),
                PROVIDER_SECRET_SCOPE,
                &format!("kimi-remove-{suffix}")
            )
            .unwrap());
        }
    }

    #[test]
    fn credential_lifecycle_rotates_rolls_back_and_deletes_as_one_owned_tuple() {
        let root = tempfile::tempdir().unwrap();
        let account_id = "kimi-lifecycle";
        let initial = token_storage_with(
            "access-initial-do-not-leak",
            "refresh-initial-do-not-leak",
            "device-initial",
        );
        assert_eq!(
            install_kimi_subscription(root.path(), account_id, &initial).unwrap(),
            1
        );
        let route = build_kimi_subscription_route(root.path(), account_id).unwrap();
        assert_eq!(route.access_token.as_str(), "access-initial-do-not-leak");
        assert_eq!(route.state.device_id, "device-initial");

        let connection =
            rusqlite::Connection::open(crate::secrets::secret_store_path(root.path())).unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TRIGGER reject_kimi_state_rotation
                BEFORE UPDATE ON ctox_secret_records
                WHEN NEW.secret_name = 'kimi-lifecycle-state'
                BEGIN
                    SELECT RAISE(ABORT, 'forced Kimi tuple rollback');
                END;
                "#,
            )
            .unwrap();
        drop(connection);

        let rotated = token_storage_with(
            "access-rotated-do-not-leak",
            "refresh-rotated-do-not-leak",
            "device-rotated",
        );
        assert!(install_kimi_subscription(root.path(), account_id, &rotated).is_err());
        assert_eq!(
            load_provider_integration_config(root.path())
                .unwrap()
                .revision,
            1
        );
        let retained = build_kimi_subscription_route(root.path(), account_id).unwrap();
        assert_eq!(retained.access_token.as_str(), "access-initial-do-not-leak");
        assert_eq!(retained.state.device_id, "device-initial");

        let connection =
            rusqlite::Connection::open(crate::secrets::secret_store_path(root.path())).unwrap();
        connection
            .execute_batch("DROP TRIGGER reject_kimi_state_rotation;")
            .unwrap();
        drop(connection);
        assert_eq!(
            install_kimi_subscription(root.path(), account_id, &rotated).unwrap(),
            2
        );
        let installed = build_kimi_subscription_route(root.path(), account_id).unwrap();
        assert_eq!(
            installed.access_token.as_str(),
            "access-rotated-do-not-leak"
        );
        assert_eq!(installed.state.device_id, "device-rotated");

        let disconnected = crate::execution::cliproxyapi_host::disconnect_provider_subscription(
            root.path(),
            "kimi",
            account_id,
        )
        .unwrap();
        assert_eq!(disconnected["revision"], 3);
        assert_eq!(disconnected["deleted_secret_records"], 3);
        assert!(build_kimi_subscription_route(root.path(), account_id).is_err());
    }

    #[tokio::test]
    async fn caller_cannot_override_provider_account_model_or_format() {
        let root = tempfile::tempdir().unwrap();
        install_kimi_subscription(root.path(), "kimi-primary", &token_storage()).unwrap();
        let route = build_kimi_subscription_route_with_http(
            root.path(),
            "kimi-primary",
            Arc::new(RecordingHttp::default()),
        )
        .unwrap();
        let base = ExecutorRequest {
            auth_id: "other".to_owned(),
            auth_provider: "kimi".to_owned(),
            model: "k3[1m]".to_owned(),
            source_format: "openai-response".to_owned(),
            format: "openai-response".to_owned(),
            payload: br#"{"input":"hello"}"#.to_vec(),
            ..ExecutorRequest::default()
        };
        assert!(route.execute(base.clone()).await.is_err());
        let mut wrong_provider = base.clone();
        wrong_provider.auth_id = "kimi-primary".to_owned();
        wrong_provider.auth_provider = "codex".to_owned();
        assert!(route.execute(wrong_provider).await.is_err());
        let mut wrong_model = base.clone();
        wrong_model.auth_id = "kimi-primary".to_owned();
        wrong_model.model = "unconfigured".to_owned();
        assert!(route.execute(wrong_model).await.is_err());
        let mut wrong_format = base;
        wrong_format.auth_id = "kimi-primary".to_owned();
        wrong_format.source_format = "claude".to_owned();
        wrong_format.format = "claude".to_owned();
        assert!(route.execute(wrong_format).await.is_err());
    }

    #[tokio::test]
    async fn stream_route_keeps_exact_endpoint_auth_and_responses_format() {
        let root = tempfile::tempdir().unwrap();
        install_kimi_subscription(root.path(), "kimi-stream", &token_storage()).unwrap();
        let http = Arc::new(RecordingHttp::default());
        let route =
            build_kimi_subscription_route_with_http(root.path(), "kimi-stream", http.clone())
                .unwrap();
        let payload = br#"{"model":"k3[1m]","input":"hello","stream":true}"#;
        let mut response = route
            .execute_stream(ExecutorRequest {
                auth_id: "kimi-stream".to_owned(),
                auth_provider: "kimi".to_owned(),
                model: "k3[1m]".to_owned(),
                source_format: "openai-response".to_owned(),
                format: "openai-response".to_owned(),
                stream: true,
                payload: payload.to_vec(),
                original_request: payload.to_vec(),
                ..ExecutorRequest::default()
            })
            .await
            .unwrap();
        let mut output = Vec::new();
        while let Some(chunk) = response.chunks.recv().await {
            assert!(chunk.error.is_none());
            output.extend_from_slice(&chunk.payload);
        }
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("response.output_text.delta"));
        assert!(output.contains("response.completed"));
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, KIMI_CHAT_COMPLETIONS_URL);
        assert_eq!(
            requests[0].headers["Authorization"],
            ["Bearer access-kimi-do-not-leak"]
        );
        assert_eq!(requests[0].headers["Accept"], ["text/event-stream"]);
    }
}
