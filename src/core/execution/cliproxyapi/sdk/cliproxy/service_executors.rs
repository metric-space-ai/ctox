// ref: sdk/cliproxy/service_executors.go @ a88197f845c979132c8978ea223c6af05cc81536
// Candidate adaptation: sdk/cliproxy/service_executors.go:164-326 @ d7914afdedca7af95ee974a42453dc49fc1388ce
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::fmt;
use std::sync::Arc;

use super::auth::{Auth, ProviderExecutorRegistration};
use super::service_auth::{ServiceAuthError, ServiceAuthRuntime};

pub const BASELINE_EXECUTOR_PROVIDERS: [&str; 14] = [
    "codex",
    "claude",
    "gemini",
    "gemini-interactions",
    "vertex",
    "aistudio",
    "antigravity",
    "kimi",
    "kimi-ai",
    "kimi.ai",
    "xai",
    "devin",
    "meta",
    "openai-compatibility",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutorFactoryError {
    Unsupported,
    InvalidRegistration,
}

impl fmt::Display for ExecutorFactoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unsupported => "executor provider is unsupported",
            Self::InvalidRegistration => "executor registration is invalid",
        })
    }
}

impl std::error::Error for ExecutorFactoryError {}

pub trait ServiceExecutorFactory: Send + Sync {
    fn registration_for(
        &self,
        provider_key: &str,
        auth: &Auth,
    ) -> Result<Arc<ProviderExecutorRegistration>, ExecutorFactoryError>;
}

#[derive(Clone, Default)]
pub struct ExecutorRegistrationOptions {
    pub include_baseline: bool,
    pub include_plugins: bool,
    pub force_replace_auths: bool,
    pub auths: Vec<Auth>,
}

impl fmt::Debug for ExecutorRegistrationOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutorRegistrationOptions")
            .field("include_baseline", &self.include_baseline)
            .field("include_plugins", &self.include_plugins)
            .field("force_replace_auths", &self.force_replace_auths)
            .field("auth_count", &self.auths.len())
            .finish()
    }
}

impl ServiceAuthRuntime {
    pub fn ensure_executors_for_auth(
        &self,
        auth: &Auth,
        force_replace: bool,
    ) -> Result<(), ServiceAuthError> {
        self.register_available_executors(ExecutorRegistrationOptions {
            auths: vec![auth.clone()],
            force_replace_auths: force_replace,
            ..ExecutorRegistrationOptions::default()
        })
    }

    pub fn register_available_executors(
        &self,
        options: ExecutorRegistrationOptions,
    ) -> Result<(), ServiceAuthError> {
        let _registration = self
            .executor_registration
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut auths = if options.include_baseline {
            baseline_executor_auths()
        } else {
            Vec::new()
        };
        auths.extend(options.auths);
        self.register_executors_for_auths(&auths, options.force_replace_auths)?;
        if options.include_plugins {
            if let Some(plugin) = self.plugin_runtime() {
                for registration in plugin.executor_registrations() {
                    self.auth_manager().register_executor(registration);
                }
            }
        }
        Ok(())
    }

    pub fn register_executors_for_auths(
        &self,
        auths: &[Auth],
        force_replace: bool,
    ) -> Result<(), ServiceAuthError> {
        let mut rebound_codex = false;
        for auth in auths {
            if auth.provider.trim().eq_ignore_ascii_case("codex") {
                if rebound_codex && force_replace {
                    continue;
                }
                rebound_codex = true;
            }
            self.register_executor_for_auth(auth, force_replace)?;
        }
        Ok(())
    }

    pub fn register_executor_for_auth(
        &self,
        auth: &Auth,
        force_replace: bool,
    ) -> Result<(), ServiceAuthError> {
        let provider = auth.provider.trim().to_ascii_lowercase();
        let (compat_key, _, is_compat) = openai_compat_info_from_auth(auth);
        let provider_key = if is_compat {
            compat_key
        } else if provider.is_empty() {
            "openai-compatibility".to_owned()
        } else {
            provider.clone()
        };
        let executors = self.auth_manager().executors();
        let existing = executors.get(&provider_key);

        if provider == "codex" && !force_replace && existing.is_some() {
            return Ok(());
        }
        if auth.disabled && provider != "codex" {
            return Ok(());
        }

        if is_compat || provider == "xai" {
            if !force_replace && existing.is_some() {
                return Ok(());
            }
        } else if !is_native_provider(&provider) {
            if let Some(plugin) = self.plugin_runtime() {
                if plugin.has_executor_candidate_provider(&provider_key)
                    && !has_native_openai_compat_executor_config(auth)
                {
                    if existing
                        .as_ref()
                        .is_some_and(|entry| !plugin.owns_executor(entry))
                    {
                        executors.unregister(&provider_key);
                    }
                    return Ok(());
                }
                if !force_replace
                    && existing
                        .as_ref()
                        .is_some_and(|entry| !plugin.owns_executor(entry))
                {
                    return Ok(());
                }
            } else if !force_replace && existing.is_some() {
                return Ok(());
            }
        }

        let registration = match self
            .executor_factory()
            .registration_for(&provider_key, auth)
        {
            Ok(registration) => registration,
            Err(ExecutorFactoryError::Unsupported) => return Ok(()),
            Err(ExecutorFactoryError::InvalidRegistration) => {
                return Err(ServiceAuthError::AuthManager);
            }
        };
        self.auth_manager().register_executor(registration);
        Ok(())
    }
}

#[must_use]
pub fn baseline_executor_auths() -> Vec<Auth> {
    BASELINE_EXECUTOR_PROVIDERS
        .into_iter()
        .map(|provider| {
            let mut auth = Auth::default();
            auth.id = provider.to_owned();
            auth.provider = provider.to_owned();
            if provider == "openai-compatibility" {
                auth.attributes
                    .insert("compat_name".to_owned(), "openai-compatibility".to_owned());
            }
            auth
        })
        .collect()
}

#[must_use]
pub fn openai_compat_info_from_auth(auth: &Auth) -> (String, String, bool) {
    let provider_key = auth
        .attributes
        .get("provider_key")
        .map_or("", String::as_str)
        .trim();
    let compat_name = auth
        .attributes
        .get("compat_name")
        .map_or("", String::as_str)
        .trim();
    if !compat_name.is_empty() {
        let key = if provider_key.is_empty() {
            compat_name
        } else {
            provider_key
        };
        return (
            openai_compatible_provider_key(key),
            compat_name.to_owned(),
            true,
        );
    }
    if auth
        .provider
        .trim()
        .eq_ignore_ascii_case("openai-compatibility")
    {
        let compat_name = auth.label.trim();
        let key = if compat_name.is_empty() {
            "openai-compatibility"
        } else {
            compat_name
        };
        return (
            openai_compatible_provider_key(key),
            compat_name.to_owned(),
            true,
        );
    }
    (String::new(), String::new(), false)
}

#[must_use]
pub fn has_native_openai_compat_executor_config(auth: &Auth) -> bool {
    auth.attributes
        .get("base_url")
        .is_some_and(|value| !value.trim().is_empty())
        || auth
            .attributes
            .get("compat_name")
            .is_some_and(|value| !value.trim().is_empty())
        || auth
            .provider
            .trim()
            .eq_ignore_ascii_case("openai-compatibility")
}

fn openai_compatible_provider_key(provider: &str) -> String {
    let provider = provider.trim().to_ascii_lowercase();
    if provider.starts_with("openai-compatible-") || provider == "openai-compatibility" {
        provider
    } else {
        format!("openai-compatible-{provider}")
    }
}

fn is_native_provider(provider: &str) -> bool {
    BASELINE_EXECUTOR_PROVIDERS.contains(&provider) || provider == "kimi.com"
}
