// ref: sdk/cliproxy/auth/conductor_execution.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex};

use crate::sdk::cliproxy::executor::RequestTerminatedError;
use crate::sdk::cliproxy::executor::{ExecutionMetadata, Options};
use crate::sdk::cliproxy::usage::UsageContext;
use crate::sdk::pluginapi::{ExecutorRequest, ExecutorResponse, PluginExecutionError};
use chrono::{DateTime, Utc};

use super::{
    access_token, prepare_executor_request, AccountExecutionResult, Auth, AuthError, AuthKind,
    AuthLifecycleRefreshError, AuthManager, AuthManagerError, AuthMutationOptions,
    AuthPreparationError, CooldownConductor, ModelResumeSink, ProviderExecutorRegistration,
    RefreshTransactionError,
};

#[must_use]
pub(crate) fn plugin_error_status(error: &PluginExecutionError) -> u16 {
    let mut current: &(dyn std::error::Error + 'static) = error.as_ref();
    loop {
        if let Some(error) = current.downcast_ref::<AuthError>() {
            return error.status_code();
        }
        if let Some(error) = current.downcast_ref::<RequestTerminatedError>() {
            return error.status_code();
        }
        let Some(source) = current.source() else {
            return 0;
        };
        current = source;
    }
}

#[must_use]
pub(crate) fn is_request_scoped_plugin_error(error: &PluginExecutionError) -> bool {
    let mut current: &(dyn std::error::Error + 'static) = error.as_ref();
    loop {
        if current.downcast_ref::<AuthError>().is_some_and(|error| {
            error.is_request_scoped()
                || (error.status_code() == 404
                    && is_request_scoped_not_found_message(&error.message))
        }) || current.downcast_ref::<RequestTerminatedError>().is_some()
        {
            return true;
        }
        let Some(source) = current.source() else {
            return false;
        };
        current = source;
    }
}

#[must_use]
pub(crate) fn is_unauthorized_plugin_error(error: &PluginExecutionError) -> bool {
    plugin_error_status(error) == 401
}

#[must_use]
pub(crate) fn is_claude_oauth_request_cancellation(
    auth: &Auth,
    error: &PluginExecutionError,
) -> bool {
    auth.provider.trim().eq_ignore_ascii_case("claude")
        && auth.auth_kind() == Some(AuthKind::OAuth)
        && is_request_scoped_plugin_error(error)
}

use super::cooldown_state::{CooldownStateStore, CooldownStoreError};
use super::scheduler::{AuthScheduler, ScheduledAccount, SchedulerPickOptions, SchedulerStrategy};
use super::selector::{AccountCandidate, AccountSelectionError};

/// Typed CTOX replacement for upstream's context-value enrichment.
#[must_use]
pub fn usage_context_with_requested_model_alias(
    mut context: UsageContext,
    options: &Options,
    fallback_model: &str,
) -> UsageContext {
    let requested = options
        .metadata
        .requested_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback_model.trim());
    if !requested.is_empty() {
        context = context.with_requested_model_alias(requested);
    }
    if let Some(effort) = options.metadata.reasoning_effort.as_deref() {
        context = context.with_reasoning_effort(effort);
    }
    if let Some(tier) = options.metadata.service_tier.as_deref() {
        context = context.with_service_tier(tier);
    }
    if let Some(generate) = options.metadata.generate {
        context = context.with_generate(generate);
    }
    context
}

pub fn publish_selected_auth_metadata(metadata: &mut ExecutionMetadata, auth: &mut Auth) {
    let index = auth.ensure_index();
    metadata.selected_auth_id = Some(auth.id.clone());
    metadata.selected_auth_index = Some(index.clone());
    metadata.notify_selected_auth(&auth.id, &index);
}

/// Provider-neutral persisted-state selection boundary used by CTOX's bounded
/// execution loops. Provider wire execution remains owned by the injected
/// `ProviderExecutorRegistry`, rather than being duplicated here.
pub struct AccountRouter {
    scheduler: AuthScheduler,
    store: Arc<dyn CooldownStateStore>,
}

impl AccountRouter {
    pub fn new(store: Arc<dyn CooldownStateStore>) -> Self {
        Self {
            scheduler: AuthScheduler::new(SchedulerStrategy::RoundRobin),
            store,
        }
    }

    pub fn with_strategy(store: Arc<dyn CooldownStateStore>, strategy: SchedulerStrategy) -> Self {
        Self {
            scheduler: AuthScheduler::new(strategy),
            store,
        }
    }

    pub fn select(
        &self,
        provider: &str,
        model: Option<&str>,
        now_ms: i64,
        candidates: &[AccountCandidate],
    ) -> Result<AccountCandidate, AccountRoutingError> {
        self.select_with_options(
            provider,
            model,
            now_ms,
            candidates,
            &SchedulerPickOptions::default(),
        )
    }

    pub fn select_with_options(
        &self,
        provider: &str,
        model: Option<&str>,
        now_ms: i64,
        candidates: &[AccountCandidate],
        options: &SchedulerPickOptions,
    ) -> Result<AccountCandidate, AccountRoutingError> {
        let cooldowns = self.store.load().map_err(AccountRoutingError::Store)?;
        self.scheduler
            .pick_single(provider, model, now_ms, candidates, &cooldowns, options)
            .map_err(AccountRoutingError::Selection)
    }

    pub fn select_mixed(
        &self,
        providers: &[String],
        model: Option<&str>,
        now_ms: i64,
        candidates: &[AccountCandidate],
        options: &SchedulerPickOptions,
    ) -> Result<ScheduledAccount, AccountRoutingError> {
        let cooldowns = self.store.load().map_err(AccountRoutingError::Store)?;
        self.scheduler
            .pick_mixed(providers, model, now_ms, candidates, &cooldowns, options)
            .map_err(AccountRoutingError::Selection)
    }
}

impl fmt::Debug for AccountRouter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccountRouter")
            .field("scheduler", &self.scheduler)
            .field("store", &"CooldownStateStore")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountRoutingError {
    Store(CooldownStoreError),
    Selection(AccountSelectionError),
}

impl fmt::Display for AccountRoutingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "account state unavailable: {error}"),
            Self::Selection(error) => write!(formatter, "account selection failed: {error}"),
        }
    }
}

impl std::error::Error for AccountRoutingError {}

/// Wall-clock authority for the non-Home conductor. Tests can pin both
/// scheduler eligibility and persisted cooldown deadlines to one instant.
pub trait GenericConductorClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug)]
pub struct SystemGenericConductorClock;

impl GenericConductorClock for SystemGenericConductorClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::<Utc>::from(std::time::SystemTime::now())
    }
}

/// Provider-independent execution owner used when Home dispatch is disabled.
///
/// `ProviderExecutorRegistry` remains the raw capability registry. This layer
/// owns Candidate's selection -> preparation -> execution -> outcome sequence
/// and is deliberately separate from `HomeAuthRuntime`, whose ephemeral auth
/// selections remain authoritative in Home mode.
pub struct GenericAuthRuntime {
    pub(super) manager: Arc<AuthManager>,
    router: Arc<AccountRouter>,
    cooldown: Arc<CooldownConductor>,
    clock: Arc<dyn GenericConductorClock>,
    publication: Arc<dyn ModelResumeSink>,
    prepare_locks: Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>,
    max_credentials: usize,
}

fn disabled_preparation_error() -> AuthPreparationError {
    Arc::new(AuthError {
        code: "account_disabled".into(),
        message: "Selected account is disabled".into(),
        retryable: false,
        http_status: 403,
    })
}

impl GenericAuthRuntime {
    #[must_use]
    pub fn new(
        manager: Arc<AuthManager>,
        state: Arc<dyn CooldownStateStore>,
        resume_sink: Arc<dyn ModelResumeSink>,
    ) -> Self {
        Self::new_with_clock(
            manager,
            state,
            resume_sink,
            Arc::new(SystemGenericConductorClock),
        )
    }

    #[must_use]
    pub fn new_with_clock(
        manager: Arc<AuthManager>,
        state: Arc<dyn CooldownStateStore>,
        resume_sink: Arc<dyn ModelResumeSink>,
        clock: Arc<dyn GenericConductorClock>,
    ) -> Self {
        let publication = manager.refresh_publication_sink(resume_sink);
        Self {
            manager,
            router: Arc::new(AccountRouter::new(state.clone())),
            cooldown: Arc::new(CooldownConductor::new(state)),
            clock,
            publication,
            prepare_locks: Mutex::new(BTreeMap::new()),
            max_credentials: 64,
        }
    }

    #[must_use]
    pub fn with_max_credentials(mut self, max_credentials: usize) -> Self {
        self.max_credentials = max_credentials.max(1);
        self
    }

    pub async fn execute(
        &self,
        providers: &[String],
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, GenericExecutionError> {
        self.execute_unary(providers, request, UnaryOperation::Execute)
            .await
    }

    pub async fn count_tokens(
        &self,
        providers: &[String],
        request: ExecutorRequest,
    ) -> Result<ExecutorResponse, GenericExecutionError> {
        self.execute_unary(providers, request, UnaryOperation::CountTokens)
            .await
    }

    async fn execute_unary(
        &self,
        providers: &[String],
        request: ExecutorRequest,
        operation: UnaryOperation,
    ) -> Result<ExecutorResponse, GenericExecutionError> {
        let providers = normalize_execution_providers(providers)?;
        let route_model = auth_selection_model(&request);
        let mut options = scheduler_options(&request);
        let mut last_error = None;

        while options.tried_auth_ids.len() < self.max_credentials {
            let (mut auth, registration) = match self.select(&providers, &route_model, &options) {
                Ok(selected) => selected,
                Err(error) => return Err(last_error.unwrap_or(error)),
            };
            options.tried_auth_ids.insert(auth.id.clone());
            if let Err(error) = self.prepare(&registration, &mut auth).await {
                if is_request_scoped_error(error.as_ref()) {
                    return Err(GenericExecutionError::Preparation(error));
                }
                self.record_outcome(&auth, &route_model, error_status(error.as_ref()), false)?;
                last_error = Some(GenericExecutionError::Preparation(error));
                continue;
            }

            let executor = registration
                .execution()
                .ok_or(GenericExecutionError::ExecutionUnavailable)?;
            let execute = |auth: &Auth| {
                let execution = selected_executor_request(
                    &self.manager,
                    &request,
                    auth,
                    registration.provider(),
                );
                let executor = executor.clone();
                async move {
                    match operation {
                        UnaryOperation::Execute => executor.execute(execution).await,
                        UnaryOperation::CountTokens => executor.count_tokens(execution).await,
                    }
                }
            };
            let mut result = execute(&auth).await;
            if result
                .as_ref()
                .err()
                .is_some_and(is_unauthorized_plugin_error)
            {
                if let Some(refreshed) = self
                    .refresh_after_unauthorized(&auth, &registration)
                    .await?
                {
                    auth = refreshed;
                    self.prepare(&registration, &mut auth)
                        .await
                        .map_err(GenericExecutionError::Preparation)?;
                    result = execute(&auth).await;
                }
            }

            match result {
                Ok(response) => {
                    self.record_outcome(&auth, &route_model, 200, true)?;
                    return Ok(response);
                }
                Err(error) if is_request_scoped_plugin_error(&error) => {
                    return Err(GenericExecutionError::Provider(error));
                }
                Err(error) => {
                    let status = plugin_error_status(&error);
                    if matches!(operation, UnaryOperation::CountTokens)
                        && is_count_tokens_endpoint_not_found(&error, &request.model)
                    {
                        self.record_availability_neutral_outcome(&auth.id, false);
                    } else {
                        self.record_outcome(&auth, &route_model, status, false)?;
                    }
                    if matches!(status, 400 | 422) {
                        return Err(GenericExecutionError::Provider(error));
                    }
                    last_error = Some(GenericExecutionError::Provider(error));
                }
            }
        }
        Err(last_error.unwrap_or(GenericExecutionError::CredentialLimit))
    }

    pub(crate) fn select(
        &self,
        providers: &[String],
        route_model: &str,
        options: &SchedulerPickOptions,
    ) -> Result<(Auth, Arc<ProviderExecutorRegistration>), GenericExecutionError> {
        let now = self.clock.now();
        let selected = self
            .router
            .select_mixed(
                providers,
                Some(route_model),
                now.timestamp_millis(),
                &self.manager.candidates(),
                options,
            )
            .map_err(GenericExecutionError::Routing)?;
        let auth = self
            .manager
            .lifecycle()
            .get_cached(&selected.candidate.auth_id)
            .ok_or(GenericExecutionError::AuthUnavailable)?;
        let registration = self
            .manager
            .executors()
            .get(&selected.provider)
            .ok_or(GenericExecutionError::ProviderNotRegistered)?;
        if registration.execution().is_none() {
            return Err(GenericExecutionError::ExecutionUnavailable);
        }
        Ok((auth, registration))
    }

    pub(crate) async fn prepare(
        &self,
        registration: &ProviderExecutorRegistration,
        auth: &mut Auth,
    ) -> Result<(), AuthPreparationError> {
        let Some(preparer) = registration.auth_preparer() else {
            return Ok(());
        };
        let _guard = self.prepare_lock(&auth.id).lock_owned().await;
        *auth = self
            .manager
            .lifecycle()
            .get_cached(&auth.id)
            .ok_or_else(|| {
                Arc::new(super::AuthLifecycleError::AuthNotRegistered) as AuthPreparationError
            })?;
        if auth.disabled {
            return Err(disabled_preparation_error());
        }
        if !preparer.should_prepare(auth) {
            return Ok(());
        }
        let base = auth.clone();
        preparer.prepare(auth).await?;
        let published = self
            .manager
            .update_prepared_auth(
                &base,
                auth.clone(),
                AuthMutationOptions::default(),
                self.clock.now(),
            )
            .map_err(|error| Arc::new(error) as AuthPreparationError)?;
        *auth = published.ok_or_else(|| {
            Arc::new(super::AuthLifecycleError::AuthNotRegistered) as AuthPreparationError
        })?;
        if auth.disabled {
            return Err(disabled_preparation_error());
        }
        Ok(())
    }

    fn prepare_lock(&self, auth_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self
            .prepare_locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        locks
            .entry(auth_id.trim().to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    pub(crate) async fn refresh_after_unauthorized(
        &self,
        auth: &Auth,
        registration: &ProviderExecutorRegistration,
    ) -> Result<Option<Auth>, GenericExecutionError> {
        if let Some(refresher) = registration.async_auth_refresher() {
            if !refresher.should_refresh(auth) {
                return Ok(None);
            }
            // Preparation and a 401 mint share one credential-scoped owner.
            // No synchronous runtime bridge or detached refresh is needed.
            let _guard = self.prepare_lock(&auth.id).lock_owned().await;
            let current = self
                .manager
                .lifecycle()
                .get_cached(&auth.id)
                .ok_or(GenericExecutionError::AuthUnavailable)?;
            if current.disabled || current.registration_epoch != auth.registration_epoch {
                return Err(GenericExecutionError::AuthUnavailable);
            }
            // Another request already replaced the failed credential while
            // this attempt waited. Reuse it without minting the old key again.
            if refresher.credential_changed(&current, auth) {
                return Ok(Some(current));
            }
            let candidate = refresher
                .refresh(&current)
                .await
                .map_err(GenericExecutionError::Preparation)?;
            let published = self
                .manager
                .update_refreshed_auth(
                    &current,
                    candidate,
                    AuthMutationOptions::default(),
                    self.clock.now(),
                )
                .map_err(|error| GenericExecutionError::Preparation(Arc::new(error)))?
                .ok_or(GenericExecutionError::AuthUnavailable)?;
            if published.disabled {
                return Err(GenericExecutionError::AuthUnavailable);
            }
            return Ok(Some(published));
        }
        if auth.auth_kind() != Some(AuthKind::OAuth) {
            return Ok(None);
        }
        let failed_token = access_token(auth).map(str::to_owned);
        match self.manager.lifecycle().refresh(
            &auth.id,
            failed_token.as_deref(),
            self.clock.now(),
            registration.refresher().as_ref(),
            self.publication.as_ref(),
        ) {
            Ok(outcome) => Ok(Some(outcome.auth)),
            Err(AuthLifecycleRefreshError::Refresh(RefreshTransactionError::NotRefreshable)) => {
                Ok(None)
            }
            Err(error) => Err(GenericExecutionError::Refresh(error)),
        }
    }

    pub(crate) fn record_outcome(
        &self,
        auth: &Auth,
        model: &str,
        status: u16,
        success: bool,
    ) -> Result<(), GenericExecutionError> {
        let observed_at = self.clock.now();
        self.cooldown
            .record(AccountExecutionResult {
                provider: auth.provider.clone(),
                auth_id: auth.id.clone(),
                model: (!model.trim().is_empty()).then(|| model.trim().to_owned()),
                status,
                retry_delay_ms: None,
                observed_at_ms: observed_at.timestamp_millis(),
            })
            .map_err(GenericExecutionError::Cooldown)?;
        self.manager
            .lifecycle()
            .record_execution_outcome(&auth.id, observed_at, success);
        Ok(())
    }

    pub(crate) fn record_availability_neutral_outcome(&self, auth_id: &str, success: bool) {
        self.manager
            .lifecycle()
            .record_execution_outcome(auth_id, self.clock.now(), success);
    }

    #[must_use]
    pub(crate) fn max_credentials(&self) -> usize {
        self.max_credentials
    }
}

impl fmt::Debug for GenericAuthRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GenericAuthRuntime")
            .field("max_credentials", &self.max_credentials)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
enum UnaryOperation {
    Execute,
    CountTokens,
}

pub(crate) fn normalize_execution_providers(
    providers: &[String],
) -> Result<Vec<String>, GenericExecutionError> {
    let mut seen = HashSet::new();
    let providers = providers
        .iter()
        .map(|provider| provider.trim().to_ascii_lowercase())
        .filter(|provider| !provider.is_empty() && seen.insert(provider.clone()))
        .collect::<Vec<_>>();
    if providers.is_empty() {
        Err(GenericExecutionError::ProviderNotSupplied)
    } else {
        Ok(providers)
    }
}

pub(crate) fn auth_selection_model(request: &ExecutorRequest) -> String {
    request
        .metadata
        .get("auth_selection_model")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| request.model.trim())
        .to_owned()
}

pub(crate) fn scheduler_options(request: &ExecutorRequest) -> SchedulerPickOptions {
    SchedulerPickOptions {
        pinned_auth_id: request
            .metadata
            .get("pinned_auth_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned),
        prefer_websocket: request
            .metadata
            .get("prefer_websocket")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        tried_auth_ids: HashSet::new(),
    }
}

pub(crate) fn selected_executor_request(
    manager: &AuthManager,
    request: &ExecutorRequest,
    auth: &Auth,
    provider: &str,
) -> ExecutorRequest {
    let mut auth = auth.clone();
    let index = auth.ensure_index();
    let mut execution = prepare_executor_request(request, &auth, provider);
    let route_model = auth_selection_model(request);
    let (_, alias, snapshot) =
        manager.execution_model_candidates_with_alias(&auth, &execution.model);
    if !alias.upstream_model.trim().is_empty() {
        execution.model = alias.upstream_model;
    }
    // Both ordinary and refreshed retries enter here after credential
    // preparation. Rebind from this selected auth/model, never JSON metadata.
    let selected = super::api_key_model_capabilities::attach_resolved_api_key_model_info(
        &snapshot,
        crate::sdk::cliproxy::executor::Request {
            model: execution.model.clone(),
            ..crate::sdk::cliproxy::executor::Request::default()
        },
        &auth,
        &route_model,
        &execution.model,
    );
    execution.resolved_model_info =
        super::api_key_model_capabilities::resolved_model_info(&selected);
    execution
        .metadata
        .insert("selected_auth_id".into(), serde_json::json!(auth.id));
    execution
        .metadata
        .insert("selected_auth_index".into(), serde_json::json!(index));
    execution
}

fn is_request_scoped_error(mut error: &(dyn std::error::Error + 'static)) -> bool {
    loop {
        if error
            .downcast_ref::<AuthError>()
            .is_some_and(AuthError::is_request_scoped)
            || error.downcast_ref::<RequestTerminatedError>().is_some()
        {
            return true;
        }
        let Some(source) = error.source() else {
            return false;
        };
        error = source;
    }
}

fn error_status(mut error: &(dyn std::error::Error + 'static)) -> u16 {
    loop {
        if let Some(error) = error.downcast_ref::<AuthError>() {
            return error.status_code();
        }
        if let Some(error) = error.downcast_ref::<RequestTerminatedError>() {
            return error.status_code();
        }
        let Some(source) = error.source() else {
            return 0;
        };
        error = source;
    }
}

fn is_request_scoped_not_found_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("item with id")
        && message.contains("not found")
        && message.contains("items are not persisted when `store` is set to false")
}

fn is_count_tokens_endpoint_not_found(error: &PluginExecutionError, requested_model: &str) -> bool {
    plugin_error_status(error) == 404
        && !is_explicit_model_not_found(error.as_ref(), requested_model)
}

fn is_explicit_model_not_found(
    mut error: &(dyn std::error::Error + 'static),
    requested_model: &str,
) -> bool {
    loop {
        if let Some(auth) = error.downcast_ref::<AuthError>() {
            if is_model_not_found_identifier(&auth.code)
                || structured_model_not_found(&auth.message, requested_model)
            {
                return true;
            }
        } else if structured_model_not_found(&error.to_string(), requested_model) {
            return true;
        }
        let Some(source) = error.source() else {
            return false;
        };
        error = source;
    }
}

fn structured_model_not_found(message: &str, requested_model: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(message.trim()) else {
        return false;
    };
    structured_value_model_not_found(&value, requested_model.trim())
}

fn structured_value_model_not_found(value: &serde_json::Value, requested_model: &str) -> bool {
    match value {
        serde_json::Value::Object(values) => {
            let mut not_found_type = false;
            let mut exact_model = false;
            for (key, value) in values {
                if let Some(text) = value.as_str() {
                    match key.trim().to_ascii_lowercase().as_str() {
                        "code" if is_model_not_found_identifier(text) => return true,
                        "type" => {
                            if is_model_not_found_identifier(text) {
                                return true;
                            }
                            not_found_type |= matches!(
                                normalize_identifier(text).as_str(),
                                "not_found" | "not_found_error"
                            );
                        }
                        "error" | "message" | "detail" | "error_description" | "title" => {
                            let lower = text.to_ascii_lowercase();
                            let model = requested_model.to_ascii_lowercase();
                            exact_model |= !model.is_empty() && lower.contains(&model);
                            if exact_model
                                && (lower.contains("does not exist")
                                    || lower.contains("not found")
                                    || lower.contains("unknown model"))
                            {
                                return true;
                            }
                        }
                        _ => {}
                    }
                }
                if (value.is_object() || value.is_array())
                    && structured_value_model_not_found(value, requested_model)
                {
                    return true;
                }
            }
            not_found_type && exact_model
        }
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| structured_value_model_not_found(value, requested_model)),
        _ => false,
    }
}

fn is_model_not_found_identifier(value: &str) -> bool {
    matches!(
        normalize_identifier(value).as_str(),
        "model_not_found"
            | "model_not_found_error"
            | "unknown_model"
            | "model_does_not_exist"
            | "model_not_exist"
    )
}

fn normalize_identifier(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    let value = value
        .rsplit_once('#')
        .map_or(value.as_str(), |(_, fragment)| fragment);
    let value = value
        .split('?')
        .next()
        .unwrap_or(value)
        .trim_end_matches('/');
    value
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(value)
        .replace(['-', ' '], "_")
}

pub enum GenericExecutionError {
    ConductorUnavailable,
    ProviderNotSupplied,
    ProviderNotRegistered,
    ExecutionUnavailable,
    AuthUnavailable,
    CredentialLimit,
    Routing(AccountRoutingError),
    Preparation(AuthPreparationError),
    Refresh(AuthLifecycleRefreshError),
    Cooldown(CooldownStoreError),
    Manager(AuthManagerError),
    Provider(PluginExecutionError),
    EmptyStream,
}

impl fmt::Debug for GenericExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ConductorUnavailable => "GenericExecutionError::ConductorUnavailable",
            Self::ProviderNotSupplied => "GenericExecutionError::ProviderNotSupplied",
            Self::ProviderNotRegistered => "GenericExecutionError::ProviderNotRegistered",
            Self::ExecutionUnavailable => "GenericExecutionError::ExecutionUnavailable",
            Self::AuthUnavailable => "GenericExecutionError::AuthUnavailable",
            Self::CredentialLimit => "GenericExecutionError::CredentialLimit",
            Self::Routing(_) => "GenericExecutionError::Routing",
            Self::Preparation(_) => "GenericExecutionError::Preparation([REDACTED])",
            Self::Refresh(_) => "GenericExecutionError::Refresh([REDACTED])",
            Self::Cooldown(_) => "GenericExecutionError::Cooldown",
            Self::Manager(_) => "GenericExecutionError::Manager",
            Self::Provider(_) => "GenericExecutionError::Provider([REDACTED])",
            Self::EmptyStream => "GenericExecutionError::EmptyStream",
        })
    }
}

impl fmt::Display for GenericExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ConductorUnavailable => {
                "non-Home execution conductor is unavailable without cooldown persistence"
            }
            Self::ProviderNotSupplied => "no provider supplied",
            Self::ProviderNotRegistered => "provider executor is not registered",
            Self::ExecutionUnavailable => "provider execution capability is unavailable",
            Self::AuthUnavailable => "selected auth is unavailable",
            Self::CredentialLimit => "credential retry limit was exhausted",
            Self::Routing(_) => "auth selection failed",
            Self::Preparation(_) => "selected auth preparation failed",
            Self::Refresh(_) => "selected auth refresh failed",
            Self::Cooldown(_) => "credential outcome persistence failed",
            Self::Manager(_) => "auth manager publication failed",
            Self::Provider(_) => "provider execution failed",
            Self::EmptyStream => "upstream stream closed before its first payload",
        })
    }
}

impl std::error::Error for GenericExecutionError {}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::sdk::cliproxy::auth::{CooldownQuotaState, CooldownStateRecord};

    struct MemoryStore(Vec<CooldownStateRecord>);

    impl CooldownStateStore for MemoryStore {
        fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
            Ok(self.0.clone())
        }

        fn save(&self, _records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
            Ok(())
        }
    }

    fn candidate(auth_id: &str) -> AccountCandidate {
        AccountCandidate {
            auth_id: auth_id.to_owned(),
            provider: "claude".to_owned(),
            priority: 0,
            weight: 1,
            websocket_enabled: false,
            supported_models: Vec::new(),
            disabled: false,
        }
    }

    #[test]
    fn router_loads_persisted_state_before_every_pick() {
        let state = CooldownStateRecord {
            provider: "claude".to_owned(),
            auth_id: "account-a".to_owned(),
            model: Some("sonnet".to_owned()),
            status: "cooling".to_owned(),
            next_retry_after_ms: Some(2_000),
            reason: "quota".to_owned(),
            quota: CooldownQuotaState::default(),
            last_error: None,
            updated_at_ms: 1_000,
        };
        let router = AccountRouter::new(Arc::new(MemoryStore(vec![state])));
        assert_eq!(
            router
                .select(
                    "claude",
                    Some("sonnet"),
                    1_500,
                    &[candidate("account-a"), candidate("account-b")],
                )
                .unwrap()
                .auth_id,
            "account-b"
        );
    }

    #[test]
    fn store_failure_is_not_downgraded_to_empty_state() {
        struct FailedStore(Mutex<()>);
        impl CooldownStateStore for FailedStore {
            fn load(&self) -> Result<Vec<CooldownStateRecord>, CooldownStoreError> {
                let _guard = self.0.lock().unwrap();
                Err(CooldownStoreError::Read)
            }

            fn save(&self, _records: &[CooldownStateRecord]) -> Result<(), CooldownStoreError> {
                Err(CooldownStoreError::Write)
            }
        }

        let router = AccountRouter::new(Arc::new(FailedStore(Mutex::new(()))));
        assert_eq!(
            router.select("claude", None, 1_000, &[candidate("account-a")]),
            Err(AccountRoutingError::Store(CooldownStoreError::Read))
        );
    }
}
