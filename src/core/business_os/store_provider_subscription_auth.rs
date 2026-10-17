pub fn save_runtime_settings_command(
    root: &Path,
    session: &BusinessOsSession,
    request: RuntimeSettingsRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_has_workspace_permission(root, session, BusinessOsPermission::RuntimeManage)?,
        "chef or admin role required"
    );
    save_runtime_settings(root, request)?;
    Ok(serde_json::json!({
        "ok": true,
        "status": "saved"
    }))
}

pub fn start_subscription_auth_command(
    root: &Path,
    session: &BusinessOsSession,
    request: SubscriptionAuthStartCommandRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_has_workspace_permission(root, session, BusinessOsPermission::IntegrationsManage)?,
        "chef or admin role required"
    );
    let account_request = ProviderSubscriptionCommandRequest {
        provider: Some(
            request
                .provider
                .clone()
                .unwrap_or_else(|| "codex".to_owned()),
        ),
        account_id: request.account_id.clone(),
    };
    let provider = account_request.normalized_provider()?;
    request.validate_public_selectors()?;
    let account_id = request
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if account_id.is_some() {
        let _ = account_request.required_account_id()?;
    }
    provider_subscription_auth_start_payload(root, &provider, request.use_device_code(), account_id)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionAuthStartCommandRequest {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub auth_mode: Option<String>,
    #[serde(default)]
    pub flow: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
}

impl SubscriptionAuthStartCommandRequest {
    fn validate_public_selectors(&self) -> anyhow::Result<()> {
        if let Some(auth_mode) = self.auth_mode.as_deref() {
            anyhow::ensure!(
                matches!(
                    auth_mode.trim().to_ascii_lowercase().as_str(),
                    "subscription" | "chatgpt_subscription" | "codex_subscription"
                ),
                "unsupported provider auth_mode"
            );
        }
        if let Some(flow) = self.flow.as_deref() {
            anyhow::ensure!(
                matches!(
                    flow.trim().to_ascii_lowercase().as_str(),
                    "device_code" | "browser_callback" | "auth_url"
                ),
                "unsupported provider auth flow"
            );
        }
        Ok(())
    }

    fn use_device_code(&self) -> bool {
        let provider = self
            .provider
            .as_deref()
            .unwrap_or("openai")
            .trim()
            .to_ascii_lowercase();
        let auth_mode = self
            .auth_mode
            .as_deref()
            .unwrap_or("chatgpt_subscription")
            .trim()
            .to_ascii_lowercase();
        let flow = self
            .flow
            .as_deref()
            .unwrap_or("device_code")
            .trim()
            .to_ascii_lowercase();
        matches!(provider.as_str(), "openai" | "codex")
            && matches!(auth_mode.as_str(), "chatgpt_subscription" | "subscription")
            && flow == "device_code"
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSubscriptionCommandRequest {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
}

impl ProviderSubscriptionCommandRequest {
    fn normalized_provider(&self) -> anyhow::Result<String> {
        let provider = self
            .provider
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let provider = match provider.as_str() {
            "openai" => "codex",
            "anthropic" => "claude",
            "anti-gravity" | "google" => "antigravity",
            "kimi-code" | "kimi_coding_plan" => "kimi_coding",
            other => other,
        };
        anyhow::ensure!(
            matches!(
                provider,
                "codex" | "claude" | "antigravity" | "kimi" | "kimi_coding" | "minimax"
            ),
            "unsupported provider subscription"
        );
        Ok(provider.to_owned())
    }

    fn required_account_id(&self) -> anyhow::Result<&str> {
        let account_id = self.account_id.as_deref().unwrap_or_default().trim();
        anyhow::ensure!(!account_id.is_empty(), "provider account_id is required");
        let mut characters = account_id.chars();
        anyhow::ensure!(
            account_id.len() <= 160
                && characters
                    .next()
                    .is_some_and(|character| character.is_ascii_alphanumeric())
                && characters.all(|character| {
                    character.is_ascii_alphanumeric() || "-_.".contains(character)
                }),
            "provider account_id is invalid"
        );
        Ok(account_id)
    }
}

/// Removes metadata that the Business OS command bus owns and injects into
/// every command payload. Provider selectors remain strict: arbitrary fields
/// (especially credential material) are deliberately not discarded here and
/// continue to fail `deny_unknown_fields` deserialization.
fn provider_subscription_selector_payload(payload: &Value) -> Value {
    let mut selectors = payload.clone();
    if let Some(object) = selectors.as_object_mut() {
        for transport_field in ["inbound_channel", "dependencies", "command_deadline_at_ms"] {
            object.remove(transport_field);
        }
    }
    selectors
}

fn provider_subscription_account_is_projected(
    root: &Path,
    provider: &str,
    account_id: &str,
) -> bool {
    provider_subscription_status_projection(root)
        .get("accounts")
        .and_then(Value::as_array)
        .is_some_and(|accounts| {
            accounts.iter().any(|account| {
                account.get("provider").and_then(Value::as_str) == Some(provider)
                    && account.get("id").and_then(Value::as_str) == Some(account_id)
            })
        })
}

fn provider_subscription_status_projection(root: &Path) -> Value {
    use crate::execution::models::kimi_coding::KimiCodingAccountPhase;
    use crate::execution::models::minimax_coding::MiniMaxCodingAccountPhase;

    let mut projection = crate::execution::cliproxyapi_host::provider_subscription_status(root);
    let Some(object) = projection.as_object_mut() else {
        return projection;
    };
    let providers = object
        .entry("providers")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(providers) = providers.as_array_mut() {
        for provider in providers.iter_mut() {
            let Some(provider_object) = provider.as_object_mut() else {
                continue;
            };
            let provider_id = provider_object
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let default_account_id = match provider_id {
                "codex" => CODEX_INSTANCE_SUBSCRIPTION_ACCOUNT_ID,
                "claude" => "claude-primary",
                "antigravity" => "antigravity-primary",
                "kimi" => "kimi-primary",
                _ => continue,
            };
            provider_object
                .entry("access_mode")
                .or_insert_with(|| Value::String("Subscription".to_owned()));
            provider_object
                .entry("default_account_id")
                .or_insert_with(|| Value::String(default_account_id.to_owned()));
            provider_object
                .entry("available")
                .or_insert(Value::Bool(true));
        }
        if !providers
            .iter()
            .any(|provider| provider.get("id").and_then(Value::as_str) == Some("minimax"))
        {
            providers.push(serde_json::json!({
                "id": "minimax",
                "label": "MiniMax",
                "flow": "credential",
                "access_mode": "Coding Plan",
                "default_account_id": "minimax-coding-primary",
                "available": true,
            }));
        }
        if !providers
            .iter()
            .any(|provider| provider.get("id").and_then(Value::as_str) == Some("kimi_coding"))
        {
            providers.push(serde_json::json!({
                "id": "kimi_coding",
                "label": "Kimi Coding Plan",
                "flow": "credential",
                "access_mode": "Coding Plan",
                "default_account_id": "kimi-coding-primary",
                "available": true,
            }));
        }
    }
    let accounts = object
        .entry("accounts")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(accounts) = accounts.as_array_mut() {
        for status in
            crate::execution::models::kimi_coding::account_statuses(root).unwrap_or_default()
        {
            let (phase, enabled) = match status.phase {
                KimiCodingAccountPhase::Disabled => ("disabled", false),
                KimiCodingAccountPhase::MissingCredential => ("missing_credential", false),
                KimiCodingAccountPhase::Ready => ("ready", true),
            };
            let preset_ids = status
                .models
                .iter()
                .map(|model| format!("kimi-coding-{}-{model}", status.id))
                .collect::<Vec<_>>();
            accounts.push(serde_json::json!({
                "id": status.id,
                "provider": "kimi_coding",
                "enabled": enabled,
                "status": phase,
                "models": status.models,
                "preset_ids": preset_ids,
            }));
        }
        for status in
            crate::execution::models::minimax_coding::account_statuses(root).unwrap_or_default()
        {
            let (phase, enabled) = match status.phase {
                MiniMaxCodingAccountPhase::Disabled => ("disabled", false),
                MiniMaxCodingAccountPhase::MissingCredential => ("missing_credential", false),
                MiniMaxCodingAccountPhase::Ready => ("ready", true),
            };
            let preset_ids = status
                .models
                .iter()
                .map(|model| format!("minimax-coding-{}-{model}", status.id))
                .collect::<Vec<_>>();
            accounts.push(serde_json::json!({
                "id": status.id,
                "provider": "minimax",
                "enabled": enabled,
                "status": phase,
                "models": status.models,
                "preset_ids": preset_ids,
            }));
        }
    }
    projection
}

pub fn provider_subscription_status_for_control_plane(root: &Path) -> Value {
    serde_json::json!({
        "ok": true,
        "provider_subscriptions": provider_subscription_status_projection(root),
    })
}

const KIMI_CODING_ACCOUNT_ID: &str = "kimi-coding-primary";
const MINIMAX_CODING_ACCOUNT_ID: &str = "minimax-coding-primary";

fn start_kimi_coding_plan(root: &Path, account_id: &str) -> anyhow::Result<Value> {
    use crate::execution::models::kimi_coding::{
        upsert_account, KimiCodingAccount, KimiCodingEndpointProfile, KimiSecretRef,
    };

    let account_id = account_id.trim();
    anyhow::ensure!(
        account_id == KIMI_CODING_ACCOUNT_ID,
        "Kimi Coding Plan currently supports only the canonical Business OS account"
    );
    upsert_account(
        root,
        KimiCodingAccount {
            id: account_id.to_owned(),
            disabled: false,
            models: Vec::new(),
            api_key_secret: KimiSecretRef {
                scope: crate::secrets::credential_scope().to_owned(),
                name: "KIMI_API_KEY".to_owned(),
            },
            endpoint_profile: KimiCodingEndpointProfile::KimiCoding,
        },
    )?;
    let ready = crate::execution::models::kimi_coding::ready_accounts(root)?
        .iter()
        .any(|account| account.id == account_id);
    Ok(serde_json::json!({
        "ok": true,
        "provider": "kimi_coding",
        "account_id": account_id,
        "status": if ready { "connected" } else { "credential_required" },
        "credential_name": "KIMI_API_KEY",
        "message": if ready {
            "Kimi Coding Plan Account ist verbunden."
        } else {
            "KIMI_API_KEY muss im verschlüsselten CTOX Credential Store hinterlegt werden."
        },
        "provider_subscriptions": provider_subscription_status_projection(root),
    }))
}

fn start_minimax_coding_plan(root: &Path, account_id: &str) -> anyhow::Result<Value> {
    use crate::execution::models::minimax_coding::{
        upsert_account, MiniMaxCodingAccount, MiniMaxCodingEndpointProfile, MiniMaxSecretRef,
    };

    let account_id = account_id.trim();
    anyhow::ensure!(
        account_id == MINIMAX_CODING_ACCOUNT_ID,
        "MiniMax Coding Plan currently supports only the canonical Business OS account"
    );
    upsert_account(
        root,
        MiniMaxCodingAccount {
            id: account_id.to_owned(),
            disabled: false,
            models: Vec::new(),
            api_key_secret: MiniMaxSecretRef {
                scope: crate::secrets::credential_scope().to_owned(),
                name: "MINIMAX_API_KEY".to_owned(),
            },
            endpoint_profile: MiniMaxCodingEndpointProfile::GlobalAnthropic,
        },
    )?;
    let ready = crate::execution::models::minimax_coding::ready_accounts(root)?
        .iter()
        .any(|account| account.id == account_id);
    Ok(serde_json::json!({
        "ok": true,
        "provider": "minimax",
        "account_id": account_id,
        "status": if ready { "connected" } else { "credential_required" },
        "credential_name": "MINIMAX_API_KEY",
        "message": if ready {
            "MiniMax Coding Plan Account ist verbunden."
        } else {
            "MINIMAX_API_KEY muss im verschlüsselten CTOX Credential Store hinterlegt werden."
        },
        "provider_subscriptions": provider_subscription_status_projection(root),
    }))
}

fn coding_plan_rotation_payload(
    root: &Path,
    provider: &str,
    account_id: &str,
) -> anyhow::Result<Value> {
    let (expected_account_id, credential_name, label) = match provider {
        "kimi_coding" => (KIMI_CODING_ACCOUNT_ID, "KIMI_API_KEY", "Kimi Coding Plan"),
        "minimax" => (
            MINIMAX_CODING_ACCOUNT_ID,
            "MINIMAX_API_KEY",
            "MiniMax Coding Plan",
        ),
        _ => anyhow::bail!("provider is not a direct coding plan"),
    };
    anyhow::ensure!(
        account_id == expected_account_id,
        "coding-plan account is not the canonical Business OS account"
    );
    Ok(serde_json::json!({
        "ok": true,
        "provider": provider,
        "account_id": account_id,
        "status": "credential_required",
        "credential_name": credential_name,
        "message": format!(
            "{credential_name} muss im verschlüsselten CTOX Credential Store ersetzt werden; der Browser nimmt keine Provider-Secrets entgegen."
        ),
        "label": label,
        "provider_subscriptions": provider_subscription_status_projection(root),
    }))
}

fn disconnect_instance_chatgpt_subscription_at(
    root: &Path,
    codex_home: &Path,
    account_id: &str,
) -> anyhow::Result<Value> {
    let _lifecycle = crate::secrets::credential_lifecycle_guard();
    anyhow::ensure!(
        account_id == CODEX_INSTANCE_SUBSCRIPTION_ACCOUNT_ID,
        "ChatGPT subscription account is unavailable"
    );

    let configured =
        crate::secrets::secret_exists(root, CHATGPT_AUTH_SECRET_SCOPE, CHATGPT_AUTH_SECRET_NAME)?;
    if configured {
        crate::secrets::delete_secret_record(
            root,
            CHATGPT_AUTH_SECRET_SCOPE,
            CHATGPT_AUTH_SECRET_NAME,
        )?;
    }

    let auth_manager = ctox_core::AuthManager::new(
        codex_home.to_path_buf(),
        false,
        ctox_core::auth::AuthCredentialsStoreMode::File,
    );
    let auth_file_removed = if auth_manager
        .auth_cached()
        .as_ref()
        .is_some_and(|auth| auth.is_chatgpt_auth())
    {
        auth_manager.logout()?
    } else {
        false
    };

    Ok(serde_json::json!({
        "ok": true,
        "provider": "codex",
        "account_id": account_id,
        "disconnected": configured || auth_file_removed,
        "deleted_secret_records": if configured { 1 } else { 0 },
        "provider_subscriptions": provider_subscription_status_projection(root),
    }))
}

fn disconnect_instance_chatgpt_subscription(
    root: &Path,
    account_id: &str,
) -> anyhow::Result<Value> {
    let codex_home = ctox_core::config::find_codex_home()
        .context("cannot resolve Codex home for ChatGPT subscription disconnect")?;
    disconnect_instance_chatgpt_subscription_at(root, &codex_home, account_id)
}

pub fn run_channel_command(
    root: &Path,
    session: &BusinessOsSession,
    command_type: &str,
    request: ChannelCommandRequest,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        session_has_workspace_permission(root, session, BusinessOsPermission::IntegrationsManage)?,
        "chef or admin role required"
    );
    match command_type {
        "ctox.channel.test"
            if request.channel.trim() == "email"
                && request.config.as_object().is_some_and(|c| !c.is_empty()) =>
        {
            Ok(channels::test_email_draft_for_business_os(
                root,
                &request.config,
            ))
        }
        "ctox.channel.test" => {
            let account_key = request.account_key.trim();
            let account_key = if account_key.is_empty() {
                None
            } else {
                Some(account_key)
            };
            channels::test_channel_for_business_os(root, request.channel.trim(), account_key)
        }
        "ctox.channel.sync" => channels::sync_channel_for_business_os(root, request.channel.trim()),
        "ctox.channel.settings.save" => channels::save_channel_settings_for_business_os(
            root,
            request.channel.trim(),
            &request.config,
        ),
        "ctox.channel.disconnect" => channels::disconnect_communication_account_for_business_os(
            root,
            request.account_key.trim(),
        ),
        "ctox.channel.pair.start" => {
            channels::start_pairing_for_business_os(root, request.channel.trim())
        }
        "ctox.channel.jami.export" => Ok(channels::export_jami_archive_for_business_os(root)),
        "ctox.channel.jami.create" => {
            let display_name = request.display_name.trim().to_string();
            let display_name = if display_name.is_empty() {
                "CTOX".to_string()
            } else {
                display_name
            };
            let config = serde_json::json!({ "profile_name": display_name });
            channels::save_channel_settings_for_business_os(root, "jami", &config)?;
            channels::start_pairing_for_business_os(root, "jami")
        }
        _ => anyhow::bail!("unsupported channel command type: {command_type}"),
    }
}

fn save_runtime_settings(root: &Path, request: RuntimeSettingsRequest) -> anyhow::Result<()> {
    let requested_provider = request.provider.trim().to_ascii_lowercase();
    let auth_mode = request.auth_mode.trim().to_ascii_lowercase();
    let subscription_provider = runtime_subscription_provider(&requested_provider)
        .filter(|_| runtime_settings_auth_mode_is_subscription(&auth_mode));
    let provider = if subscription_provider.is_some() {
        "ctox_subscription"
    } else {
        crate::inference::runtime_state::normalize_api_provider(&requested_provider)
    };
    let mut env_map = crate::inference::runtime_env::effective_operator_env_map(root)
        .unwrap_or_else(|_| BTreeMap::new());
    let chat_model = request.chat_model.trim();
    let reasoning_effort = request.reasoning_effort.trim().to_ascii_lowercase();
    let preset = request.preset.trim();
    let requested_context = request.context.trim();
    let context = runtime_settings_context(
        (!requested_context.is_empty()).then(|| requested_context.to_owned()),
    );
    if provider.eq_ignore_ascii_case("local") {
        env_map.insert("CTOX_CHAT_SOURCE".to_owned(), "local".to_owned());
        env_map.remove("CTOX_API_PROVIDER");
        env_map.remove("CTOX_UPSTREAM_BASE_URL");
        env_map.remove("OPENAI_AUTH_MODE");
        env_map.remove("CTOX_OPENAI_AUTH_MODE");
        env_map.remove(crate::inference::runtime_state::CTOX_SUBSCRIPTION_PROVIDER_ENV);
    } else {
        env_map.insert("CTOX_CHAT_SOURCE".to_owned(), "api".to_owned());
        env_map.insert("CTOX_API_PROVIDER".to_owned(), provider.to_owned());
        env_map.insert(
            "CTOX_UPSTREAM_BASE_URL".to_owned(),
            runtime_settings_api_upstream_base_url(provider, &env_map),
        );
        if let Some(subscription_provider) = subscription_provider {
            env_map.insert(
                crate::inference::runtime_state::CTOX_SUBSCRIPTION_PROVIDER_ENV.to_owned(),
                subscription_provider.to_owned(),
            );
        } else {
            env_map.remove(crate::inference::runtime_state::CTOX_SUBSCRIPTION_PROVIDER_ENV);
        }
    }
    if !chat_model.is_empty() {
        env_map.insert("CTOX_CHAT_MODEL".to_owned(), chat_model.to_owned());
        env_map.insert("CTOX_CHAT_MODEL_BASE".to_owned(), chat_model.to_owned());
    } else {
        env_map.remove("CTOX_CHAT_MODEL");
        env_map.remove("CTOX_CHAT_MODEL_BASE");
    }
    if matches!(
        reasoning_effort.as_str(),
        "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
    ) {
        env_map.insert("CTOX_CHAT_REASONING_EFFORT".to_owned(), reasoning_effort);
    } else {
        env_map.remove("CTOX_CHAT_REASONING_EFFORT");
    }
    if let Some(preset) = normalize_runtime_preset(preset) {
        env_map.insert("CTOX_CHAT_LOCAL_PRESET".to_owned(), preset.to_owned());
    }
    if !requested_context.is_empty() {
        env_map.insert("CTOX_CHAT_MODEL_MAX_CONTEXT".to_owned(), context.to_owned());
    }
    if let Some(max_run_secs) = request.max_run_secs.filter(|value| *value > 0) {
        env_map.insert(
            "CTOX_CHAT_TURN_TIMEOUT_SECS".to_owned(),
            max_run_secs.to_string(),
        );
    }
    if subscription_provider.is_some()
        || (provider.eq_ignore_ascii_case("openai")
            && runtime_settings_auth_mode_is_subscription(&auth_mode))
    {
        env_map.insert("OPENAI_AUTH_MODE".to_owned(), "subscription".to_owned());
        env_map.insert(
            "CTOX_OPENAI_AUTH_MODE".to_owned(),
            "subscription".to_owned(),
        );
    } else {
        env_map.insert("OPENAI_AUTH_MODE".to_owned(), "api_key".to_owned());
        env_map.insert("CTOX_OPENAI_AUTH_MODE".to_owned(), "api_key".to_owned());
    }
    let api_key = request.api_key.trim();
    if subscription_provider.is_none() && !api_key.is_empty() {
        let key_name = crate::inference::runtime_state::api_key_env_var_for_provider(provider);
        env_map.insert(key_name.to_owned(), api_key.to_owned());
    }
    crate::inference::runtime_env::save_runtime_env_map(root, &env_map)
}

fn runtime_subscription_provider(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai" | "codex" => Some("codex"),
        "anthropic" | "claude" => Some("claude"),
        "antigravity" | "google" => Some("antigravity"),
        "kimi" => Some("kimi"),
        _ => None,
    }
}

fn runtime_provider_for_subscription(provider: &str) -> &'static str {
    match provider.trim().to_ascii_lowercase().as_str() {
        "codex" => "openai",
        "claude" => "anthropic",
        "antigravity" => "antigravity",
        "kimi" => "kimi",
        _ => "openai",
    }
}

fn runtime_settings_preset(
    runtime_state: Option<&crate::inference::runtime_state::InferenceRuntimeState>,
    env_map: &BTreeMap<String, String>,
) -> String {
    runtime_state
        .and_then(|state| state.local_preset.as_deref())
        .or_else(|| env_map.get("CTOX_CHAT_LOCAL_PRESET").map(String::as_str))
        .and_then(normalize_runtime_preset)
        .unwrap_or("Quality")
        .to_owned()
}

fn normalize_runtime_preset(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "quality" => Some("Quality"),
        "performance" => Some("Performance"),
        _ => None,
    }
}

fn runtime_settings_api_upstream_base_url(
    provider: &str,
    env_map: &BTreeMap<String, String>,
) -> String {
    let provider = crate::inference::runtime_state::normalize_api_provider(provider);
    if provider.eq_ignore_ascii_case("ctox_subscription") {
        return crate::execution::cliproxyapi_host::instance_codex_proxy_base_url();
    }
    if provider.eq_ignore_ascii_case("ctox_proxy") {
        return env_map
            .get(crate::inference::runtime_state::CTOX_LLM_PROXY_BASE_URL_ENV)
            .or_else(|| env_map.get("CTOX_UPSTREAM_BASE_URL"))
            .filter(|value| crate::inference::runtime_state::is_ctox_llm_proxy_base_url(value))
            .cloned()
            .unwrap_or_else(|| "https://llm.ctox.dev".to_owned());
    }
    env_map
        .get("CTOX_UPSTREAM_BASE_URL")
        .filter(|value| !value.trim().is_empty())
        .filter(|value| {
            crate::inference::runtime_state::api_provider_for_upstream_base_url(value)
                .eq_ignore_ascii_case(provider)
        })
        .cloned()
        .unwrap_or_else(|| {
            crate::inference::runtime_state::default_api_upstream_base_url_for_provider(provider)
                .to_owned()
        })
}

fn runtime_settings_context(value: Option<String>) -> String {
    let Some(value) = value else {
        return "256k".to_owned();
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "131072" | "128000" | "128k" | "262144" | "256000" | "256k" | "" => "256k".to_owned(),
        _ => "256k".to_owned(),
    }
}

fn runtime_settings_auth_mode_is_subscription(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "chatgpt_subscription" | "subscription" | "codex_subscription" | "chatgpt"
    )
}

fn chatgpt_subscription_auth_status(root: &Path) -> ChatgptSubscriptionAuthStatus {
    let Ok(codex_home) = ctox_core::config::find_codex_home() else {
        return ChatgptSubscriptionAuthStatus::default();
    };
    let _ = restore_chatgpt_subscription_auth_from_instance(root, &codex_home);
    let auth_manager = ctox_core::AuthManager::new(
        codex_home.clone(),
        false,
        ctox_core::auth::AuthCredentialsStoreMode::default(),
    );
    let Some(auth) = auth_manager.auth_cached() else {
        return ChatgptSubscriptionAuthStatus::default();
    };
    if !auth.is_chatgpt_auth() {
        return ChatgptSubscriptionAuthStatus::default();
    }
    ChatgptSubscriptionAuthStatus {
        configured: true,
        account_email: auth.get_account_email(),
        plan: auth.account_plan_type().map(|plan| format!("{plan:?}")),
    }
}

fn runtime_auth_message(
    provider: &str,
    key_name: &str,
    key_configured: bool,
    subscription_selected: bool,
    subscription_auth: &ChatgptSubscriptionAuthStatus,
) -> String {
    if provider.eq_ignore_ascii_case("local") {
        return "Lokale CTOX Runtime ausgewählt; keine API-Autorisierung nötig.".to_owned();
    }
    if subscription_selected {
        return if subscription_auth.configured {
            match (
                subscription_auth.account_email.as_deref(),
                subscription_auth.plan.as_deref(),
            ) {
                (Some(email), Some(plan)) => {
                    format!("ChatGPT Subscription autorisiert: {email} ({plan}).")
                }
                (Some(email), None) => format!("ChatGPT Subscription autorisiert: {email}."),
                _ => format!(
                    "{} Subscription ist verbunden.",
                    runtime_provider_for_subscription(
                        runtime_subscription_provider(provider).unwrap_or(provider)
                    )
                ),
            }
        } else {
            format!(
                "{} Subscription ist ausgewählt, aber noch nicht verbunden.",
                provider
            )
        };
    }
    if key_configured {
        format!("{key_name} ist im CTOX Secret Store vorhanden.")
    } else {
        format!("{key_name} fehlt im CTOX Secret Store.")
    }
}

fn subscription_auth_start_payload(root: &Path, use_device_code: bool) -> anyhow::Result<Value> {
    let login = start_chatgpt_subscription_login(root, use_device_code)?;
    Ok(serde_json::json!({
        "ok": true,
        "status": if login.device_user_code.is_some() { "device_code" } else { "auth_url" },
        "login_id": login.login_id,
        "auth_url": login.auth_url,
        "redirect_uri": login.redirect_uri,
        "verification_url": login.verification_url,
        "user_code": login.device_user_code,
        "message": "ChatGPT Subscription Autorisierung gestartet."
    }))
}

fn provider_subscription_auth_start_payload(
    root: &Path,
    provider: &str,
    use_device_code: bool,
    account_id: Option<&str>,
) -> anyhow::Result<Value> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai" | "codex" => subscription_auth_start_payload(root, use_device_code),
        "claude" | "anthropic" => start_claude_subscription_login(
            root,
            account_id.context("Claude subscription account_id is required")?,
        ),
        "antigravity" | "anti-gravity" | "google" => start_antigravity_subscription_login(
            root,
            account_id.context("Antigravity subscription account_id is required")?,
        ),
        "kimi" => start_kimi_subscription_login(
            root,
            account_id.context("Kimi subscription account_id is required")?,
        ),
        "kimi_coding" => start_kimi_coding_plan(
            root,
            account_id.context("Kimi coding-plan account_id is required")?,
        ),
        "minimax" => start_minimax_coding_plan(
            root,
            account_id.context("MiniMax coding-plan account_id is required")?,
        ),
        other => anyhow::bail!("unsupported subscription provider: {other}"),
    }
}

fn start_kimi_subscription_login(root: &Path, account_id: &str) -> anyhow::Result<Value> {
    use ctox_cliproxyapi::internal::auth::kimi::KimiDeviceIdentity;
    use ctox_cliproxyapi::sdk::auth::LoginCancellation;

    let account_id = account_id.trim().to_owned();
    let device_id = format!(
        "ctox-{}",
        channels::stable_digest(root.to_string_lossy().as_ref())
    );
    let identity = KimiDeviceIdentity::new(
        device_id,
        "CTOX Business OS",
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        env!("CARGO_PKG_VERSION"),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let auth = crate::execution::cliproxyapi_integration::build_instance_kimi_auth(identity);
    let cancellation = LoginCancellation::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let device_code = runtime
        .block_on(auth.start_device_flow(&cancellation))
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let verification_url = if device_code.verification_uri_complete.trim().is_empty() {
        device_code.verification_uri.clone()
    } else {
        device_code.verification_uri_complete.clone()
    };
    anyhow::ensure!(
        !device_code.user_code.trim().is_empty() && !verification_url.trim().is_empty(),
        "Kimi device authorization did not return a public code and verification URL"
    );

    let root = root.to_path_buf();
    let login_id = Uuid::new_v4().to_string();
    let worker_id = login_id.clone();
    let worker_device_code = device_code.clone();
    let worker_account_id = account_id.clone();
    thread::spawn(move || {
        let result = (|| -> anyhow::Result<()> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let bundle = runtime
                .block_on(auth.wait_for_authorization(&cancellation, &worker_device_code))
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let storage = auth.create_token_storage(&bundle);
            let callback_root = root.clone();
            run_subscription_persistence(&root, move || {
                crate::execution::cliproxyapi_integration::install_kimi_subscription(
                    &callback_root,
                    &worker_account_id,
                    &storage,
                )
                .map(|_| ())
            })?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("CTOX Kimi subscription login {worker_id} failed: {error}");
        }
    });

    Ok(serde_json::json!({
        "ok": true,
        "status": "device_code",
        "provider": "kimi",
        "account_id": account_id,
        "login_id": login_id,
        "auth_url": verification_url.clone(),
        "verification_url": verification_url,
        "user_code": device_code.user_code,
        "expires_in": device_code.expires_in,
        "message": "Kimi Subscription Device-Flow gestartet."
    }))
}

fn provider_callback_values(
    request: Request,
    callback_path: &str,
    expected_state: &str,
) -> anyhow::Result<Option<String>> {
    let url_raw = request.url().to_owned();
    let parsed = Url::parse(&format!("http://localhost{url_raw}"))?;
    if parsed.path() != callback_path {
        respond_html(request, 404, "Not Found")?;
        return Ok(None);
    }
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    if params.get("state").map(String::as_str) != Some(expected_state) {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: state mismatch.",
        )?;
        anyhow::bail!("OAuth state mismatch");
    }
    if let Some(error) = params.get("error") {
        respond_html(request, 400, "CTOX Login wurde vom Provider abgelehnt.")?;
        anyhow::bail!("provider rejected OAuth login: {error}");
    }
    let code = params
        .get("code")
        .map(String::as_str)
        .unwrap_or_default()
        .trim();
    if code.is_empty() {
        respond_html(
            request,
            400,
            "CTOX Login lieferte keinen Autorisierungscode.",
        )?;
        anyhow::bail!("OAuth callback contains no authorization code");
    }
    respond_html(
        request,
        200,
        "CTOX Subscription wurde autorisiert. Dieses Fenster kann geschlossen werden.",
    )?;
    Ok(Some(code.to_owned()))
}

fn start_claude_subscription_login(root: &Path, account_id: &str) -> anyhow::Result<Value> {
    use ctox_cliproxyapi::internal::auth::claude::{
        generate_pkce_codes, AnthropicHttpTransport, ClaudeAuth, SecretString,
    };
    let server = Server::http("127.0.0.1:54545")
        .map_err(|_| anyhow::anyhow!("Claude OAuth callback port 54545 is unavailable"))?;
    let pkce = generate_pkce_codes().map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let state_raw = chatgpt_login_state();
    let state =
        SecretString::new(state_raw.clone()).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let transport =
        AnthropicHttpTransport::new(None).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let auth = ClaudeAuth::new(transport);
    let (auth_url, _) = auth
        .generate_auth_url(&state, &pkce)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let root = root.to_path_buf();
    let account_id = account_id.to_owned();
    let worker_account_id = account_id.clone();
    let login_id = Uuid::new_v4().to_string();
    let worker_id = login_id.clone();
    thread::spawn(move || {
        let result = (|| -> anyhow::Result<()> {
            let request = server
                .recv_timeout(Duration::from_secs(300))?
                .context("Claude OAuth callback timed out")?;
            let Some(code) = provider_callback_values(request, "/callback", &state_raw)? else {
                anyhow::bail!("unexpected Claude callback path");
            };
            let code =
                SecretString::new(code).map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let bundle = runtime
                .block_on(auth.exchange_code_for_tokens(&code, &state, &pkce))
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let tokens = bundle.token_data();
            let credentials =
                ctox_cliproxyapi::internal::auth::claude::ClaudeStoredCredentials::new(
                    tokens.access_token().clone(),
                    tokens.refresh_token().clone(),
                );
            let callback_root = root.clone();
            run_subscription_persistence(&root, move || {
                crate::execution::cliproxyapi_host::install_claude_subscription(
                    &callback_root,
                    &worker_account_id,
                    &credentials,
                )
                .map(|_| ())
            })?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("CTOX Claude subscription login {worker_id} failed: {error}");
        }
        server.unblock();
    });
    Ok(serde_json::json!({
        "ok": true, "status": "auth_url", "provider": "claude", "account_id": account_id, "login_id": login_id,
        "auth_url": auth_url, "redirect_uri": "http://localhost:54545/callback",
        "message": "Claude Subscription Autorisierung gestartet."
    }))
}

fn start_antigravity_subscription_login(root: &Path, account_id: &str) -> anyhow::Result<Value> {
    use ctox_cliproxyapi::internal::auth::antigravity::{
        build_auth_url, AntigravityAuth, AntigravityHttpTransport, SecretString, CALLBACK_PORT,
    };
    use ctox_cliproxyapi::sdk::auth::LoginCancellation;
    let server = Server::http(format!("127.0.0.1:{CALLBACK_PORT}")).map_err(|_| {
        anyhow::anyhow!("Antigravity OAuth callback port {CALLBACK_PORT} is unavailable")
    })?;
    let state = chatgpt_login_state();
    let redirect_uri = format!("http://localhost:{CALLBACK_PORT}/oauth-callback");
    let auth_url = build_auth_url(&state, Some(&redirect_uri));
    let root = root.to_path_buf();
    let account_id = account_id.to_owned();
    let worker_account_id = account_id.clone();
    let login_id = Uuid::new_v4().to_string();
    let worker_id = login_id.clone();
    let worker_redirect = redirect_uri.clone();
    thread::spawn(move || {
        let result = (|| -> anyhow::Result<()> {
            let request = server
                .recv_timeout(Duration::from_secs(300))?
                .context("Antigravity OAuth callback timed out")?;
            let Some(code) = provider_callback_values(request, "/oauth-callback", &state)? else {
                anyhow::bail!("unexpected Antigravity callback path");
            };
            let transport = AntigravityHttpTransport::new(None)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let auth = AntigravityAuth::new(Arc::new(transport));
            let cancellation = LoginCancellation::default();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let (token, _email, project_id) = runtime
                .block_on(async {
                    let token = auth
                        .exchange_code_for_tokens(&cancellation, &code, &worker_redirect)
                        .await?;
                    let email = auth
                        .fetch_user_info(&cancellation, token.access_token())
                        .await?;
                    let project_id = auth
                        .fetch_project_id(&cancellation, token.access_token())
                        .await?;
                    Ok::<_, ctox_cliproxyapi::internal::auth::antigravity::AntigravityAuthError>((
                        token, email, project_id,
                    ))
                })
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let refresh = token
                .refresh_token()
                .cloned()
                .context("Antigravity did not return a refresh token")?;
            let expiry = SystemTime::now()
                .checked_add(Duration::from_secs(token.expires_in.max(0) as u64))
                .context("Antigravity token expiry overflow")?;
            let credentials =
                ctox_cliproxyapi::internal::auth::antigravity::AntigravityStoredCredentials::new(
                    SecretString::new(token.access_token().expose_secret().to_owned())?,
                    refresh,
                    expiry,
                    project_id,
                )?;
            let callback_root = root.clone();
            run_subscription_persistence(&root, move || {
                crate::execution::cliproxyapi_host::install_antigravity_subscription(
                    &callback_root,
                    &worker_account_id,
                    &credentials,
                )
                .map(|_| ())
            })?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("CTOX Antigravity subscription login {worker_id} failed: {error}");
        }
        server.unblock();
    });
    Ok(serde_json::json!({
        "ok": true, "status": "auth_url", "provider": "antigravity", "account_id": account_id, "login_id": login_id,
        "auth_url": auth_url, "redirect_uri": redirect_uri,
        "message": "Antigravity Subscription Autorisierung gestartet."
    }))
}

struct StartedChatgptSubscriptionLogin {
    login_id: String,
    auth_url: String,
    redirect_uri: String,
    device_user_code: Option<String>,
    verification_url: Option<String>,
}

#[derive(Clone)]
struct ChatgptLoginPkce {
    verifier: String,
    challenge: String,
}

fn start_chatgpt_subscription_login(
    root: &Path,
    use_device_code: bool,
) -> anyhow::Result<StartedChatgptSubscriptionLogin> {
    let codex_home = ctox_core::config::find_codex_home()
        .context("Codex/CTOX Auth-Store konnte nicht aufgelöst werden")?;
    let pkce = chatgpt_login_pkce();
    let state = chatgpt_login_state();
    let login_id = Uuid::new_v4().to_string();
    if use_device_code {
        let device = request_chatgpt_device_code()?;
        let verification_url = format!("{CHATGPT_AUTH_ISSUER}/codex/device");
        let redirect_uri = format!("{CHATGPT_AUTH_ISSUER}/deviceauth/callback");
        let auth_url = verification_url.clone();
        let device_auth_id = device.device_auth_id.clone();
        let device_user_code = device.user_code.clone();
        let device_interval_secs = device.interval_secs;
        let worker_login_id = login_id.clone();
        let worker_redirect_uri = redirect_uri.clone();
        let worker_root = root.to_path_buf();
        thread::spawn(move || {
            if let Err(err) = complete_chatgpt_device_code_login(
                &worker_root,
                &codex_home,
                device_auth_id,
                device_user_code,
                device_interval_secs,
                worker_redirect_uri,
            ) {
                eprintln!("CTOX ChatGPT subscription device login {worker_login_id} failed: {err}");
            }
        });
        return Ok(StartedChatgptSubscriptionLogin {
            login_id,
            auth_url,
            redirect_uri,
            device_user_code: Some(device.user_code),
            verification_url: Some(verification_url),
        });
    }
    let (server, port) = bind_chatgpt_login_server()
        .context("Lokaler ChatGPT-Login-Callback konnte nicht gestartet werden")?;
    let redirect_uri = format!("http://localhost:{port}/auth/callback");
    let auth_url = build_chatgpt_authorize_url(&redirect_uri, &pkce.challenge, &state);
    let worker_login_id = login_id.clone();
    let worker_redirect_uri = redirect_uri.clone();
    let root = root.to_path_buf();
    thread::spawn(move || {
        if let Err(err) = run_chatgpt_login_callback_server(
            server,
            root,
            codex_home,
            worker_redirect_uri,
            pkce,
            state,
        ) {
            eprintln!("CTOX ChatGPT subscription login {worker_login_id} failed: {err}");
        }
    });
    Ok(StartedChatgptSubscriptionLogin {
        login_id,
        auth_url,
        redirect_uri,
        device_user_code: None,
        verification_url: None,
    })
}

fn chatgpt_login_pkce() -> ChatgptLoginPkce {
    let verifier = format!(
        "{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    ChatgptLoginPkce {
        verifier,
        challenge,
    }
}

fn chatgpt_login_state() -> String {
    let seed = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let digest = Sha256::digest(seed.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn bind_chatgpt_login_server() -> anyhow::Result<(Server, u16)> {
    for port in [
        CHATGPT_AUTH_CALLBACK_PORT,
        CHATGPT_AUTH_CALLBACK_FALLBACK_PORT,
    ] {
        match Server::http(format!("127.0.0.1:{port}")) {
            Ok(server) => return Ok((server, port)),
            Err(_) => continue,
        }
    }
    anyhow::bail!(
        "Ports {CHATGPT_AUTH_CALLBACK_PORT} und {CHATGPT_AUTH_CALLBACK_FALLBACK_PORT} sind belegt"
    )
}

fn build_chatgpt_authorize_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    let query = [
        ("response_type", "code"),
        ("client_id", ctox_core::auth::CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("scope", CHATGPT_AUTH_SCOPE),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", "ctox_business_os"),
    ];
    let qs = query
        .into_iter()
        .map(|(key, value)| format!("{key}={}", urlencoding_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{CHATGPT_AUTH_ISSUER}/oauth/authorize?{qs}")
}

fn run_chatgpt_login_callback_server(
    server: Server,
    root: PathBuf,
    codex_home: PathBuf,
    redirect_uri: String,
    pkce: ChatgptLoginPkce,
    state: String,
) -> anyhow::Result<()> {
    for request in server.incoming_requests() {
        let url_raw = request.url().to_owned();
        let handled = handle_chatgpt_login_callback_request(
            request,
            &url_raw,
            &root,
            &codex_home,
            &redirect_uri,
            &pkce,
            &state,
        )?;
        if handled {
            break;
        }
    }
    server.unblock();
    Ok(())
}

fn handle_chatgpt_login_callback_request(
    request: Request,
    url_raw: &str,
    root: &Path,
    codex_home: &Path,
    redirect_uri: &str,
    pkce: &ChatgptLoginPkce,
    expected_state: &str,
) -> anyhow::Result<bool> {
    let parsed = Url::parse(&format!("http://localhost{url_raw}"))?;
    if parsed.path() != "/auth/callback" {
        respond_html(request, 404, "Not Found")?;
        return Ok(false);
    }
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    if params.get("state").map(String::as_str) != Some(expected_state) {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: state mismatch.",
        )?;
        return Ok(true);
    }
    if let Some(error) = params.get("error") {
        let description = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or(error);
        respond_html(
            request,
            400,
            &format!("CTOX Login wurde von ChatGPT abgelehnt: {description}"),
        )?;
        return Ok(true);
    }
    let Some(code) = params.get("code").filter(|value| !value.trim().is_empty()) else {
        respond_html(
            request,
            400,
            "CTOX Login konnte nicht abgeschlossen werden: code fehlt.",
        )?;
        return Ok(true);
    };
    match exchange_chatgpt_authorization_code(code, redirect_uri, &pkce.verifier)
        .and_then(|tokens| persist_chatgpt_subscription_auth(root, codex_home, tokens))
    {
        Ok(()) => {
            respond_html(
                request,
                200,
                "CTOX ChatGPT Subscription ist autorisiert. Dieses Fenster kann geschlossen werden.",
            )?;
            Ok(true)
        }
        Err(err) => {
            respond_html(
                request,
                500,
                &format!("CTOX konnte die ChatGPT Subscription nicht speichern: {err}"),
            )?;
            Ok(true)
        }
    }
}

fn respond_html(request: Request, status: u16, body: &str) -> anyhow::Result<()> {
    let response = Response::from_string(format!(
        "<!doctype html><meta charset=\"utf-8\"><title>CTOX Login</title><body style=\"font:16px system-ui;padding:32px;background:#10181b;color:#eef5f3\"><h1>CTOX Login</h1><p>{}</p></body>",
        html_escape(body)
    ))
    .with_status_code(status)
    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
    request.respond(response).map_err(io::Error::other)?;
    Ok(())
}

struct ChatgptDeviceCode {
    device_auth_id: String,
    user_code: String,
    interval_secs: u64,
}

#[derive(Debug, Deserialize)]
struct ChatgptDeviceTokenResponse {
    authorization_code: String,
    code_verifier: String,
}

fn request_chatgpt_device_code() -> anyhow::Result<ChatgptDeviceCode> {
    let response = ureq::post(&format!(
        "{CHATGPT_AUTH_ISSUER}/api/accounts/deviceauth/usercode"
    ))
    .set("Content-Type", "application/json")
    .send_json(serde_json::json!({
        "client_id": ctox_core::auth::CLIENT_ID,
    }));
    let body: Value = match response {
        Ok(response) => response.into_json().map_err(anyhow::Error::from)?,
        Err(ureq::Error::Status(status, response)) => {
            let body = response.into_string().unwrap_or_default();
            anyhow::bail!("Device-Code-Anforderung fehlgeschlagen ({status}): {body}")
        }
        Err(err) => return Err(anyhow::Error::from(err)),
    };
    let device_auth_id = body
        .get("device_auth_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .context("Device-Code-Antwort enthält keine device_auth_id")?;
    let user_code = body
        .get("user_code")
        .or_else(|| body.get("usercode"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .context("Device-Code-Antwort enthält keinen user_code")?;
    let interval_secs = body
        .get("interval")
        .and_then(|value| match value {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.trim().parse::<u64>().ok(),
            _ => None,
        })
        .unwrap_or(5)
        .max(1);
    Ok(ChatgptDeviceCode {
        device_auth_id,
        user_code,
        interval_secs,
    })
}

fn complete_chatgpt_device_code_login(
    root: &Path,
    codex_home: &Path,
    device_auth_id: String,
    user_code: String,
    interval_secs: u64,
    redirect_uri: String,
) -> anyhow::Result<()> {
    let token = poll_chatgpt_device_token(device_auth_id, user_code, interval_secs)?;
    let tokens = exchange_chatgpt_authorization_code(
        &token.authorization_code,
        &redirect_uri,
        &token.code_verifier,
    )?;
    persist_chatgpt_subscription_auth(root, codex_home, tokens)
}

fn poll_chatgpt_device_token(
    device_auth_id: String,
    user_code: String,
    interval_secs: u64,
) -> anyhow::Result<ChatgptDeviceTokenResponse> {
    let started = Instant::now();
    let max_wait = Duration::from_secs(15 * 60);
    let sleep_for = Duration::from_secs(interval_secs).min(Duration::from_secs(15));
    loop {
        let response = ureq::post(&format!(
            "{CHATGPT_AUTH_ISSUER}/api/accounts/deviceauth/token"
        ))
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({
            "device_auth_id": &device_auth_id,
            "user_code": &user_code,
        }));
        match response {
            Ok(response) => return response.into_json().map_err(anyhow::Error::from),
            Err(ureq::Error::Status(status, response)) if status == 403 || status == 404 => {
                if started.elapsed() >= max_wait {
                    anyhow::bail!("Device-Code-Login ist nach 15 Minuten abgelaufen");
                }
                let _ = response.into_string();
                thread::sleep(sleep_for);
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                anyhow::bail!("Device-Code-Token-Abfrage fehlgeschlagen ({status}): {body}")
            }
            Err(err) => return Err(anyhow::Error::from(err)),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatgptTokenExchangeResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

fn exchange_chatgpt_authorization_code(
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> anyhow::Result<ChatgptTokenExchangeResponse> {
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        urlencoding_encode(code),
        urlencoding_encode(redirect_uri),
        urlencoding_encode(ctox_core::auth::CLIENT_ID),
        urlencoding_encode(code_verifier)
    );
    let response = ureq::post(&format!("{CHATGPT_AUTH_ISSUER}/oauth/token"))
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body);
    match response {
        Ok(response) => response.into_json().map_err(anyhow::Error::from),
        Err(ureq::Error::Status(status, response)) => {
            let body = response.into_string().unwrap_or_default();
            anyhow::bail!("OAuth Token-Exchange fehlgeschlagen ({status}): {body}")
        }
        Err(err) => Err(anyhow::Error::from(err)),
    }
}

/// Provider callback threads retain no admission while waiting for the network.
/// Only their final local persistence phase borrows the existing Core runtime.
pub(crate) fn run_subscription_persistence(
    root: &Path,
    operation: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
) -> anyhow::Result<()> {
    crate::persistence::run_core_blocking_at(root, operation)
}

fn persist_chatgpt_subscription_auth(
    root: &Path,
    codex_home: &Path,
    tokens: ChatgptTokenExchangeResponse,
) -> anyhow::Result<()> {
    let work = root.to_owned();
    let codex_home = codex_home.to_owned();
    run_subscription_persistence(root, move || {
        persist_admitted_chatgpt_subscription_auth(&work, &codex_home, tokens)
    })
}

fn persist_admitted_chatgpt_subscription_auth(
    root: &Path,
    codex_home: &Path,
    tokens: ChatgptTokenExchangeResponse,
) -> anyhow::Result<()> {
    let token_data = ctox_core::token_data::TokenData {
        id_token: ctox_core::token_data::parse_chatgpt_jwt_claims(&tokens.id_token)
            .map_err(anyhow::Error::msg)?,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        account_id: chatgpt_account_id_from_jwt(&tokens.id_token),
    };
    let auth = ctox_core::auth::AuthDotJson {
        auth_mode: Some(ApiAuthMode::Chatgpt),
        openai_api_key: None,
        tokens: Some(token_data),
        last_refresh: Some(chrono::Utc::now()),
    };
    ctox_core::auth::save_auth(
        codex_home,
        &auth,
        ctox_core::auth::AuthCredentialsStoreMode::File,
    )?;
    crate::secrets::write_secret_record(
        root,
        CHATGPT_AUTH_SECRET_SCOPE,
        CHATGPT_AUTH_SECRET_NAME,
        &serde_json::to_string(&auth)?,
        Some("ChatGPT Subscription OAuth state for this CTOX instance".to_owned()),
        serde_json::json!({"source": "business_os_subscription_login", "auth_mode": "chatgpt_subscription"}),
    )?;
    Ok(())
}

fn restore_chatgpt_subscription_auth_from_instance(
    root: &Path,
    codex_home: &Path,
) -> anyhow::Result<bool> {
    let auth_manager = ctox_core::AuthManager::new(
        codex_home.to_path_buf(),
        false,
        ctox_core::auth::AuthCredentialsStoreMode::default(),
    );
    if auth_manager
        .auth_cached()
        .as_ref()
        .is_some_and(|auth| auth.is_chatgpt_auth())
    {
        return Ok(false);
    }
    let serialized = crate::secrets::read_secret_value(
        root,
        CHATGPT_AUTH_SECRET_SCOPE,
        CHATGPT_AUTH_SECRET_NAME,
    )
    .context("no instance ChatGPT auth backup")?;
    let auth: ctox_core::auth::AuthDotJson =
        serde_json::from_str(&serialized).context("instance ChatGPT auth backup is invalid")?;
    if auth.tokens.is_none() {
        anyhow::bail!("instance ChatGPT auth backup has no tokens");
    }
    ctox_core::auth::save_auth(
        codex_home,
        &auth,
        ctox_core::auth::AuthCredentialsStoreMode::File,
    )?;
    Ok(true)
}

fn chatgpt_account_id_from_jwt(jwt: &str) -> Option<String> {
    let mut parts = jwt.split('.');
    let (_header, payload, _signature) = (parts.next()?, parts.next()?, parts.next()?);
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let value = serde_json::from_slice::<Value>(&bytes).ok()?;
    value
        .get("https://api.openai.com/auth")
        .and_then(Value::as_object)
        .and_then(|claims| claims.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod runtime_subscription_settings_tests {
    use super::*;

    #[test]
    fn subscription_selection_persists_cli_proxy_route_for_main_harness() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        save_runtime_settings(
            temp.path(),
            RuntimeSettingsRequest {
                provider: "anthropic".to_owned(),
                auth_mode: "subscription".to_owned(),
                chat_model: "claude-opus-4-6".to_owned(),
                reasoning_effort: "high".to_owned(),
                preset: "Quality".to_owned(),
                context: "256k".to_owned(),
                max_run_secs: Some(1800),
                api_key: "must-not-be-stored".to_owned(),
            },
        )?;

        let settings = crate::inference::runtime_env::effective_runtime_env_map(temp.path())?;
        assert_eq!(
            settings.get("CTOX_API_PROVIDER").map(String::as_str),
            Some("ctox_subscription")
        );
        assert_eq!(
            settings
                .get(crate::inference::runtime_state::CTOX_SUBSCRIPTION_PROVIDER_ENV)
                .map(String::as_str),
            Some("claude")
        );
        assert_eq!(
            settings.get("CTOX_UPSTREAM_BASE_URL").map(String::as_str),
            Some("http://127.0.0.1:12435/v1")
        );
        assert!(!settings.contains_key("ANTHROPIC_API_KEY"));
        assert_eq!(
            settings
                .get("CTOX_CHAT_REASONING_EFFORT")
                .map(String::as_str),
            Some("high")
        );
        Ok(())
    }
}

fn urlencoding_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn module_has_active_responsibility(conn: &Connection, module_id: &str) -> anyhow::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM business_module_acl acl
         LEFT JOIN business_users user ON user.user_id = acl.user_id
         WHERE acl.module_id = ?1
           AND acl.role = 'founder'
           AND acl.active = 1
           AND COALESCE(user.active, 1) = 1",
        params![module_id.trim()],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn orphan_private_module_ids(
    root: &Path,
    conn: &Connection,
    module_filter: &str,
) -> anyhow::Result<Vec<String>> {
    let module_ids = current_business_os_module_ids(root)?;
    let mut orphan_ids = Vec::new();
    for module_id in module_ids {
        if !module_filter.is_empty() && module_id != module_filter {
            continue;
        }
        if module_requires_active_responsibility(root, &module_id)?
            && !module_has_active_responsibility(conn, &module_id)?
        {
            orphan_ids.push(module_id);
        }
    }
    Ok(orphan_ids)
}

fn repair_orphan_private_app_responsibility(
    root: &Path,
    conn: &Connection,
    session: &BusinessOsSession,
    module_filter: &str,
    dry_run: bool,
    now: i64,
) -> anyhow::Result<Vec<Value>> {
    let recovery_user_id = session_user_id(session)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("authenticated recovery user is required")?
        .to_owned();
    let orphan_ids = orphan_private_module_ids(root, conn, module_filter)?;
    if !dry_run && !orphan_ids.is_empty() {
        seed_session_user(conn, session)?;
    }
    let mut actions = Vec::new();
    for module_id in orphan_ids {
        actions.push(serde_json::json!({
            "kind": "orphan_private_app_responsibility",
            "module_id": module_id.as_str(),
            "recovery_user_id": recovery_user_id.as_str(),
            "action": "assign_recovery_responsibility",
            "apply": !dry_run,
        }));
        if !dry_run {
            upsert_module_founder_assignment_record(
                conn,
                session,
                &module_id,
                &recovery_user_id,
                true,
                now,
            )?;
        }
    }
    Ok(actions)
}

fn current_business_os_module_ids(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let app_root = resolve_business_os_app_root(root)?;
    let installed_app_root = resolve_business_os_installed_app_root(root);
    let modules = load_module_manifests(root, &app_root, &installed_app_root)?;
    Ok(modules.into_iter().map(|manifest| manifest.id).collect())
}

fn repair_stale_module_permission_grants(
    conn: &Connection,
    module_ids: &BTreeSet<String>,
    module_filter: &str,
    dry_run: bool,
    now: i64,
) -> anyhow::Result<Vec<Value>> {
    let mut query = String::from(
        "SELECT grant_id, subject_type, subject_id, permission, scope_id, reason
         FROM business_permission_grants
         WHERE active = 1 AND scope_type = 'module'",
    );
    if !module_filter.is_empty() {
        query.push_str(" AND scope_id = ?1");
    }
    query.push_str(" ORDER BY scope_id ASC, permission ASC, subject_type ASC, subject_id ASC");
    let mut stmt = conn.prepare(&query)?;
    let rows = if module_filter.is_empty() {
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map(params![module_filter], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    drop(stmt);

    let mut actions = Vec::new();
    for (grant_id, subject_type, subject_id, permission, scope_id, reason) in rows {
        if module_ids.contains(&scope_id) {
            continue;
        }
        actions.push(serde_json::json!({
            "kind": "stale_module_permission_grant",
            "grant_id": grant_id,
            "scope_id": scope_id,
            "permission": permission,
            "subject_type": subject_type,
            "subject_id": subject_id,
            "action": "deactivate",
            "apply": !dry_run,
        }));
        if !dry_run {
            let repaired_reason = if reason.trim().is_empty() {
                "deactivated by lifecycle projection repair: module scope no longer exists"
                    .to_owned()
            } else {
                format!(
                    "{}; deactivated by lifecycle projection repair: module scope no longer exists",
                    reason.trim()
                )
            };
            conn.execute(
                "UPDATE business_permission_grants
                 SET active = 0, reason = ?2, updated_at_ms = ?3
                 WHERE grant_id = ?1",
                params![grant_id, repaired_reason, now],
            )?;
        }
    }
    Ok(actions)
}
