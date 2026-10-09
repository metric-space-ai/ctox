// Origin: CTOX
// License: AGPL-3.0-only

//! CTOX host adapters for the portable CLIProxyAPI Rust port.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use ctox_cliproxyapi::internal::api::server_routes::{
    AuxiliaryRouteChain, AuxiliaryRouteHandler, ClaudeCountTokensRouteHandler,
    CodexAlphaSearchAuthSelector, CodexAlphaSearchClient, CodexAlphaSearchCredentialSource,
    CodexAlphaSearchCredentials, CodexAlphaSearchCredentialsFuture, CodexAlphaSearchError,
    CodexAlphaSearchHttpTransport, CodexAlphaSearchRefresher, CodexAlphaSearchRouteHandler,
    CodexAlphaSearchSelectionFuture, CodexAlphaSearchStatusFuture,
};
use ctox_cliproxyapi::internal::auth::antigravity as antigravity_auth;
use ctox_cliproxyapi::internal::auth::claude::{
    AnthropicHttpTransport, ClaudeCredentialHandles, ClaudeRefreshCoordinator,
    ClaudeRefreshTransport, ClaudeSecretKind, ClaudeSecretStore, ClaudeStoredCredentials,
    OAuthInspectKind, OAuthInspectRequest, OAuthProfile, RefreshClock, SecretStoreError,
    SecretString,
};
use ctox_cliproxyapi::internal::auth::codex as codex_auth;
use ctox_cliproxyapi::internal::auth::codex::parse_jwt_token;
use ctox_cliproxyapi::internal::auth::codex::CodexHttpTransport;
use ctox_cliproxyapi::internal::cache::antigravity_reasoning_replay_cache::AntigravityReasoningReplayCache;
use ctox_cliproxyapi::internal::cache::{SignatureCacheStoreError, SignatureKvStore};
use ctox_cliproxyapi::internal::client::claude::models::ClaudeModel;
use ctox_cliproxyapi::internal::config::{
    AntigravitySubscriptionAccountConfig, ClaudeSubscriptionAccountConfig, CliproxyRuntimeConfig,
    CodexSubscriptionAccountConfig, RuntimeConfigError, RuntimeSecretRef, ValidatedRuntimeConfig,
};
use ctox_cliproxyapi::internal::runtime::executor::{
    AccountStateClock, AntigravityAccountPoolError, AntigravityAuthClock,
    AntigravityGenerateHttpTransport, AntigravityGenerateStreamingTransport,
    AntigravityGenerateTransport, AntigravitySubscriptionAccountPool, AntigravitySubscriptionAuth,
    AntigravitySubscriptionExecutor, ClaudeAccountPoolError, ClaudeCloakPolicy,
    ClaudeMessagesHttpTransport, ClaudeMessagesStreamingTransport, ClaudeMessagesTransport,
    ClaudeOAuthProfile, ClaudeOAuthProfileFetcher, ClaudeRequestAuthPreparer,
    ClaudeSubscriptionAccountPool, ClaudeSubscriptionAuth, ClaudeSubscriptionMessagesExecutor,
    CodexAccountPoolError, CodexResponsesHttpTransport, CodexResponsesStreamingTransport,
    CodexResponsesTransport, CodexSubscriptionAccountPool, CodexSubscriptionAuth,
    CodexSubscriptionResponsesExecutor,
};
use ctox_cliproxyapi::sdk::api::handlers::claude::code_handlers::{
    claude_models_response, ClaudeMessagesAntigravityHandler, ClaudeMessagesHttpResponse,
};
use ctox_cliproxyapi::sdk::api::handlers::openai::openai_responses_handlers::{
    OpenAiResponsesAntigravityHandler, OpenAiResponsesClaudeHandler, OpenAiResponsesCodexHandler,
    OpenAiResponsesHttpResponse, OpenAiResponsesProviderRouter, OpenAiResponsesRouteHandler,
    OpenAiResponsesRouteResponse,
};
use ctox_cliproxyapi::sdk::cliproxy::antigravity_models::{
    antigravity_model_discovery_targets, refresh_antigravity_model_capability_catalog,
    AntigravityModelCapabilityCatalog,
};
use ctox_cliproxyapi::sdk::cliproxy::auth::{
    AccountCandidate, AccountExecutionResult, AccountRouter, Auth, CooldownConductor,
    CooldownStateRecord, CooldownStateStore, CooldownStoreError, SchedulerStrategy,
};
use ctox_cliproxyapi::sdk::pluginapi::ExecutorRequest;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const COOLDOWN_STATE_PAYLOAD_KEY: &str = "cliproxyapi.auth.cooldown-state.v1";
const INSTANCE_CHATGPT_AUTH_SCOPE: &str = "ctox-auth";
const INSTANCE_CHATGPT_AUTH_NAME: &str = "chatgpt_subscription_auth_json";
const INSTANCE_CODEX_SECRET_SCOPE: &str = "provider-subscriptions";
const INSTANCE_CODEX_ID_TOKEN_NAME: &str = "codex-instance-id-token";
const INSTANCE_CODEX_ACCESS_TOKEN_NAME: &str = "codex-instance-access-token";
const INSTANCE_CODEX_REFRESH_TOKEN_NAME: &str = "codex-instance-refresh-token";
const INSTANCE_CODEX_ACCOUNT_ID: &str = "codex-instance-primary";
pub const INSTANCE_CODEX_PROXY_PORT: u16 = 12_435;
const INSTANCE_CODEX_PROXY_RETRY_SECONDS: u64 = 1;
const INSTANCE_MANAGEMENT_SECRET_SCOPE: &str = "cliproxyapi-management";
const INSTANCE_MANAGEMENT_SECRET_NAME: &str = "management-api-key";
pub const INSTANCE_MANAGEMENT_PORT: u16 = 12_436;
const INSTANCE_MANAGEMENT_RETRY_SECONDS: u64 = 1;
const INSTANCE_PROXY_CONFIG_TABLE: &str = "cliproxyapi_runtime_config";
const INSTANCE_PROXY_CONFIG_SCHEMA: &str = "ctox.cliproxyapi.runtime-config.v1";
const INSTANCE_ANTIGRAVITY_CAPABILITY_REFRESH_SECONDS: u64 = 10 * 60;
const INSTANCE_SIGNATURE_CACHE_TABLE: &str = "cliproxyapi_signature_cache";
const MAX_SIGNATURE_CACHE_KEY_BYTES: usize = 256;
const MAX_SIGNATURE_CACHE_VALUE_BYTES: usize = 1024 * 1024;

/// Runs the mirrored Antigravity catalog command through explicit CTOX-owned
/// capabilities. The portable command never discovers credentials, opens a
/// network client, or writes a path by itself.
#[allow(clippy::too_many_arguments)]
pub fn run_antigravity_catalog_command(
    options: &ctox_cliproxyapi::internal::cmd::fetch_antigravity_models::Options,
    auth_store: &dyn ctox_cliproxyapi::sdk::cliproxy::auth::AuthStore,
    secrets: &dyn ctox_cliproxyapi::internal::cmd::fetch_antigravity_models::SecretResolver,
    http: &dyn ctox_cliproxyapi::internal::cmd::fetch_antigravity_models::HttpTransport,
    cancellation: &dyn ctox_cliproxyapi::internal::cmd::fetch_antigravity_models::Cancellation,
    files: &dyn ctox_cliproxyapi::internal::cmd::fetch_antigravity_models::FileOutput,
    output: &dyn ctox_cliproxyapi::internal::cmd::fetch_antigravity_models::CommandOutput,
    request_timeout: Duration,
    user_agent: &str,
) -> Result<usize, ctox_cliproxyapi::internal::cmd::fetch_antigravity_models::CommandError> {
    use ctox_cliproxyapi::internal::cmd::fetch_antigravity_models as command;

    command::run(
        options,
        &command::Dependencies {
            auth_store,
            secrets,
            http,
            cancellation,
            files,
            output,
            request_timeout,
            user_agent,
        },
    )
}

/// Runs Codex catalog discovery through explicit CTOX stores, refresh,
/// transport, time, cancellation and filesystem capabilities.
#[allow(clippy::too_many_arguments)]
pub fn run_codex_catalog_command(
    options: &ctox_cliproxyapi::internal::cmd::fetch_codex_models::Options,
    auth_store: &dyn ctox_cliproxyapi::sdk::cliproxy::auth::AuthStore,
    secrets: &dyn ctox_cliproxyapi::internal::cmd::fetch_codex_models::SecretResolver,
    refresher: &dyn ctox_cliproxyapi::internal::cmd::fetch_codex_models::TokenRefresher,
    http: &dyn ctox_cliproxyapi::internal::cmd::fetch_codex_models::HttpTransport,
    clock: &dyn ctox_cliproxyapi::internal::cmd::fetch_codex_models::Clock,
    cancellation: &dyn ctox_cliproxyapi::internal::cmd::fetch_codex_models::Cancellation,
    files: &dyn ctox_cliproxyapi::internal::cmd::fetch_codex_models::FileOutput,
    output: &dyn ctox_cliproxyapi::internal::cmd::fetch_codex_models::CommandOutput,
    request_timeout: Duration,
) -> Result<usize, ctox_cliproxyapi::internal::cmd::fetch_codex_models::CommandError> {
    use ctox_cliproxyapi::internal::cmd::fetch_codex_models as command;

    command::run(
        options,
        &command::Dependencies {
            auth_store,
            secrets,
            refresher,
            http,
            clock,
            cancellation,
            files,
            output,
            request_timeout,
        },
    )
}

/// Runs the mirrored server command only after the outer CTOX runtime binds
/// typed config, filesystem, lifecycle, clock, cancellation and output
/// capabilities. No ambient cwd/config/service fallback exists here.
#[allow(clippy::too_many_arguments)]
pub fn run_cliproxy_server_command(
    options: &ctox_cliproxyapi::internal::cmd::server::Options,
    raw_args: &[String],
    config: &dyn ctox_cliproxyapi::internal::cmd::server::ConfigSource,
    files: &dyn ctox_cliproxyapi::internal::cmd::server::FileSystem,
    service: &dyn ctox_cliproxyapi::internal::cmd::server::ServiceHost,
    clock: &dyn ctox_cliproxyapi::internal::cmd::server::Clock,
    cancellation: &dyn ctox_cliproxyapi::internal::cmd::server::Cancellation,
    output: &dyn ctox_cliproxyapi::internal::cmd::server::CommandOutput,
) -> Result<(), ctox_cliproxyapi::internal::cmd::server::CommandError> {
    use ctox_cliproxyapi::internal::cmd::server as command;

    command::run(
        options,
        raw_args,
        &command::Dependencies {
            config,
            files,
            service,
            clock,
            cancellation,
            output,
        },
    )
}

#[derive(Debug)]
struct ProxyConfigRevisionConflict;

impl fmt::Display for ProxyConfigRevisionConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("proxy runtime config revision conflict")
    }
}

impl std::error::Error for ProxyConfigRevisionConflict {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredInstanceProxyConfig {
    pub schema: String,
    pub revision: u64,
    pub default_provider: String,
    pub runtime: CliproxyRuntimeConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EffectiveInstanceProxyConfig {
    default_provider: String,
    runtime: ValidatedRuntimeConfig,
    integration: crate::execution::cliproxyapi_integration::StoredProviderIntegrationConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceCodexProxyPhase {
    Stopped,
    WaitingForSubscription,
    Starting,
    Ready,
    Faulted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceCodexProxyStatus {
    pub phase: InstanceCodexProxyPhase,
    pub listen_addr: SocketAddr,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceProxyRouteCapability {
    pub provider: String,
    pub model: String,
    pub default: bool,
}

impl Default for InstanceCodexProxyStatus {
    fn default() -> Self {
        Self {
            phase: InstanceCodexProxyPhase::Stopped,
            listen_addr: instance_codex_proxy_addr(),
            last_error: None,
        }
    }
}

fn instance_codex_proxy_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), INSTANCE_CODEX_PROXY_PORT)
}

pub fn instance_codex_proxy_base_url() -> String {
    // pi-ai's OpenAI Responses client appends `responses` to this base URL.
    // The ported HTTP surface deliberately exposes only `/v1/responses`.
    format!("http://{}/v1", instance_codex_proxy_addr())
}

pub fn instance_claude_messages_base_url() -> String {
    // Anthropic-compatible clients append `/v1/messages` to the provider base.
    format!("http://{}", instance_codex_proxy_addr())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceManagementPhase {
    Stopped,
    WaitingForSecret,
    Starting,
    Ready,
    Faulted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceManagementStatus {
    pub phase: InstanceManagementPhase,
    pub listen_addr: SocketAddr,
    pub last_error: Option<String>,
}

impl Default for InstanceManagementStatus {
    fn default() -> Self {
        Self {
            phase: InstanceManagementPhase::Stopped,
            listen_addr: instance_management_addr(),
            last_error: None,
        }
    }
}

fn instance_management_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), INSTANCE_MANAGEMENT_PORT)
}

pub fn instance_management_base_url() -> String {
    format!("http://{}/v0/management", instance_management_addr())
}

fn instance_management_status_cell() -> &'static Mutex<Option<(PathBuf, InstanceManagementStatus)>>
{
    static STATUS: std::sync::OnceLock<Mutex<Option<(PathBuf, InstanceManagementStatus)>>> =
        std::sync::OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(None))
}

fn set_instance_management_status(
    root: &Path,
    phase: InstanceManagementPhase,
    last_error: Option<String>,
) {
    let mut status = instance_management_status_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *status = Some((
        root.to_path_buf(),
        InstanceManagementStatus {
            phase,
            listen_addr: instance_management_addr(),
            last_error,
        },
    ));
}

pub fn instance_management_status(root: &Path) -> InstanceManagementStatus {
    instance_management_status_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .filter(|(status_root, _)| status_root == root)
        .map(|(_, status)| status.clone())
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
struct CtoxManagementRuntimeStatusSource {
    root: PathBuf,
}

impl CtoxManagementRuntimeStatusSource {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeStatusSource
    for CtoxManagementRuntimeStatusSource
{
    fn snapshot(
        &self,
    ) -> ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeStatus {
        use ctox_cliproxyapi::internal::api::server_management::{
            ManagementRuntimeEndpoint, ManagementRuntimePhase, ManagementRuntimeStatus,
        };

        let main = crate::execution::responses::gateway::main_responses_gateway_status(&self.root);
        let codex = instance_codex_proxy_status(&self.root);
        let management = instance_management_status(&self.root);
        let runtime = crate::inference::runtime_state::load_runtime_state(&self.root)
            .ok()
            .flatten();

        ManagementRuntimeStatus {
            schema: "ctox.cliproxyapi.runtime-status.v1".to_owned(),
            main_responses_gateway: ManagementRuntimeEndpoint {
                phase: match main.phase {
                    crate::execution::responses::gateway::MainResponsesGatewayPhase::Stopped => {
                        ManagementRuntimePhase::Stopped
                    }
                    crate::execution::responses::gateway::MainResponsesGatewayPhase::Starting => {
                        ManagementRuntimePhase::Starting
                    }
                    crate::execution::responses::gateway::MainResponsesGatewayPhase::Ready => {
                        ManagementRuntimePhase::Ready
                    }
                    crate::execution::responses::gateway::MainResponsesGatewayPhase::Faulted => {
                        ManagementRuntimePhase::Faulted
                    }
                },
                listen_addr: main.listen_addr,
            },
            codex_subscription_gateway: ManagementRuntimeEndpoint {
                phase: match codex.phase {
                    InstanceCodexProxyPhase::Stopped => ManagementRuntimePhase::Stopped,
                    InstanceCodexProxyPhase::WaitingForSubscription => {
                        ManagementRuntimePhase::WaitingForSubscription
                    }
                    InstanceCodexProxyPhase::Starting => ManagementRuntimePhase::Starting,
                    InstanceCodexProxyPhase::Ready => ManagementRuntimePhase::Ready,
                    InstanceCodexProxyPhase::Faulted => ManagementRuntimePhase::Faulted,
                },
                listen_addr: codex.listen_addr.to_string(),
            },
            management_gateway: ManagementRuntimeEndpoint {
                phase: match management.phase {
                    InstanceManagementPhase::Stopped => ManagementRuntimePhase::Stopped,
                    InstanceManagementPhase::WaitingForSecret => {
                        ManagementRuntimePhase::WaitingForSecret
                    }
                    InstanceManagementPhase::Starting => ManagementRuntimePhase::Starting,
                    InstanceManagementPhase::Ready => ManagementRuntimePhase::Ready,
                    InstanceManagementPhase::Faulted => ManagementRuntimePhase::Faulted,
                },
                listen_addr: management.listen_addr.to_string(),
            },
            active_provider: runtime.as_ref().map(|state| {
                crate::inference::runtime_state::api_provider_for_runtime_state(state).to_owned()
            }),
            active_model: runtime
                .as_ref()
                .and_then(|state| state.active_or_selected_model().map(ToOwned::to_owned)),
        }
    }
}

#[derive(Debug, Clone)]
struct CtoxManagementRuntimeConfigSource {
    root: PathBuf,
}

impl CtoxManagementRuntimeConfigSource {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

fn management_runtime_config_summary(
    stored: &StoredInstanceProxyConfig,
) -> ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeConfigSummary {
    use ctox_cliproxyapi::internal::api::server_management::{
        ManagementProviderConfigSummary, ManagementRuntimeConfigSummary,
        MANAGEMENT_RUNTIME_CONFIG_SCHEMA,
    };

    let mut providers = Vec::new();
    let mut push = |provider: &str, accounts: usize, enabled: usize, models: Vec<String>| {
        if accounts == 0 {
            return;
        }
        let mut models = models;
        models.sort();
        models.dedup();
        providers.push(ManagementProviderConfigSummary {
            provider: provider.to_owned(),
            account_count: accounts,
            enabled_account_count: enabled,
            models,
        });
    };
    push(
        "claude",
        stored.runtime.claude_accounts.len(),
        stored
            .runtime
            .claude_accounts
            .iter()
            .filter(|account| !account.disabled)
            .count(),
        stored
            .runtime
            .claude_accounts
            .iter()
            .flat_map(|account| account.models.iter().cloned())
            .collect(),
    );
    push(
        "codex",
        stored.runtime.codex_accounts.len(),
        stored
            .runtime
            .codex_accounts
            .iter()
            .filter(|account| !account.disabled)
            .count(),
        stored
            .runtime
            .codex_accounts
            .iter()
            .flat_map(|account| account.models.iter().cloned())
            .collect(),
    );
    push(
        "antigravity",
        stored.runtime.antigravity_accounts.len(),
        stored
            .runtime
            .antigravity_accounts
            .iter()
            .filter(|account| !account.disabled)
            .count(),
        stored
            .runtime
            .antigravity_accounts
            .iter()
            .flat_map(|account| account.models.iter().cloned())
            .collect(),
    );
    providers.sort_by(|left, right| left.provider.cmp(&right.provider));
    ManagementRuntimeConfigSummary {
        schema: MANAGEMENT_RUNTIME_CONFIG_SCHEMA.to_owned(),
        revision: stored.revision,
        default_provider: stored.default_provider.clone(),
        providers,
    }
}

impl ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeConfigSource
    for CtoxManagementRuntimeConfigSource
{
    fn snapshot(
        &self,
    ) -> Result<
        Option<ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeConfigSummary>,
        ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeConfigError,
    > {
        load_instance_proxy_config(&self.root)
            .map(|stored| stored.as_ref().map(management_runtime_config_summary))
            .map_err(|_| {
                ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeConfigError::StoreUnavailable
            })
    }

    fn replace(
        &self,
        mutation: ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeConfigMutation,
    ) -> Result<
        ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeConfigSummary,
        ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeConfigError,
    > {
        use ctox_cliproxyapi::internal::api::server_management::ManagementRuntimeConfigError;

        if mutation.schema != INSTANCE_PROXY_CONFIG_SCHEMA
            || mutation.runtime.clone().validate().is_err()
            || validate_default_provider(&mutation.default_provider, &mutation.runtime).is_err()
            || !management_runtime_uses_provider_secret_scope(&mutation.runtime)
        {
            return Err(ManagementRuntimeConfigError::Invalid);
        }
        ensure_runtime_secrets_available(&self.root, &mutation.runtime)
            .map_err(|_| ManagementRuntimeConfigError::CredentialUnavailable)?;
        let stored = save_instance_proxy_config(
            &self.root,
            mutation.expected_revision,
            &mutation.default_provider,
            mutation.runtime,
        )
        .map_err(|error| {
            if error
                .downcast_ref::<ProxyConfigRevisionConflict>()
                .is_some()
            {
                ManagementRuntimeConfigError::RevisionConflict
            } else {
                ManagementRuntimeConfigError::StoreUnavailable
            }
        })?;
        Ok(management_runtime_config_summary(&stored))
    }
}

struct InstanceManagementKeySnapshot {
    key: Zeroizing<String>,
    fingerprint: [u8; 32],
}

impl InstanceManagementKeySnapshot {
    fn load(root: &Path) -> anyhow::Result<Option<Self>> {
        if !crate::secrets::secret_exists(
            root,
            INSTANCE_MANAGEMENT_SECRET_SCOPE,
            INSTANCE_MANAGEMENT_SECRET_NAME,
        )? {
            return Ok(None);
        }
        let key = Zeroizing::new(crate::secrets::read_secret_value(
            root,
            INSTANCE_MANAGEMENT_SECRET_SCOPE,
            INSTANCE_MANAGEMENT_SECRET_NAME,
        )?);
        anyhow::ensure!(
            key.as_bytes().len() >= 32,
            "CLIProxyAPI management key must contain at least 32 bytes"
        );
        let fingerprint = Sha256::digest(key.as_bytes()).into();
        Ok(Some(Self { key, fingerprint }))
    }
}

impl fmt::Debug for InstanceManagementKeySnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InstanceManagementKeySnapshot")
            .field("key", &"[REDACTED]")
            .field("fingerprint", &"[REDACTED]")
            .finish()
    }
}

fn instance_codex_proxy_status_cell() -> &'static Mutex<Option<(PathBuf, InstanceCodexProxyStatus)>>
{
    static STATUS: std::sync::OnceLock<Mutex<Option<(PathBuf, InstanceCodexProxyStatus)>>> =
        std::sync::OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(None))
}

fn set_instance_codex_proxy_status(
    root: &Path,
    phase: InstanceCodexProxyPhase,
    last_error: Option<String>,
) {
    let mut status = instance_codex_proxy_status_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *status = Some((
        root.to_path_buf(),
        InstanceCodexProxyStatus {
            phase,
            listen_addr: instance_codex_proxy_addr(),
            last_error,
        },
    ));
}

pub fn instance_codex_proxy_status(root: &Path) -> InstanceCodexProxyStatus {
    instance_codex_proxy_status_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .filter(|(status_root, _)| status_root == root)
        .map(|(_, status)| status.clone())
        .unwrap_or_default()
}

pub fn instance_proxy_route_capabilities(root: &Path) -> Vec<InstanceProxyRouteCapability> {
    if instance_codex_proxy_status(root).phase != InstanceCodexProxyPhase::Ready {
        return Vec::new();
    }
    let Ok(Some(effective)) = effective_instance_proxy_config(root) else {
        return Vec::new();
    };
    configured_instance_proxy_route_capabilities(root, &effective)
}

/// Resolve the sole account that can back a published provider/model route.
///
/// This is intentionally not part of the browser capability document. The
/// operator coding-agent smoke uses it to produce an account-id *hash* after
/// pinning the scheduler to a one-account topology. Multiple eligible accounts
/// fail closed because the selected scheduler account would otherwise not be
/// knowable before the request completes.
pub fn unique_instance_proxy_account_for_route(
    root: &Path,
    provider: &str,
    model: &str,
) -> anyhow::Result<String> {
    anyhow::ensure!(
        instance_codex_proxy_status(root).phase == InstanceCodexProxyPhase::Ready,
        "provider subscription listener is not ready"
    );
    let effective = effective_instance_proxy_config(root)?
        .context("provider subscription topology is unavailable")?;
    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim();
    let supports = |models: &[String]| {
        models.is_empty() || models.iter().any(|candidate| candidate.trim() == model)
    };
    let mut account_ids = match provider.as_str() {
        "claude" => effective
            .runtime
            .claude_accounts()
            .iter()
            .filter(|account| !account.disabled && supports(&account.models))
            .map(|account| account.id.clone())
            .collect::<Vec<_>>(),
        "codex" => effective
            .runtime
            .codex_accounts()
            .iter()
            .filter(|account| !account.disabled && supports(&account.models))
            .map(|account| account.id.clone())
            .collect::<Vec<_>>(),
        "antigravity" => effective
            .runtime
            .antigravity_accounts()
            .iter()
            .filter(|account| !account.disabled && supports(&account.models))
            .map(|account| account.id.clone())
            .collect::<Vec<_>>(),
        "kimi" => effective
            .integration
            .config
            .kimi_subscription_accounts
            .iter()
            .filter(|account| !account.disabled && supports(&account.effective_models()))
            .map(|account| account.id.clone())
            .collect::<Vec<_>>(),
        _ => anyhow::bail!("provider subscription route is unsupported"),
    };
    account_ids.sort();
    account_ids.dedup();
    anyhow::ensure!(
        account_ids.len() == 1,
        "provider subscription smoke requires exactly one eligible account"
    );
    Ok(account_ids.remove(0))
}

fn configured_instance_proxy_route_capabilities(
    root: &Path,
    effective: &EffectiveInstanceProxyConfig,
) -> Vec<InstanceProxyRouteCapability> {
    let _ = root;
    let mut routes = Vec::new();
    let mut append = |provider: &str, disabled: bool, models: &[String]| {
        if disabled {
            return;
        }
        let discovered;
        let models = if models.is_empty() {
            discovered = ctox_cliproxyapi::internal::registry::embedded_models_catalog()
                .ok()
                .and_then(|catalog| {
                    ctox_cliproxyapi::internal::registry::models_for_channel(&catalog, provider)
                })
                .unwrap_or_default()
                .into_iter()
                .map(|model| model.id)
                .collect::<Vec<_>>();
            discovered.as_slice()
        } else {
            models
        };
        routes.extend(models.iter().filter_map(|model| {
            let model = model.trim();
            (!model.is_empty()).then(|| InstanceProxyRouteCapability {
                provider: provider.to_owned(),
                model: model.to_owned(),
                default: provider == effective.default_provider,
            })
        }));
    };
    for account in effective.runtime.claude_accounts() {
        append("claude", account.disabled, &account.models);
    }
    for account in effective.runtime.codex_accounts() {
        append("codex", account.disabled, &account.models);
    }
    for account in effective.runtime.antigravity_accounts() {
        append("antigravity", account.disabled, &account.models);
    }
    for account in &effective.integration.config.kimi_subscription_accounts {
        append("kimi", account.disabled, &account.effective_models());
    }
    routes.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.model.cmp(&right.model))
    });
    routes.dedup_by(|left, right| left.provider == right.provider && left.model == right.model);
    routes
}

fn provider_model_catalog_snapshot(
    root: &Path,
    effective: &EffectiveInstanceProxyConfig,
) -> Vec<ClaudeModel> {
    let mut models = BTreeMap::<String, (BTreeSet<String>, bool)>::new();
    for route in configured_instance_proxy_route_capabilities(root, effective) {
        let entry = models.entry(route.model).or_default();
        entry.0.insert(route.provider);
        entry.1 |= route.default;
    }
    models
        .into_iter()
        .filter_map(|(model, (providers, default))| {
            serde_json::json!({
                "id": model,
                "object": "model",
                "owned_by": "ctox",
                "display_name": model,
                "providers": providers,
                "default": default,
            })
            .as_object()
            .cloned()
        })
        .collect()
}

#[cfg(test)]
pub fn mark_instance_codex_proxy_ready_for_test(root: &Path) {
    set_instance_codex_proxy_status(root, InstanceCodexProxyPhase::Ready, None);
}

/// Encrypted SQLite-backed Claude credential store.
///
/// This is the only layer allowed to translate portable secret handles into
/// CTOX `ctox-secrets.sqlite3` operations. It never falls back to runtime env.
#[derive(Clone)]
pub struct CtoxClaudeSecretStore {
    root: PathBuf,
}

impl CtoxClaudeSecretStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl fmt::Debug for CtoxClaudeSecretStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CtoxClaudeSecretStore")
            .field("root", &self.root)
            .field("backend", &"encrypted-sqlite")
            .finish()
    }
}

impl ClaudeSecretStore for CtoxClaudeSecretStore {
    fn load_credentials(
        &self,
        handles: &ClaudeCredentialHandles,
    ) -> Result<ClaudeStoredCredentials, SecretStoreError> {
        let access = handles.access_token();
        let refresh = handles.refresh_token();
        let access_exists =
            crate::secrets::secret_exists(&self.root, access.scope(), access.name())
                .map_err(|_| SecretStoreError::Read)?;
        let refresh_exists =
            crate::secrets::secret_exists(&self.root, refresh.scope(), refresh.name())
                .map_err(|_| SecretStoreError::Read)?;
        if !access_exists || !refresh_exists {
            return Err(SecretStoreError::Missing);
        }
        let values = crate::secrets::read_secret_values(
            &self.root,
            &[
                (access.scope(), access.name()),
                (refresh.scope(), refresh.name()),
            ],
        )
        .map_err(|_| SecretStoreError::Read)?;
        let mut values = values.into_iter();
        let access_token = values
            .next()
            .ok_or(SecretStoreError::Read)
            .and_then(|value| {
                SecretString::new(value).map_err(|_| SecretStoreError::InvalidValue)
            })?;
        let refresh_token = values
            .next()
            .ok_or(SecretStoreError::Read)
            .and_then(|value| {
                SecretString::new(value).map_err(|_| SecretStoreError::InvalidValue)
            })?;
        Ok(ClaudeStoredCredentials::new(access_token, refresh_token))
    }

    fn store_credentials(
        &self,
        handles: &ClaudeCredentialHandles,
        credentials: &ClaudeStoredCredentials,
    ) -> Result<(), SecretStoreError> {
        let access = handles.access_token();
        let refresh = handles.refresh_token();
        crate::secrets::write_secret_records(
            &self.root,
            &[
                crate::secrets::SecretRecordWrite {
                    scope: access.scope(),
                    name: access.name(),
                    value: credentials.access_token().expose_secret(),
                    description: Some("Claude subscription access_token"),
                    metadata: secret_metadata(ClaudeSecretKind::AccessToken),
                },
                crate::secrets::SecretRecordWrite {
                    scope: refresh.scope(),
                    name: refresh.name(),
                    value: credentials.refresh_token().expose_secret(),
                    description: Some("Claude subscription refresh_token"),
                    metadata: secret_metadata(ClaudeSecretKind::RefreshToken),
                },
            ],
        )
        .map_err(|_| SecretStoreError::Write)?;
        Ok(())
    }
}

/// Encrypted SQLite-backed Codex credential store.
///
/// ID, access and refresh tokens are loaded from one snapshot and rotated by
/// one `write_secret_records` transaction. No ambient environment fallback is
/// permitted.
#[derive(Clone)]
pub struct CtoxCodexSecretStore {
    root: PathBuf,
}

impl CtoxCodexSecretStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl fmt::Debug for CtoxCodexSecretStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CtoxCodexSecretStore")
            .field("root", &self.root)
            .field("backend", &"encrypted-sqlite")
            .finish()
    }
}

impl codex_auth::CodexSecretStore for CtoxCodexSecretStore {
    fn load_credentials(
        &self,
        handles: &codex_auth::CodexCredentialHandles,
    ) -> Result<codex_auth::CodexStoredCredentials, codex_auth::SecretStoreError> {
        if is_instance_codex_handles(handles) {
            let auth = load_instance_chatgpt_auth(&self.root)?;
            let tokens = auth.tokens.ok_or(codex_auth::SecretStoreError::Missing)?;
            return Ok(codex_auth::CodexStoredCredentials::new(
                codex_secret(tokens.id_token.raw_jwt)?,
                codex_secret(tokens.access_token)?,
                codex_secret(tokens.refresh_token)?,
            ));
        }
        let ordered = [
            handles.id_token(),
            handles.access_token(),
            handles.refresh_token(),
        ];
        for handle in ordered {
            let exists = crate::secrets::secret_exists(&self.root, handle.scope(), handle.name())
                .map_err(|_| codex_auth::SecretStoreError::Read)?;
            if !exists {
                return Err(codex_auth::SecretStoreError::Missing);
            }
        }
        let values = crate::secrets::read_secret_values(
            &self.root,
            &ordered
                .iter()
                .map(|handle| (handle.scope(), handle.name()))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| codex_auth::SecretStoreError::Read)?;
        let mut values = values.into_iter();
        let mut next = || {
            values
                .next()
                .ok_or(codex_auth::SecretStoreError::Read)
                .and_then(|value| {
                    codex_auth::SecretString::new(value)
                        .map_err(|_| codex_auth::SecretStoreError::InvalidValue)
                })
        };
        Ok(codex_auth::CodexStoredCredentials::new(
            next()?,
            next()?,
            next()?,
        ))
    }

    fn store_credentials(
        &self,
        handles: &codex_auth::CodexCredentialHandles,
        credentials: &codex_auth::CodexStoredCredentials,
    ) -> Result<(), codex_auth::SecretStoreError> {
        if is_instance_codex_handles(handles) {
            let mut auth = load_instance_chatgpt_auth(&self.root)?;
            let previous_account_id = auth
                .tokens
                .as_ref()
                .and_then(|tokens| tokens.account_id.clone());
            let id_token = credentials.id_token().expose_secret();
            auth.tokens = Some(ctox_core::token_data::TokenData {
                id_token: ctox_core::token_data::parse_chatgpt_jwt_claims(id_token)
                    .map_err(|_| codex_auth::SecretStoreError::InvalidValue)?,
                access_token: credentials.access_token().expose_secret().to_owned(),
                refresh_token: credentials.refresh_token().expose_secret().to_owned(),
                account_id: previous_account_id,
            });
            auth.last_refresh = Some(chrono::Utc::now());
            let serialized =
                serde_json::to_string(&auth).map_err(|_| codex_auth::SecretStoreError::Write)?;
            crate::secrets::write_secret_record(
                &self.root,
                INSTANCE_CHATGPT_AUTH_SCOPE,
                INSTANCE_CHATGPT_AUTH_NAME,
                &serialized,
                Some("ChatGPT Subscription OAuth state for this CTOX instance".to_owned()),
                serde_json::json!({
                    "source": "cliproxyapi_refresh",
                    "auth_mode": "chatgpt_subscription"
                }),
            )
            .map_err(|_| codex_auth::SecretStoreError::Write)?;
            return Ok(());
        }
        let id = handles.id_token();
        let access = handles.access_token();
        let refresh = handles.refresh_token();
        crate::secrets::write_secret_records(
            &self.root,
            &[
                crate::secrets::SecretRecordWrite {
                    scope: id.scope(),
                    name: id.name(),
                    value: credentials.id_token().expose_secret(),
                    description: Some("Codex subscription id_token"),
                    metadata: codex_secret_metadata(codex_auth::CodexSecretKind::IdToken),
                },
                crate::secrets::SecretRecordWrite {
                    scope: access.scope(),
                    name: access.name(),
                    value: credentials.access_token().expose_secret(),
                    description: Some("Codex subscription access_token"),
                    metadata: codex_secret_metadata(codex_auth::CodexSecretKind::AccessToken),
                },
                crate::secrets::SecretRecordWrite {
                    scope: refresh.scope(),
                    name: refresh.name(),
                    value: credentials.refresh_token().expose_secret(),
                    description: Some("Codex subscription refresh_token"),
                    metadata: codex_secret_metadata(codex_auth::CodexSecretKind::RefreshToken),
                },
            ],
        )
        .map_err(|_| codex_auth::SecretStoreError::Write)
    }
}

fn codex_secret(value: String) -> Result<codex_auth::SecretString, codex_auth::SecretStoreError> {
    codex_auth::SecretString::new(value).map_err(|_| codex_auth::SecretStoreError::InvalidValue)
}

fn is_instance_codex_handles(handles: &codex_auth::CodexCredentialHandles) -> bool {
    let expected = [
        (INSTANCE_CODEX_ID_TOKEN_NAME, handles.id_token()),
        (INSTANCE_CODEX_ACCESS_TOKEN_NAME, handles.access_token()),
        (INSTANCE_CODEX_REFRESH_TOKEN_NAME, handles.refresh_token()),
    ];
    expected.iter().all(|(name, handle)| {
        handle.scope() == INSTANCE_CODEX_SECRET_SCOPE && handle.name() == *name
    })
}

fn load_instance_chatgpt_auth(
    root: &Path,
) -> Result<ctox_core::auth::AuthDotJson, codex_auth::SecretStoreError> {
    let exists = crate::secrets::secret_exists(
        root,
        INSTANCE_CHATGPT_AUTH_SCOPE,
        INSTANCE_CHATGPT_AUTH_NAME,
    )
    .map_err(|_| codex_auth::SecretStoreError::Read)?;
    if !exists {
        return Err(codex_auth::SecretStoreError::Missing);
    }
    let serialized = crate::secrets::read_secret_value(
        root,
        INSTANCE_CHATGPT_AUTH_SCOPE,
        INSTANCE_CHATGPT_AUTH_NAME,
    )
    .map_err(|_| codex_auth::SecretStoreError::Read)?;
    serde_json::from_str(&serialized).map_err(|_| codex_auth::SecretStoreError::InvalidValue)
}

fn open_instance_proxy_config_db(root: &Path) -> anyhow::Result<Connection> {
    let path = crate::inference::runtime_env::runtime_config_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {INSTANCE_PROXY_CONFIG_TABLE} (\
         config_id INTEGER PRIMARY KEY CHECK(config_id = 1), \
         revision INTEGER NOT NULL, default_provider TEXT NOT NULL, \
         config_json TEXT NOT NULL, updated_at_ms INTEGER NOT NULL)"
    ))?;
    Ok(conn)
}

fn open_instance_proxy_config_db_read_only(root: &Path) -> anyhow::Result<Option<Connection>> {
    let path = crate::inference::runtime_env::runtime_config_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let table_exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [INSTANCE_PROXY_CONFIG_TABLE],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(table_exists.then_some(conn))
}

/// Provider-scoped durable signature replay state. Keys contain only the
/// provider/model group plus a full SHA-256 digest of thinking text; values are
/// opaque provider signatures. The store is bounded and never exposed through
/// Business OS or an HTTP management endpoint.
#[derive(Clone)]
pub struct CtoxSignatureKvStore {
    root: PathBuf,
    operation_lock: Arc<Mutex<()>>,
}

impl CtoxSignatureKvStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    fn open(&self) -> Result<Connection, SignatureCacheStoreError> {
        let conn = open_instance_proxy_config_db(&self.root)
            .map_err(|_| SignatureCacheStoreError::Unavailable)?;
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {INSTANCE_SIGNATURE_CACHE_TABLE} (\
             cache_key TEXT PRIMARY KEY, value BLOB NOT NULL, \
             expires_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL)"
        ))
        .map_err(|_| SignatureCacheStoreError::Unavailable)?;
        Ok(conn)
    }

    fn now_ms() -> Result<i64, SignatureCacheStoreError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_millis()).ok())
            .ok_or(SignatureCacheStoreError::Unavailable)
    }

    fn deadline_ms(now_ms: i64, ttl: std::time::Duration) -> Result<i64, SignatureCacheStoreError> {
        let ttl_ms = i64::try_from(ttl.as_millis())
            .ok()
            .filter(|ttl_ms| *ttl_ms > 0)
            .ok_or(SignatureCacheStoreError::Write)?;
        now_ms
            .checked_add(ttl_ms)
            .ok_or(SignatureCacheStoreError::Write)
    }

    fn valid_key(key: &str) -> bool {
        !key.is_empty()
            && key.len() <= MAX_SIGNATURE_CACHE_KEY_BYTES
            && !key.chars().any(char::is_control)
    }
}

impl fmt::Debug for CtoxSignatureKvStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CtoxSignatureKvStore")
            .field("root", &self.root)
            .field("backend", &"ctox-sqlite-runtime")
            .finish()
    }
}

impl SignatureKvStore for CtoxSignatureKvStore {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, SignatureCacheStoreError> {
        if !Self::valid_key(key) {
            return Err(SignatureCacheStoreError::Read);
        }
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| SignatureCacheStoreError::Read)?;
        let mut conn = self.open()?;
        let now_ms = Self::now_ms()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| SignatureCacheStoreError::Read)?;
        transaction
            .execute(
                &format!(
                    "DELETE FROM {INSTANCE_SIGNATURE_CACHE_TABLE} \
                     WHERE cache_key = ?1 AND expires_at_ms <= ?2"
                ),
                params![key, now_ms],
            )
            .map_err(|_| SignatureCacheStoreError::Read)?;
        let value = transaction
            .query_row(
                &format!("SELECT value FROM {INSTANCE_SIGNATURE_CACHE_TABLE} WHERE cache_key = ?1"),
                [key],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(|_| SignatureCacheStoreError::Read)?;
        transaction
            .commit()
            .map_err(|_| SignatureCacheStoreError::Read)?;
        Ok(value)
    }

    fn set(
        &self,
        key: &str,
        value: &[u8],
        ttl: std::time::Duration,
    ) -> Result<bool, SignatureCacheStoreError> {
        if !Self::valid_key(key)
            || value.is_empty()
            || value.len() > MAX_SIGNATURE_CACHE_VALUE_BYTES
        {
            return Err(SignatureCacheStoreError::Write);
        }
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| SignatureCacheStoreError::Write)?;
        let conn = self.open()?;
        let now_ms = Self::now_ms()?;
        let expires_at_ms = Self::deadline_ms(now_ms, ttl)?;
        let changed = conn
            .execute(
                &format!(
                    "INSERT INTO {INSTANCE_SIGNATURE_CACHE_TABLE} \
                     (cache_key, value, expires_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT(cache_key) DO UPDATE SET value = excluded.value, \
                     expires_at_ms = excluded.expires_at_ms, updated_at_ms = excluded.updated_at_ms"
                ),
                params![key, value, expires_at_ms, now_ms],
            )
            .map_err(|_| SignatureCacheStoreError::Write)?;
        Ok(changed == 1)
    }

    fn delete(&self, key: &str) -> Result<bool, SignatureCacheStoreError> {
        if !Self::valid_key(key) {
            return Err(SignatureCacheStoreError::Delete);
        }
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| SignatureCacheStoreError::Delete)?;
        let conn = self.open()?;
        conn.execute(
            &format!("DELETE FROM {INSTANCE_SIGNATURE_CACHE_TABLE} WHERE cache_key = ?1"),
            [key],
        )
        .map(|changed| changed == 1)
        .map_err(|_| SignatureCacheStoreError::Delete)
    }

    fn expire(
        &self,
        key: &str,
        ttl: std::time::Duration,
    ) -> Result<bool, SignatureCacheStoreError> {
        if !Self::valid_key(key) {
            return Err(SignatureCacheStoreError::Expire);
        }
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| SignatureCacheStoreError::Expire)?;
        let mut conn = self.open()?;
        let now_ms = Self::now_ms()?;
        let expires_at_ms =
            Self::deadline_ms(now_ms, ttl).map_err(|_| SignatureCacheStoreError::Expire)?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| SignatureCacheStoreError::Expire)?;
        transaction
            .execute(
                &format!(
                    "DELETE FROM {INSTANCE_SIGNATURE_CACHE_TABLE} \
                     WHERE cache_key = ?1 AND expires_at_ms <= ?2"
                ),
                params![key, now_ms],
            )
            .map_err(|_| SignatureCacheStoreError::Expire)?;
        let changed = transaction
            .execute(
                &format!(
                    "UPDATE {INSTANCE_SIGNATURE_CACHE_TABLE} SET expires_at_ms = ?2, \
                     updated_at_ms = ?3 WHERE cache_key = ?1"
                ),
                params![key, expires_at_ms, now_ms],
            )
            .map_err(|_| SignatureCacheStoreError::Expire)?;
        transaction
            .commit()
            .map_err(|_| SignatureCacheStoreError::Expire)?;
        Ok(changed == 1)
    }
}

fn validate_default_provider(
    default_provider: &str,
    config: &CliproxyRuntimeConfig,
) -> anyhow::Result<String> {
    let provider = default_provider.trim().to_ascii_lowercase();
    let has_enabled = config
        .claude_accounts
        .iter()
        .any(|account| !account.disabled)
        || config
            .codex_accounts
            .iter()
            .any(|account| !account.disabled)
        || config
            .antigravity_accounts
            .iter()
            .any(|account| !account.disabled);
    if !has_enabled && provider.is_empty() {
        return Ok(provider);
    }
    let configured = match provider.as_str() {
        "claude" => config
            .claude_accounts
            .iter()
            .any(|account| !account.disabled),
        "codex" => config
            .codex_accounts
            .iter()
            .any(|account| !account.disabled),
        "antigravity" => config
            .antigravity_accounts
            .iter()
            .any(|account| !account.disabled),
        _ => false,
    };
    anyhow::ensure!(configured, "default proxy provider is not enabled");
    Ok(provider)
}

fn validate_persisted_proxy_topology(config: &CliproxyRuntimeConfig) -> anyhow::Result<()> {
    let empty = config.claude_accounts.is_empty()
        && config.codex_accounts.is_empty()
        && config.antigravity_accounts.is_empty();
    if empty {
        anyhow::ensure!(
            config.request_timeout_ms > 0 && config.request_timeout_ms <= 10 * 60 * 1_000,
            "proxy runtime config is invalid"
        );
        return Ok(());
    }
    config
        .clone()
        .validate()
        .map(|_| ())
        .map_err(|_| anyhow::anyhow!("proxy runtime config is invalid"))
}

/// Loads the strict, non-secret proxy topology from the typed runtime SQLite
/// store. Secret values remain referenced by scope/name and are never encoded
/// into this document.
pub fn load_instance_proxy_config(
    root: &Path,
) -> anyhow::Result<Option<StoredInstanceProxyConfig>> {
    let Some(conn) = open_instance_proxy_config_db_read_only(root)? else {
        return Ok(None);
    };
    let row = conn
        .query_row(
            &format!(
                "SELECT revision, default_provider, config_json \
                 FROM {INSTANCE_PROXY_CONFIG_TABLE} WHERE config_id = 1"
            ),
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((revision, default_provider, config_json)) = row else {
        return Ok(None);
    };
    let revision =
        u64::try_from(revision).map_err(|_| anyhow::anyhow!("invalid proxy revision"))?;
    let runtime: CliproxyRuntimeConfig = serde_json::from_str(&config_json)
        .map_err(|_| anyhow::anyhow!("stored proxy runtime config is invalid"))?;
    validate_persisted_proxy_topology(&runtime)
        .map_err(|_| anyhow::anyhow!("stored proxy runtime config is invalid"))?;
    let default_provider = validate_default_provider(&default_provider, &runtime)?;
    Ok(Some(StoredInstanceProxyConfig {
        schema: INSTANCE_PROXY_CONFIG_SCHEMA.to_owned(),
        revision,
        default_provider,
        runtime,
    }))
}

/// Persists one complete proxy topology with optimistic revision checking.
/// The immediate transaction makes configuration replacement a single-writer
/// operation and prevents a stale Business OS/Management mutation from
/// silently overwriting a newer operator choice.
pub fn save_instance_proxy_config(
    root: &Path,
    expected_revision: u64,
    default_provider: &str,
    runtime: CliproxyRuntimeConfig,
) -> anyhow::Result<StoredInstanceProxyConfig> {
    validate_persisted_proxy_topology(&runtime)?;
    let default_provider = validate_default_provider(default_provider, &runtime)?;
    let config_json = serde_json::to_string(&runtime)?;
    let mut conn = open_instance_proxy_config_db(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = tx
        .query_row(
            &format!("SELECT revision FROM {INSTANCE_PROXY_CONFIG_TABLE} WHERE config_id = 1"),
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(u64::try_from)
        .transpose()
        .map_err(|_| anyhow::anyhow!("invalid proxy revision"))?
        .unwrap_or(0);
    if current != expected_revision {
        return Err(anyhow::Error::new(ProxyConfigRevisionConflict));
    }
    let revision = current
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("proxy runtime config revision overflow"))?;
    let updated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let updated_at_ms = i64::try_from(updated_at_ms).unwrap_or(i64::MAX);
    tx.execute(
        &format!(
            "INSERT INTO {INSTANCE_PROXY_CONFIG_TABLE} \
             (config_id, revision, default_provider, config_json, updated_at_ms) \
             VALUES (1, ?1, ?2, ?3, ?4) \
             ON CONFLICT(config_id) DO UPDATE SET revision=excluded.revision, \
             default_provider=excluded.default_provider, config_json=excluded.config_json, \
             updated_at_ms=excluded.updated_at_ms"
        ),
        params![revision, default_provider, config_json, updated_at_ms],
    )?;
    tx.commit()?;
    Ok(StoredInstanceProxyConfig {
        schema: INSTANCE_PROXY_CONFIG_SCHEMA.to_owned(),
        revision,
        default_provider,
        runtime,
    })
}

fn subscription_secret(account_id: &str, suffix: &str) -> RuntimeSecretRef {
    RuntimeSecretRef {
        scope: INSTANCE_CODEX_SECRET_SCOPE.to_owned(),
        name: format!("{account_id}-{suffix}"),
    }
}

fn editable_proxy_config(root: &Path) -> anyhow::Result<(u64, String, CliproxyRuntimeConfig)> {
    if let Some(stored) = load_instance_proxy_config(root)? {
        return Ok((stored.revision, stored.default_provider, stored.runtime));
    }
    Ok((
        0,
        String::new(),
        CliproxyRuntimeConfig {
            request_timeout_ms: 30_000,
            routing_strategy: SchedulerStrategy::RoundRobin,
            claude_accounts: Vec::new(),
            codex_accounts: Vec::new(),
            antigravity_accounts: Vec::new(),
        },
    ))
}

fn mutate_proxy_config(
    root: &Path,
    preferred_provider: &str,
    mut mutation: impl FnMut(&mut CliproxyRuntimeConfig),
) -> anyhow::Result<()> {
    for _ in 0..4 {
        let (revision, mut default_provider, mut runtime) = editable_proxy_config(root)?;
        mutation(&mut runtime);
        if default_provider.is_empty() {
            default_provider = preferred_provider.to_owned();
        }
        match save_instance_proxy_config(root, revision, &default_provider, runtime) {
            Ok(_) => return Ok(()),
            Err(error)
                if error
                    .downcast_ref::<ProxyConfigRevisionConflict>()
                    .is_some() =>
            {
                continue
            }
            Err(error) => return Err(error),
        }
    }
    anyhow::bail!("proxy runtime config changed concurrently")
}

/// Installs a completed Claude OAuth exchange into the encrypted CTOX store
/// and atomically adds the account to the portable proxy topology.
pub fn install_claude_subscription(
    root: &Path,
    account_id: &str,
    credentials: &ClaudeStoredCredentials,
) -> anyhow::Result<()> {
    let _lifecycle = crate::secrets::credential_lifecycle_guard();
    let account_id = account_id.trim();
    anyhow::ensure!(!account_id.is_empty(), "Claude account id is required");
    let access = subscription_secret(account_id, "access-token");
    let refresh = subscription_secret(account_id, "refresh-token");
    let handles = ClaudeCredentialHandles::new(
        ctox_cliproxyapi::internal::auth::claude::ClaudeSecretHandle::new(
            access.scope.clone(),
            access.name.clone(),
            ClaudeSecretKind::AccessToken,
        )?,
        ctox_cliproxyapi::internal::auth::claude::ClaudeSecretHandle::new(
            refresh.scope.clone(),
            refresh.name.clone(),
            ClaudeSecretKind::RefreshToken,
        )?,
    )?;
    CtoxClaudeSecretStore::new(root).store_credentials(&handles, credentials)?;
    mutate_proxy_config(root, "claude", |runtime| {
        runtime
            .claude_accounts
            .retain(|account| account.id != account_id);
        runtime
            .claude_accounts
            .push(ClaudeSubscriptionAccountConfig {
                id: account_id.to_owned(),
                disabled: false,
                priority: 100,
                weight: 1,
                websockets: false,
                models: Vec::new(),
                access_token_secret: access.clone(),
                refresh_token_secret: refresh.clone(),
                upstream_scheme: "https".to_owned(),
                upstream_authority: "api.anthropic.com".to_owned(),
                proxy_url_secret: None,
                device_profile: None,
                timezone: "UTC".to_owned(),
            });
    })
}

/// Installs a completed Antigravity OAuth exchange without exposing tokens to
/// Business OS or its replicated command documents.
pub fn install_antigravity_subscription(
    root: &Path,
    account_id: &str,
    credentials: &antigravity_auth::AntigravityStoredCredentials,
) -> anyhow::Result<()> {
    let _lifecycle = crate::secrets::credential_lifecycle_guard();
    let account_id = account_id.trim();
    anyhow::ensure!(!account_id.is_empty(), "Antigravity account id is required");
    let access = subscription_secret(account_id, "access-token");
    let refresh = subscription_secret(account_id, "refresh-token");
    let state = subscription_secret(account_id, "state");
    let handles = antigravity_auth::AntigravityCredentialHandles::new(
        antigravity_auth::AntigravitySecretHandle::new(
            access.scope.clone(),
            access.name.clone(),
            antigravity_auth::AntigravitySecretKind::AccessToken,
        )?,
        antigravity_auth::AntigravitySecretHandle::new(
            refresh.scope.clone(),
            refresh.name.clone(),
            antigravity_auth::AntigravitySecretKind::RefreshToken,
        )?,
        antigravity_auth::AntigravitySecretHandle::new(
            state.scope.clone(),
            state.name.clone(),
            antigravity_auth::AntigravitySecretKind::State,
        )?,
    )?;
    use antigravity_auth::AntigravitySecretStore as _;
    CtoxAntigravitySecretStore::new(root).store_credentials(&handles, credentials)?;
    mutate_proxy_config(root, "antigravity", |runtime| {
        runtime
            .antigravity_accounts
            .retain(|account| account.id != account_id);
        runtime
            .antigravity_accounts
            .push(AntigravitySubscriptionAccountConfig {
                id: account_id.to_owned(),
                disabled: false,
                priority: 100,
                weight: 1,
                websockets: false,
                models: Vec::new(),
                access_token_secret: access.clone(),
                refresh_token_secret: refresh.clone(),
                state_secret: state.clone(),
                upstream_base_url:
                    ctox_cliproxyapi::internal::runtime::executor::DEFAULT_ANTIGRAVITY_BASE_URL
                        .to_owned(),
                proxy_url_secret: None,
            });
    })
}

/// Non-secret Business OS projection of all executable subscription pools.
fn projected_credential_tuple_is_ready(root: &Path, secrets: &[&RuntimeSecretRef]) -> bool {
    if secrets.is_empty()
        || secrets
            .iter()
            .any(|secret| secret.scope != INSTANCE_CODEX_SECRET_SCOPE)
    {
        return false;
    }
    let keys = secrets
        .iter()
        .map(|secret| (secret.scope.as_str(), secret.name.as_str()))
        .collect::<Vec<_>>();
    // A rotation must not expose a mixture of old and new token readiness.
    // Reuse the secret store's single-snapshot read instead of reopening it
    // for each member of the credential tuple.
    crate::secrets::read_secret_values(root, &keys)
        .is_ok_and(|values| values.iter().all(|value| !value.trim().is_empty()))
}

pub fn provider_subscription_status(root: &Path) -> serde_json::Value {
    let stored = load_instance_proxy_config(root).ok().flatten();
    let integration =
        crate::execution::cliproxyapi_integration::load_provider_integration_config(root).ok();
    let mut accounts = Vec::new();
    if let Some(stored) = &stored {
        accounts.extend(stored.runtime.claude_accounts.iter().map(|account| {
            let ready = !account.disabled
                && projected_credential_tuple_is_ready(
                    root,
                    &[&account.access_token_secret, &account.refresh_token_secret],
                );
            serde_json::json!({
                "id": account.id, "provider": "claude", "enabled": !account.disabled, "ready": ready,
                "status": if account.disabled { "disabled" } else if ready { "ready" } else { "error" }
            })
        }));
        accounts.extend(stored.runtime.codex_accounts.iter().map(|account| {
            let ready = !account.disabled
                && projected_credential_tuple_is_ready(
                    root,
                    &[
                        &account.id_token_secret,
                        &account.access_token_secret,
                        &account.refresh_token_secret,
                    ],
                );
            serde_json::json!({
                "id": account.id, "provider": "codex", "enabled": !account.disabled, "ready": ready,
                "status": if account.disabled { "disabled" } else if ready { "ready" } else { "error" }
            })
        }));
        accounts.extend(stored.runtime.antigravity_accounts.iter().map(|account| {
            let ready = !account.disabled
                && projected_credential_tuple_is_ready(
                    root,
                    &[
                        &account.access_token_secret,
                        &account.refresh_token_secret,
                        &account.state_secret,
                    ],
                );
            serde_json::json!({
                "id": account.id, "provider": "antigravity", "enabled": !account.disabled, "ready": ready,
                "status": if account.disabled { "disabled" } else if ready { "ready" } else { "error" }
            })
        }));
    }
    if let Some(integration) = &integration {
        accounts.extend(
            integration
                .config
                .kimi_subscription_accounts
                .iter()
                .map(|account| {
                    let ready = !account.disabled
                        && crate::execution::cliproxyapi_integration::build_kimi_subscription_route(
                            root,
                            &account.id,
                        )
                        .is_ok();
                    serde_json::json!({
                        "id": account.id,
                        "provider": "kimi",
                        "enabled": !account.disabled,
                        "ready": ready,
                        "status": if account.disabled { "disabled" } else if ready { "ready" } else { "error" },
                        "models": account.effective_models(),
                    })
                }),
        );
    }
    if crate::execution::cliproxyapi_xai::subscription_installed(root) {
        accounts.push(serde_json::json!({"id": crate::execution::cliproxyapi_xai::ACCOUNT_ID, "provider": "xai", "enabled": true, "ready": true, "status": "authenticated"}));
    }
    if instance_codex_runtime_config(root).ok().flatten().is_some()
        && !accounts.iter().any(|a| {
            a.get("id").and_then(serde_json::Value::as_str) == Some(INSTANCE_CODEX_ACCOUNT_ID)
        })
    {
        accounts.push(serde_json::json!({"id": INSTANCE_CODEX_ACCOUNT_ID, "provider": "codex", "enabled": true, "managed_by": "ctox-auth"}));
    }
    serde_json::json!({
        "schema": "ctox.provider-subscriptions.v1",
        "revision": stored.as_ref().map_or(0, |value| value.revision),
        "integration_revision": integration.as_ref().map_or(0, |value| value.revision),
        "default_provider": stored.as_ref().map_or("", |value| value.default_provider.as_str()),
        "accounts": accounts,
        "providers": [
            {"id": "codex", "label": "ChatGPT / Codex", "flow": "device_code"},
            {"id": "claude", "label": "Claude", "flow": "browser_callback"},
            {"id": "antigravity", "label": "Google Antigravity", "flow": "browser_callback"},
            {"id": "kimi", "label": "Kimi Code", "flow": "device_code"},
            {"id": "xai", "label": "Grok Build", "flow": "device_code"}
        ]
    })
}

fn first_enabled_proxy_provider(runtime: &CliproxyRuntimeConfig) -> String {
    if runtime
        .claude_accounts
        .iter()
        .any(|account| !account.disabled)
    {
        "claude".to_owned()
    } else if runtime
        .codex_accounts
        .iter()
        .any(|account| !account.disabled)
    {
        "codex".to_owned()
    } else if runtime
        .antigravity_accounts
        .iter()
        .any(|account| !account.disabled)
    {
        "antigravity".to_owned()
    } else {
        String::new()
    }
}

fn runtime_secret_keys(runtime: &CliproxyRuntimeConfig) -> Vec<(String, String)> {
    let mut keys = Vec::new();
    let mut push = |secret: &RuntimeSecretRef| {
        keys.push((secret.scope.clone(), secret.name.clone()));
    };
    for account in &runtime.claude_accounts {
        push(&account.access_token_secret);
        push(&account.refresh_token_secret);
        if let Some(secret) = &account.proxy_url_secret {
            push(secret);
        }
    }
    for account in &runtime.codex_accounts {
        push(&account.id_token_secret);
        push(&account.access_token_secret);
        push(&account.refresh_token_secret);
        if let Some(secret) = &account.proxy_url_secret {
            push(secret);
        }
    }
    for account in &runtime.antigravity_accounts {
        push(&account.access_token_secret);
        push(&account.refresh_token_secret);
        push(&account.state_secret);
        if let Some(secret) = &account.proxy_url_secret {
            push(secret);
        }
    }
    keys
}

fn remove_instance_proxy_subscription_from_topology(
    root: &Path,
    provider: &str,
    account_id: &str,
) -> anyhow::Result<(u64, Vec<RuntimeSecretRef>)> {
    for _ in 0..4 {
        let stored = load_instance_proxy_config(root)?
            .ok_or_else(|| anyhow::anyhow!("provider subscription account was not found"))?;
        let mut runtime = stored.runtime;
        let mut secrets = Vec::new();
        match provider {
            "claude" => {
                let account = runtime
                    .claude_accounts
                    .iter()
                    .find(|account| account.id == account_id)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!("provider subscription account was not found")
                    })?;
                secrets.extend([account.access_token_secret, account.refresh_token_secret]);
                if let Some(secret) = account.proxy_url_secret {
                    secrets.push(secret);
                }
                runtime
                    .claude_accounts
                    .retain(|account| account.id != account_id);
            }
            "codex" => {
                anyhow::ensure!(
                    account_id != INSTANCE_CODEX_ACCOUNT_ID,
                    "the instance-managed ChatGPT subscription cannot be disconnected here"
                );
                let account = runtime
                    .codex_accounts
                    .iter()
                    .find(|account| account.id == account_id)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!("provider subscription account was not found")
                    })?;
                secrets.extend([
                    account.id_token_secret,
                    account.access_token_secret,
                    account.refresh_token_secret,
                ]);
                if let Some(secret) = account.proxy_url_secret {
                    secrets.push(secret);
                }
                runtime
                    .codex_accounts
                    .retain(|account| account.id != account_id);
            }
            "antigravity" => {
                let account = runtime
                    .antigravity_accounts
                    .iter()
                    .find(|account| account.id == account_id)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!("provider subscription account was not found")
                    })?;
                secrets.extend([
                    account.access_token_secret,
                    account.refresh_token_secret,
                    account.state_secret,
                ]);
                if let Some(secret) = account.proxy_url_secret {
                    secrets.push(secret);
                }
                runtime
                    .antigravity_accounts
                    .retain(|account| account.id != account_id);
            }
            _ => anyhow::bail!("unsupported provider subscription"),
        }
        let mut unique = BTreeSet::new();
        secrets.retain(|secret| unique.insert((secret.scope.clone(), secret.name.clone())));
        anyhow::ensure!(
            secrets
                .iter()
                .all(|secret| secret.scope == INSTANCE_CODEX_SECRET_SCOPE),
            "provider credential reference is outside the provider subscription scope"
        );
        let remaining = runtime_secret_keys(&runtime);
        anyhow::ensure!(
            secrets.iter().all(|secret| !remaining
                .iter()
                .any(|key| key == &(secret.scope.clone(), secret.name.clone()))),
            "provider credential reference is shared by another account"
        );
        let default_provider = if stored.default_provider == provider
            && !match provider {
                "claude" => runtime
                    .claude_accounts
                    .iter()
                    .any(|account| !account.disabled),
                "codex" => runtime
                    .codex_accounts
                    .iter()
                    .any(|account| !account.disabled),
                "antigravity" => runtime
                    .antigravity_accounts
                    .iter()
                    .any(|account| !account.disabled),
                _ => false,
            } {
            first_enabled_proxy_provider(&runtime)
        } else {
            stored.default_provider
        };
        match save_instance_proxy_config(root, stored.revision, &default_provider, runtime) {
            Ok(stored) => return Ok((stored.revision, secrets)),
            Err(error)
                if error
                    .downcast_ref::<ProxyConfigRevisionConflict>()
                    .is_some() =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    anyhow::bail!("proxy runtime config changed concurrently")
}

/// Removes one server-owned provider account before deleting its encrypted
/// credential tuple. The implementation is completed below with the topology
/// helpers; this public boundary is intentionally provider-neutral for native
/// Business OS commands.
pub fn disconnect_provider_subscription(
    root: &Path,
    provider: &str,
    account_id: &str,
) -> anyhow::Result<serde_json::Value> {
    let _lifecycle = crate::secrets::credential_lifecycle_guard();
    let provider = provider.trim().to_ascii_lowercase();
    let account_id = account_id.trim();
    anyhow::ensure!(!account_id.is_empty(), "provider account id is required");
    anyhow::ensure!(
        !(provider == "codex" && account_id == INSTANCE_CODEX_ACCOUNT_ID),
        "the instance-managed ChatGPT subscription cannot be disconnected here"
    );
    let (revision, keys) = if provider == "kimi" {
        let (revision, secrets) =
            crate::execution::cliproxyapi_integration::remove_kimi_subscription_from_topology(
                root, account_id,
            )?;
        (
            revision,
            secrets
                .into_iter()
                .map(|secret| (secret.scope, secret.name))
                .collect::<Vec<_>>(),
        )
    } else {
        let (revision, secrets) =
            remove_instance_proxy_subscription_from_topology(root, &provider, account_id)?;
        (
            revision,
            secrets
                .into_iter()
                .map(|secret| (secret.scope, secret.name))
                .collect::<Vec<_>>(),
        )
    };
    let borrowed = keys
        .iter()
        .map(|(scope, name)| (scope.as_str(), name.as_str()))
        .collect::<Vec<_>>();
    let deleted = crate::secrets::delete_secret_records(root, &borrowed)?;
    Ok(serde_json::json!({
        "provider": provider,
        "account_id": account_id,
        "revision": revision,
        "deleted_secret_records": deleted,
    }))
}

fn ensure_runtime_secret(root: &Path, secret: &RuntimeSecretRef) -> anyhow::Result<()> {
    anyhow::ensure!(
        crate::secrets::secret_exists(root, &secret.scope, &secret.name)?,
        "proxy credential is unavailable"
    );
    let value = Zeroizing::new(crate::secrets::read_secret_value(
        root,
        &secret.scope,
        &secret.name,
    )?);
    anyhow::ensure!(!value.trim().is_empty(), "proxy credential is unavailable");
    Ok(())
}

fn ensure_runtime_secrets_available(
    root: &Path,
    runtime: &CliproxyRuntimeConfig,
) -> anyhow::Result<()> {
    for account in &runtime.claude_accounts {
        ensure_runtime_secret(root, &account.access_token_secret)?;
        ensure_runtime_secret(root, &account.refresh_token_secret)?;
        if let Some(secret) = &account.proxy_url_secret {
            ensure_runtime_secret(root, secret)?;
        }
    }
    for account in &runtime.codex_accounts {
        ensure_runtime_secret(root, &account.id_token_secret)?;
        ensure_runtime_secret(root, &account.access_token_secret)?;
        ensure_runtime_secret(root, &account.refresh_token_secret)?;
        if let Some(secret) = &account.proxy_url_secret {
            ensure_runtime_secret(root, secret)?;
        }
    }
    for account in &runtime.antigravity_accounts {
        ensure_runtime_secret(root, &account.access_token_secret)?;
        ensure_runtime_secret(root, &account.refresh_token_secret)?;
        ensure_runtime_secret(root, &account.state_secret)?;
        if let Some(secret) = &account.proxy_url_secret {
            ensure_runtime_secret(root, secret)?;
        }
    }
    Ok(())
}

fn management_runtime_uses_provider_secret_scope(runtime: &CliproxyRuntimeConfig) -> bool {
    let allowed = |secret: &RuntimeSecretRef| secret.scope == INSTANCE_CODEX_SECRET_SCOPE;
    runtime.claude_accounts.iter().all(|account| {
        allowed(&account.access_token_secret)
            && allowed(&account.refresh_token_secret)
            && account.proxy_url_secret.as_ref().map_or(true, allowed)
    }) && runtime.codex_accounts.iter().all(|account| {
        allowed(&account.id_token_secret)
            && allowed(&account.access_token_secret)
            && allowed(&account.refresh_token_secret)
            && account.proxy_url_secret.as_ref().map_or(true, allowed)
    }) && runtime.antigravity_accounts.iter().all(|account| {
        allowed(&account.access_token_secret)
            && allowed(&account.refresh_token_secret)
            && allowed(&account.state_secret)
            && account.proxy_url_secret.as_ref().map_or(true, allowed)
    })
}

/// Resolves the Business OS ChatGPT subscription into the portable proxy's
/// typed runtime configuration without copying credentials into config or
/// ambient environment. `None` means that this CTOX instance has no usable
/// ChatGPT subscription snapshot.
pub fn instance_codex_runtime_config(
    root: &Path,
) -> anyhow::Result<Option<ValidatedRuntimeConfig>> {
    if !crate::secrets::secret_exists(
        root,
        INSTANCE_CHATGPT_AUTH_SCOPE,
        INSTANCE_CHATGPT_AUTH_NAME,
    )? {
        return Ok(None);
    }
    let auth = load_instance_chatgpt_auth(root)
        .map_err(|_| anyhow::anyhow!("instance ChatGPT subscription snapshot is invalid"))?;
    let Some(tokens) = auth.tokens else {
        return Ok(None);
    };
    if tokens.id_token.raw_jwt.trim().is_empty()
        || tokens.access_token.trim().is_empty()
        || tokens.refresh_token.trim().is_empty()
    {
        return Ok(None);
    }
    let secret = |name: &str| RuntimeSecretRef {
        scope: INSTANCE_CODEX_SECRET_SCOPE.to_owned(),
        name: name.to_owned(),
    };
    let plan_type = tokens
        .id_token
        .get_chatgpt_plan_type()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let config = CliproxyRuntimeConfig {
        request_timeout_ms: 30_000,
        routing_strategy: SchedulerStrategy::RoundRobin,
        claude_accounts: Vec::new(),
        codex_accounts: vec![CodexSubscriptionAccountConfig {
            id: INSTANCE_CODEX_ACCOUNT_ID.to_owned(),
            disabled: false,
            priority: 100,
            weight: 1,
            websockets: false,
            models: Vec::new(),
            id_token_secret: secret(INSTANCE_CODEX_ID_TOKEN_NAME),
            access_token_secret: secret(INSTANCE_CODEX_ACCESS_TOKEN_NAME),
            refresh_token_secret: secret(INSTANCE_CODEX_REFRESH_TOKEN_NAME),
            upstream_base_url:
                ctox_cliproxyapi::internal::runtime::executor::DEFAULT_CODEX_BASE_URL.to_owned(),
            plan_type,
            proxy_url_secret: None,
        }],
        antigravity_accounts: Vec::new(),
    }
    .validate()
    .map_err(|_| anyhow::anyhow!("instance ChatGPT subscription proxy config is invalid"))?;
    Ok(Some(config))
}

fn effective_instance_proxy_config(
    root: &Path,
) -> anyhow::Result<Option<EffectiveInstanceProxyConfig>> {
    let integration =
        crate::execution::cliproxyapi_integration::load_provider_integration_config(root)?;
    let kimi_enabled = integration
        .config
        .kimi_subscription_accounts
        .iter()
        .any(|account| !account.disabled);
    let xai_enabled = crate::execution::cliproxyapi_xai::subscription_installed(root);
    let stored = load_instance_proxy_config(root)?;
    let automatic_codex = instance_codex_runtime_config(root)?;
    let portable = match (stored, automatic_codex) {
        (None, None) => None,
        (None, Some(runtime)) => Some(("codex".to_owned(), runtime)),
        (Some(stored), automatic) => {
            let mut runtime = stored.runtime;
            // Persisted accounts reference ordinary CTOX secret records. The
            // automatic Codex account instead resolves typed handles from the
            // single ChatGPT subscription snapshot, so validate only the
            // persisted topology through the generic secret path.
            ensure_runtime_secrets_available(root, &runtime)?;
            if let Some(automatic) = automatic {
                for account in automatic.into_config().codex_accounts {
                    if !runtime
                        .codex_accounts
                        .iter()
                        .any(|configured| configured.id == account.id)
                    {
                        runtime.codex_accounts.push(account);
                    }
                }
            }
            let default_provider = if stored.default_provider.is_empty() {
                first_enabled_proxy_provider(&runtime)
            } else {
                stored.default_provider
            };
            if default_provider.is_empty() {
                None
            } else {
                let runtime = runtime
                    .validate()
                    .map_err(|_| anyhow::anyhow!("effective proxy runtime config is invalid"))?;
                Some((default_provider, runtime))
            }
        }
    };
    let (default_provider, runtime) = match portable {
        Some(portable) => portable,
        None if kimi_enabled || xai_enabled => {
            let runtime = CliproxyRuntimeConfig {
                request_timeout_ms: 30_000,
                routing_strategy: SchedulerStrategy::RoundRobin,
                claude_accounts: Vec::new(),
                codex_accounts: Vec::new(),
                antigravity_accounts: Vec::new(),
            }
            .validate_for_extension_host()
            .map_err(|_| anyhow::anyhow!("empty portable proxy config is invalid"))?;
            (
                if kimi_enabled {
                    "kimi".to_owned()
                } else {
                    String::new()
                },
                runtime,
            )
        }
        None => return Ok(None),
    };
    Ok(Some(EffectiveInstanceProxyConfig {
        default_provider,
        runtime,
        integration,
    }))
}

#[derive(Debug)]
struct SystemCtoxAccountStateClock;

impl AccountStateClock for SystemCtoxAccountStateClock {
    fn now_ms(&self) -> i64 {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        i64::try_from(millis).unwrap_or(i64::MAX)
    }
}

struct CtoxCodexAlphaSearchAuthority {
    router: Arc<AccountRouter>,
    conductor: Arc<CooldownConductor>,
    candidates: Vec<AccountCandidate>,
    accounts: HashMap<String, Arc<CodexSubscriptionAuth>>,
    clock: Arc<dyn AccountStateClock>,
}

impl CtoxCodexAlphaSearchAuthority {
    fn auth(&self, auth_id: &str) -> Result<Arc<CodexSubscriptionAuth>, CodexAlphaSearchError> {
        self.accounts
            .get(auth_id)
            .cloned()
            .ok_or(CodexAlphaSearchError::RefreshUnavailable)
    }
}

impl CodexAlphaSearchAuthSelector for CtoxCodexAlphaSearchAuthority {
    fn select<'a>(
        &'a self,
        model: &'a str,
        _headers: &'a ctox_cliproxyapi::sdk::cliproxy::executor::Headers,
        _original_body: &'a [u8],
    ) -> CodexAlphaSearchSelectionFuture<'a> {
        Box::pin(async move {
            let selected = self
                .router
                .select("codex", Some(model), self.clock.now_ms(), &self.candidates)
                .map_err(|_| CodexAlphaSearchError::RefreshUnavailable)?;
            let mut auth = Auth::default();
            auth.id = selected.auth_id;
            auth.provider = "codex".to_owned();
            auth.attributes
                .insert("auth_kind".to_owned(), "oauth".to_owned());
            Ok(auth)
        })
    }
}

impl CodexAlphaSearchCredentialSource for CtoxCodexAlphaSearchAuthority {
    fn credentials<'a>(&'a self, auth_id: &'a str) -> CodexAlphaSearchCredentialsFuture<'a> {
        Box::pin(async move {
            let credentials = self
                .auth(auth_id)?
                .load()
                .await
                .map_err(|_| CodexAlphaSearchError::RefreshUnavailable)?;
            let account_id = parse_jwt_token(credentials.id_token().expose_secret())
                .map(|claims| claims.account_id().to_owned())
                .unwrap_or_default();
            Ok(CodexAlphaSearchCredentials {
                access_token: credentials.access_token().clone(),
                account_id,
            })
        })
    }
}

impl CodexAlphaSearchRefresher for CtoxCodexAlphaSearchAuthority {
    fn report_unauthorized<'a>(
        &'a self,
        current: &'a Auth,
        model: &'a str,
    ) -> CodexAlphaSearchStatusFuture<'a> {
        Box::pin(async move {
            let conductor = Arc::clone(&self.conductor);
            let result = AccountExecutionResult {
                provider: "codex".to_owned(),
                auth_id: current.id.clone(),
                model: Some(model.to_owned()),
                status: 401,
                retry_delay_ms: None,
                observed_at_ms: self.clock.now_ms(),
            };
            tokio::task::spawn_blocking(move || conductor.record(result))
                .await
                .map_err(|_| CodexAlphaSearchError::RefreshUnavailable)?
                .map(|_| ())
                .map_err(|_| CodexAlphaSearchError::RefreshUnavailable)
        })
    }

    fn refresh_after_unauthorized<'a>(
        &'a self,
        current: &'a Auth,
        _model: &'a str,
    ) -> CodexAlphaSearchSelectionFuture<'a> {
        Box::pin(async move {
            self.auth(&current.id)?
                .refresh_after_status(401)
                .await
                .map_err(|_| CodexAlphaSearchError::RefreshUnavailable)?;
            Ok(current.clone())
        })
    }

    fn report_status<'a>(
        &'a self,
        current: &'a Auth,
        model: &'a str,
        status: u16,
    ) -> CodexAlphaSearchStatusFuture<'a> {
        Box::pin(async move {
            let conductor = Arc::clone(&self.conductor);
            let result = AccountExecutionResult {
                provider: "codex".to_owned(),
                auth_id: current.id.clone(),
                model: Some(model.to_owned()),
                status,
                retry_delay_ms: None,
                observed_at_ms: self.clock.now_ms(),
            };
            tokio::task::spawn_blocking(move || conductor.record(result))
                .await
                .map_err(|_| CodexAlphaSearchError::RefreshUnavailable)?
                .map(|_| ())
                .map_err(|_| CodexAlphaSearchError::RefreshUnavailable)
        })
    }
}

fn build_ctox_codex_alpha_search_authority(
    root: &Path,
    config: &ValidatedRuntimeConfig,
    accounts: HashMap<String, Arc<CodexSubscriptionAuth>>,
    account_clock: Arc<dyn AccountStateClock>,
) -> anyhow::Result<Arc<CtoxCodexAlphaSearchAuthority>> {
    let cooldown_store = Arc::new(CtoxCooldownStateStore::new(root));
    Ok(Arc::new(CtoxCodexAlphaSearchAuthority {
        router: Arc::new(AccountRouter::with_strategy(
            cooldown_store.clone(),
            config.routing_strategy(),
        )),
        conductor: Arc::new(CooldownConductor::new(cooldown_store)),
        candidates: config.codex_candidates(),
        accounts,
        clock: account_clock,
    }))
}

/// Builds the daemon-owned provider-independent Responses router. The legacy
/// function name is retained for host compatibility. Construction performs no
/// upstream request; native transports remain inert until the supervised
/// listener dispatches a request.
pub fn build_instance_codex_responses_router(
    root: &Path,
) -> anyhow::Result<Option<Arc<InstanceResponsesRouter>>> {
    let Some(config) = effective_instance_proxy_config(root)? else {
        return Ok(None);
    };
    Ok(Some(build_provider_routes(root, &config)?.responses))
}

struct KimiResponsesHandler {
    routes: Vec<Arc<crate::execution::cliproxyapi_integration::KimiSubscriptionRoute>>,
}

impl KimiResponsesHandler {
    async fn handle_route(&self, body: &[u8]) -> OpenAiResponsesRouteResponse {
        let Ok(request) = serde_json::from_slice::<serde_json::Value>(body) else {
            return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                400,
                "invalid JSON request body",
            ));
        };
        let Some(model) = request
            .get("model")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty())
        else {
            return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                400,
                "model is required",
            ));
        };
        let requested = ctox_cliproxyapi::internal::api::account_selection::requested_account();
        let Some(route) = self.routes.iter().find(|route| {
            route.supports_model(model)
                && requested
                    .as_deref()
                    .is_none_or(|id| id == route.account_id())
        }) else {
            return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                400,
                "requested Kimi model is not configured",
            ));
        };
        let stream = request
            .get("stream")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        ctox_cliproxyapi::internal::api::account_selection::record_selected(route.account_id());
        let executor_request = ExecutorRequest {
            auth_id: route.account_id().to_owned(),
            auth_provider: "kimi".to_owned(),
            model: model.to_owned(),
            source_format: "openai-response".to_owned(),
            format: "openai-response".to_owned(),
            stream,
            payload: body.to_vec(),
            original_request: body.to_vec(),
            ..ExecutorRequest::default()
        };
        if !stream {
            return match route.execute(executor_request).await {
                Ok(response) => OpenAiResponsesRouteResponse::Buffered(
                    OpenAiResponsesHttpResponse::json(200, response.payload),
                ),
                Err(_) => OpenAiResponsesRouteResponse::Buffered(
                    OpenAiResponsesHttpResponse::error(502, "Kimi upstream request failed"),
                ),
            };
        }
        let mut response = match route.execute_stream(executor_request).await {
            Ok(response) => response,
            Err(_) => {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    502,
                    "Kimi upstream stream failed",
                ))
            }
        };
        // The portable Kimi executor already emits Responses-shaped SSE. The
        // current server enum has provider-specific streaming owners, so Kimi
        // is deliberately bounded-buffered until that generic owner lands.
        let mut body = Vec::new();
        while let Some(chunk) = response.chunks.recv().await {
            let delimiter = if chunk.payload.ends_with(b"\n\n") {
                &b""[..]
            } else if chunk.payload.ends_with(b"\n") {
                &b"\n"[..]
            } else {
                &b"\n\n"[..]
            };
            if chunk.error.is_some()
                || body
                    .len()
                    .checked_add(chunk.payload.len().saturating_add(delimiter.len()))
                    .is_none_or(|len| len > 32 * 1024 * 1024)
            {
                return OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    502,
                    "Kimi upstream stream failed",
                ));
            }
            body.extend_from_slice(&chunk.payload);
            body.extend_from_slice(delimiter);
        }
        OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::event_stream(200, body))
    }
}

/// CTOX-owned extension router. It keeps the portable three-provider router
/// unchanged while attaching Kimi as a product-integration route.
pub struct InstanceResponsesRouter {
    xai_root: Option<PathBuf>,
    default_provider: String,
    portable: Option<Arc<OpenAiResponsesProviderRouter>>,
    kimi: Option<Arc<KimiResponsesHandler>>,
}

impl fmt::Debug for InstanceResponsesRouter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InstanceResponsesRouter")
            .field("default_provider", &self.default_provider)
            .field("portable", &self.portable)
            .field("kimi", &self.kimi.as_ref().map(|_| "configured"))
            .finish()
    }
}

impl OpenAiResponsesRouteHandler for InstanceResponsesRouter {
    fn handle_provider_route<'a>(
        &'a self,
        provider: Option<&'a str>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            let headers = std::collections::BTreeMap::new();
            self.handle_provider_route_with_headers(provider, &headers, body)
                .await
        })
    }

    fn handle_provider_route_with_headers<'a>(
        &'a self,
        provider: Option<&'a str>,
        headers: &'a std::collections::BTreeMap<String, Vec<String>>,
        body: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = OpenAiResponsesRouteResponse> + Send + 'a>> {
        Box::pin(async move {
            let provider = provider.unwrap_or(&self.default_provider).trim();
            if provider.eq_ignore_ascii_case("xai") {
                if let Some(root) = &self.xai_root {
                    return crate::execution::cliproxyapi_xai::handle_route(root, body).await;
                }
            }
            if provider.eq_ignore_ascii_case("kimi") {
                return match &self.kimi {
                    Some(handler) => handler.handle_route(body).await,
                    None => {
                        OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                            400,
                            "requested provider is not configured",
                        ))
                    }
                };
            }
            match &self.portable {
                Some(router) => {
                    router
                        .handle_provider_route_with_headers(Some(provider), headers, body)
                        .await
                }
                None => OpenAiResponsesRouteResponse::Buffered(OpenAiResponsesHttpResponse::error(
                    400,
                    "requested provider is not configured",
                )),
            }
        })
    }
}

struct InstanceProviderRoutes {
    responses: Arc<InstanceResponsesRouter>,
    messages: Option<Arc<ClaudeMessagesAntigravityHandler>>,
    auxiliary: Option<Arc<dyn AuxiliaryRouteHandler>>,
    models: ClaudeMessagesHttpResponse,
    antigravity_capabilities: HashMap<String, Arc<AntigravityModelCapabilityCatalog>>,
}

struct CtoxClaudeOAuthProfileFetcher {
    transport: Arc<dyn ClaudeRefreshTransport>,
}

impl ClaudeOAuthProfileFetcher for CtoxClaudeOAuthProfileFetcher {
    fn fetch<'a>(
        &'a self,
        access_token: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ClaudeOAuthProfile, String>> + Send + 'a>,
    > {
        Box::pin(async move {
            let access_token =
                SecretString::new(access_token.to_owned()).map_err(|error| error.to_string())?;
            let request = OAuthInspectRequest::new(OAuthInspectKind::Profile, access_token);
            let response = self
                .transport
                .inspect(&request, Duration::from_secs(10))
                .await
                .map_err(|error| format!("Claude OAuth profile transport failed: {error:?}"))?;
            if !(200..300).contains(&response.status()) {
                return Err(format!(
                    "Claude OAuth profile returned status {}",
                    response.status()
                ));
            }
            let profile: OAuthProfile = serde_json::from_slice(response.body())
                .map_err(|_| "Claude OAuth profile response is invalid".to_owned())?;
            let user = profile.user_info();
            Ok(ClaudeOAuthProfile {
                account_uuid: user.account_uuid().to_owned(),
                email: user.email().to_owned(),
                organization_uuid: user.organization_uuid().to_owned(),
                organization_name: user.organization_name().to_owned(),
            })
        })
    }
}

fn build_instance_provider_routes(root: &Path) -> anyhow::Result<Option<InstanceProviderRoutes>> {
    let Some(config) = effective_instance_proxy_config(root)? else {
        return Ok(None);
    };
    Ok(Some(build_provider_routes(root, &config)?))
}

fn runtime_proxy_url(
    root: &Path,
    secret: Option<&RuntimeSecretRef>,
) -> anyhow::Result<Option<String>> {
    secret
        .map(|secret| crate::secrets::read_secret_value(root, &secret.scope, &secret.name))
        .transpose()
}

fn build_provider_routes(
    root: &Path,
    effective: &EffectiveInstanceProxyConfig,
) -> anyhow::Result<InstanceProviderRoutes> {
    let factory = CtoxCliproxyRuntimeFactory::new(root);
    let account_clock: Arc<dyn AccountStateClock> = Arc::new(SystemCtoxAccountStateClock);
    let mut auxiliary_handlers: Vec<Arc<dyn AuxiliaryRouteHandler>> = Vec::new();
    let mut kimi_accounts = effective
        .integration
        .config
        .kimi_subscription_accounts
        .iter()
        .filter(|account| !account.disabled)
        .collect::<Vec<_>>();
    kimi_accounts.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.id.cmp(&right.id))
    });
    let kimi_routes = kimi_accounts
        .into_iter()
        .map(|account| {
            crate::execution::cliproxyapi_integration::build_kimi_subscription_route(
                root,
                &account.id,
            )
            .map(Arc::new)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let kimi = (!kimi_routes.is_empty()).then(|| {
        Arc::new(KimiResponsesHandler {
            routes: kimi_routes,
        })
    });

    let claude = if effective.runtime.claude_accounts().is_empty() {
        None
    } else {
        let mut transports = HashMap::new();
        for account in effective.runtime.claude_accounts() {
            let proxy_url = runtime_proxy_url(root, account.proxy_url_secret.as_ref())?;
            let refresh = Arc::new(
                AnthropicHttpTransport::new(proxy_url.as_deref())
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            );
            let messages = Arc::new(
                ClaudeMessagesHttpTransport::new(proxy_url.as_deref())
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            );
            transports.insert(
                account.id.clone(),
                ClaudeAccountTransports {
                    refresh,
                    messages: messages.clone(),
                    messages_stream: Some(messages),
                },
            );
        }
        let pool = Arc::new(factory.build_claude_pool(
            &effective.runtime,
            &transports,
            Arc::new(ctox_cliproxyapi::internal::auth::claude::SystemRefreshClock),
            Arc::clone(&account_clock),
        )?);
        auxiliary_handlers.push(Arc::new(ClaudeCountTokensRouteHandler::new(Arc::clone(
            &pool,
        ))));
        Some(Arc::new(OpenAiResponsesClaudeHandler::new(pool)))
    };

    let codex = if effective.runtime.codex_accounts().is_empty() {
        None
    } else {
        let mut transports = HashMap::new();
        let mut proxy_urls = HashMap::new();
        for account in effective.runtime.codex_accounts() {
            let proxy_url = runtime_proxy_url(root, account.proxy_url_secret.as_ref())?;
            proxy_urls.insert(account.id.clone(), proxy_url.clone());
            let refresh = Arc::new(
                CodexHttpTransport::new(proxy_url.as_deref())
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            );
            let responses = Arc::new(
                CodexResponsesHttpTransport::new(proxy_url.as_deref())
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            );
            transports.insert(
                account.id.clone(),
                CodexAccountTransports {
                    refresh,
                    responses: responses.clone(),
                    responses_stream: Some(responses),
                },
            );
        }
        let refresh_clock: Arc<dyn codex_auth::RefreshClock> =
            Arc::new(codex_auth::SystemRefreshClock);
        let (pool, alpha_accounts) = factory.build_codex_pool_with_shared_auth(
            &effective.runtime,
            &transports,
            refresh_clock,
            Arc::clone(&account_clock),
        )?;
        let authority = build_ctox_codex_alpha_search_authority(
            root,
            &effective.runtime,
            alpha_accounts,
            Arc::clone(&account_clock),
        )?;
        let credential_source: Arc<dyn CodexAlphaSearchCredentialSource> = authority.clone();
        let alpha_transport = Arc::new(
            CodexAlphaSearchHttpTransport::new(
                &proxy_urls,
                credential_source,
                effective.runtime.request_timeout(),
            )
            .map_err(|_| anyhow::anyhow!("Codex Alpha Search HTTP transport is invalid"))?,
        );
        let refresher: Arc<dyn CodexAlphaSearchRefresher> = authority.clone();
        let alpha_client =
            Arc::new(CodexAlphaSearchClient::new(alpha_transport).with_refresher(refresher));
        let selector: Arc<dyn CodexAlphaSearchAuthSelector> = authority;
        let auxiliary: Arc<dyn AuxiliaryRouteHandler> =
            Arc::new(CodexAlphaSearchRouteHandler::new(selector, alpha_client));
        auxiliary_handlers.push(auxiliary);
        Some(Arc::new(OpenAiResponsesCodexHandler::new(Arc::new(pool))))
    };

    let mut messages = None;
    let mut antigravity_capabilities = HashMap::new();
    let antigravity = if effective.runtime.antigravity_accounts().is_empty() {
        None
    } else {
        let mut transports = HashMap::new();
        for account in effective.runtime.antigravity_accounts() {
            let proxy_url = runtime_proxy_url(root, account.proxy_url_secret.as_ref())?;
            let refresh = Arc::new(
                antigravity_auth::AntigravityHttpTransport::new(proxy_url.as_deref())
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            );
            let generate = Arc::new(
                AntigravityGenerateHttpTransport::new(proxy_url.as_deref())
                    .map_err(|_| anyhow::anyhow!("Antigravity client configuration is invalid"))?,
            );
            transports.insert(
                account.id.clone(),
                AntigravityAccountTransports {
                    refresh,
                    generate: generate.clone(),
                    generate_stream: Some(generate),
                },
            );
        }
        let pool = Arc::new(factory.build_antigravity_pool(
            &effective.runtime,
            &transports,
            Arc::new(ctox_cliproxyapi::internal::runtime::executor::SystemAntigravityAuthClock),
            account_clock,
        )?);
        antigravity_capabilities = effective
            .runtime
            .antigravity_accounts()
            .iter()
            .map(|account| {
                (
                    account.id.clone(),
                    Arc::new(AntigravityModelCapabilityCatalog::new()),
                )
            })
            .collect();
        let resolver_catalogs = antigravity_capabilities.clone();
        let resolver = Arc::new(move |auth_id: &str, model: &str| {
            resolver_catalogs
                .get(auth_id)
                .is_some_and(|catalog| catalog.supports_web_search(model))
        });
        let signature_store: Arc<dyn SignatureKvStore> = Arc::new(CtoxSignatureKvStore::new(root));
        messages = Some(Arc::new(ClaudeMessagesAntigravityHandler::new(
            Arc::clone(&pool),
            Some(signature_store),
            resolver,
        )));
        Some(Arc::new(OpenAiResponsesAntigravityHandler::new(pool)))
    };

    let portable_default = if effective.default_provider == "kimi" {
        if claude.is_some() {
            "claude"
        } else if codex.is_some() {
            "codex"
        } else {
            "antigravity"
        }
    } else {
        effective.default_provider.as_str()
    };
    let portable = if claude.is_some() || codex.is_some() || antigravity.is_some() {
        Some(Arc::new(
            OpenAiResponsesProviderRouter::new(portable_default, claude, codex, antigravity)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
        ))
    } else {
        None
    };
    Ok(InstanceProviderRoutes {
        responses: Arc::new(InstanceResponsesRouter {
            xai_root: Some(root.to_path_buf()),
            default_provider: effective.default_provider.clone(),
            portable,
            kimi,
        }),
        messages,
        auxiliary: (!auxiliary_handlers.is_empty()).then(|| {
            Arc::new(AuxiliaryRouteChain::new(auxiliary_handlers)) as Arc<dyn AuxiliaryRouteHandler>
        }),
        models: claude_models_response(&provider_model_catalog_snapshot(root, effective), false),
        antigravity_capabilities,
    })
}

async fn refresh_instance_antigravity_capabilities(
    root: &Path,
    effective: &EffectiveInstanceProxyConfig,
    catalogs: &HashMap<String, Arc<AntigravityModelCapabilityCatalog>>,
) {
    for account in effective.runtime.antigravity_accounts() {
        let Some(catalog) = catalogs.get(&account.id) else {
            continue;
        };
        let handles = match account.credential_handles() {
            Ok(handles) => handles,
            Err(_) => {
                catalog.clear();
                continue;
            }
        };
        let credentials = match antigravity_auth::AntigravitySecretStore::load_credentials(
            &CtoxAntigravitySecretStore::new(root),
            &handles,
        ) {
            Ok(credentials) => credentials,
            Err(_) => {
                catalog.clear();
                continue;
            }
        };
        let proxy_url = match runtime_proxy_url(root, account.proxy_url_secret.as_ref()) {
            Ok(proxy_url) => proxy_url,
            Err(_) => {
                catalog.clear();
                continue;
            }
        };
        let transport = match AntigravityGenerateHttpTransport::new(proxy_url.as_deref()) {
            Ok(transport) => transport,
            Err(_) => {
                catalog.clear();
                continue;
            }
        };
        let targets = match antigravity_model_discovery_targets(Some(&account.upstream_base_url)) {
            Ok(targets) => targets,
            Err(_) => {
                catalog.clear();
                continue;
            }
        };
        let mut available_models = account.models.clone();
        if available_models.is_empty() {
            if let Some(model) = crate::inference::runtime_env::effective_chat_model(root) {
                let model = model.trim();
                if !model.is_empty() {
                    available_models.push(model.to_owned());
                }
            }
        }
        if available_models.is_empty() {
            catalog.clear();
            continue;
        }
        let _ = refresh_antigravity_model_capability_catalog(
            &transport,
            &targets,
            credentials.access_token(),
            &available_models,
            catalog,
            effective.runtime.request_timeout(),
        )
        .await;
    }
}

/// Builds the native Codex route from an already validated configuration.
/// Keeping this seam below instance config resolution lets differential and
/// integration tests point the upstream transport at a controlled loopback
/// server without introducing a production runtime toggle.
fn build_codex_responses_router(
    root: &Path,
    config: &ValidatedRuntimeConfig,
) -> anyhow::Result<Arc<OpenAiResponsesProviderRouter>> {
    let mut transports = HashMap::new();
    for account in config.codex_accounts() {
        let proxy_url = account
            .proxy_url_secret
            .as_ref()
            .map(|secret| crate::secrets::read_secret_value(root, &secret.scope, &secret.name))
            .transpose()?;
        let refresh = Arc::new(
            CodexHttpTransport::new(proxy_url.as_deref())
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
        );
        let responses = Arc::new(
            CodexResponsesHttpTransport::new(proxy_url.as_deref())
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
        );
        transports.insert(
            account.id.clone(),
            CodexAccountTransports {
                refresh,
                responses: responses.clone(),
                responses_stream: Some(responses),
            },
        );
    }
    let pool = Arc::new(CtoxCliproxyRuntimeFactory::new(root).build_codex_pool(
        &config,
        &transports,
        Arc::new(codex_auth::SystemRefreshClock),
        Arc::new(SystemCtoxAccountStateClock),
    )?);
    let handler = Arc::new(OpenAiResponsesCodexHandler::new(pool));
    let router = OpenAiResponsesProviderRouter::new("codex", None, Some(handler), None)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(Arc::new(router))
}

/// Starts one process-wide supervisor for the instance subscription proxy.
/// It waits fail-closed when no usable provider account is configured,
/// rebuilds the router when the typed auth/config snapshot changes, and binds
/// loopback only. A bind or construction failure is retried without taking
/// down CTOX. The legacy function name is retained for host compatibility.
pub fn start_instance_codex_proxy_supervisor(root: PathBuf) -> anyhow::Result<()> {
    fn started_root_cell() -> &'static Mutex<Option<PathBuf>> {
        static STARTED_ROOT: std::sync::OnceLock<Mutex<Option<PathBuf>>> =
            std::sync::OnceLock::new();
        STARTED_ROOT.get_or_init(|| Mutex::new(None))
    }

    let mut started_root = started_root_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = started_root.as_ref() {
        anyhow::ensure!(
            existing == &root,
            "subscription proxy supervisor already belongs to another CTOX root"
        );
        return Ok(());
    }
    let thread_root = root.clone();
    std::thread::Builder::new()
        .name("ctox-provider-subscription-proxy".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    set_instance_codex_proxy_status(
                        &thread_root,
                        InstanceCodexProxyPhase::Faulted,
                        Some(format!("Tokio runtime could not start: {error}")),
                    );
                    return;
                }
            };
            runtime.block_on(run_instance_codex_proxy_supervisor(thread_root));
        })
        .map_err(|error| {
            anyhow::anyhow!("failed to spawn subscription proxy supervisor: {error}")
        })?;
    *started_root = Some(root);
    Ok(())
}

async fn run_instance_codex_proxy_supervisor(root: PathBuf) {
    use ctox_cliproxyapi::internal::api::server::serve_provider_connection_with_auxiliary_logging;
    use ctox_cliproxyapi::internal::api::server_middleware::{
        RequestLoggingMetricsRegistry, RequestLoggingPolicy,
    };
    let logging_metrics = RequestLoggingMetricsRegistry::default();
    let logging_policy = Arc::new(RequestLoggingPolicy::error_only_scoped(
        &logging_metrics,
        &root,
        "provider-subscriptions",
        root.join("runtime/logs/cliproxyapi-subscriptions"),
        10,
    ));

    loop {
        let config = match effective_instance_proxy_config(&root) {
            Ok(Some(config)) => config,
            Ok(None) => {
                set_instance_codex_proxy_status(
                    &root,
                    InstanceCodexProxyPhase::WaitingForSubscription,
                    None,
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_CODEX_PROXY_RETRY_SECONDS,
                ))
                .await;
                continue;
            }
            Err(error) => {
                set_instance_codex_proxy_status(
                    &root,
                    InstanceCodexProxyPhase::Faulted,
                    Some(error.to_string()),
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_CODEX_PROXY_RETRY_SECONDS,
                ))
                .await;
                continue;
            }
        };
        set_instance_codex_proxy_status(&root, InstanceCodexProxyPhase::Starting, None);
        let routes = match build_instance_provider_routes(&root) {
            Ok(Some(routes)) => routes,
            Ok(None) => {
                // The configuration is re-read here, so it can disappear
                // between the check above and this call. Without the same
                // backoff every other branch uses, that race turns the
                // supervisor into a busy loop that reopens the runtime SQLite
                // databases as fast as the CPU allows.
                tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_CODEX_PROXY_RETRY_SECONDS,
                ))
                .await;
                continue;
            }
            Err(error) => {
                set_instance_codex_proxy_status(
                    &root,
                    InstanceCodexProxyPhase::Faulted,
                    Some(error.to_string()),
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_CODEX_PROXY_RETRY_SECONDS,
                ))
                .await;
                continue;
            }
        };
        let listener = match tokio::net::TcpListener::bind(instance_codex_proxy_addr()).await {
            Ok(listener) => listener,
            Err(error) => {
                set_instance_codex_proxy_status(
                    &root,
                    InstanceCodexProxyPhase::Faulted,
                    Some(format!("loopback bind failed: {error}")),
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_CODEX_PROXY_RETRY_SECONDS,
                ))
                .await;
                continue;
            }
        };
        set_instance_codex_proxy_status(&root, InstanceCodexProxyPhase::Ready, None);

        let capability_refresh_task = if routes.antigravity_capabilities.is_empty() {
            None
        } else {
            let capability_root = root.clone();
            let capability_config = config.clone();
            let catalogs = routes.antigravity_capabilities.clone();
            Some(tokio::spawn(async move {
                loop {
                    refresh_instance_antigravity_capabilities(
                        &capability_root,
                        &capability_config,
                        &catalogs,
                    )
                    .await;
                    tokio::time::sleep(std::time::Duration::from_secs(
                        INSTANCE_ANTIGRAVITY_CAPABILITY_REFRESH_SECONDS,
                    ))
                    .await;
                }
            }))
        };

        loop {
            tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok((mut stream, _peer)) => {
                        let responses = Arc::clone(&routes.responses);
                        let messages = routes.messages.clone();
                        let auxiliary = routes.auxiliary.clone();
                        let models = routes.models.clone();
                        let logging_policy = Arc::clone(&logging_policy);
                        tokio::spawn(async move {
                            let result = serve_provider_connection_with_auxiliary_logging(
                                &mut stream,
                                responses.as_ref(),
                                messages.as_deref(),
                                &models,
                                auxiliary.as_deref(),
                                logging_policy.as_ref(),
                            ).await;
                            if let Err(error) = result {
                                eprintln!("ctox subscription proxy connection failed: {error}");
                            }
                        });
                    }
                    Err(error) => {
                        set_instance_codex_proxy_status(
                            &root,
                            InstanceCodexProxyPhase::Faulted,
                            Some(format!("loopback accept failed: {error}")),
                        );
                        break;
                    }
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_CODEX_PROXY_RETRY_SECONDS,
                )) => {
                    match effective_instance_proxy_config(&root) {
                        Ok(Some(current)) if current == config => {}
                        _ => break,
                    }
                }
            }
        }
        if let Some(task) = capability_refresh_task {
            task.abort();
        }
    }
}

/// Starts the instance-scoped CLIProxyAPI management listener. The listener is
/// loopback-only and remains unbound until a sufficiently strong key exists at
/// `cliproxyapi-management/management-api-key` in the encrypted CTOX secret
/// store. Secret rotation rebuilds the authenticator without restarting CTOX.
pub fn start_instance_management_supervisor(root: PathBuf) -> anyhow::Result<()> {
    fn started_root_cell() -> &'static Mutex<Option<PathBuf>> {
        static STARTED_ROOT: std::sync::OnceLock<Mutex<Option<PathBuf>>> =
            std::sync::OnceLock::new();
        STARTED_ROOT.get_or_init(|| Mutex::new(None))
    }

    let mut started_root = started_root_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = started_root.as_ref() {
        anyhow::ensure!(
            existing == &root,
            "CLIProxyAPI management supervisor already belongs to another CTOX root"
        );
        return Ok(());
    }
    let thread_root = root.clone();
    std::thread::Builder::new()
        .name("ctox-cliproxyapi-management".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    set_instance_management_status(
                        &thread_root,
                        InstanceManagementPhase::Faulted,
                        Some(format!("Tokio runtime could not start: {error}")),
                    );
                    return;
                }
            };
            runtime.block_on(run_instance_management_supervisor(thread_root));
        })
        .map_err(|error| anyhow::anyhow!("failed to spawn management supervisor: {error}"))?;
    *started_root = Some(root);
    Ok(())
}

async fn run_instance_management_supervisor(root: PathBuf) {
    use ctox_cliproxyapi::internal::api::handlers::management::{
        ManagementAuthenticator, SystemManagementAuthClock,
    };
    use ctox_cliproxyapi::internal::api::server_management::{
        serve_management_connection, ManagementHandler,
    };

    loop {
        let snapshot = match InstanceManagementKeySnapshot::load(&root) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => {
                set_instance_management_status(
                    &root,
                    InstanceManagementPhase::WaitingForSecret,
                    None,
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_MANAGEMENT_RETRY_SECONDS,
                ))
                .await;
                continue;
            }
            Err(error) => {
                set_instance_management_status(
                    &root,
                    InstanceManagementPhase::Faulted,
                    Some(error.to_string()),
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_MANAGEMENT_RETRY_SECONDS,
                ))
                .await;
                continue;
            }
        };
        set_instance_management_status(&root, InstanceManagementPhase::Starting, None);
        let authenticator = match ManagementAuthenticator::new(
            snapshot.key.as_str(),
            false,
            Arc::new(SystemManagementAuthClock),
        ) {
            Ok(authenticator) => Arc::new(authenticator),
            Err(error) => {
                set_instance_management_status(
                    &root,
                    InstanceManagementPhase::Faulted,
                    Some(error.to_string()),
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_MANAGEMENT_RETRY_SECONDS,
                ))
                .await;
                continue;
            }
        };
        let key_fingerprint = snapshot.fingerprint;
        drop(snapshot);
        let runtime_status = Arc::new(CtoxManagementRuntimeStatusSource::new(root.clone()));
        let runtime_config = Arc::new(CtoxManagementRuntimeConfigSource::new(root.clone()));
        let handler = Arc::new(ManagementHandler::with_runtime_sources(
            authenticator,
            runtime_status,
            runtime_config,
        ));
        let listener = match tokio::net::TcpListener::bind(instance_management_addr()).await {
            Ok(listener) => listener,
            Err(error) => {
                set_instance_management_status(
                    &root,
                    InstanceManagementPhase::Faulted,
                    Some(format!("loopback bind failed: {error}")),
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_MANAGEMENT_RETRY_SECONDS,
                ))
                .await;
                continue;
            }
        };
        set_instance_management_status(&root, InstanceManagementPhase::Ready, None);

        loop {
            tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok((mut stream, peer)) => {
                        let handler = Arc::clone(&handler);
                        tokio::spawn(async move {
                            if let Err(error) = serve_management_connection(
                                &mut stream,
                                handler.as_ref(),
                                peer.ip(),
                            ).await {
                                eprintln!("ctox CLIProxyAPI management connection failed: {error}");
                            }
                        });
                    }
                    Err(error) => {
                        set_instance_management_status(
                            &root,
                            InstanceManagementPhase::Faulted,
                            Some(format!("loopback accept failed: {error}")),
                        );
                        break;
                    }
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(
                    INSTANCE_MANAGEMENT_RETRY_SECONDS,
                )) => {
                    match InstanceManagementKeySnapshot::load(&root) {
                        Ok(Some(current)) if current.fingerprint == key_fingerprint => {}
                        _ => break,
                    }
                }
            }
        }
    }
}

/// Encrypted SQLite-backed Antigravity credential snapshot.
///
/// Access token, refresh token, project routing ID and expiry are written by
/// one SQLite transaction. The state record is encrypted like the tokens even
/// though project ID and expiry are not themselves credentials; this keeps the
/// snapshot atomic and prevents mismatched routing state after a crash.
#[derive(Clone)]
pub struct CtoxAntigravitySecretStore {
    root: PathBuf,
}

impl CtoxAntigravitySecretStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl fmt::Debug for CtoxAntigravitySecretStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CtoxAntigravitySecretStore")
            .field("root", &self.root)
            .field("backend", &"encrypted-sqlite")
            .finish()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct AntigravityStoredState {
    expires_at_unix_ms: u64,
    project_id: String,
}

impl antigravity_auth::AntigravitySecretStore for CtoxAntigravitySecretStore {
    fn load_credentials(
        &self,
        handles: &antigravity_auth::AntigravityCredentialHandles,
    ) -> Result<
        antigravity_auth::AntigravityStoredCredentials,
        antigravity_auth::AntigravityTokenError,
    > {
        let ordered = [
            handles.access_token(),
            handles.refresh_token(),
            handles.state(),
        ];
        for handle in ordered {
            if !crate::secrets::secret_exists(&self.root, handle.scope(), handle.name())
                .map_err(|_| antigravity_auth::AntigravityTokenError::Read)?
            {
                return Err(antigravity_auth::AntigravityTokenError::Missing);
            }
        }
        let values = crate::secrets::read_secret_values(
            &self.root,
            &ordered
                .iter()
                .map(|handle| (handle.scope(), handle.name()))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| antigravity_auth::AntigravityTokenError::Read)?;
        let mut values = values.into_iter();
        let access = antigravity_auth::SecretString::new(
            values
                .next()
                .ok_or(antigravity_auth::AntigravityTokenError::Read)?,
        )?;
        let refresh = antigravity_auth::SecretString::new(
            values
                .next()
                .ok_or(antigravity_auth::AntigravityTokenError::Read)?,
        )?;
        let state: AntigravityStoredState = serde_json::from_str(
            &values
                .next()
                .ok_or(antigravity_auth::AntigravityTokenError::Read)?,
        )
        .map_err(|_| antigravity_auth::AntigravityTokenError::Read)?;
        let expires_at = std::time::UNIX_EPOCH
            .checked_add(std::time::Duration::from_millis(state.expires_at_unix_ms))
            .ok_or(antigravity_auth::AntigravityTokenError::ExpiryOverflow)?;
        antigravity_auth::AntigravityStoredCredentials::new(
            access,
            refresh,
            expires_at,
            state.project_id,
        )
    }

    fn store_credentials(
        &self,
        handles: &antigravity_auth::AntigravityCredentialHandles,
        credentials: &antigravity_auth::AntigravityStoredCredentials,
    ) -> Result<(), antigravity_auth::AntigravityTokenError> {
        let expires_at_unix_ms = credentials
            .expires_at()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| antigravity_auth::AntigravityTokenError::ExpiryOverflow)?
            .as_millis()
            .try_into()
            .map_err(|_| antigravity_auth::AntigravityTokenError::ExpiryOverflow)?;
        let state = serde_json::to_string(&AntigravityStoredState {
            expires_at_unix_ms,
            project_id: credentials.project_id().to_owned(),
        })
        .map_err(|_| antigravity_auth::AntigravityTokenError::Write)?;
        let access = handles.access_token();
        let refresh = handles.refresh_token();
        let state_handle = handles.state();
        crate::secrets::write_secret_records(
            &self.root,
            &[
                crate::secrets::SecretRecordWrite {
                    scope: access.scope(),
                    name: access.name(),
                    value: credentials.access_token().expose_secret(),
                    description: Some("Antigravity subscription access_token"),
                    metadata: antigravity_secret_metadata(access.kind()),
                },
                crate::secrets::SecretRecordWrite {
                    scope: refresh.scope(),
                    name: refresh.name(),
                    value: credentials.refresh_token().expose_secret(),
                    description: Some("Antigravity subscription refresh_token"),
                    metadata: antigravity_secret_metadata(refresh.kind()),
                },
                crate::secrets::SecretRecordWrite {
                    scope: state_handle.scope(),
                    name: state_handle.name(),
                    value: &state,
                    description: Some("Antigravity subscription routing state"),
                    metadata: antigravity_secret_metadata(state_handle.kind()),
                },
            ],
        )
        .map_err(|_| antigravity_auth::AntigravityTokenError::Write)
    }
}

fn antigravity_secret_metadata(kind: antigravity_auth::AntigravitySecretKind) -> serde_json::Value {
    let kind = match kind {
        antigravity_auth::AntigravitySecretKind::AccessToken => "access_token",
        antigravity_auth::AntigravitySecretKind::RefreshToken => "refresh_token",
        antigravity_auth::AntigravitySecretKind::State => "state",
    };
    serde_json::json!({
        "source": "cliproxyapi",
        "provider": "antigravity",
        "credential_kind": kind,
    })
}

fn codex_secret_metadata(kind: codex_auth::CodexSecretKind) -> serde_json::Value {
    let kind = match kind {
        codex_auth::CodexSecretKind::IdToken => "id_token",
        codex_auth::CodexSecretKind::AccessToken => "access_token",
        codex_auth::CodexSecretKind::RefreshToken => "refresh_token",
    };
    serde_json::json!({
        "source": "cliproxyapi",
        "provider": "codex",
        "credential_kind": kind,
    })
}

fn secret_metadata(kind: ClaudeSecretKind) -> serde_json::Value {
    let kind = match kind {
        ClaudeSecretKind::AccessToken => "access_token",
        ClaudeSecretKind::RefreshToken => "refresh_token",
    };
    serde_json::json!({
        "source": "cliproxyapi",
        "provider": "claude",
        "credential_kind": kind,
    })
}

/// CTOX SQLite-backed non-secret runtime state for account cooldowns.
///
/// The upstream `.cds` file set is adapted to one typed CTOX payload. The
/// conductor owns read/modify/write serialization; this adapter additionally
/// serializes operations made through the same cloned instance.
#[derive(Clone)]
pub struct CtoxCooldownStateStore {
    root: PathBuf,
    operation_lock: Arc<Mutex<()>>,
}

impl CtoxCooldownStateStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl fmt::Debug for CtoxCooldownStateStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CtoxCooldownStateStore")
            .field("root", &self.root)
            .field("backend", &"ctox-sqlite-payload")
            .finish()
    }
}

impl CooldownStateStore for CtoxCooldownStateStore {
    fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| CooldownStoreError::Read)?;
        let records = crate::persistence::load_json_payload::<Vec<CooldownStateRecord>>(
            &self.root,
            COOLDOWN_STATE_PAYLOAD_KEY,
        )
        .map_err(|_| CooldownStoreError::Read)?
        .unwrap_or_default();
        for record in &records {
            record.validate()?;
        }
        Ok(records)
    }

    fn save(&self, records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| CooldownStoreError::Write)?;
        for record in records {
            record.validate()?;
        }
        if records.is_empty() {
            crate::persistence::store_json_payload::<Vec<CooldownStateRecord>>(
                &self.root,
                COOLDOWN_STATE_PAYLOAD_KEY,
                None,
            )
            .map_err(|_| CooldownStoreError::Write)?;
            return Ok(());
        }

        let mut snapshot = records.to_vec();
        snapshot.sort_by(|left, right| {
            (&left.provider, &left.auth_id, &left.model).cmp(&(
                &right.provider,
                &right.auth_id,
                &right.model,
            ))
        });
        crate::persistence::store_json_payload(
            &self.root,
            COOLDOWN_STATE_PAYLOAD_KEY,
            Some(&snapshot),
        )
        .map_err(|_| CooldownStoreError::Write)
    }
}

#[derive(Clone)]
pub struct ClaudeAccountTransports {
    pub refresh: Arc<dyn ClaudeRefreshTransport>,
    pub messages: Arc<dyn ClaudeMessagesTransport>,
    pub messages_stream: Option<Arc<dyn ClaudeMessagesStreamingTransport>>,
}

#[derive(Clone)]
pub struct CodexAccountTransports {
    pub refresh: Arc<dyn codex_auth::CodexRefreshTransport>,
    pub responses: Arc<dyn CodexResponsesTransport>,
    pub responses_stream: Option<Arc<dyn CodexResponsesStreamingTransport>>,
}

#[derive(Clone)]
pub struct AntigravityAccountTransports {
    pub refresh: Arc<dyn antigravity_auth::AntigravityRefreshTransport>,
    pub generate: Arc<dyn AntigravityGenerateTransport>,
    pub generate_stream: Option<Arc<dyn AntigravityGenerateStreamingTransport>>,
}

impl fmt::Debug for AntigravityAccountTransports {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AntigravityAccountTransports")
            .field("refresh", &"AntigravityRefreshTransport")
            .field("generate", &"AntigravityGenerateTransport")
            .field(
                "generate_stream",
                &self.generate_stream.as_ref().map(|_| "attached"),
            )
            .finish()
    }
}

impl fmt::Debug for CodexAccountTransports {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexAccountTransports")
            .field("refresh", &"CodexRefreshTransport")
            .field("responses", &"CodexResponsesTransport")
            .field(
                "responses_stream",
                &self.responses_stream.as_ref().map(|_| "attached"),
            )
            .finish()
    }
}

impl fmt::Debug for ClaudeAccountTransports {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeAccountTransports")
            .field("refresh", &"ClaudeRefreshTransport")
            .field("messages", &"ClaudeMessagesTransport")
            .field(
                "messages_stream",
                &self.messages_stream.as_ref().map(|_| "attached"),
            )
            .finish()
    }
}

/// Composes validated portable configuration with CTOX-owned persistence.
/// Network transports are injected explicitly, so proxy selection can never
/// fall back to process environment discovery inside this factory.
#[derive(Debug, Clone)]
pub struct CtoxCliproxyRuntimeFactory {
    root: PathBuf,
}

impl CtoxCliproxyRuntimeFactory {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn build_claude_pool(
        &self,
        config: &ValidatedRuntimeConfig,
        transports: &HashMap<String, ClaudeAccountTransports>,
        refresh_clock: Arc<dyn RefreshClock>,
        account_clock: Arc<dyn AccountStateClock>,
    ) -> Result<ClaudeSubscriptionAccountPool, CtoxCliproxyRuntimeBuildError> {
        let cooldown_store = Arc::new(CtoxCooldownStateStore::new(&self.root));
        let conductor = Arc::new(CooldownConductor::new(cooldown_store.clone()));
        let router = Arc::new(AccountRouter::with_strategy(
            cooldown_store,
            config.routing_strategy(),
        ));
        let mut executors = HashMap::new();
        let mut targets = HashMap::new();

        for account in config.claude_accounts() {
            let account_transports = transports
                .get(&account.id)
                .ok_or(CtoxCliproxyRuntimeBuildError::MissingTransport)?;
            let handles = account
                .credential_handles()
                .map_err(CtoxCliproxyRuntimeBuildError::Config)?;
            let target = account
                .upstream_target()
                .map_err(CtoxCliproxyRuntimeBuildError::Config)?;
            let auth = Arc::new(ClaudeSubscriptionAuth::new(
                handles,
                Arc::new(CtoxClaudeSecretStore::new(&self.root)),
                Arc::clone(&account_transports.refresh),
                Arc::clone(&refresh_clock),
                Arc::new(ClaudeRefreshCoordinator::default()),
            ));
            let mut executor = ClaudeSubscriptionMessagesExecutor::new(
                auth,
                Arc::clone(&account_transports.messages),
                config.request_timeout(),
            )
            .with_account_state_clock(
                account.id.clone(),
                Arc::clone(&conductor),
                Arc::clone(&account_clock),
            )
            .map_err(CtoxCliproxyRuntimeBuildError::Executor)?;
            executor = executor
                .with_request_auth_preparer(
                    account.id.clone(),
                    Arc::new(ClaudeRequestAuthPreparer::new(
                        None,
                        Arc::new(CtoxClaudeOAuthProfileFetcher {
                            transport: Arc::clone(&account_transports.refresh),
                        }),
                    )),
                )
                .map_err(CtoxCliproxyRuntimeBuildError::Executor)?;
            executor = executor.with_cloak_policy(
                ClaudeCloakPolicy::oauth_default().with_timezone(
                    account
                        .timezone()
                        .map_err(CtoxCliproxyRuntimeBuildError::Config)?,
                ),
            );
            if let Some(stream_transport) = account_transports.messages_stream.as_ref() {
                executor = executor.with_stream_transport(Arc::clone(stream_transport));
            }
            if let Some(profile) = account.device_profile.clone() {
                executor = executor.with_device_profile(
                    profile
                        .into_profile()
                        .map_err(CtoxCliproxyRuntimeBuildError::Config)?,
                );
            }
            executors.insert(account.id.clone(), Arc::new(executor));
            targets.insert(account.id.clone(), target);
        }

        ClaudeSubscriptionAccountPool::with_clock(
            router,
            config.claude_candidates(),
            executors,
            account_clock,
        )
        .and_then(|pool| pool.with_targets(targets))
        .map_err(CtoxCliproxyRuntimeBuildError::Pool)
    }

    pub fn build_codex_pool(
        &self,
        config: &ValidatedRuntimeConfig,
        transports: &HashMap<String, CodexAccountTransports>,
        refresh_clock: Arc<dyn codex_auth::RefreshClock>,
        account_clock: Arc<dyn AccountStateClock>,
    ) -> Result<CodexSubscriptionAccountPool, CtoxCliproxyRuntimeBuildError> {
        self.build_codex_pool_with_shared_auth(config, transports, refresh_clock, account_clock)
            .map(|(pool, _)| pool)
    }

    pub fn build_codex_pool_with_shared_auth(
        &self,
        config: &ValidatedRuntimeConfig,
        transports: &HashMap<String, CodexAccountTransports>,
        refresh_clock: Arc<dyn codex_auth::RefreshClock>,
        account_clock: Arc<dyn AccountStateClock>,
    ) -> Result<
        (
            CodexSubscriptionAccountPool,
            HashMap<String, Arc<CodexSubscriptionAuth>>,
        ),
        CtoxCliproxyRuntimeBuildError,
    > {
        let cooldown_store = Arc::new(CtoxCooldownStateStore::new(&self.root));
        let conductor = Arc::new(CooldownConductor::new(cooldown_store.clone()));
        let router = Arc::new(AccountRouter::with_strategy(
            cooldown_store,
            config.routing_strategy(),
        ));
        let mut executors = HashMap::new();
        let mut targets = HashMap::new();
        let mut auths = HashMap::new();

        for account in config.codex_accounts() {
            let account_transports = transports
                .get(&account.id)
                .ok_or(CtoxCliproxyRuntimeBuildError::MissingTransport)?;
            let handles = account
                .credential_handles()
                .map_err(CtoxCliproxyRuntimeBuildError::Config)?;
            let target = account
                .upstream_target()
                .map_err(CtoxCliproxyRuntimeBuildError::Config)?;
            let auth = Arc::new(CodexSubscriptionAuth::new(
                handles,
                Arc::new(CtoxCodexSecretStore::new(&self.root)),
                Arc::clone(&account_transports.refresh),
                Arc::clone(&refresh_clock),
                Arc::new(codex_auth::CodexRefreshCoordinator::default()),
            ));
            let mut executor = CodexSubscriptionResponsesExecutor::new(
                Arc::clone(&auth),
                Arc::clone(&account_transports.responses),
                config.request_timeout(),
            )
            .map_err(CtoxCliproxyRuntimeBuildError::CodexExecutor)?
            .with_plan_type(account.plan_type.clone());
            if let Some(stream_transport) = account_transports.responses_stream.as_ref() {
                executor = executor.with_stream_transport(Arc::clone(stream_transport));
            }
            executors.insert(account.id.clone(), Arc::new(executor));
            targets.insert(account.id.clone(), target);
            auths.insert(account.id.clone(), auth);
        }

        CodexSubscriptionAccountPool::with_clock(
            router,
            conductor,
            config.codex_candidates(),
            executors,
            targets,
            account_clock,
        )
        .map(|pool| (pool, auths))
        .map_err(CtoxCliproxyRuntimeBuildError::CodexPool)
    }

    pub fn build_antigravity_pool(
        &self,
        config: &ValidatedRuntimeConfig,
        transports: &HashMap<String, AntigravityAccountTransports>,
        refresh_clock: Arc<dyn AntigravityAuthClock>,
        account_clock: Arc<dyn AccountStateClock>,
    ) -> Result<AntigravitySubscriptionAccountPool, CtoxCliproxyRuntimeBuildError> {
        let cooldown_store = Arc::new(CtoxCooldownStateStore::new(&self.root));
        let conductor = Arc::new(CooldownConductor::new(cooldown_store.clone()));
        let router = Arc::new(AccountRouter::with_strategy(
            cooldown_store,
            config.routing_strategy(),
        ));
        let replay_cache = Arc::new(AntigravityReasoningReplayCache::new());
        let mut executors = HashMap::new();
        let mut targets = HashMap::new();

        for account in config.antigravity_accounts() {
            let account_transports = transports
                .get(&account.id)
                .ok_or(CtoxCliproxyRuntimeBuildError::MissingTransport)?;
            let handles = account
                .credential_handles()
                .map_err(CtoxCliproxyRuntimeBuildError::Config)?;
            let target = account
                .upstream_target()
                .map_err(CtoxCliproxyRuntimeBuildError::Config)?;
            let auth = Arc::new(AntigravitySubscriptionAuth::new(
                handles,
                Arc::new(CtoxAntigravitySecretStore::new(&self.root)),
                Arc::clone(&account_transports.refresh),
                Arc::clone(&refresh_clock),
                Arc::new(antigravity_auth::AntigravityRefreshCoordinator::default()),
            ));
            let mut executor = AntigravitySubscriptionExecutor::new(
                auth,
                Arc::clone(&account_transports.generate),
                config.request_timeout(),
            )
            .map_err(CtoxCliproxyRuntimeBuildError::AntigravityExecutor)?
            .with_reasoning_replay_cache(Arc::clone(&replay_cache));
            if let Some(stream_transport) = account_transports.generate_stream.as_ref() {
                executor = executor.with_stream_transport(Arc::clone(stream_transport));
            }
            executors.insert(account.id.clone(), Arc::new(executor));
            targets.insert(account.id.clone(), target);
        }

        AntigravitySubscriptionAccountPool::with_clock(
            router,
            conductor,
            config.antigravity_candidates(),
            executors,
            targets,
            account_clock,
        )
        .map_err(CtoxCliproxyRuntimeBuildError::AntigravityPool)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtoxCliproxyRuntimeBuildError {
    Config(RuntimeConfigError),
    MissingTransport,
    Executor(ctox_cliproxyapi::internal::runtime::executor::ClaudeExecutionError),
    Pool(ClaudeAccountPoolError),
    CodexExecutor(ctox_cliproxyapi::internal::runtime::executor::CodexExecutionError),
    CodexPool(CodexAccountPoolError),
    AntigravityExecutor(ctox_cliproxyapi::internal::runtime::executor::AntigravityExecutionError),
    AntigravityPool(AntigravityAccountPoolError),
}

impl fmt::Display for CtoxCliproxyRuntimeBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Config(_) => "CLIProxy runtime configuration is invalid",
            Self::MissingTransport => "CLIProxy account transport is missing",
            Self::Executor(_) => "CLIProxy account executor construction failed",
            Self::Pool(_) => "CLIProxy account pool construction failed",
            Self::CodexExecutor(_) => "CLIProxy Codex executor construction failed",
            Self::CodexPool(_) => "CLIProxy Codex account pool construction failed",
            Self::AntigravityExecutor(_) => "CLIProxy Antigravity executor construction failed",
            Self::AntigravityPool(_) => "CLIProxy Antigravity account pool construction failed",
        })
    }
}

impl std::error::Error for CtoxCliproxyRuntimeBuildError {}

#[cfg(test)]
mod tests {
    include!("cliproxyapi_instance_headers_candidate_test.rs");

    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime};

    use antigravity_auth::AntigravitySecretStore as _;
    use base64::Engine as _;
    use codex_auth::CodexSecretStore as _;
    use ctox_cliproxyapi::internal::api::server_routes::{
        CodexAlphaSearchFuture, CodexAlphaSearchResponse, CodexAlphaSearchTransport,
        CodexAlphaSearchTransportRequest,
    };
    use ctox_cliproxyapi::internal::auth::claude::{
        RefreshHttpResponse, RefreshRequest, RefreshTransportFailure,
    };
    use ctox_cliproxyapi::internal::runtime::executor::{
        AntigravityGenerateRequest, AntigravityGenerateResponse, AntigravityGenerateStreamResponse,
        AntigravityGenerateTransportFailure, ClaudeMessagesRequest, ClaudeMessagesResponse,
        ClaudeMessagesTransportFailure, CodexResponsesRequest, CodexResponsesResponse,
        CodexResponsesTransportFailure,
    };
    use ctox_cliproxyapi::sdk::pluginapi::{
        HostHttpClient, HttpRequest, HttpResponse, HttpStreamChunk, HttpStreamResponse,
        PluginFuture,
    };

    use super::*;

    // The daemon owns one active instance per process. Fixtures that publish
    // its process-wide listener status must retain that slot until their Pi
    // turn and capability assertions finish; otherwise a parallel root can
    // replace Ready with its own status between the two observations.
    fn instance_status_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static SLOT: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
        SLOT.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    struct HostAlphaSearchProbe {
        authority: Arc<CtoxCodexAlphaSearchAuthority>,
        saw_typed_credentials: AtomicBool,
        requests: Mutex<Vec<CodexAlphaSearchTransportRequest>>,
    }

    #[derive(Default)]
    struct HostKimiHttpProbe {
        requests: Mutex<Vec<HttpRequest>>,
    }

    #[derive(Default)]
    struct HostKimiPiStream {
        requests: Mutex<Vec<HttpRequest>>,
        turn: AtomicUsize,
    }

    #[derive(Default)]
    struct HostAntigravityPiStream {
        requests: Mutex<Vec<(String, Vec<u8>)>>,
        saw_expected_access_token: AtomicBool,
        turn: AtomicUsize,
    }

    impl AntigravityGenerateTransport for HostAntigravityPiStream {
        fn execute<'a>(
            &'a self,
            _request: &'a AntigravityGenerateRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            AntigravityGenerateResponse,
                            AntigravityGenerateTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async { panic!("Pi subscription smoke must stream") })
        }
    }

    impl AntigravityGenerateStreamingTransport for HostAntigravityPiStream {
        fn execute_stream<'a>(
            &'a self,
            request: &'a AntigravityGenerateRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            AntigravityGenerateStreamResponse,
                            AntigravityGenerateTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            self.requests
                .lock()
                .unwrap()
                .push((request.url().to_owned(), request.body().to_vec()));
            self.saw_expected_access_token.store(
                request.access_token().expose_secret() == "antigravity-pi-e2e-access-do-not-leak",
                Ordering::SeqCst,
            );
            let turn = self.turn.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let responses = if turn == 0 {
                    vec![
                        serde_json::json!({"response": {
                            "responseId":"antigravity-tool",
                            "modelVersion":"gemini-3-flash-agent",
                            "candidates":[{"content":{"parts":[{"functionCall":{
                                "id":"antigravity-edit",
                                "name":"edit",
                                "args":{"path":"index.js","edits":[{"oldText":"v = 1","newText":"v = 2"}]}
                            }}]}}]
                        }}),
                        serde_json::json!({"response": {
                            "responseId":"antigravity-tool",
                            "candidates":[{"finishReason":"STOP"}],
                            "usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}
                        }}),
                    ]
                } else {
                    vec![serde_json::json!({"response": {
                        "responseId":"antigravity-done",
                        "modelVersion":"gemini-3-flash-agent",
                        "candidates":[{"content":{"parts":[{"text":"Done."}]},"finishReason":"STOP"}],
                        "usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}
                    }})]
                };
                let (sender, receiver) = tokio::sync::mpsc::channel(responses.len());
                for response in responses {
                    sender
                        .send(Ok(format!("data: {response}\n\n").into_bytes()))
                        .await
                        .unwrap();
                }
                drop(sender);
                Ok(AntigravityGenerateStreamResponse::new(200, None, receiver))
            })
        }
    }

    impl HostHttpClient for HostKimiPiStream {
        fn execute<'a>(&'a self, _request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
            Box::pin(async { panic!("Pi subscription smoke must stream") })
        }

        fn execute_stream<'a>(
            &'a self,
            request: HttpRequest,
        ) -> PluginFuture<'a, HttpStreamResponse> {
            self.requests.lock().unwrap().push(request);
            let turn = self.turn.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let arguments = serde_json::json!({
                    "path":"index.js",
                    "edits":[{"oldText":"v = 1","newText":"v = 2"}]
                })
                .to_string();
                let events = if turn == 0 {
                    let call = serde_json::json!({
                        "id":"kimi-pi-tool",
                        "object":"chat.completion.chunk",
                        "created":1,
                        "model":"kimi-k3[1m]",
                        "choices":[{
                            "index":0,
                            "delta":{"role":"assistant","tool_calls":[{
                                "index":0,
                                "id":"kimi-edit-call",
                                "type":"function",
                                "function":{"name":"edit","arguments":arguments}
                            }]},
                            "finish_reason":null
                        }],
                    });
                    let done = serde_json::json!({
                        "id":"kimi-pi-tool","object":"chat.completion.chunk","created":1,
                        "model":"kimi-k3[1m]","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],
                        "usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}
                    });
                    vec![
                        format!("data: {call}\n\n").into_bytes(),
                        format!("data: {done}\n\n").into_bytes(),
                        b"data: [DONE]\n\n".to_vec(),
                    ]
                } else {
                    let text = serde_json::json!({
                        "id":"kimi-pi-done",
                        "object":"chat.completion.chunk",
                        "created":1,
                        "model":"kimi-k3[1m]",
                        "choices":[{
                            "index":0,
                            "delta":{"role":"assistant","content":"Done."},
                            "finish_reason":null
                        }],
                    });
                    let done = serde_json::json!({
                        "id":"kimi-pi-done","object":"chat.completion.chunk","created":1,
                        "model":"kimi-k3[1m]","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],
                        "usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}
                    });
                    vec![
                        format!("data: {text}\n\n").into_bytes(),
                        format!("data: {done}\n\n").into_bytes(),
                        b"data: [DONE]\n\n".to_vec(),
                    ]
                };
                let (sender, receiver) = tokio::sync::mpsc::channel(events.len());
                for payload in events {
                    sender
                        .send(HttpStreamChunk {
                            payload,
                            error: None,
                        })
                        .await
                        .unwrap();
                }
                drop(sender);
                Ok(HttpStreamResponse {
                    status_code: 200,
                    headers: Default::default(),
                    chunks: receiver,
                })
            })
        }
    }

    impl HostHttpClient for HostKimiHttpProbe {
        fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
            self.requests.lock().unwrap().push(request);
            Box::pin(async {
                Ok(HttpResponse {
                    status_code: 200,
                    body: br#"{"id":"chat_1","object":"chat.completion","created":1,"model":"k3","choices":[{"index":0,"message":{"role":"assistant","content":"host-kimi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#.to_vec(),
                    ..HttpResponse::default()
                })
            })
        }

        fn execute_stream<'a>(
            &'a self,
            _request: HttpRequest,
        ) -> PluginFuture<'a, HttpStreamResponse> {
            Box::pin(async { panic!("non-stream host test") })
        }
    }

    impl CodexAlphaSearchTransport for HostAlphaSearchProbe {
        fn execute<'a>(
            &'a self,
            request: CodexAlphaSearchTransportRequest,
        ) -> CodexAlphaSearchFuture<'a> {
            Box::pin(async move {
                let credentials = self.authority.credentials(&request.auth_id).await?;
                self.saw_typed_credentials.store(
                    credentials.access_token.expose_secret() == "access-alpha-do-not-leak"
                        && credentials.account_id == "workspace-alpha",
                    Ordering::SeqCst,
                );
                self.requests.lock().unwrap().push(request);
                Ok(CodexAlphaSearchResponse {
                    status: 200,
                    headers: BTreeMap::from([(
                        "Content-Type".to_owned(),
                        vec!["application/json".to_owned()],
                    )]),
                    body: br#"{"results":["host-alpha"]}"#.to_vec(),
                })
            })
        }
    }

    fn handles() -> ClaudeCredentialHandles {
        ClaudeCredentialHandles::new(
            ctox_cliproxyapi::internal::auth::claude::ClaudeSecretHandle::new(
                "provider-subscriptions",
                "claude-primary-access",
                ClaudeSecretKind::AccessToken,
            )
            .unwrap(),
            ctox_cliproxyapi::internal::auth::claude::ClaudeSecretHandle::new(
                "provider-subscriptions",
                "claude-primary-refresh",
                ClaudeSecretKind::RefreshToken,
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn credentials(access: &str, refresh: &str) -> ClaudeStoredCredentials {
        ClaudeStoredCredentials::new(
            SecretString::new(access).unwrap(),
            SecretString::new(refresh).unwrap(),
        )
    }

    #[test]
    fn business_os_subscription_projection_tracks_installed_claude_account() {
        let root = tempfile::tempdir().unwrap();
        install_claude_subscription(
            root.path(),
            "claude-test-account",
            &credentials("claude-access-do-not-leak", "claude-refresh-do-not-leak"),
        )
        .unwrap();
        let projection = provider_subscription_status(root.path());
        assert_eq!(projection["revision"], 1);
        assert_eq!(projection["default_provider"], "claude");
        assert_eq!(projection["accounts"][0]["id"], "claude-test-account");
        assert_eq!(projection["accounts"][0]["ready"], true);
        let rendered = projection.to_string();
        assert!(!rendered.contains("do-not-leak"));
    }

    #[test]
    fn business_os_subscription_projection_requires_a_complete_credential_tuple() {
        let root = tempfile::tempdir().unwrap();
        install_claude_subscription(
            root.path(),
            "claude-tuple",
            &credentials("tuple-access-do-not-leak", "tuple-refresh-do-not-leak"),
        )
        .unwrap();
        let stored = load_instance_proxy_config(root.path()).unwrap().unwrap();
        let account = &stored.runtime.claude_accounts[0];
        let access = &account.access_token_secret;
        let refresh = &account.refresh_token_secret;
        assert!(projected_credential_tuple_is_ready(
            root.path(),
            &[access, refresh]
        ));

        let mut invalid = refresh.clone();
        invalid.scope = "unrelated-scope".to_string();
        assert!(!projected_credential_tuple_is_ready(
            root.path(),
            &[access, &invalid]
        ));
        invalid.scope = refresh.scope.clone();
        invalid.name.push_str("-missing");
        assert!(!projected_credential_tuple_is_ready(
            root.path(),
            &[access, &invalid]
        ));

        crate::secrets::write_secret_records(
            root.path(),
            &[crate::secrets::SecretRecordWrite {
                scope: &refresh.scope,
                name: &refresh.name,
                value: " ",
                description: None,
                metadata: serde_json::json!({}),
            }],
        )
        .unwrap();
        let projection = provider_subscription_status(root.path());
        assert_eq!(projection["accounts"][0]["ready"], false);
        assert_eq!(projection["accounts"][0]["status"], "error");
        assert!(!projection.to_string().contains("do-not-leak"));
    }

    #[test]
    fn disconnect_removes_topology_before_encrypted_claude_tuple() {
        let root = tempfile::tempdir().unwrap();
        install_claude_subscription(
            root.path(),
            "claude-disconnect",
            &credentials(
                "claude-disconnect-access-do-not-leak",
                "claude-disconnect-refresh-do-not-leak",
            ),
        )
        .unwrap();
        let result =
            disconnect_provider_subscription(root.path(), "claude", "claude-disconnect").unwrap();
        assert_eq!(result["provider"], "claude");
        assert_eq!(result["revision"], 2);
        assert_eq!(result["deleted_secret_records"], 2);
        let stored = load_instance_proxy_config(root.path()).unwrap().unwrap();
        assert!(stored.runtime.claude_accounts.is_empty());
        assert!(stored.default_provider.is_empty());
        assert!(!crate::secrets::secret_exists(
            root.path(),
            INSTANCE_CODEX_SECRET_SCOPE,
            "claude-disconnect-access-token"
        )
        .unwrap());
        assert!(effective_instance_proxy_config(root.path())
            .unwrap()
            .is_none());
    }

    #[test]
    fn disconnect_rejects_instance_managed_chatgpt_account() {
        let root = tempfile::tempdir().unwrap();
        let error =
            disconnect_provider_subscription(root.path(), "codex", INSTANCE_CODEX_ACCOUNT_ID)
                .unwrap_err();
        assert!(error.to_string().contains("instance-managed"));
    }

    #[test]
    fn ready_kimi_route_is_published_as_provider_model_capability() {
        let _listener_guard = instance_status_test_guard();
        use ctox_cliproxyapi::internal::auth::kimi::{
            KimiAuthBundle, KimiTokenData, KimiTokenStorage, SecretString as KimiSecretString,
        };

        let root = tempfile::tempdir().unwrap();
        let token = KimiTokenData::new(
            KimiSecretString::new("capability-access").unwrap(),
            Some(KimiSecretString::new("capability-refresh").unwrap()),
            "Bearer",
            Some(SystemTime::now() + Duration::from_secs(3600)),
            "kimi-code",
        );
        let storage = KimiTokenStorage::from_bundle(&KimiAuthBundle::new(token, "device"));
        crate::execution::cliproxyapi_integration::install_kimi_subscription(
            root.path(),
            "kimi-capability",
            &storage,
        )
        .unwrap();
        mark_instance_codex_proxy_ready_for_test(root.path());
        assert_eq!(
            instance_proxy_route_capabilities(root.path()),
            [InstanceProxyRouteCapability {
                provider: "kimi".to_owned(),
                model: "kimi-k3[1m]".to_owned(),
                default: true,
            }]
        );
    }

    #[test]
    fn business_os_subscription_projection_tracks_installed_antigravity_account() {
        let root = tempfile::tempdir().unwrap();
        let creds = antigravity_credentials(
            "google-access-do-not-leak",
            "google-refresh-do-not-leak",
            SystemTime::now() + Duration::from_secs(3600),
            "project-test",
        );
        install_antigravity_subscription(root.path(), "antigravity-test-account", &creds).unwrap();
        let projection = provider_subscription_status(root.path());
        assert_eq!(projection["revision"], 1);
        assert_eq!(projection["default_provider"], "antigravity");
        assert_eq!(projection["accounts"][0]["id"], "antigravity-test-account");
        assert!(!projection.to_string().contains("do-not-leak"));
    }

    fn codex_handles() -> codex_auth::CodexCredentialHandles {
        let handle = |name, kind| {
            codex_auth::CodexSecretHandle::new("provider-subscriptions", name, kind).unwrap()
        };
        codex_auth::CodexCredentialHandles::new(
            handle("codex-primary-id", codex_auth::CodexSecretKind::IdToken),
            handle(
                "codex-primary-access",
                codex_auth::CodexSecretKind::AccessToken,
            ),
            handle(
                "codex-primary-refresh",
                codex_auth::CodexSecretKind::RefreshToken,
            ),
        )
        .unwrap()
    }

    fn fake_chatgpt_jwt(plan_type: &str, account_id: &str) -> String {
        let payload = serde_json::json!({
            "email": "proxy-test@example.invalid",
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": plan_type,
                "chatgpt_user_id": "user-proxy-test",
                "chatgpt_account_id": account_id
            }
        });
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        format!("e30.{payload}.signature")
    }

    fn write_instance_chatgpt_snapshot(
        root: &Path,
        id_token: &str,
        access_token: &str,
        refresh_token: &str,
        account_id: &str,
    ) {
        let snapshot = serde_json::json!({
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "id_token": id_token,
                "access_token": access_token,
                "refresh_token": refresh_token,
                "account_id": account_id
            },
            "last_refresh": null
        });
        crate::secrets::write_secret_record(
            root,
            INSTANCE_CHATGPT_AUTH_SCOPE,
            INSTANCE_CHATGPT_AUTH_NAME,
            &serde_json::to_string(&snapshot).unwrap(),
            Some("test ChatGPT subscription snapshot".to_owned()),
            serde_json::json!({"test": true}),
        )
        .unwrap();
    }

    fn antigravity_handles() -> antigravity_auth::AntigravityCredentialHandles {
        let handle = |name, kind| {
            antigravity_auth::AntigravitySecretHandle::new("provider-subscriptions", name, kind)
                .unwrap()
        };
        antigravity_auth::AntigravityCredentialHandles::new(
            handle(
                "antigravity-primary-access",
                antigravity_auth::AntigravitySecretKind::AccessToken,
            ),
            handle(
                "antigravity-primary-refresh",
                antigravity_auth::AntigravitySecretKind::RefreshToken,
            ),
            handle(
                "antigravity-primary-state",
                antigravity_auth::AntigravitySecretKind::State,
            ),
        )
        .unwrap()
    }

    fn antigravity_credentials(
        access: &str,
        refresh: &str,
        expires_at: SystemTime,
        project_id: &str,
    ) -> antigravity_auth::AntigravityStoredCredentials {
        antigravity_auth::AntigravityStoredCredentials::new(
            antigravity_auth::SecretString::new(access).unwrap(),
            antigravity_auth::SecretString::new(refresh).unwrap(),
            expires_at,
            project_id,
        )
        .unwrap()
    }

    fn cooldown(auth_id: &str, retry_after_ms: i64) -> CooldownStateRecord {
        CooldownStateRecord {
            provider: "claude".to_owned(),
            auth_id: auth_id.to_owned(),
            model: None,
            status: "cooling".to_owned(),
            next_retry_after_ms: Some(retry_after_ms),
            reason: "rate_limit".to_owned(),
            quota: Default::default(),
            last_error: None,
            updated_at_ms: 1_000,
        }
    }

    fn stored_claude_runtime() -> CliproxyRuntimeConfig {
        CliproxyRuntimeConfig {
            request_timeout_ms: 30_000,
            routing_strategy: SchedulerStrategy::WeightedRoundRobin,
            claude_accounts: vec![
                ctox_cliproxyapi::internal::config::ClaudeSubscriptionAccountConfig {
                    id: "claude-persisted".to_owned(),
                    disabled: false,
                    priority: 10,
                    weight: 2,
                    websockets: false,
                    models: vec!["claude-sonnet-4-6".to_owned()],
                    access_token_secret: RuntimeSecretRef {
                        scope: "provider-subscriptions".to_owned(),
                        name: "persisted-claude-access".to_owned(),
                    },
                    refresh_token_secret: RuntimeSecretRef {
                        scope: "provider-subscriptions".to_owned(),
                        name: "persisted-claude-refresh".to_owned(),
                    },
                    upstream_scheme: "https".to_owned(),
                    upstream_authority: "api.anthropic.com".to_owned(),
                    proxy_url_secret: None,
                    device_profile: None,
                    timezone: "Europe/Berlin".to_owned(),
                },
            ],
            codex_accounts: Vec::new(),
            antigravity_accounts: Vec::new(),
        }
    }

    #[test]
    fn typed_proxy_config_store_is_revisioned_and_secret_free() {
        let root = tempfile::tempdir().unwrap();
        let first =
            save_instance_proxy_config(root.path(), 0, "claude", stored_claude_runtime()).unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(
            load_instance_proxy_config(root.path()).unwrap(),
            Some(first.clone())
        );
        assert!(
            save_instance_proxy_config(root.path(), 0, "claude", stored_claude_runtime(),).is_err()
        );

        let conn = Connection::open(crate::inference::runtime_env::runtime_config_path(
            root.path(),
        ))
        .unwrap();
        let json: String = conn
            .query_row(
                &format!(
                    "SELECT config_json FROM {INSTANCE_PROXY_CONFIG_TABLE} WHERE config_id = 1"
                ),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(json.contains("persisted-claude-access"));
        assert!(!json.contains("access-token-do-not-store"));
    }

    #[test]
    fn ready_persisted_provider_becomes_a_public_model_route() {
        let _listener_guard = instance_status_test_guard();
        let root = tempfile::tempdir().unwrap();
        for (name, value) in [
            ("persisted-claude-access", "access-token-do-not-store"),
            ("persisted-claude-refresh", "refresh-token-do-not-store"),
        ] {
            crate::secrets::write_secret_record(
                root.path(),
                "provider-subscriptions",
                name,
                value,
                Some("test provider credential".to_owned()),
                serde_json::json!({"test": true}),
            )
            .unwrap();
        }
        save_instance_proxy_config(root.path(), 0, "claude", stored_claude_runtime()).unwrap();
        assert!(instance_proxy_route_capabilities(root.path()).is_empty());
        set_instance_codex_proxy_status(root.path(), InstanceCodexProxyPhase::Ready, None);
        assert_eq!(
            instance_proxy_route_capabilities(root.path()),
            vec![InstanceProxyRouteCapability {
                provider: "claude".to_owned(),
                model: "claude-sonnet-4-6".to_owned(),
                default: true,
            }]
        );
    }

    #[test]
    fn empty_subscription_model_list_uses_provider_catalog_not_ctox_main_model() {
        let _listener_guard = instance_status_test_guard();
        let root = tempfile::tempdir().unwrap();
        for (name, value) in [
            ("persisted-claude-access", "access-token-do-not-store"),
            ("persisted-claude-refresh", "refresh-token-do-not-store"),
        ] {
            crate::secrets::write_secret_record(
                root.path(),
                "provider-subscriptions",
                name,
                value,
                Some("test provider credential".to_owned()),
                serde_json::json!({"test": true}),
            )
            .unwrap();
        }
        let mut runtime = stored_claude_runtime();
        runtime.claude_accounts[0].models.clear();
        save_instance_proxy_config(root.path(), 0, "claude", runtime).unwrap();
        set_instance_codex_proxy_status(root.path(), InstanceCodexProxyPhase::Ready, None);

        let routes = instance_proxy_route_capabilities(root.path());
        assert!(!routes.is_empty());
        assert!(routes.iter().all(|route| route.provider == "claude"));
        assert!(routes.iter().all(|route| route.model != "ctox-gateway"));
        assert!(routes
            .iter()
            .any(|route| route.model == "claude-sonnet-4-6"));
    }

    #[test]
    fn provider_model_catalog_snapshot_is_typed_deduplicated_and_secret_free() {
        let root = tempfile::tempdir().unwrap();
        for (name, value) in [
            ("persisted-claude-access", "access-token-do-not-store"),
            ("persisted-claude-refresh", "refresh-token-do-not-store"),
        ] {
            crate::secrets::write_secret_record(
                root.path(),
                "provider-subscriptions",
                name,
                value,
                Some("test provider credential".to_owned()),
                serde_json::json!({"test": true}),
            )
            .unwrap();
        }
        save_instance_proxy_config(root.path(), 0, "claude", stored_claude_runtime()).unwrap();
        let effective = effective_instance_proxy_config(root.path())
            .unwrap()
            .expect("stored provider config");

        let catalog = provider_model_catalog_snapshot(root.path(), &effective);
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0]["id"], "claude-sonnet-4-6");
        assert_eq!(catalog[0]["display_name"], "claude-sonnet-4-6");
        assert_eq!(catalog[0]["providers"], serde_json::json!(["claude"]));
        assert_eq!(catalog[0]["default"], true);
        let serialized = serde_json::to_string(&catalog).unwrap();
        assert!(!serialized.contains("persisted-claude-access"));
        assert!(!serialized.contains("access-token-do-not-store"));
        assert!(!serialized.contains("provider-subscriptions"));
    }

    #[test]
    fn persisted_provider_and_automatic_codex_snapshot_merge_without_duplicate_secrets() {
        let root = tempfile::tempdir().unwrap();
        for (name, value) in [
            ("persisted-claude-access", "access-token-do-not-store"),
            ("persisted-claude-refresh", "refresh-token-do-not-store"),
        ] {
            crate::secrets::write_secret_record(
                root.path(),
                "provider-subscriptions",
                name,
                value,
                Some("test provider credential".to_owned()),
                serde_json::json!({"test": true}),
            )
            .unwrap();
        }
        save_instance_proxy_config(root.path(), 0, "claude", stored_claude_runtime()).unwrap();
        write_instance_chatgpt_snapshot(
            root.path(),
            &fake_chatgpt_jwt("pro", "workspace-a"),
            "codex-access-do-not-store",
            "codex-refresh-do-not-store",
            "routing-account-a",
        );

        let effective = effective_instance_proxy_config(root.path())
            .unwrap()
            .expect("the stored and automatic providers should merge");
        assert_eq!(effective.default_provider, "claude");
        assert_eq!(effective.runtime.claude_accounts().len(), 1);
        assert_eq!(effective.runtime.codex_accounts().len(), 1);
        assert_eq!(
            effective.runtime.codex_accounts()[0].id,
            INSTANCE_CODEX_ACCOUNT_ID
        );
    }

    #[test]
    fn encrypted_store_round_trip_uses_typed_handles() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxClaudeSecretStore::new(root.path());
        let handles = handles();
        let initial = credentials("access-initial-do-not-leak", "refresh-initial-do-not-leak");
        store.store_credentials(&handles, &initial).unwrap();

        let loaded = store.load_credentials(&handles).unwrap();
        assert_eq!(
            loaded.access_token().expose_secret(),
            "access-initial-do-not-leak"
        );
        assert_eq!(
            loaded.refresh_token().expose_secret(),
            "refresh-initial-do-not-leak"
        );
        assert!(!format!("{loaded:?}").contains("refresh-initial-do-not-leak"));

        let rotated = credentials("access-rotated-do-not-leak", "refresh-rotated-do-not-leak");
        store.store_credentials(&handles, &rotated).unwrap();
        assert_eq!(
            crate::secrets::read_secret_value(
                root.path(),
                handles.refresh_token().scope(),
                handles.refresh_token().name()
            )
            .unwrap(),
            "refresh-rotated-do-not-leak"
        );
    }

    #[test]
    fn management_key_snapshot_is_store_backed_strong_and_redacted() {
        let root = tempfile::tempdir().unwrap();
        assert!(InstanceManagementKeySnapshot::load(root.path())
            .unwrap()
            .is_none());

        crate::secrets::write_secret_record(
            root.path(),
            INSTANCE_MANAGEMENT_SECRET_SCOPE,
            INSTANCE_MANAGEMENT_SECRET_NAME,
            "too-short",
            Some("test management key".to_owned()),
            serde_json::json!({"test": true}),
        )
        .unwrap();
        assert!(InstanceManagementKeySnapshot::load(root.path()).is_err());

        let key = "management-key-with-at-least-32-bytes";
        crate::secrets::write_secret_record(
            root.path(),
            INSTANCE_MANAGEMENT_SECRET_SCOPE,
            INSTANCE_MANAGEMENT_SECRET_NAME,
            key,
            Some("test management key".to_owned()),
            serde_json::json!({"test": true}),
        )
        .unwrap();
        let snapshot = InstanceManagementKeySnapshot::load(root.path())
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.key.as_str(), key);
        assert!(!format!("{snapshot:?}").contains(key));
        assert_eq!(
            instance_management_base_url(),
            "http://127.0.0.1:12436/v0/management"
        );
    }

    #[test]
    fn management_runtime_source_projects_only_public_typed_facts() {
        let _listener_guard = instance_status_test_guard();
        use ctox_cliproxyapi::internal::api::server_management::{
            ManagementRuntimePhase, ManagementRuntimeStatusSource as _,
        };

        let root = tempfile::tempdir().unwrap();
        set_instance_codex_proxy_status(
            root.path(),
            InstanceCodexProxyPhase::WaitingForSubscription,
            Some("must-not-cross-status-boundary".to_owned()),
        );
        set_instance_management_status(
            root.path(),
            InstanceManagementPhase::Ready,
            Some("must-not-cross-status-boundary".to_owned()),
        );

        let source = CtoxManagementRuntimeStatusSource::new(root.path().to_path_buf());
        let status = source.snapshot();
        assert_eq!(status.schema, "ctox.cliproxyapi.runtime-status.v1");
        assert_eq!(
            status.codex_subscription_gateway.phase,
            ManagementRuntimePhase::WaitingForSubscription
        );
        assert_eq!(
            status.management_gateway.phase,
            ManagementRuntimePhase::Ready
        );
        assert_eq!(status.main_responses_gateway.listen_addr, "127.0.0.1:12434");
        assert_eq!(
            status.codex_subscription_gateway.listen_addr,
            "127.0.0.1:12435"
        );
        assert_eq!(status.management_gateway.listen_addr, "127.0.0.1:12436");
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("must-not-cross-status-boundary"));
        assert!(!json.contains("last_error"));
        assert!(!json.contains("upstream_base_url"));
    }

    #[tokio::test]
    async fn store_backed_management_handler_serves_only_with_the_stored_key() {
        use ctox_cliproxyapi::internal::api::handlers::management::{
            ManagementAuthenticator, SystemManagementAuthClock,
        };
        use ctox_cliproxyapi::internal::api::server_management::{
            serve_one_management_connection, ManagementHandler,
        };
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let root = tempfile::tempdir().unwrap();
        let key = "store-backed-management-key-32-bytes";
        crate::secrets::write_secret_record(
            root.path(),
            INSTANCE_MANAGEMENT_SECRET_SCOPE,
            INSTANCE_MANAGEMENT_SECRET_NAME,
            key,
            Some("test management key".to_owned()),
            serde_json::json!({"test": true}),
        )
        .unwrap();
        let snapshot = InstanceManagementKeySnapshot::load(root.path())
            .unwrap()
            .unwrap();
        let handler = ManagementHandler::new(Arc::new(
            ManagementAuthenticator::new(
                snapshot.key.as_str(),
                false,
                Arc::new(SystemManagementAuthClock),
            )
            .unwrap(),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        assert!(addr.ip().is_loopback());
        let server = tokio::spawn(async move {
            serve_one_management_connection(&listener, &handler)
                .await
                .unwrap();
        });
        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        client
            .write_all(
                format!(
                    "GET /v0/management/model-definitions/claude HTTP/1.1\r\nHost: localhost\r\nX-Management-Key: {key}\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(!response.contains(key));
    }

    #[tokio::test]
    async fn management_put_atomically_activates_secret_free_provider_topology() {
        use ctox_cliproxyapi::internal::api::handlers::management::{
            ManagementAuthenticator, SystemManagementAuthClock,
        };
        use ctox_cliproxyapi::internal::api::server_management::{
            serve_one_management_connection, ManagementHandler,
        };
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let root = tempfile::tempdir().unwrap();
        for (name, value) in [
            ("persisted-claude-access", "access-token-do-not-store"),
            ("persisted-claude-refresh", "refresh-token-do-not-store"),
        ] {
            crate::secrets::write_secret_record(
                root.path(),
                "provider-subscriptions",
                name,
                value,
                Some("test provider credential".to_owned()),
                serde_json::json!({"test": true}),
            )
            .unwrap();
        }
        let key = "management-runtime-config-key-32-bytes";
        let handler = ManagementHandler::with_runtime_sources(
            Arc::new(
                ManagementAuthenticator::new(key, false, Arc::new(SystemManagementAuthClock))
                    .unwrap(),
            ),
            Arc::new(CtoxManagementRuntimeStatusSource::new(
                root.path().to_path_buf(),
            )),
            Arc::new(CtoxManagementRuntimeConfigSource::new(
                root.path().to_path_buf(),
            )),
        );
        let body = serde_json::to_vec(&serde_json::json!({
            "schema": INSTANCE_PROXY_CONFIG_SCHEMA,
            "expected_revision": 0,
            "default_provider": "claude",
            "runtime": stored_claude_runtime(),
        }))
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            serve_one_management_connection(&listener, &handler)
                .await
                .unwrap();
        });
        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        client
            .write_all(
                format!(
                    "PUT /v0/management/runtime-config HTTP/1.1\r\nHost: localhost\r\nX-Management-Key: {key}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        client.write_all(&body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        let head = String::from_utf8_lossy(&response[..split]);
        assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
        let payload: serde_json::Value = serde_json::from_slice(&response[split + 4..]).unwrap();
        assert_eq!(payload["revision"], 1);
        assert_eq!(payload["default_provider"], "claude");
        assert_eq!(payload["providers"][0]["enabled_account_count"], 1);
        let response_text = String::from_utf8_lossy(&response);
        assert!(!response_text.contains("persisted-claude-access"));
        assert!(!response_text.contains("access-token-do-not-store"));
        let stored = load_instance_proxy_config(root.path()).unwrap().unwrap();
        assert_eq!(stored.revision, 1);
        assert_eq!(stored.runtime.claude_accounts.len(), 1);

        use ctox_cliproxyapi::internal::api::server_management::{
            ManagementRuntimeConfigError, ManagementRuntimeConfigMutation,
            ManagementRuntimeConfigSource as _,
        };
        let stale = CtoxManagementRuntimeConfigSource::new(root.path().to_path_buf()).replace(
            ManagementRuntimeConfigMutation {
                schema: INSTANCE_PROXY_CONFIG_SCHEMA.to_owned(),
                expected_revision: 0,
                default_provider: "claude".to_owned(),
                runtime: stored_claude_runtime(),
            },
        );
        assert_eq!(
            stale.unwrap_err(),
            ManagementRuntimeConfigError::RevisionConflict
        );
    }

    #[test]
    fn management_config_source_rejects_missing_credentials_without_persisting() {
        use ctox_cliproxyapi::internal::api::server_management::{
            ManagementRuntimeConfigError, ManagementRuntimeConfigMutation,
            ManagementRuntimeConfigSource as _,
        };

        let root = tempfile::tempdir().unwrap();
        assert!(load_instance_proxy_config(root.path()).unwrap().is_none());
        let result = CtoxManagementRuntimeConfigSource::new(root.path().to_path_buf()).replace(
            ManagementRuntimeConfigMutation {
                schema: INSTANCE_PROXY_CONFIG_SCHEMA.to_owned(),
                expected_revision: 0,
                default_provider: "claude".to_owned(),
                runtime: stored_claude_runtime(),
            },
        );
        assert_eq!(
            result.unwrap_err(),
            ManagementRuntimeConfigError::CredentialUnavailable
        );
        assert!(load_instance_proxy_config(root.path()).unwrap().is_none());

        let mut foreign_scope = stored_claude_runtime();
        foreign_scope.claude_accounts[0].access_token_secret.scope = "ctox-auth".to_owned();
        let result = CtoxManagementRuntimeConfigSource::new(root.path().to_path_buf()).replace(
            ManagementRuntimeConfigMutation {
                schema: INSTANCE_PROXY_CONFIG_SCHEMA.to_owned(),
                expected_revision: 0,
                default_provider: "claude".to_owned(),
                runtime: foreign_scope,
            },
        );
        assert_eq!(result.unwrap_err(), ManagementRuntimeConfigError::Invalid);
        assert!(load_instance_proxy_config(root.path()).unwrap().is_none());
    }

    #[test]
    fn instance_codex_config_and_refresh_share_one_encrypted_snapshot() {
        let root = tempfile::tempdir().unwrap();
        assert!(instance_codex_runtime_config(root.path())
            .unwrap()
            .is_none());

        let original_id = fake_chatgpt_jwt("pro", "workspace-a");
        write_instance_chatgpt_snapshot(
            root.path(),
            &original_id,
            "access-original-do-not-leak",
            "refresh-original-do-not-leak",
            "routing-account-a",
        );

        let config = instance_codex_runtime_config(root.path())
            .unwrap()
            .expect("subscription snapshot should enable the Codex route");
        assert_eq!(config.codex_accounts().len(), 1);
        let account = &config.codex_accounts()[0];
        assert_eq!(account.id, INSTANCE_CODEX_ACCOUNT_ID);
        assert_eq!(account.plan_type, "pro");
        assert_eq!(
            account.upstream_base_url,
            ctox_cliproxyapi::internal::runtime::executor::DEFAULT_CODEX_BASE_URL
        );
        let handles = account.credential_handles().unwrap();
        assert!(is_instance_codex_handles(&handles));

        let store = CtoxCodexSecretStore::new(root.path());
        let loaded = store.load_credentials(&handles).unwrap();
        assert_eq!(loaded.id_token().expose_secret(), original_id);
        assert_eq!(
            loaded.access_token().expose_secret(),
            "access-original-do-not-leak"
        );

        let rotated_id = fake_chatgpt_jwt("business", "workspace-b");
        store
            .store_credentials(
                &handles,
                &codex_auth::CodexStoredCredentials::new(
                    codex_auth::SecretString::new(rotated_id.clone()).unwrap(),
                    codex_auth::SecretString::new("access-rotated-do-not-leak").unwrap(),
                    codex_auth::SecretString::new("refresh-rotated-do-not-leak").unwrap(),
                ),
            )
            .unwrap();

        let stored: ctox_core::auth::AuthDotJson = serde_json::from_str(
            &crate::secrets::read_secret_value(
                root.path(),
                INSTANCE_CHATGPT_AUTH_SCOPE,
                INSTANCE_CHATGPT_AUTH_NAME,
            )
            .unwrap(),
        )
        .unwrap();
        let stored_tokens = stored.tokens.unwrap();
        assert_eq!(stored_tokens.id_token.raw_jwt, rotated_id);
        assert_eq!(stored_tokens.access_token, "access-rotated-do-not-leak");
        assert_eq!(stored_tokens.refresh_token, "refresh-rotated-do-not-leak");
        assert_eq!(
            stored_tokens.account_id.as_deref(),
            Some("routing-account-a")
        );
        assert!(stored.last_refresh.is_some());

        let refreshed_config = instance_codex_runtime_config(root.path())
            .unwrap()
            .expect("rotated snapshot should remain routable");
        assert_eq!(refreshed_config.codex_accounts()[0].plan_type, "business");
    }

    #[test]
    fn instance_codex_router_builds_native_transports_without_network_io() {
        let root = tempfile::tempdir().unwrap();
        assert!(build_instance_codex_responses_router(root.path())
            .unwrap()
            .is_none());

        let id_token = fake_chatgpt_jwt("pro", "workspace-router");
        write_instance_chatgpt_snapshot(
            root.path(),
            &id_token,
            "access-router-do-not-leak",
            "refresh-router-do-not-leak",
            "routing-account-router",
        );
        let router = build_instance_codex_responses_router(root.path())
            .unwrap()
            .expect("valid subscription snapshot should build a native Codex router");
        let debug = format!("{router:?}");
        assert!(debug.contains("codex"));
        assert!(!debug.contains("access-router-do-not-leak"));
        assert!(!debug.contains("refresh-router-do-not-leak"));
        assert!(!debug.contains(&id_token));
        let routes = build_instance_provider_routes(root.path())
            .unwrap()
            .expect("Codex subscription should construct all provider routes");
        assert!(routes.auxiliary.is_some());
    }

    #[tokio::test]
    async fn instance_codex_listener_is_loopback_and_dispatches_bounded_http() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let root = tempfile::tempdir().unwrap();
        let id_token = fake_chatgpt_jwt("pro", "workspace-listener");
        write_instance_chatgpt_snapshot(
            root.path(),
            &id_token,
            "access-listener-do-not-leak",
            "refresh-listener-do-not-leak",
            "routing-account-listener",
        );
        let router = build_instance_codex_responses_router(root.path())
            .unwrap()
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        assert!(addr.ip().is_loopback());
        let server = tokio::spawn(async move {
            ctox_cliproxyapi::internal::api::server::serve_one_responses_connection(
                &listener,
                router.as_ref(),
            )
            .await
        });

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        client
            .write_all(
                b"GET /v1/responses HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap().unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
        assert!(!response.contains("access-listener-do-not-leak"));
        assert!(!response.contains("refresh-listener-do-not-leak"));
    }

    #[tokio::test]
    async fn instance_listener_dispatches_kimi_responses_by_explicit_provider() {
        use ctox_cliproxyapi::internal::api::server::serve_responses_connection_with_logging;
        use ctox_cliproxyapi::internal::api::server_middleware::{
            RequestLoggingMetricsRegistry, RequestLoggingPolicy,
        };
        use ctox_cliproxyapi::internal::auth::kimi::{
            KimiAuthBundle, KimiTokenData, KimiTokenStorage, SecretString as KimiSecretString,
        };
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let root = tempfile::tempdir().unwrap();
        let token = KimiTokenData::new(
            KimiSecretString::new("host-kimi-access-do-not-leak").unwrap(),
            Some(KimiSecretString::new("host-kimi-refresh-do-not-leak").unwrap()),
            "Bearer",
            Some(SystemTime::now() + Duration::from_secs(3600)),
            "kimi-code",
        );
        let storage = KimiTokenStorage::from_bundle(&KimiAuthBundle::new(token, "host-device"));
        crate::execution::cliproxyapi_integration::install_kimi_subscription(
            root.path(),
            "host-kimi",
            &storage,
        )
        .unwrap();
        let upstream = Arc::new(HostKimiHttpProbe::default());
        let route =
            crate::execution::cliproxyapi_integration::build_kimi_subscription_route_with_http(
                root.path(),
                "host-kimi",
                upstream.clone(),
            )
            .unwrap();
        let router = Arc::new(InstanceResponsesRouter {
            xai_root: None,
            default_provider: "kimi".to_owned(),
            portable: None,
            kimi: Some(Arc::new(KimiResponsesHandler {
                routes: vec![Arc::new(route)],
            })),
        });
        let logs = root.path().join("kimi-listener-logs");
        let metrics = RequestLoggingMetricsRegistry::default();
        let policy = Arc::new(RequestLoggingPolicy::error_only_scoped(
            &metrics,
            root.path(),
            "kimi-listener-test",
            &logs,
            1,
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server_router = router.clone();
        let server_policy = policy.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            serve_responses_connection_with_logging(
                &mut stream,
                server_router.as_ref(),
                server_policy.as_ref(),
            )
            .await
            .unwrap();
        });
        let body = br#"{"model":"k3[1m]","input":"hello"}"#;
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        client
            .write_all(
                format!(
                    "POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nX-CTOX-Provider: kimi\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("host-kimi"));
        assert!(!response.contains("do-not-leak"));
        let requests = upstream.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].url,
            ctox_cliproxyapi::internal::runtime::executor::kimi_executor::KIMI_CHAT_COMPLETIONS_URL
        );
        assert_eq!(
            requests[0].headers["Authorization"],
            ["Bearer host-kimi-access-do-not-leak"]
        );
    }

    #[tokio::test]
    async fn host_alpha_search_crosses_tcp_selector_and_typed_credential_transport() {
        use ctox_cliproxyapi::internal::api::server::serve_provider_connection_with_auxiliary_logging;
        use ctox_cliproxyapi::internal::api::server_middleware::{
            RequestLoggingMetricsRegistry, RequestLoggingPolicy,
        };
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let root = tempfile::tempdir().unwrap();
        let id_token = fake_chatgpt_jwt("pro", "workspace-alpha");
        write_instance_chatgpt_snapshot(
            root.path(),
            &id_token,
            "access-alpha-do-not-leak",
            "refresh-alpha-do-not-leak",
            "routing-alpha",
        );
        let config = instance_codex_runtime_config(root.path()).unwrap().unwrap();
        let refresh: Arc<dyn codex_auth::CodexRefreshTransport> =
            Arc::new(UnusedCodexRefreshTransport);
        let responses_transport: Arc<dyn CodexResponsesTransport> =
            Arc::new(SuccessCodexResponsesTransport);
        let transports = HashMap::from([(
            INSTANCE_CODEX_ACCOUNT_ID.to_owned(),
            CodexAccountTransports {
                refresh,
                responses: responses_transport,
                responses_stream: None,
            },
        )]);
        let (_, shared_auth) = CtoxCliproxyRuntimeFactory::new(root.path())
            .build_codex_pool_with_shared_auth(
                &config,
                &transports,
                Arc::new(FixedCodexRuntimeClock),
                Arc::new(FixedRuntimeClock),
            )
            .unwrap();
        let authority = build_ctox_codex_alpha_search_authority(
            root.path(),
            &config,
            shared_auth,
            Arc::new(FixedRuntimeClock),
        )
        .unwrap();
        let probe = Arc::new(HostAlphaSearchProbe {
            authority: authority.clone(),
            saw_typed_credentials: AtomicBool::new(false),
            requests: Mutex::new(Vec::new()),
        });
        let refresher: Arc<dyn CodexAlphaSearchRefresher> = authority.clone();
        let selector: Arc<dyn CodexAlphaSearchAuthSelector> = authority;
        let auxiliary: Arc<dyn AuxiliaryRouteHandler> =
            Arc::new(CodexAlphaSearchRouteHandler::new(
                selector,
                Arc::new(CodexAlphaSearchClient::new(probe.clone()).with_refresher(refresher)),
            ));
        let responses = build_codex_responses_router(root.path(), &config).unwrap();
        let models = claude_models_response(&[], false);
        let logs_dir = root.path().join("alpha-logs");
        let metrics = RequestLoggingMetricsRegistry::default();
        let policy = Arc::new(RequestLoggingPolicy::error_only_scoped(
            &metrics,
            root.path(),
            "host-alpha-test",
            &logs_dir,
            2,
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server_policy = policy.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            serve_provider_connection_with_auxiliary_logging(
                &mut stream,
                responses.as_ref(),
                Option::<&ClaudeMessagesAntigravityHandler>::None,
                &models,
                Some(auxiliary.as_ref()),
                server_policy.as_ref(),
            )
            .await
            .unwrap();
        });

        let body = br#"{"id":"host-session","model":"gpt-5-search","query":"rust","prompt_cache_key":"strip-me"}"#;
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        client
            .write_all(
                format!(
                    "POST /v1/alpha/search HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        client.write_all(body).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();

        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.ends_with(r#"{"results":["host-alpha"]}"#));
        assert!(!response.contains("access-alpha-do-not-leak"));
        assert!(probe.saw_typed_credentials.load(Ordering::SeqCst));
        let requests = probe.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].auth_id, INSTANCE_CODEX_ACCOUNT_ID);
        let upstream: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(upstream["query"], "rust");
        assert!(upstream.get("prompt_cache_key").is_none());
    }

    async fn read_complete_http_request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
        use tokio::io::AsyncReadExt as _;

        let mut request = Vec::new();
        let header_end = loop {
            let mut chunk = [0_u8; 4096];
            let read = stream.read(&mut chunk).await.unwrap();
            assert!(read > 0, "upstream request ended before its headers");
            request.extend_from_slice(&chunk[..read]);
            if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                break index;
            }
        };
        let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        let body_start = header_end + 4;
        if request.len() < body_start + content_length {
            let available = request.len().saturating_sub(body_start);
            request.resize(body_start + content_length, 0);
            stream
                .read_exact(&mut request[body_start + available..body_start + content_length])
                .await
                .unwrap();
        }
        request
    }

    async fn write_http_event_stream(stream: &mut tokio::net::TcpStream, body: &str) {
        use tokio::io::AsyncWriteExt as _;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    }

    fn seed_coding_smoke_main_model(root: &Path) {
        let mut main_env = BTreeMap::new();
        main_env.insert("CTOX_API_PROVIDER".to_owned(), "openai".to_owned());
        main_env.insert(
            "CTOX_CHAT_MODEL_BASE".to_owned(),
            "main-model-must-stay-selected".to_owned(),
        );
        main_env.insert(
            "CTOX_CHAT_MODEL".to_owned(),
            "main-model-must-stay-selected".to_owned(),
        );
        crate::inference::runtime_env::save_runtime_env_map(root, &main_env).unwrap();
    }

    fn opaque_subscription_preset(root: &Path, provider: &str, model: &str) -> String {
        let capabilities = crate::coding_agents::pi_sidecar::coding_model_capabilities(root);
        capabilities["presets"]
            .as_array()
            .and_then(|presets| {
                presets.iter().find(|preset| {
                    preset["model"]["headers"]["X-CTOX-Provider"].as_str() == Some(provider)
                        && preset["model"]["id"].as_str() == Some(model)
                })
            })
            .and_then(|preset| preset["id"].as_str())
            .unwrap_or_else(|| panic!("opaque {provider} subscription preset: {capabilities}"))
            .to_owned()
    }

    async fn spawn_two_turn_instance_proxy(
        router: Arc<InstanceResponsesRouter>,
    ) -> (
        SocketAddr,
        tokio::task::JoinHandle<Result<(), std::io::Error>>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            for _ in 0..2 {
                ctox_cliproxyapi::internal::api::server::serve_one_responses_connection(
                    &listener,
                    router.as_ref(),
                )
                .await?;
            }
            Ok(())
        });
        (address, task)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn codex_subscription_preset_drives_real_pi_edit_through_native_transport() {
        let _listener_guard = instance_status_test_guard();
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        if std::process::Command::new("node")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_err()
        {
            eprintln!("SKIP: `node` is not available");
            return;
        }

        let root = tempfile::tempdir().unwrap();
        let mut main_env = BTreeMap::new();
        main_env.insert("CTOX_API_PROVIDER".to_owned(), "openai".to_owned());
        main_env.insert(
            "CTOX_CHAT_MODEL_BASE".to_owned(),
            "main-model-must-stay-selected".to_owned(),
        );
        main_env.insert(
            "CTOX_CHAT_MODEL".to_owned(),
            "main-model-must-stay-selected".to_owned(),
        );
        crate::inference::runtime_env::save_runtime_env_map(root.path(), &main_env).unwrap();
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let id_token = fake_chatgpt_jwt("pro", "workspace-pi-e2e");

        // The native transport points at a controlled loopback upstream. This
        // is a validated test-only config seam, not a production env override.
        let secret = |name: &str| RuntimeSecretRef {
            scope: INSTANCE_CODEX_SECRET_SCOPE.to_owned(),
            name: name.to_owned(),
        };
        let runtime = CliproxyRuntimeConfig {
            request_timeout_ms: 10_000,
            routing_strategy: SchedulerStrategy::RoundRobin,
            claude_accounts: Vec::new(),
            codex_accounts: vec![CodexSubscriptionAccountConfig {
                id: "codex-pi-e2e".to_owned(),
                disabled: false,
                priority: 100,
                weight: 1,
                websockets: false,
                models: vec!["gpt-5.6-sol".to_owned()],
                id_token_secret: secret("codex-pi-e2e-id-token"),
                access_token_secret: secret("codex-pi-e2e-access-token"),
                refresh_token_secret: secret("codex-pi-e2e-refresh-token"),
                upstream_base_url: format!("http://{upstream_addr}/backend-api/codex"),
                plan_type: "pro".to_owned(),
                proxy_url_secret: None,
            }],
            antigravity_accounts: Vec::new(),
        };
        CtoxCodexSecretStore::new(root.path())
            .store_credentials(
                &runtime.codex_accounts[0].credential_handles().unwrap(),
                &codex_auth::CodexStoredCredentials::new(
                    codex_auth::SecretString::new(id_token).unwrap(),
                    codex_auth::SecretString::new("access-pi-e2e-do-not-leak").unwrap(),
                    codex_auth::SecretString::new("refresh-pi-e2e-do-not-leak").unwrap(),
                ),
            )
            .unwrap();
        save_instance_proxy_config(root.path(), 0, "codex", runtime.clone()).unwrap();
        mark_instance_codex_proxy_ready_for_test(root.path());
        assert_eq!(
            instance_codex_proxy_status(root.path()).phase,
            InstanceCodexProxyPhase::Ready
        );
        let effective = effective_instance_proxy_config(root.path())
            .expect("effective test subscription topology")
            .expect("configured test subscription topology");
        assert_eq!(
            configured_instance_proxy_route_capabilities(root.path(), &effective),
            [InstanceProxyRouteCapability {
                provider: "codex".to_owned(),
                model: "gpt-5.6-sol".to_owned(),
                default: true,
            }]
        );
        let config = runtime.validate().unwrap();
        let router = build_codex_responses_router(root.path(), &config).unwrap();

        let upstream = tokio::spawn(async move {
            let mut requests = Vec::new();
            for turn in 0..2 {
                let (mut stream, _) = upstream_listener.accept().await.unwrap();
                let mut request = Vec::new();
                let header_end = loop {
                    let mut chunk = [0_u8; 4096];
                    let read = stream.read(&mut chunk).await.unwrap();
                    assert!(read > 0, "upstream request ended before its headers");
                    request.extend_from_slice(&chunk[..read]);
                    if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n")
                    {
                        break index;
                    }
                };
                let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                let body_start = header_end + 4;
                if request.len() < body_start + content_length {
                    let available = request.len().saturating_sub(body_start);
                    request.resize(body_start + content_length, 0);
                    stream
                        .read_exact(
                            &mut request[body_start + available..body_start + content_length],
                        )
                        .await
                        .unwrap();
                }

                let events = if turn == 0 {
                    let arguments = serde_json::json!({
                        "path": "index.js",
                        "edits": [{"oldText":"v = 1", "newText":"v = 2"}]
                    })
                    .to_string();
                    let call = serde_json::json!({
                        "id":"fc_pi_e2e",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_pi_e2e",
                        "name":"edit",
                        "arguments":arguments
                    });
                    vec![
                        serde_json::json!({"type":"response.created","response":{"id":"resp_pi_tool","object":"response","status":"in_progress","model":"gpt-5.6-sol","output":[]}}),
                        serde_json::json!({"type":"response.output_item.added","output_index":0,"item":{"id":"fc_pi_e2e","type":"function_call","status":"in_progress","call_id":"call_pi_e2e","name":"edit","arguments":""}}),
                        serde_json::json!({"type":"response.function_call_arguments.delta","output_index":0,"item_id":"fc_pi_e2e","delta":arguments}),
                        serde_json::json!({"type":"response.function_call_arguments.done","output_index":0,"item_id":"fc_pi_e2e","arguments":call["arguments"]}),
                        serde_json::json!({"type":"response.output_item.done","output_index":0,"item":call.clone()}),
                        serde_json::json!({"type":"response.completed","response":{"id":"resp_pi_tool","object":"response","status":"completed","model":"gpt-5.6-sol","output":[call],"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}}),
                    ]
                } else {
                    let message = serde_json::json!({
                        "id": "msg_pi_e2e",
                        "type": "message",
                        "status": "completed",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": "Done.", "annotations": []}]
                    });
                    vec![
                        serde_json::json!({"type":"response.created","response":{"id":"resp_pi_done","object":"response","status":"in_progress","model":"gpt-5.6-sol","output":[]}}),
                        serde_json::json!({"type":"response.output_item.added","output_index":0,"item":{"id":"msg_pi_e2e","type":"message","status":"in_progress","role":"assistant","content":[]}}),
                        serde_json::json!({"type":"response.content_part.added","item_id":"msg_pi_e2e","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
                        serde_json::json!({"type":"response.output_text.delta","item_id":"msg_pi_e2e","output_index":0,"content_index":0,"delta":"Done.","logprobs":[]}),
                        serde_json::json!({"type":"response.output_item.done","output_index":0,"item":message.clone()}),
                        serde_json::json!({"type":"response.completed","response":{"id":"resp_pi_done","object":"response","status":"completed","model":"gpt-5.6-sol","output":[message],"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}}),
                    ]
                };
                let body = events
                    .iter()
                    .map(|event| format!("data: {event}\n\n"))
                    .collect::<String>();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.shutdown().await.unwrap();
                requests.push(request);
            }
            requests
        });

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy = tokio::spawn(async move {
            for _ in 0..2 {
                ctox_cliproxyapi::internal::api::server::serve_one_responses_connection(
                    &proxy_listener,
                    router.as_ref(),
                )
                .await?;
            }
            Ok::<_, std::io::Error>(())
        });

        let dist = crate::coding_agents::pi_sidecar::sidecar_dist_path(Path::new(env!(
            "CARGO_MANIFEST_DIR"
        )));
        let capabilities = crate::coding_agents::pi_sidecar::coding_model_capabilities(root.path());
        let preset_id = capabilities["presets"]
            .as_array()
            .and_then(|presets| {
                presets.iter().find(|preset| {
                    preset["model"]["headers"]["X-CTOX-Provider"].as_str() == Some("codex")
                        && preset["model"]["id"].as_str() == Some("gpt-5.6-sol")
                })
            })
            .and_then(|preset| preset["id"].as_str())
            .unwrap_or_else(|| panic!("opaque Codex subscription preset: {capabilities}"))
            .to_owned();
        let proxy_base_url = format!("http://{proxy_addr}/v1");
        let root_path = root.path().to_owned();
        let evidence = tokio::task::spawn_blocking(move || {
            crate::coding_agents::pi_sidecar::run_coding_preset_smoke_with_subscription_proxy_for_test(
                &root_path,
                &dist,
                &preset_id,
                &proxy_base_url,
            )
        })
        .await
        .unwrap()
        .unwrap();

        proxy.await.unwrap().unwrap();
        let upstream_requests = upstream.await.unwrap();
        assert_eq!(upstream_requests.len(), 2);
        let upstream_request = &upstream_requests[0];
        let upstream_text = String::from_utf8_lossy(&upstream_request);
        assert!(upstream_text.starts_with("POST /backend-api/codex/responses HTTP/1.1\r\n"));
        assert!(upstream_text
            .to_ascii_lowercase()
            .contains("authorization: bearer access-pi-e2e-do-not-leak\r\n"));
        assert!(upstream_text
            .to_ascii_lowercase()
            .contains("chatgpt-account-id: workspace-pi-e2e\r\n"));
        let upstream_body = upstream_request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| &upstream_request[index + 4..])
            .unwrap();
        let upstream_json: serde_json::Value = serde_json::from_slice(upstream_body).unwrap();
        assert_eq!(upstream_json["model"], "gpt-5.6-sol");
        assert_eq!(upstream_json["stream"], true);

        assert_eq!(evidence["ok"], true, "real preset smoke: {evidence}");
        assert_eq!(evidence["provider"], "codex");
        assert_eq!(evidence["model"], "gpt-5.6-sol");
        assert_eq!(evidence["bounded_edit_verified"], true);
        assert_eq!(evidence["main_model_unchanged"], true);
        assert_eq!(
            crate::inference::runtime_env::effective_chat_model(root.path()).as_deref(),
            Some("main-model-must-stay-selected")
        );
        let serialized = evidence.to_string();
        assert!(!serialized.contains("access-pi-e2e-do-not-leak"));
        assert!(!serialized.contains("refresh-pi-e2e-do-not-leak"));
        assert!(!serialized.contains("workspace-pi-e2e"));
        assert!(!serialized.contains(&proxy_addr.to_string()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn claude_subscription_preset_drives_real_pi_edit_through_format_bridge() {
        let _listener_guard = instance_status_test_guard();
        if std::process::Command::new("node")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_err()
        {
            eprintln!("SKIP: `node` is not available");
            return;
        }

        let root = tempfile::tempdir().unwrap();
        seed_coding_smoke_main_model(root.path());
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let secret = |name: &str| RuntimeSecretRef {
            scope: INSTANCE_CODEX_SECRET_SCOPE.to_owned(),
            name: name.to_owned(),
        };
        let runtime = CliproxyRuntimeConfig {
            request_timeout_ms: 10_000,
            routing_strategy: SchedulerStrategy::RoundRobin,
            claude_accounts: vec![ClaudeSubscriptionAccountConfig {
                id: "claude-pi-e2e".to_owned(),
                disabled: false,
                priority: 100,
                weight: 1,
                websockets: false,
                models: vec!["claude-sonnet-4-6".to_owned()],
                access_token_secret: secret("claude-pi-e2e-access-token"),
                refresh_token_secret: secret("claude-pi-e2e-refresh-token"),
                upstream_scheme: "http".to_owned(),
                upstream_authority: upstream_addr.to_string(),
                proxy_url_secret: None,
                device_profile: None,
                timezone: "UTC".to_owned(),
            }],
            codex_accounts: Vec::new(),
            antigravity_accounts: Vec::new(),
        };
        CtoxClaudeSecretStore::new(root.path())
            .store_credentials(
                &runtime.claude_accounts[0].credential_handles().unwrap(),
                &credentials(
                    "claude-pi-e2e-access-do-not-leak",
                    "claude-pi-e2e-refresh-do-not-leak",
                ),
            )
            .unwrap();
        save_instance_proxy_config(root.path(), 0, "claude", runtime).unwrap();
        mark_instance_codex_proxy_ready_for_test(root.path());
        let router = build_instance_codex_responses_router(root.path())
            .unwrap()
            .expect("native Claude Responses router");

        let upstream = tokio::spawn(async move {
            let mut requests = Vec::new();
            for turn in 0..2 {
                let (mut stream, _) = upstream_listener.accept().await.unwrap();
                let request = read_complete_http_request(&mut stream).await;
                let body = if turn == 0 {
                    concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-claude-tool\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tool-claude-edit\",\"name\":\"edit\",\"input\":{}}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\\\"index.js\\\",\\\"edits\\\":[{\\\"oldText\\\":\\\"v = 1\\\",\\\"newText\\\":\\\"v = 2\\\"}]}\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_delta\n",
                        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":8}}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n"
                    )
                } else {
                    concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-claude-done\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Done.\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_delta\n",
                        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":2}}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n"
                    )
                };
                write_http_event_stream(&mut stream, body).await;
                requests.push(request);
            }
            requests
        });
        let (proxy_addr, proxy) = spawn_two_turn_instance_proxy(router).await;
        let preset_id = opaque_subscription_preset(root.path(), "claude", "claude-sonnet-4-6");
        let dist = crate::coding_agents::pi_sidecar::sidecar_dist_path(Path::new(env!(
            "CARGO_MANIFEST_DIR"
        )));
        let root_path = root.path().to_owned();
        let proxy_base_url = format!("http://{proxy_addr}/v1");
        let evidence = tokio::task::spawn_blocking(move || {
            crate::coding_agents::pi_sidecar::run_coding_preset_smoke_with_subscription_proxy_for_test(
                &root_path,
                &dist,
                &preset_id,
                &proxy_base_url,
            )
        })
        .await
        .unwrap()
        .unwrap();

        proxy.await.unwrap().unwrap();
        let requests = upstream.await.unwrap();
        assert_eq!(requests.len(), 2);
        let first = String::from_utf8_lossy(&requests[0]);
        assert!(
            first.starts_with("POST /v1/messages"),
            "Claude target: {first}"
        );
        assert!(first
            .to_ascii_lowercase()
            .contains("authorization: bearer claude-pi-e2e-access-do-not-leak\r\n"));
        assert_eq!(evidence["provider"], "claude");
        assert_eq!(evidence["model"], "claude-sonnet-4-6");
        assert_eq!(evidence["bounded_edit_verified"], true);
        assert_eq!(evidence["main_model_unchanged"], true);
        let serialized = evidence.to_string();
        assert!(!serialized.contains("claude-pi-e2e"));
        assert!(!serialized.contains("do-not-leak"));
        assert!(!serialized.contains(&proxy_addr.to_string()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn antigravity_subscription_preset_drives_real_pi_edit_through_format_bridge() {
        let _listener_guard = instance_status_test_guard();
        if std::process::Command::new("node")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_err()
        {
            eprintln!("SKIP: `node` is not available");
            return;
        }

        let root = tempfile::tempdir().unwrap();
        seed_coding_smoke_main_model(root.path());
        let secret = |name: &str| RuntimeSecretRef {
            scope: INSTANCE_CODEX_SECRET_SCOPE.to_owned(),
            name: name.to_owned(),
        };
        let runtime = CliproxyRuntimeConfig {
            request_timeout_ms: 10_000,
            routing_strategy: SchedulerStrategy::RoundRobin,
            claude_accounts: Vec::new(),
            codex_accounts: Vec::new(),
            antigravity_accounts: vec![AntigravitySubscriptionAccountConfig {
                id: "antigravity-pi-e2e".to_owned(),
                disabled: false,
                priority: 100,
                weight: 1,
                websockets: false,
                models: vec!["gemini-3-flash-agent".to_owned()],
                access_token_secret: secret("antigravity-pi-e2e-access-token"),
                refresh_token_secret: secret("antigravity-pi-e2e-refresh-token"),
                state_secret: secret("antigravity-pi-e2e-state"),
                upstream_base_url: "https://daily-cloudcode-pa.googleapis.com".to_owned(),
                proxy_url_secret: None,
            }],
        };
        CtoxAntigravitySecretStore::new(root.path())
            .store_credentials(
                &runtime.antigravity_accounts[0]
                    .credential_handles()
                    .unwrap(),
                &antigravity_credentials(
                    "antigravity-pi-e2e-access-do-not-leak",
                    "antigravity-pi-e2e-refresh-do-not-leak",
                    SystemTime::now() + Duration::from_secs(3_600),
                    "antigravity-pi-e2e-project",
                ),
            )
            .unwrap();
        let validated = runtime.clone().validate().unwrap();
        save_instance_proxy_config(root.path(), 0, "antigravity", runtime).unwrap();
        mark_instance_codex_proxy_ready_for_test(root.path());
        let upstream = Arc::new(HostAntigravityPiStream::default());
        let transports = HashMap::from([(
            "antigravity-pi-e2e".to_owned(),
            AntigravityAccountTransports {
                refresh: Arc::new(UnusedAntigravityRefreshTransport),
                generate: upstream.clone(),
                generate_stream: Some(upstream.clone()),
            },
        )]);
        let pool = Arc::new(
            CtoxCliproxyRuntimeFactory::new(root.path())
                .build_antigravity_pool(
                    &validated,
                    &transports,
                    Arc::new(
                        ctox_cliproxyapi::internal::runtime::executor::SystemAntigravityAuthClock,
                    ),
                    Arc::new(FixedRuntimeClock),
                )
                .unwrap(),
        );
        let portable = Arc::new(
            OpenAiResponsesProviderRouter::new(
                "antigravity",
                None,
                None,
                Some(Arc::new(OpenAiResponsesAntigravityHandler::new(pool))),
            )
            .unwrap(),
        );
        let router = Arc::new(InstanceResponsesRouter {
            xai_root: None,
            default_provider: "antigravity".to_owned(),
            portable: Some(portable),
            kimi: None,
        });
        let (proxy_addr, proxy) = spawn_two_turn_instance_proxy(router).await;
        let preset_id =
            opaque_subscription_preset(root.path(), "antigravity", "gemini-3-flash-agent");
        let dist = crate::coding_agents::pi_sidecar::sidecar_dist_path(Path::new(env!(
            "CARGO_MANIFEST_DIR"
        )));
        let root_path = root.path().to_owned();
        let proxy_base_url = format!("http://{proxy_addr}/v1");
        let evidence = tokio::task::spawn_blocking(move || {
            crate::coding_agents::pi_sidecar::run_coding_preset_smoke_with_subscription_proxy_for_test(
                &root_path,
                &dist,
                &preset_id,
                &proxy_base_url,
            )
        })
        .await
        .unwrap()
        .unwrap();

        proxy.await.unwrap().unwrap();
        let requests = upstream.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].0.ends_with("/v1internal:streamGenerateContent"));
        assert!(upstream.saw_expected_access_token.load(Ordering::SeqCst));
        assert!(!String::from_utf8_lossy(&requests[0].1).contains("do-not-leak"));
        assert_eq!(evidence["provider"], "antigravity");
        assert_eq!(evidence["model"], "gemini-3-flash-agent");
        assert_eq!(evidence["bounded_edit_verified"], true);
        assert_eq!(evidence["main_model_unchanged"], true);
        let serialized = evidence.to_string();
        assert!(!serialized.contains("antigravity-pi-e2e"));
        assert!(!serialized.contains("do-not-leak"));
        assert!(!serialized.contains(&proxy_addr.to_string()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn kimi_subscription_preset_drives_real_pi_edit_through_format_bridge() {
        let _listener_guard = instance_status_test_guard();
        use ctox_cliproxyapi::internal::auth::kimi::{
            KimiAuthBundle, KimiTokenData, KimiTokenStorage, SecretString as KimiSecretString,
        };

        if std::process::Command::new("node")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_err()
        {
            eprintln!("SKIP: `node` is not available");
            return;
        }

        let root = tempfile::tempdir().unwrap();
        seed_coding_smoke_main_model(root.path());
        let storage = KimiTokenStorage::from_bundle(&KimiAuthBundle::new(
            KimiTokenData::new(
                KimiSecretString::new("kimi-pi-e2e-access-do-not-leak").unwrap(),
                Some(KimiSecretString::new("kimi-pi-e2e-refresh-do-not-leak").unwrap()),
                "Bearer",
                Some(SystemTime::now() + Duration::from_secs(3_600)),
                "kimi-code",
            ),
            "kimi-pi-e2e-device",
        ));
        crate::execution::cliproxyapi_integration::install_kimi_subscription(
            root.path(),
            "kimi-pi-e2e",
            &storage,
        )
        .unwrap();
        mark_instance_codex_proxy_ready_for_test(root.path());
        let effective = effective_instance_proxy_config(root.path())
            .expect("effective Kimi integration topology")
            .expect("configured Kimi integration topology");
        assert!(
            configured_instance_proxy_route_capabilities(root.path(), &effective)
                .iter()
                .any(|route| route.provider == "kimi" && route.model == "kimi-k3[1m]"),
            "Kimi route capabilities: {:?}",
            configured_instance_proxy_route_capabilities(root.path(), &effective)
        );
        let upstream = Arc::new(HostKimiPiStream::default());
        let route =
            crate::execution::cliproxyapi_integration::build_kimi_subscription_route_with_http(
                root.path(),
                "kimi-pi-e2e",
                upstream.clone(),
            )
            .unwrap();
        let router = Arc::new(InstanceResponsesRouter {
            xai_root: None,
            default_provider: "kimi".to_owned(),
            portable: None,
            kimi: Some(Arc::new(KimiResponsesHandler {
                routes: vec![Arc::new(route)],
            })),
        });
        let (proxy_addr, proxy) = spawn_two_turn_instance_proxy(router).await;
        let preset_id = opaque_subscription_preset(root.path(), "kimi", "kimi-k3[1m]");
        let dist = crate::coding_agents::pi_sidecar::sidecar_dist_path(Path::new(env!(
            "CARGO_MANIFEST_DIR"
        )));
        let root_path = root.path().to_owned();
        let proxy_base_url = format!("http://{proxy_addr}/v1");
        let evidence = tokio::task::spawn_blocking(move || {
            crate::coding_agents::pi_sidecar::run_coding_preset_smoke_with_subscription_proxy_for_test(
                &root_path,
                &dist,
                &preset_id,
                &proxy_base_url,
            )
        })
        .await
        .unwrap()
        .unwrap();

        proxy.await.unwrap().unwrap();
        let requests = upstream.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].url,
            ctox_cliproxyapi::internal::runtime::executor::kimi_executor::KIMI_CHAT_COMPLETIONS_URL
        );
        assert_eq!(
            requests[0].headers["Authorization"],
            ["Bearer kimi-pi-e2e-access-do-not-leak"]
        );
        assert_eq!(evidence["provider"], "kimi");
        assert_eq!(evidence["model"], "kimi-k3[1m]");
        assert_eq!(evidence["bounded_edit_verified"], true);
        assert_eq!(evidence["main_model_unchanged"], true);
        let serialized = evidence.to_string();
        assert!(!serialized.contains("kimi-pi-e2e"));
        assert!(!serialized.contains("do-not-leak"));
        assert!(!serialized.contains(&proxy_addr.to_string()));
    }

    #[test]
    fn missing_secret_returns_redacted_typed_error() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxClaudeSecretStore::new(root.path());
        assert_eq!(
            store.load_credentials(&handles()),
            Err(SecretStoreError::Missing)
        );
    }

    #[test]
    fn failed_refresh_write_rolls_back_both_credentials() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxClaudeSecretStore::new(root.path());
        let handles = handles();
        let initial = credentials("access-initial-do-not-leak", "refresh-initial-do-not-leak");
        store.store_credentials(&handles, &initial).unwrap();

        let conn =
            rusqlite::Connection::open(crate::secrets::secret_store_path(root.path())).unwrap();
        conn.execute_batch(
            r#"
            CREATE TRIGGER reject_claude_refresh_rotation
            BEFORE UPDATE ON ctox_secret_records
            WHEN NEW.secret_name = 'claude-primary-refresh'
            BEGIN
                SELECT RAISE(ABORT, 'forced refresh rollback');
            END;
            "#,
        )
        .unwrap();
        drop(conn);

        let rotated = credentials("access-rotated-do-not-leak", "refresh-rotated-do-not-leak");
        assert_eq!(
            store.store_credentials(&handles, &rotated),
            Err(SecretStoreError::Write)
        );
        let retained = store.load_credentials(&handles).unwrap();
        assert_eq!(
            retained.access_token().expose_secret(),
            "access-initial-do-not-leak"
        );
        assert_eq!(
            retained.refresh_token().expose_secret(),
            "refresh-initial-do-not-leak"
        );
    }

    #[test]
    fn codex_store_atomically_round_trips_three_redacted_credentials() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxCodexSecretStore::new(root.path());
        let handles = codex_handles();
        let credentials = codex_auth::CodexStoredCredentials::new(
            codex_auth::SecretString::new("id-do-not-leak").unwrap(),
            codex_auth::SecretString::new("access-do-not-leak").unwrap(),
            codex_auth::SecretString::new("refresh-do-not-leak").unwrap(),
        );
        store.store_credentials(&handles, &credentials).unwrap();
        let loaded = store.load_credentials(&handles).unwrap();
        assert_eq!(loaded.id_token().expose_secret(), "id-do-not-leak");
        assert_eq!(loaded.access_token().expose_secret(), "access-do-not-leak");
        assert_eq!(
            loaded.refresh_token().expose_secret(),
            "refresh-do-not-leak"
        );
        let rendered = format!("{store:?} {loaded:?}");
        assert!(!rendered.contains("do-not-leak"));
    }

    #[test]
    fn codex_store_rolls_back_all_three_credentials_when_refresh_write_fails() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxCodexSecretStore::new(root.path());
        let handles = codex_handles();
        let initial = codex_auth::CodexStoredCredentials::new(
            codex_auth::SecretString::new("id-initial").unwrap(),
            codex_auth::SecretString::new("access-initial").unwrap(),
            codex_auth::SecretString::new("refresh-initial").unwrap(),
        );
        store.store_credentials(&handles, &initial).unwrap();

        let conn =
            rusqlite::Connection::open(crate::secrets::secret_store_path(root.path())).unwrap();
        conn.execute_batch(
            r#"
            CREATE TRIGGER reject_codex_refresh_rotation
            BEFORE UPDATE ON ctox_secret_records
            WHEN NEW.secret_name = 'codex-primary-refresh'
            BEGIN
                SELECT RAISE(ABORT, 'forced Codex refresh rollback');
            END;
            "#,
        )
        .unwrap();
        drop(conn);

        let rotated = codex_auth::CodexStoredCredentials::new(
            codex_auth::SecretString::new("id-rotated").unwrap(),
            codex_auth::SecretString::new("access-rotated").unwrap(),
            codex_auth::SecretString::new("refresh-rotated").unwrap(),
        );
        assert_eq!(
            store.store_credentials(&handles, &rotated),
            Err(codex_auth::SecretStoreError::Write)
        );
        let retained = store.load_credentials(&handles).unwrap();
        assert_eq!(retained.id_token().expose_secret(), "id-initial");
        assert_eq!(retained.access_token().expose_secret(), "access-initial");
        assert_eq!(retained.refresh_token().expose_secret(), "refresh-initial");
    }

    #[test]
    fn antigravity_store_atomically_round_trips_tokens_expiry_and_project() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxAntigravitySecretStore::new(root.path());
        let handles = antigravity_handles();
        let expiry = SystemTime::UNIX_EPOCH + Duration::from_millis(1_234_567);
        store
            .store_credentials(
                &handles,
                &antigravity_credentials(
                    "access-do-not-leak",
                    "refresh-do-not-leak",
                    expiry,
                    "project-1",
                ),
            )
            .unwrap();
        let loaded = store.load_credentials(&handles).unwrap();
        assert_eq!(loaded.access_token().expose_secret(), "access-do-not-leak");
        assert_eq!(
            loaded.refresh_token().expose_secret(),
            "refresh-do-not-leak"
        );
        assert_eq!(loaded.expires_at(), expiry);
        assert_eq!(loaded.project_id(), "project-1");
        let rendered = format!("{store:?} {loaded:?}");
        assert!(!rendered.contains("do-not-leak"));
    }

    #[test]
    fn antigravity_store_rolls_back_all_records_when_state_write_fails() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxAntigravitySecretStore::new(root.path());
        let handles = antigravity_handles();
        let initial_expiry = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        store
            .store_credentials(
                &handles,
                &antigravity_credentials(
                    "access-initial",
                    "refresh-initial",
                    initial_expiry,
                    "project-initial",
                ),
            )
            .unwrap();
        let conn =
            rusqlite::Connection::open(crate::secrets::secret_store_path(root.path())).unwrap();
        conn.execute_batch(
            r#"
            CREATE TRIGGER reject_antigravity_state_rotation
            BEFORE UPDATE ON ctox_secret_records
            WHEN NEW.secret_name = 'antigravity-primary-state'
            BEGIN
                SELECT RAISE(ABORT, 'forced state rollback');
            END;
            "#,
        )
        .unwrap();
        drop(conn);
        let error = store.store_credentials(
            &handles,
            &antigravity_credentials(
                "access-rotated",
                "refresh-rotated",
                initial_expiry + Duration::from_secs(3_600),
                "project-rotated",
            ),
        );
        assert_eq!(error, Err(antigravity_auth::AntigravityTokenError::Write));
        let retained = store.load_credentials(&handles).unwrap();
        assert_eq!(retained.access_token().expose_secret(), "access-initial");
        assert_eq!(retained.refresh_token().expose_secret(), "refresh-initial");
        assert_eq!(retained.project_id(), "project-initial");
        assert_eq!(retained.expires_at(), initial_expiry);
    }

    #[test]
    fn cooldown_snapshot_persists_in_stable_order_and_can_be_cleared() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxCooldownStateStore::new(root.path());
        store
            .save(&[cooldown("account-b", 3_000), cooldown("account-a", 2_000)])
            .unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(
            loaded
                .iter()
                .map(|record| record.auth_id.as_str())
                .collect::<Vec<_>>(),
            ["account-a", "account-b"]
        );

        store.save(&[]).unwrap();
        assert!(store.load().unwrap().is_empty());
    }

    #[test]
    fn invalid_cooldown_snapshot_does_not_replace_existing_state() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxCooldownStateStore::new(root.path());
        store.save(&[cooldown("account-a", 2_000)]).unwrap();

        let invalid = cooldown(" ", 4_000);
        assert_eq!(
            store.save(&[invalid]),
            Err(CooldownStoreError::InvalidRecord)
        );
        assert_eq!(store.load().unwrap()[0].auth_id, "account-a");
    }

    #[test]
    fn signature_store_persists_refreshes_expires_and_deletes_exact_keys() {
        let root = tempfile::tempdir().unwrap();
        let store = CtoxSignatureKvStore::new(root.path());
        let key = "cpa:signature:claude:0123456789abcdef";
        let value = b"provider-signature";

        assert!(store.set(key, value, Duration::from_secs(60)).unwrap());
        assert_eq!(store.get(key).unwrap(), Some(value.to_vec()));
        assert!(store.expire(key, Duration::from_secs(120)).unwrap());
        assert!(store.delete(key).unwrap());
        assert_eq!(store.get(key).unwrap(), None);

        assert!(store.set(key, value, Duration::from_secs(60)).unwrap());
        let conn = Connection::open(crate::inference::runtime_env::runtime_config_path(
            root.path(),
        ))
        .unwrap();
        conn.execute(
            &format!(
                "UPDATE {INSTANCE_SIGNATURE_CACHE_TABLE} SET expires_at_ms = 0 WHERE cache_key = ?1"
            ),
            [key],
        )
        .unwrap();
        drop(conn);
        assert_eq!(store.get(key).unwrap(), None);

        assert_eq!(
            store.set("", value, Duration::from_secs(60)),
            Err(SignatureCacheStoreError::Write)
        );
        assert_eq!(
            store.set(key, b"", Duration::from_secs(60)),
            Err(SignatureCacheStoreError::Write)
        );
    }

    struct UnusedRefreshTransport;

    impl ClaudeRefreshTransport for UnusedRefreshTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a RefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<RefreshHttpResponse, RefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Err(RefreshTransportFailure::Protocol) })
        }
    }

    struct SuccessMessagesTransport;

    impl ClaudeMessagesTransport for SuccessMessagesTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a ClaudeMessagesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<ClaudeMessagesResponse, ClaudeMessagesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(ClaudeMessagesResponse::new(200, b"{}".to_vec())) })
        }
    }

    #[derive(Default)]
    struct ProfileRefreshTransport {
        inspections: AtomicUsize,
    }

    impl ClaudeRefreshTransport for ProfileRefreshTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a RefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<RefreshHttpResponse, RefreshTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Err(RefreshTransportFailure::Protocol) })
        }

        fn inspect<'a>(
            &'a self,
            request: &'a OAuthInspectRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            ctox_cliproxyapi::internal::auth::claude::OAuthInspectHttpResponse,
                            RefreshTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            assert_eq!(request.kind(), OAuthInspectKind::Profile);
            self.inspections.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(
                    ctox_cliproxyapi::internal::auth::claude::OAuthInspectHttpResponse::new(
                        200,
                        br#"{"account":{"uuid":"host-account","email":"host@example.com"},"organization":{"uuid":"host-org","name":"Host Org"}}"#.to_vec(),
                    ),
                )
            })
        }
    }

    #[derive(Default)]
    struct CapturingMessagesTransport(Mutex<Vec<Vec<u8>>>);

    impl ClaudeMessagesTransport for CapturingMessagesTransport {
        fn execute<'a>(
            &'a self,
            request: &'a ClaudeMessagesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<ClaudeMessagesResponse, ClaudeMessagesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            self.0.lock().unwrap().push(request.body().to_vec());
            Box::pin(async { Ok(ClaudeMessagesResponse::new(200, b"{}".to_vec())) })
        }
    }

    struct FixedRuntimeClock;

    impl RefreshClock for FixedRuntimeClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(10)
        }

        fn sleep(
            &self,
            _duration: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<(), RefreshTransportFailure>> + Send + '_>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    impl AccountStateClock for FixedRuntimeClock {
        fn now_ms(&self) -> i64 {
            10_000
        }
    }

    struct UnusedCodexRefreshTransport;

    impl codex_auth::CodexRefreshTransport for UnusedCodexRefreshTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a codex_auth::CodexRefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            codex_auth::CodexRefreshHttpResponse,
                            codex_auth::CodexRefreshTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Err(codex_auth::CodexRefreshTransportFailure::Protocol) })
        }
    }

    struct FixedCodexRuntimeClock;

    impl codex_auth::RefreshClock for FixedCodexRuntimeClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(10)
        }

        fn sleep(
            &self,
            _duration: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<(), codex_auth::CodexRefreshTransportFailure>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async { Ok(()) })
        }
    }

    struct SuccessCodexResponsesTransport;

    impl CodexResponsesTransport for SuccessCodexResponsesTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a CodexResponsesRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<CodexResponsesResponse, CodexResponsesTransportFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async {
                Ok(CodexResponsesResponse::new(
                    200,
                    None,
                    b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_runtime\",\"object\":\"response\",\"status\":\"completed\",\"output\":[]}}\n\n".to_vec(),
                ))
            })
        }
    }

    struct UnusedAntigravityRefreshTransport;

    impl antigravity_auth::AntigravityRefreshTransport for UnusedAntigravityRefreshTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a antigravity_auth::AntigravityRefreshRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            antigravity_auth::AntigravityRefreshHttpResponse,
                            antigravity_auth::AntigravityRefreshTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Err(antigravity_auth::AntigravityRefreshTransportFailure::Protocol) })
        }
    }

    struct FixedAntigravityRuntimeClock;

    impl AntigravityAuthClock for FixedAntigravityRuntimeClock {
        fn now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_secs(10)
        }
    }

    struct SuccessAntigravityGenerateTransport;

    impl AntigravityGenerateTransport for SuccessAntigravityGenerateTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a AntigravityGenerateRequest,
            _timeout: Duration,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            AntigravityGenerateResponse,
                            AntigravityGenerateTransportFailure,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async {
                Ok(AntigravityGenerateResponse::new(
                    200,
                    None,
                    br#"{"response":{"candidates":[{"content":{"parts":[{"text":"hello"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}}}"#.to_vec(),
                ))
            })
        }
    }

    #[tokio::test]
    async fn typed_runtime_factory_composes_ctox_secrets_and_cooldown_store() {
        let root = tempfile::tempdir().unwrap();
        let account = ctox_cliproxyapi::internal::config::ClaudeSubscriptionAccountConfig {
            id: "account-a".to_owned(),
            disabled: false,
            priority: 0,
            weight: 1,
            websockets: false,
            models: Vec::new(),
            access_token_secret: ctox_cliproxyapi::internal::config::RuntimeSecretRef {
                scope: "provider-subscriptions".to_owned(),
                name: "claude-primary-access".to_owned(),
            },
            refresh_token_secret: ctox_cliproxyapi::internal::config::RuntimeSecretRef {
                scope: "provider-subscriptions".to_owned(),
                name: "claude-primary-refresh".to_owned(),
            },
            upstream_scheme: "https".to_owned(),
            upstream_authority: "api.anthropic.com".to_owned(),
            proxy_url_secret: None,
            device_profile: None,
            timezone: "Pacific/Honolulu".to_owned(),
        };
        let config = ctox_cliproxyapi::internal::config::CliproxyRuntimeConfig {
            request_timeout_ms: 5_000,
            routing_strategy: SchedulerStrategy::RoundRobin,
            claude_accounts: vec![account],
            codex_accounts: Vec::new(),
            antigravity_accounts: Vec::new(),
        }
        .validate()
        .unwrap();
        CtoxClaudeSecretStore::new(root.path())
            .store_credentials(
                &config.claude_accounts()[0].credential_handles().unwrap(),
                &credentials("access-runtime", "refresh-runtime"),
            )
            .unwrap();
        let transports = HashMap::from([(
            "account-a".to_owned(),
            ClaudeAccountTransports {
                refresh: Arc::new(UnusedRefreshTransport),
                messages: Arc::new(SuccessMessagesTransport),
                messages_stream: None,
            },
        )]);
        let pool = CtoxCliproxyRuntimeFactory::new(root.path())
            .build_claude_pool(
                &config,
                &transports,
                Arc::new(FixedRuntimeClock),
                Arc::new(FixedRuntimeClock),
            )
            .unwrap();

        let outcome = pool
            .execute_configured("sonnet", b"{}".to_vec(), false)
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "account-a");
        assert_eq!(outcome.outcome().response().status(), 200);
        assert_eq!(outcome.outcome().state_persisted(), Some(true));
    }

    #[tokio::test]
    async fn typed_runtime_factory_prepares_profile_and_device_before_first_claude_request() {
        let root = tempfile::tempdir().unwrap();
        let mut runtime = stored_claude_runtime();
        runtime.claude_accounts[0].id = "prepared-account".to_owned();
        runtime.claude_accounts[0].access_token_secret.name = "prepared-access".to_owned();
        runtime.claude_accounts[0].refresh_token_secret.name = "prepared-refresh".to_owned();
        let config = runtime.validate().unwrap();
        CtoxClaudeSecretStore::new(root.path())
            .store_credentials(
                &config.claude_accounts()[0].credential_handles().unwrap(),
                &credentials("sk-ant-oat-host-profile", "refresh-runtime"),
            )
            .unwrap();
        let refresh = Arc::new(ProfileRefreshTransport::default());
        let messages = Arc::new(CapturingMessagesTransport::default());
        let transports = HashMap::from([(
            "prepared-account".to_owned(),
            ClaudeAccountTransports {
                refresh: refresh.clone(),
                messages: messages.clone(),
                messages_stream: None,
            },
        )]);
        let pool = CtoxCliproxyRuntimeFactory::new(root.path())
            .build_claude_pool(
                &config,
                &transports,
                Arc::new(FixedRuntimeClock),
                Arc::new(FixedRuntimeClock),
            )
            .unwrap();

        pool.execute_configured(
            "claude-sonnet-4-6",
            br#"{"messages":[{"role":"user","content":"hello"}]}"#.to_vec(),
            false,
        )
        .await
        .unwrap();
        pool.execute_configured(
            "claude-sonnet-4-6",
            br#"{"messages":[{"role":"user","content":"again"}]}"#.to_vec(),
            false,
        )
        .await
        .unwrap();

        assert_eq!(refresh.inspections.load(Ordering::SeqCst), 1);
        let bodies = messages.0.lock().unwrap();
        assert_eq!(bodies.len(), 2);
        for body in bodies.iter() {
            let value: serde_json::Value = serde_json::from_slice(body).unwrap();
            let user_id = value["metadata"]["user_id"].as_str().unwrap();
            assert!(user_id.contains("host-account"));
            assert!(!user_id.contains("sk-ant-oat-host-profile"));
        }
    }

    #[tokio::test]
    async fn typed_runtime_factory_builds_codex_pool_from_ctox_secrets() {
        let root = tempfile::tempdir().unwrap();
        let secret = |name: &str| ctox_cliproxyapi::internal::config::RuntimeSecretRef {
            scope: "provider-subscriptions".to_owned(),
            name: name.to_owned(),
        };
        let account = ctox_cliproxyapi::internal::config::CodexSubscriptionAccountConfig {
            id: "codex-a".to_owned(),
            disabled: false,
            priority: 0,
            weight: 1,
            websockets: false,
            models: Vec::new(),
            id_token_secret: secret("codex-primary-id"),
            access_token_secret: secret("codex-primary-access"),
            refresh_token_secret: secret("codex-primary-refresh"),
            upstream_base_url: "https://chatgpt.example/backend-api/codex".to_owned(),
            plan_type: "pro".to_owned(),
            proxy_url_secret: None,
        };
        let config = ctox_cliproxyapi::internal::config::CliproxyRuntimeConfig {
            request_timeout_ms: 5_000,
            routing_strategy: SchedulerStrategy::RoundRobin,
            claude_accounts: Vec::new(),
            codex_accounts: vec![account],
            antigravity_accounts: Vec::new(),
        }
        .validate()
        .unwrap();
        CtoxCodexSecretStore::new(root.path())
            .store_credentials(
                &config.codex_accounts()[0].credential_handles().unwrap(),
                &codex_auth::CodexStoredCredentials::new(
                    codex_auth::SecretString::new("invalid-jwt").unwrap(),
                    codex_auth::SecretString::new("codex-access-runtime").unwrap(),
                    codex_auth::SecretString::new("codex-refresh-runtime").unwrap(),
                ),
            )
            .unwrap();
        let transports = HashMap::from([(
            "codex-a".to_owned(),
            CodexAccountTransports {
                refresh: Arc::new(UnusedCodexRefreshTransport),
                responses: Arc::new(SuccessCodexResponsesTransport),
                responses_stream: None,
            },
        )]);
        let pool = CtoxCliproxyRuntimeFactory::new(root.path())
            .build_codex_pool(
                &config,
                &transports,
                Arc::new(FixedCodexRuntimeClock),
                Arc::new(FixedRuntimeClock),
            )
            .unwrap();

        let outcome = pool
            .execute_configured("gpt-5.5", br#"{"input":"hello"}"#.to_vec(), false)
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "codex-a");
        assert_eq!(outcome.outcome().attempts(), 1);
        assert!(
            !String::from_utf8_lossy(outcome.outcome().payload()).contains("codex-access-runtime")
        );
    }

    #[tokio::test]
    async fn typed_runtime_factory_builds_antigravity_pool_with_shared_replay() {
        let root = tempfile::tempdir().unwrap();
        let secret = |name: &str| ctox_cliproxyapi::internal::config::RuntimeSecretRef {
            scope: "provider-subscriptions".to_owned(),
            name: name.to_owned(),
        };
        let account = ctox_cliproxyapi::internal::config::AntigravitySubscriptionAccountConfig {
            id: "antigravity-a".to_owned(),
            disabled: false,
            priority: 0,
            weight: 1,
            websockets: false,
            models: Vec::new(),
            access_token_secret: secret("antigravity-primary-access"),
            refresh_token_secret: secret("antigravity-primary-refresh"),
            state_secret: secret("antigravity-primary-state"),
            upstream_base_url: "https://daily-cloudcode-pa.googleapis.com".to_owned(),
            proxy_url_secret: None,
        };
        let config = ctox_cliproxyapi::internal::config::CliproxyRuntimeConfig {
            request_timeout_ms: 5_000,
            routing_strategy: SchedulerStrategy::RoundRobin,
            claude_accounts: Vec::new(),
            codex_accounts: Vec::new(),
            antigravity_accounts: vec![account],
        }
        .validate()
        .unwrap();
        CtoxAntigravitySecretStore::new(root.path())
            .store_credentials(
                &config.antigravity_accounts()[0]
                    .credential_handles()
                    .unwrap(),
                &antigravity_credentials(
                    "antigravity-access-runtime",
                    "antigravity-refresh-runtime",
                    SystemTime::UNIX_EPOCH + Duration::from_secs(3_600),
                    "project-runtime",
                ),
            )
            .unwrap();
        let transports = HashMap::from([(
            "antigravity-a".to_owned(),
            AntigravityAccountTransports {
                refresh: Arc::new(UnusedAntigravityRefreshTransport),
                generate: Arc::new(SuccessAntigravityGenerateTransport),
                generate_stream: None,
            },
        )]);
        let pool = CtoxCliproxyRuntimeFactory::new(root.path())
            .build_antigravity_pool(
                &config,
                &transports,
                Arc::new(FixedAntigravityRuntimeClock),
                Arc::new(FixedRuntimeClock),
            )
            .unwrap();
        let outcome = pool
            .execute_configured(
                "gemini-3-flash-agent",
                br#"{"model":"gemini-3-flash-agent","input":"hello"}"#.to_vec(),
                br#"{"request":{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}}"#
                    .to_vec(),
            )
            .await
            .unwrap();
        assert_eq!(outcome.selected_auth_id(), "antigravity-a");
        assert_eq!(outcome.outcome().attempts(), 1);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(outcome.outcome().payload()).unwrap()
                ["status"],
            "completed"
        );
    }
}
