// ref: internal/thinking/apply.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::SystemTime,
};

use serde_json::Value;

use crate::internal::{
    logging::global_logger::{LogEntry, LogLevel, LogOutputController},
    modelconfig,
    registry::{
        embedded_models_catalog, lookup_model_info, lookup_static_registry_model_info, ModelInfo,
        ModelRegistry,
    },
};

use super::{
    configuration_update::{
        extract_configuration_update_config, is_responses_format, strip_configuration_updates,
        strip_responses_effort, valid_document,
    },
    convert_budget_to_level, convert_level_to_budget, extract_summary_config,
    is_budget_capable_provider,
    model_view::is_user_defined_model_view as is_user_defined_model,
    parse_level_suffix, parse_numeric_suffix, parse_special_suffix, parse_suffix,
    strip_thinking_config,
    summary::strip_inferred_claude_summary_activation_view as strip_inferred_claude_summary_activation,
    validate::validate_config_view as validate_config,
    AntigravityApplier, ClaudeApplier, CodexApplier, GeminiApplier, InteractionsApplier,
    KimiApplier, ModelInfoView, OpenAiApplier, ProviderApplier, SummaryConfig, SummaryMode,
    ThinkingConfig, ThinkingError, ThinkingLevel, ThinkingMode, XaiApplier, LEVEL_AUTO, LEVEL_HIGH,
    LEVEL_MAX, LEVEL_NONE, LEVEL_XHIGH,
};

/// Instance-owned model capability lookup used by [`ThinkingEngine`].
///
/// Upstream obtains this information from a package-global registry. CTOX
/// injects the owning registry boundary instead, so independent gateway hosts
/// cannot observe or mutate each other's model selection state.
pub trait ModelInfoResolver: Send + Sync {
    fn lookup_model_info(&self, model: &str, provider: &str) -> Option<ModelInfo>;

    /// Complete instance-owned capability snapshot. Legacy resolvers retain
    /// their original static descriptor when this returns None; converting it
    /// into the signed owned type would narrow unsigned budget bounds.
    fn lookup_owned_model_info(
        &self,
        _model: &str,
        _provider: &str,
    ) -> Option<modelconfig::ModelInfo> {
        None
    }
}

/// Resolver backed by the embedded, immutable model definitions.
#[derive(Default)]
pub struct EmbeddedModelInfoResolver;

impl ModelInfoResolver for EmbeddedModelInfoResolver {
    fn lookup_model_info(&self, model: &str, provider: &str) -> Option<ModelInfo> {
        lookup_model_info(model, provider)
    }

    fn lookup_owned_model_info(
        &self,
        model: &str,
        _provider: &str,
    ) -> Option<modelconfig::ModelInfo> {
        let catalog = embedded_models_catalog().ok()?;
        lookup_static_registry_model_info(&catalog, model.trim()).map(modelconfig::ModelInfo::from)
    }
}

/// Uses the host's live registry, including provider-specific registrations
/// and its refreshable catalog, without introducing package-global ownership.
pub struct RegistryModelInfoResolver {
    registry: Arc<ModelRegistry>,
}

impl RegistryModelInfoResolver {
    pub fn new(registry: Arc<ModelRegistry>) -> Self {
        Self { registry }
    }
}

impl ModelInfoResolver for RegistryModelInfoResolver {
    fn lookup_model_info(&self, _model: &str, _provider: &str) -> Option<ModelInfo> {
        None
    }

    fn lookup_owned_model_info(
        &self,
        model: &str,
        provider: &str,
    ) -> Option<modelconfig::ModelInfo> {
        self.registry
            .lookup_model_info(model, provider)
            .map(modelconfig::ModelInfo::from)
    }
}

#[derive(Clone)]
struct PluginProviderApplier {
    owner: String,
    priority: i32,
    applier: Arc<dyn ProviderApplier>,
}

struct ProviderAppliers {
    native: BTreeMap<String, Arc<dyn ProviderApplier>>,
    plugins: BTreeMap<String, PluginProviderApplier>,
}

impl ProviderAppliers {
    fn builtins() -> Self {
        let mut native = BTreeMap::<String, Arc<dyn ProviderApplier>>::new();
        native.insert("gemini".into(), Arc::new(GeminiApplier::new()));
        native.insert("claude".into(), Arc::new(ClaudeApplier::new()));
        native.insert("openai".into(), Arc::new(OpenAiApplier::new()));
        native.insert("codex".into(), Arc::new(CodexApplier::new()));
        native.insert("antigravity".into(), Arc::new(AntigravityApplier::new()));
        native.insert("kimi".into(), Arc::new(KimiApplier::new()));
        native.insert("xai".into(), Arc::new(XaiApplier::new()));
        native.insert("interactions".into(), Arc::new(InteractionsApplier::new()));
        Self {
            native,
            plugins: BTreeMap::new(),
        }
    }
}

/// Owner-scoped equivalent of upstream's thinking package entry points.
pub struct ThinkingEngine {
    resolver: Arc<dyn ModelInfoResolver>,
    providers: RwLock<ProviderAppliers>,
    debug_log_output: Option<Arc<LogOutputController>>,
}

#[derive(Clone, Copy, Debug)]
pub struct ThinkingRequest<'a> {
    pub body: &'a [u8],
    pub model: &'a str,
    pub from_format: &'a str,
    pub to_format: &'a str,
    pub provider_key: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct ResolvedThinkingRequest<'a> {
    pub body: &'a [u8],
    pub source_body: &'a [u8],
    pub model: &'a str,
    pub from_format: &'a str,
    pub to_format: &'a str,
    pub provider_key: &'a str,
    pub model_info: Option<&'a ModelInfo>,
}

/// Explicitly distinguishes a selected descriptor from registry fallback.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedCapabilityThinkingRequest<'a> {
    pub body: &'a [u8],
    pub source_body: &'a [u8],
    pub model: &'a str,
    pub from_format: &'a str,
    pub to_format: &'a str,
    pub provider_key: &'a str,
    pub model_info: Option<&'a ModelInfoView<'a>>,
    pub model_info_resolved: bool,
    pub normalized_updates_changed: bool,
}

#[derive(Clone, Copy)]
struct ApplyRequest<'a> {
    body: &'a [u8],
    source_body: &'a [u8],
    model: &'a str,
    from_format: &'a str,
    to_format: &'a str,
    provider_key: &'a str,
    resolved_model_info: Option<&'a ModelInfoView<'a>>,
    model_info_resolved: bool,
    normalized_updates_changed: bool,
    summary: &'a SummaryConfig,
}

struct UserDefinedRequest<'a> {
    body: &'a [u8],
    model_info: Option<&'a ModelInfoView<'a>>,
    from_format: &'a str,
    to_format: &'a str,
    provider_key: &'a str,
    suffix: &'a super::SuffixResult,
    source_config: ThinkingConfig,
    native_responses: bool,
    summary: &'a SummaryConfig,
}

impl Default for ThinkingEngine {
    fn default() -> Self {
        Self::new(Arc::new(EmbeddedModelInfoResolver))
    }
}

impl ThinkingEngine {
    pub fn new(resolver: Arc<dyn ModelInfoResolver>) -> Self {
        Self {
            resolver,
            providers: RwLock::new(ProviderAppliers::builtins()),
            debug_log_output: None,
        }
    }

    /// Bind the owner's existing log output and typed logging level. No process
    /// logger or environment configuration is consulted. Rebinding on logging
    /// configuration reload remains the responsibility of the owning host.
    pub fn with_log_output(mut self, output: Arc<LogOutputController>, level: LogLevel) -> Self {
        self.debug_log_output =
            matches!(level, LogLevel::Trace | LogLevel::Debug).then_some(output);
        self
    }

    /// Returns a cloned handle to the registered provider applier.
    pub fn provider_applier(&self, provider: &str) -> Option<Arc<dyn ProviderApplier>> {
        let provider = normalized_provider_name(provider);
        if provider.is_empty() {
            return None;
        }
        let providers = read_unpoisoned(&self.providers);
        providers.native.get(&provider).cloned().or_else(|| {
            providers
                .plugins
                .get(&provider)
                .map(|record| Arc::clone(&record.applier))
        })
    }

    /// Registers or replaces a native provider. Native names are reserved from
    /// plugin ownership, matching upstream's precedence rule.
    pub fn register_provider(&self, name: &str, applier: Arc<dyn ProviderApplier>) {
        let name = normalized_provider_name(name);
        if name.is_empty() {
            return;
        }
        write_unpoisoned(&self.providers)
            .native
            .insert(name, applier);
    }

    /// Registers a plugin provider using upstream's deterministic
    /// priority/owner tie-break. Returns whether the candidate became active.
    pub fn register_plugin_provider(
        &self,
        owner: &str,
        name: &str,
        priority: i32,
        applier: Arc<dyn ProviderApplier>,
    ) -> bool {
        let owner = owner.trim();
        let name = normalized_provider_name(name);
        if owner.is_empty() || name.is_empty() {
            return false;
        }
        let mut providers = write_unpoisoned(&self.providers);
        if providers.native.contains_key(&name) {
            return false;
        }
        if providers.plugins.get(&name).is_some_and(|current| {
            current.priority > priority
                || (current.priority == priority && current.owner.as_str() <= owner)
        }) {
            return false;
        }
        providers.plugins.insert(
            name,
            PluginProviderApplier {
                owner: owner.to_owned(),
                priority,
                applier,
            },
        );
        true
    }

    pub fn unregister_plugin_providers(&self, owner: &str) {
        let owner = owner.trim();
        if owner.is_empty() {
            return;
        }
        write_unpoisoned(&self.providers)
            .plugins
            .retain(|_, record| record.owner != owner);
    }

    pub fn clear_plugin_providers(&self) {
        write_unpoisoned(&self.providers).plugins.clear();
    }

    pub fn apply_thinking(&self, request: ThinkingRequest<'_>) -> Result<Vec<u8>, ThinkingError> {
        let summary = extract_summary_config(request.body, request.to_format);
        self.apply(ApplyRequest {
            body: request.body,
            source_body: &[],
            model: request.model,
            from_format: request.from_format,
            to_format: request.to_format,
            provider_key: request.provider_key,
            resolved_model_info: None,
            model_info_resolved: false,
            normalized_updates_changed: false,
            summary: &summary,
        })
    }

    pub fn apply_thinking_with_summary(
        &self,
        request: ThinkingRequest<'_>,
        summary: &SummaryConfig,
    ) -> Result<Vec<u8>, ThinkingError> {
        self.apply(ApplyRequest {
            body: request.body,
            source_body: &[],
            model: request.model,
            from_format: request.from_format,
            to_format: request.to_format,
            provider_key: request.provider_key,
            resolved_model_info: None,
            model_info_resolved: false,
            normalized_updates_changed: false,
            summary,
        })
    }

    pub fn apply_thinking_with_model_info(
        &self,
        request: ResolvedThinkingRequest<'_>,
    ) -> Result<Vec<u8>, ThinkingError> {
        let summary = if request.source_body.is_empty() {
            extract_summary_config(request.body, request.to_format)
        } else {
            extract_summary_config(request.source_body, request.from_format)
        };
        self.apply_thinking_with_model_info_and_summary(request, &summary)
    }

    pub fn apply_thinking_with_model_info_and_summary(
        &self,
        request: ResolvedThinkingRequest<'_>,
        summary: &SummaryConfig,
    ) -> Result<Vec<u8>, ThinkingError> {
        let view = request.model_info.map(ModelInfoView::from);
        self.apply_thinking_with_capability_info_and_summary(
            ResolvedCapabilityThinkingRequest {
                body: request.body,
                source_body: request.source_body,
                model: request.model,
                from_format: request.from_format,
                to_format: request.to_format,
                provider_key: request.provider_key,
                model_info: view.as_ref(),
                model_info_resolved: true,
                normalized_updates_changed: false,
            },
            summary,
        )
    }

    pub fn apply_thinking_with_capability_info_and_summary(
        &self,
        request: ResolvedCapabilityThinkingRequest<'_>,
        summary: &SummaryConfig,
    ) -> Result<Vec<u8>, ThinkingError> {
        let selected_model_view = request.model_info.as_ref().map(|info| info.reborrow());
        self.apply(ApplyRequest {
            body: request.body,
            source_body: request.source_body,
            model: request.model,
            from_format: request.from_format,
            to_format: request.to_format,
            provider_key: request.provider_key,
            resolved_model_info: selected_model_view.as_ref(),
            model_info_resolved: request.model_info_resolved,
            normalized_updates_changed: request.normalized_updates_changed,
            summary,
        })
    }

    fn apply(&self, request: ApplyRequest<'_>) -> Result<Vec<u8>, ThinkingError> {
        let mut provider_format = normalized_provider_name(request.to_format);
        // ref: internal/thinking/apply.go:193-196 @ d7914afd
        if provider_format == "openai-response" {
            provider_format = "codex".into();
        }
        let mut provider_key = normalized_provider_name(request.provider_key);
        if provider_key.is_empty() {
            provider_key.clone_from(&provider_format);
        }
        let mut from_format = normalized_provider_name(request.from_format);
        if from_format.is_empty() {
            from_format.clone_from(&provider_format);
        }

        let suffix = parse_suffix(request.model);
        let looked_up_owned = if request.model_info_resolved {
            None
        } else {
            self.resolver
                .lookup_owned_model_info(&suffix.model_name, &provider_key)
        };
        let looked_up = if request.model_info_resolved || looked_up_owned.is_some() {
            None
        } else {
            self.resolver
                .lookup_model_info(&suffix.model_name, &provider_key)
        };
        let looked_up_view = looked_up_owned
            .as_ref()
            .map(ModelInfoView::from)
            .or_else(|| looked_up.as_ref().map(ModelInfoView::from));
        let selected_model_view = request
            .resolved_model_info
            .as_ref()
            .map(|info| info.reborrow());
        let model_info = if request.model_info_resolved {
            selected_model_view.as_ref()
        } else {
            looked_up_view.as_ref()
        };

        // ref: internal/thinking/apply.go::applyThinking @ d7914afd
        // Resolve current source intent before removing unsupported target updates.
        let source_config = if is_responses_format(&from_format)
            && (!request.normalized_updates_changed
                || matches!(provider_format.as_str(), "codex" | "xai"))
        {
            let source = if !request.normalized_updates_changed && !request.source_body.is_empty() {
                request.source_body
            } else {
                request.body
            };
            extract_codex_usage_config(source)
        } else {
            ThinkingConfig::default()
        };
        let response_target = matches!(provider_format.as_str(), "codex" | "xai");
        let supports_updates = model_info.is_some_and(|info| info.support_configuration_update);
        let body = if response_target && !supports_updates {
            strip_configuration_updates(request.body)
        } else {
            request.body.to_vec()
        };
        let native_responses =
            response_target && is_responses_format(&from_format) && supports_updates;
        let Some(applier) = self.provider_applier(&provider_format) else {
            return Ok(body);
        };
        if !suffix.has_suffix
            && !request.source_body.is_empty()
            && is_responses_format(&from_format)
            && valid_document(&body).is_none()
            && has_thinking_config(&extract_configuration_update_config(request.source_body))
        {
            // A separate source update cannot repair a malformed target request.
            return Ok(body);
        }
        if native_responses && !suffix.has_suffix {
            // Preserve the baseline and input bytes for native prompt-prefix caching.
            self.log_native_responses_config(&body, &provider_format, model_info);
            return Ok(body);
        }

        if is_user_defined_model(model_info) {
            return self.apply_user_defined_model(UserDefinedRequest {
                body: &body,
                model_info,
                from_format: &from_format,
                to_format: &provider_format,
                provider_key: &provider_key,
                suffix: &suffix,
                source_config,
                native_responses,
                summary: request.summary,
            });
        }
        let model_info = model_info.expect("registered model established above");
        if model_info.thinking.is_none() {
            let config = extract_thinking_config(&body, &provider_format);
            return if has_thinking_config(&config)
                || request.summary.mode != SummaryMode::Unspecified
            {
                Ok(if response_target {
                    strip_responses_effort(&body)
                } else {
                    strip_thinking_config(&body, &provider_format)
                })
            } else {
                Ok(body)
            };
        }

        let mut config = if suffix.has_suffix {
            parse_suffix_to_config(&suffix.raw_suffix)
        } else {
            let mut config = source_config;
            if !has_thinking_config(&config)
                && !request.normalized_updates_changed
                && request.model_info_resolved
                && !request.source_body.is_empty()
            {
                config = extract_source_thinking_config(request.source_body, &from_format);
            }
            if !has_thinking_config(&config) {
                config = extract_thinking_config(&body, &provider_format);
            }
            config
        };

        if !has_thinking_config(&config) {
            if native_responses {
                return Ok(body);
            }
            let mut output = body.clone();
            if request.model_info_resolved
                && provider_format == "claude"
                && from_format != provider_format
                && extract_summary_config(request.source_body, &from_format).mode
                    == SummaryMode::Enabled
            {
                output = strip_inferred_claude_summary_activation(&output, Some(model_info));
            }
            return Ok(super::summary::apply_summary_config_for_provider_view(
                &output,
                &provider_format,
                &suffix.model_name,
                &provider_key,
                Some(model_info),
                request.summary,
            ));
        }

        if request.model_info_resolved
            && config.mode == ThinkingMode::Level
            && should_map_configured_high_intent(&from_format, &provider_format, model_info)
        {
            config.level = map_configured_high_intent(config.level, model_info);
        }

        let validated = validate_config(
            config,
            Some(model_info),
            &from_format,
            &provider_format,
            suffix.has_suffix,
        )
        .map_err(|error| error.with_target_body(&body))?;
        let applied = applier
            .apply_model_info(&body, &validated, Some(model_info))
            .map_err(|error| error.with_target_body(&body))?;
        if thinking_is_fully_disabled(&validated) || native_responses {
            return Ok(applied);
        }
        Ok(super::summary::apply_summary_config_for_provider_view(
            &applied,
            &provider_format,
            &suffix.model_name,
            &provider_key,
            Some(model_info),
            request.summary,
        ))
    }

    // ref: internal/thinking/apply.go:267-287 @ d7914afd
    fn log_native_responses_config(
        &self,
        body: &[u8],
        provider: &str,
        model: Option<&ModelInfoView<'_>>,
    ) {
        let (Some(output), Some(model)) = (self.debug_log_output.as_ref(), model) else {
            return;
        };
        if model.thinking.is_none() && !model.user_defined {
            return;
        }
        let config = extract_codex_usage_config(body);
        if !has_thinking_config(&config) {
            return;
        }
        let mut entry = LogEntry::new(
            LogLevel::Debug,
            "thinking: original config from request |",
            SystemTime::now(),
        );
        entry.fields.extend([
            ("provider".into(), provider.to_owned()),
            ("model".into(), model.id.to_owned()),
            ("mode".into(), config.mode.to_string()),
            ("budget".into(), config.budget.to_string()),
            ("level".into(), config.level.to_string()),
        ]);
        let baseline = extract_codex_config(body);
        if baseline.mode == ThinkingMode::Level {
            entry
                .fields
                .insert("baseline_level".into(), baseline.level.to_string());
        }
        // Diagnostic sink failures must not change native inference or cache bytes.
        let _ = output.log(&entry);
        entry.message = "thinking: processed config to apply |".into();
        let _ = output.log(&entry);
    }

    fn apply_user_defined_model(
        &self,
        request: UserDefinedRequest<'_>,
    ) -> Result<Vec<u8>, ThinkingError> {
        let model_id = request
            .model_info
            .map(|info| info.id)
            .unwrap_or(&request.suffix.model_name);
        let mut config = if request.suffix.has_suffix {
            parse_suffix_to_config(&request.suffix.raw_suffix)
        } else {
            let mut config = request.source_config;
            if !has_thinking_config(&config) {
                config = extract_thinking_config(request.body, request.from_format);
            }
            if !has_thinking_config(&config) && request.from_format != request.to_format {
                config = extract_thinking_config(request.body, request.to_format);
            }
            config
        };
        if !has_thinking_config(&config) {
            return Ok(super::summary::apply_summary_config_for_provider_view(
                request.body,
                request.to_format,
                model_id,
                request.provider_key,
                request.model_info,
                request.summary,
            ));
        }
        let Some(applier) = self.provider_applier(request.to_format) else {
            return Ok(request.body.to_vec());
        };
        config = normalize_user_defined_config(config, request.from_format, request.to_format);
        let applied = applier
            .apply_model_info(request.body, &config, request.model_info)
            .map_err(|error| error.with_target_body(request.body))?;
        if thinking_is_fully_disabled(&config) || request.native_responses {
            return Ok(applied);
        }
        Ok(super::summary::apply_summary_config_for_provider_view(
            &applied,
            request.to_format,
            model_id,
            request.provider_key,
            request.model_info,
            request.summary,
        ))
    }
}

fn normalized_provider_name(provider: &str) -> String {
    provider.trim().to_ascii_lowercase()
}

fn thinking_is_fully_disabled(config: &ThinkingConfig) -> bool {
    config.mode == ThinkingMode::None && config.budget == 0 && config.level.is_empty()
}

fn should_map_configured_high_intent(
    from_format: &str,
    to_format: &str,
    model_info: &ModelInfoView<'_>,
) -> bool {
    if !from_format.trim().eq_ignore_ascii_case(to_format.trim()) {
        return true;
    }
    let model_type = model_info.provider_type.trim().to_ascii_lowercase();
    !model_type.is_empty() && !is_same_provider_family(to_format, &model_type)
}

fn map_configured_high_intent(
    level: ThinkingLevel,
    model_info: &ModelInfoView<'_>,
) -> ThinkingLevel {
    let Some(support) = model_info.thinking.as_ref() else {
        return level;
    };
    if support.levels.is_empty() {
        return level;
    }
    let level = level.as_str().trim().to_ascii_lowercase();
    let candidates: &[&str] = match level.as_str() {
        LEVEL_XHIGH => &[LEVEL_XHIGH, LEVEL_MAX, LEVEL_HIGH],
        LEVEL_MAX => &[LEVEL_MAX, LEVEL_XHIGH, LEVEL_HIGH],
        _ => return ThinkingLevel::new(level),
    };
    candidates
        .iter()
        .find(|candidate| {
            support
                .levels
                .iter()
                .any(|supported| candidate.eq_ignore_ascii_case(supported.trim()))
        })
        .map(|candidate| ThinkingLevel::new(*candidate))
        .unwrap_or_else(|| ThinkingLevel::new(level))
}

fn extract_source_thinking_config(body: &[u8], provider: &str) -> ThinkingConfig {
    if provider.trim().eq_ignore_ascii_case("openai-response") {
        extract_codex_config(body)
    } else {
        extract_thinking_config(body, provider)
    }
}

fn parse_suffix_to_config(raw_suffix: &str) -> ThinkingConfig {
    if let Some(mode) = parse_special_suffix(raw_suffix) {
        return ThinkingConfig {
            mode,
            budget: if mode == ThinkingMode::Auto { -1 } else { 0 },
            ..ThinkingConfig::default()
        };
    }
    if let Some(level) = parse_level_suffix(raw_suffix) {
        return ThinkingConfig {
            mode: ThinkingMode::Level,
            level,
            ..ThinkingConfig::default()
        };
    }
    if let Some(budget) = parse_numeric_suffix(raw_suffix) {
        return ThinkingConfig {
            mode: if budget == 0 {
                ThinkingMode::None
            } else {
                ThinkingMode::Budget
            },
            budget,
            ..ThinkingConfig::default()
        };
    }
    ThinkingConfig::default()
}

fn normalize_user_defined_config(
    mut config: ThinkingConfig,
    _from_format: &str,
    to_format: &str,
) -> ThinkingConfig {
    if config.mode != ThinkingMode::Level
        || to_format == "claude"
        || !is_budget_capable_provider(to_format)
    {
        return config;
    }
    let Some(budget) = convert_level_to_budget(config.level.as_str()) else {
        return config;
    };
    config.mode = ThinkingMode::Budget;
    config.budget = budget;
    config.level = ThinkingLevel::default();
    config
}

fn extract_thinking_config(body: &[u8], provider: &str) -> ThinkingConfig {
    let provider = normalized_provider_name(provider);
    if valid_document(body).is_none() {
        return ThinkingConfig::default();
    }
    match provider.as_str() {
        "codex" | "xai" => return extract_codex_config(body),
        "openai" => return extract_openai_config_raw(body),
        _ => {}
    }
    let Ok(document) = serde_json::from_slice::<Value>(body) else {
        return ThinkingConfig::default();
    };
    match provider.as_str() {
        "claude" => extract_claude_config(&document),
        "gemini" | "antigravity" => extract_gemini_config(&document, &provider),
        "interactions" => extract_interactions_config(&document),
        "openai" => extract_openai_config(&document),
        "kimi" => extract_kimi_config(&document),
        _ => ThinkingConfig::default(),
    }
}

fn has_thinking_config(config: &ThinkingConfig) -> bool {
    config.mode != ThinkingMode::Budget || config.budget != 0 || !config.level.is_empty()
}

/// Returns the effective source effort. For Responses, a nonempty in-turn
/// update takes precedence over the model suffix; the suffix still controls
/// the baseline sent by a suffix-specific applier.
pub fn extract_reasoning_effort(body: &[u8], provider: &str, model: &str) -> String {
    let provider = normalized_provider_name(provider);
    if is_responses_format(&provider) {
        let effort = reasoning_effort_from_config(&extract_configuration_update_config(body));
        if !effort.is_empty() {
            return effort;
        }
    }
    let suffix = parse_suffix(model);
    if suffix.has_suffix {
        let effort = reasoning_effort_from_config(&parse_suffix_to_config(&suffix.raw_suffix));
        if !effort.is_empty() {
            return effort;
        }
    }
    let mut config = extract_thinking_config_for_usage(body, &provider);
    if !has_thinking_config(&config) && matches!(provider.as_str(), "openai" | "openai-response") {
        config = extract_codex_usage_config(body);
    }
    reasoning_effort_from_config(&config)
}

/// Returns the final payload's last effective update, falling back to baseline.
pub fn extract_translated_reasoning_effort(body: &[u8], provider: &str) -> String {
    let provider = normalized_provider_name(provider);
    let mut config = extract_thinking_config_for_usage(body, &provider);
    if !has_thinking_config(&config) && matches!(provider.as_str(), "openai" | "openai-response") {
        config = extract_codex_usage_config(body);
        if !has_thinking_config(&config) {
            config = extract_openai_config_raw(body);
        }
    }
    reasoning_effort_from_config(&config)
}

fn extract_thinking_config_for_usage(body: &[u8], provider: &str) -> ThinkingConfig {
    if matches!(provider, "codex" | "xai" | "openai-response") {
        extract_codex_usage_config(body)
    } else {
        extract_thinking_config(body, provider)
    }
}

fn extract_codex_usage_config(body: &[u8]) -> ThinkingConfig {
    if valid_document(body).is_none() {
        return ThinkingConfig::default();
    }
    let config = extract_configuration_update_config(body);
    if has_thinking_config(&config) {
        config
    } else {
        extract_codex_config(body)
    }
}

fn reasoning_effort_from_config(config: &ThinkingConfig) -> String {
    if !has_thinking_config(config) {
        return String::new();
    }
    match config.mode {
        ThinkingMode::None => LEVEL_NONE.into(),
        ThinkingMode::Auto => LEVEL_AUTO.into(),
        ThinkingMode::Level => config.level.as_str().trim().to_ascii_lowercase(),
        ThinkingMode::Budget => convert_budget_to_level(config.budget)
            .map(|level| level.to_string())
            .unwrap_or_default(),
        ThinkingMode::Unknown(_) => String::new(),
    }
}

fn extract_claude_config(document: &Value) -> ThinkingConfig {
    let thinking_type = string_path(document, "thinking.type");
    if thinking_type == "disabled" {
        return none_config();
    }
    if matches!(thinking_type.as_str(), "adaptive" | "auto") {
        let effort = string_path(document, "output_config.effort");
        return normalized_level_value_config(&effort, true);
    }
    if let Some(budget) = integer_path(document, "thinking.budget_tokens") {
        return budget_value_config(budget);
    }
    if thinking_type == "enabled" {
        return auto_config();
    }
    ThinkingConfig::default()
}

fn extract_gemini_config(document: &Value, provider: &str) -> ThinkingConfig {
    let prefix = if provider.trim().eq_ignore_ascii_case("antigravity") {
        "request.generationConfig.thinkingConfig"
    } else {
        "generationConfig.thinkingConfig"
    };
    for field in ["thinkingLevel", "thinking_level"] {
        if let Some(value) = path(document, &format!("{prefix}.{field}")) {
            return raw_level_value_config(&gjson_string(value), true);
        }
    }
    for field in ["thinkingBudget", "thinking_budget"] {
        if let Some(value) = path(document, &format!("{prefix}.{field}")).and_then(json_isize) {
            return budget_value_config(value);
        }
    }
    ThinkingConfig::default()
}

fn extract_interactions_config(document: &Value) -> ThinkingConfig {
    for candidate in [
        "generation_config.thinking_level",
        "generation_config.thinkingLevel",
        "generation_config.thinking_config.thinking_level",
        "generation_config.thinking_config.thinkingLevel",
        "generation_config.thinkingConfig.thinking_level",
        "generation_config.thinkingConfig.thinkingLevel",
    ] {
        if let Some(value) = path(document, candidate) {
            return normalized_level_value_config(&gjson_string(value), true);
        }
    }
    for candidate in [
        "generation_config.thinking_budget",
        "generation_config.thinkingBudget",
        "generation_config.thinking_config.thinking_budget",
        "generation_config.thinking_config.thinkingBudget",
        "generation_config.thinkingConfig.thinking_budget",
        "generation_config.thinkingConfig.thinkingBudget",
    ] {
        if let Some(value) = path(document, candidate).and_then(json_isize) {
            return budget_value_config(value);
        }
    }
    ThinkingConfig::default()
}

fn extract_openai_config(document: &Value) -> ThinkingConfig {
    path(document, "reasoning_effort")
        .map(|value| raw_level_value_config(&gjson_string(value), false))
        .unwrap_or_default()
}

fn extract_kimi_config(document: &Value) -> ThinkingConfig {
    if let Some(thinking_type) = path(document, "thinking.type") {
        let thinking_type = thinking_type
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if thinking_type == "disabled" {
            return none_config();
        }
        if thinking_type == "enabled" && path(document, "thinking.effort").is_none() {
            return ThinkingConfig::default();
        }
        if let Some(effort) = path(document, "thinking.effort") {
            return normalized_level_value_config(&gjson_string(effort), true);
        }
        return ThinkingConfig::default();
    }
    if let Some(effort) = path(document, "thinking.effort") {
        return normalized_level_value_config(&gjson_string(effort), true);
    }
    extract_openai_config(document)
}

fn extract_codex_config(body: &[u8]) -> ThinkingConfig {
    extract_raw_effort(body, "reasoning.effort")
}

fn extract_openai_config_raw(body: &[u8]) -> ThinkingConfig {
    extract_raw_effort(body, "reasoning_effort")
}

fn extract_raw_effort(body: &[u8], path: &str) -> ThinkingConfig {
    let value = crate::internal::util::get_gjson_bytes_no_copy(body, path);
    if !value.exists() {
        return ThinkingConfig::default();
    }
    if value.str() == LEVEL_NONE {
        none_config()
    } else {
        ThinkingConfig {
            mode: ThinkingMode::Level,
            level: ThinkingLevel::new(value.str()),
            ..Default::default()
        }
    }
}

fn normalized_level_value_config(value: &str, accepts_auto: bool) -> ThinkingConfig {
    let value = value.trim().to_ascii_lowercase();
    raw_level_value_config(&value, accepts_auto)
}

fn raw_level_value_config(value: &str, accepts_auto: bool) -> ThinkingConfig {
    if value.is_empty() {
        ThinkingConfig::default()
    } else if value == LEVEL_NONE {
        none_config()
    } else if accepts_auto && value == LEVEL_AUTO {
        auto_config()
    } else {
        ThinkingConfig {
            mode: ThinkingMode::Level,
            level: ThinkingLevel::new(value),
            ..ThinkingConfig::default()
        }
    }
}

fn budget_value_config(value: isize) -> ThinkingConfig {
    match value {
        0 => none_config(),
        -1 => auto_config(),
        budget => ThinkingConfig {
            mode: ThinkingMode::Budget,
            budget,
            ..ThinkingConfig::default()
        },
    }
}

fn none_config() -> ThinkingConfig {
    ThinkingConfig {
        mode: ThinkingMode::None,
        ..ThinkingConfig::default()
    }
}

fn auto_config() -> ThinkingConfig {
    ThinkingConfig {
        mode: ThinkingMode::Auto,
        budget: -1,
        ..ThinkingConfig::default()
    }
}

fn string_path(document: &Value, candidate: &str) -> String {
    path(document, candidate)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn integer_path(document: &Value, candidate: &str) -> Option<isize> {
    path(document, candidate).and_then(json_isize)
}

fn json_isize(value: &Value) -> Option<isize> {
    value
        .as_i64()
        .or_else(|| value.as_str()?.parse::<i64>().ok())
        .and_then(|number| isize::try_from(number).ok())
}

fn gjson_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn path<'a>(document: &'a Value, candidate: &str) -> Option<&'a Value> {
    candidate
        .split('.')
        .try_fold(document, |current, segment| current.get(segment))
}

fn is_gemini_family(provider: &str) -> bool {
    matches!(provider, "gemini" | "antigravity")
}

fn is_openai_family(provider: &str) -> bool {
    matches!(provider, "openai" | "openai-response" | "codex")
}

fn is_same_provider_family(from: &str, to: &str) -> bool {
    from == to
        || (is_gemini_family(from) && is_gemini_family(to))
        || (is_openai_family(from) && is_openai_family(to))
}

fn read_unpoisoned<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_unpoisoned<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
