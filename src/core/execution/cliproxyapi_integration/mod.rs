// Origin: CTOX
// License: AGPL-3.0-only

//! CTOX-owned provider integration types.
//!
//! The portable CLIProxyAPI crate deliberately does not own Business OS,
//! CTOX persistence or Pi model-selection state. This module is the typed
//! boundary for those product concerns.

use std::collections::HashSet;
use std::fmt;

use ctox_cliproxyapi::internal::config::validate_credential_weight;
use serde::{Deserialize, Serialize};

mod kimi_host;
mod api_key_config;
pub(crate) use kimi_host::save_provider_integration_config;
pub use api_key_config::{ApiKeyAccountConfig, ApiKeyEndpoint};

pub use kimi_host::{
    build_instance_kimi_auth, build_kimi_subscription_route,
    build_kimi_subscription_route_with_http, install_kimi_subscription,
    load_provider_integration_config, remove_kimi_subscription_from_topology,
    KimiSubscriptionRoute, StoredProviderIntegrationConfig,
};

pub const PROVIDER_INTEGRATION_SCHEMA: &str = "ctox.provider-integration.config.v1";
pub const DEFAULT_KIMI_CODING_BASE_URL: &str = "https://api.kimi.com/coding";
pub const DEFAULT_KIMI_SUBSCRIPTION_MODEL: &str = "kimi-k3[1m]";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KimiEndpointProfile {
    #[default]
    CodingSubscription,
}

impl KimiEndpointProfile {
    pub fn base_url(self) -> &'static str {
        match self {
            Self::CodingSubscription => DEFAULT_KIMI_CODING_BASE_URL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSecretRef {
    pub scope: String,
    pub name: String,
}

impl ProviderSecretRef {
    pub fn new(scope: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            name: name.into(),
        }
    }

    fn validate(&self) -> Result<(), ProviderIntegrationConfigError> {
        if self.scope.trim().is_empty()
            || self.name.trim().is_empty()
            || self.scope.chars().any(char::is_control)
            || self.name.chars().any(char::is_control)
        {
            return Err(ProviderIntegrationConfigError::InvalidSecretReference);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KimiSubscriptionAccountConfig {
    pub id: String,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_account_weight")]
    pub weight: i64,
    #[serde(default)]
    pub models: Vec<String>,
    pub access_token_secret: ProviderSecretRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token_secret: Option<ProviderSecretRef>,
    pub state_secret: ProviderSecretRef,
    #[serde(default)]
    pub endpoint_profile: KimiEndpointProfile,
}

impl KimiSubscriptionAccountConfig {
    pub fn validate(&self) -> Result<(), ProviderIntegrationConfigError> {
        if self.id.trim().is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(ProviderIntegrationConfigError::InvalidAccountId);
        }
        validate_credential_weight(Some(self.weight))
            .map_err(|_| ProviderIntegrationConfigError::InvalidCredentialWeight)?;
        self.access_token_secret.validate()?;
        self.state_secret.validate()?;
        if let Some(refresh) = &self.refresh_token_secret {
            refresh.validate()?;
        }
        let mut refs = HashSet::new();
        refs.insert((
            self.access_token_secret.scope.trim(),
            self.access_token_secret.name.trim(),
        ));
        if !refs.insert((
            self.state_secret.scope.trim(),
            self.state_secret.name.trim(),
        )) {
            return Err(ProviderIntegrationConfigError::DuplicateSecretReference);
        }
        if self
            .refresh_token_secret
            .as_ref()
            .is_some_and(|refresh| !refs.insert((refresh.scope.trim(), refresh.name.trim())))
        {
            return Err(ProviderIntegrationConfigError::DuplicateSecretReference);
        }
        if self
            .models
            .iter()
            .any(|model| model.trim().is_empty() || model.chars().any(char::is_control))
        {
            return Err(ProviderIntegrationConfigError::InvalidModel);
        }
        Ok(())
    }

    pub fn effective_models(&self) -> Vec<String> {
        if self.models.is_empty() {
            vec![DEFAULT_KIMI_SUBSCRIPTION_MODEL.to_owned()]
        } else {
            self.models
                .iter()
                .map(|model| model.trim().to_owned())
                .collect()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderIntegrationConfig {
    #[serde(default = "provider_integration_schema")]
    pub schema: String,
    #[serde(default)]
    pub kimi_subscription_accounts: Vec<KimiSubscriptionAccountConfig>,
    #[serde(default)]
    pub api_key_accounts: Vec<ApiKeyAccountConfig>,
}

impl Default for ProviderIntegrationConfig {
    fn default() -> Self {
        Self {
            schema: PROVIDER_INTEGRATION_SCHEMA.to_owned(),
            kimi_subscription_accounts: Vec::new(),
            api_key_accounts: Vec::new(),
        }
    }
}

impl ProviderIntegrationConfig {
    pub fn validate(&self) -> Result<(), ProviderIntegrationConfigError> {
        if self.schema != PROVIDER_INTEGRATION_SCHEMA {
            return Err(ProviderIntegrationConfigError::InvalidSchema);
        }
        let mut ids = HashSet::new();
        let mut secret_refs = HashSet::new();
        for account in &self.api_key_accounts {
            account.validate()?;
            if !ids.insert(account.id.as_str()) {
                return Err(ProviderIntegrationConfigError::DuplicateAccountId);
            }
            if !secret_refs.insert((account.api_key_secret.scope.trim(), account.api_key_secret.name.trim())) {
                return Err(ProviderIntegrationConfigError::DuplicateSecretReference);
            }
        }
        for account in &self.kimi_subscription_accounts {
            account.validate()?;
            if !ids.insert(account.id.trim()) {
                return Err(ProviderIntegrationConfigError::DuplicateAccountId);
            }
            for secret in [
                Some(&account.access_token_secret),
                account.refresh_token_secret.as_ref(),
                Some(&account.state_secret),
            ]
            .into_iter()
            .flatten()
            {
                if !secret_refs.insert((secret.scope.trim(), secret.name.trim())) {
                    return Err(ProviderIntegrationConfigError::DuplicateSecretReference);
                }
            }
        }
        Ok(())
    }
}

fn default_account_weight() -> i64 {
    1
}

fn provider_integration_schema() -> String {
    PROVIDER_INTEGRATION_SCHEMA.to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderIntegrationConfigError {
    InvalidSchema,
    InvalidAccountId,
    DuplicateAccountId,
    InvalidCredentialWeight,
    InvalidSecretReference,
    DuplicateSecretReference,
    InvalidModel,
}

impl fmt::Display for ProviderIntegrationConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSchema => "provider integration schema is invalid",
            Self::InvalidAccountId => "provider account id is invalid",
            Self::DuplicateAccountId => "provider account id is duplicated",
            Self::InvalidCredentialWeight => "provider account weight is invalid",
            Self::InvalidSecretReference => "provider secret reference is invalid",
            Self::DuplicateSecretReference => "provider secret references must be distinct",
            Self::InvalidModel => "provider model is invalid",
        })
    }
}

impl std::error::Error for ProviderIntegrationConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(name: &str) -> ProviderSecretRef {
        ProviderSecretRef::new("provider-subscriptions", name)
    }

    fn account(id: &str) -> KimiSubscriptionAccountConfig {
        KimiSubscriptionAccountConfig {
            id: id.to_owned(),
            disabled: false,
            priority: 100,
            weight: 1,
            models: Vec::new(),
            access_token_secret: secret(&format!("{id}-access")),
            refresh_token_secret: Some(secret(&format!("{id}-refresh"))),
            state_secret: secret(&format!("{id}-state")),
            endpoint_profile: KimiEndpointProfile::CodingSubscription,
        }
    }

    #[test]
    fn kimi_account_is_typed_without_embedding_credentials() {
        let config = ProviderIntegrationConfig {
            kimi_subscription_accounts: vec![account("kimi-primary")],
            ..ProviderIntegrationConfig::default()
        };
        config.validate().unwrap();
        assert_eq!(
            config.kimi_subscription_accounts[0].effective_models(),
            [DEFAULT_KIMI_SUBSCRIPTION_MODEL]
        );
        let serialized = serde_json::to_string(&config).unwrap();
        assert!(serialized.contains("provider-subscriptions"));
        assert!(!serialized.contains("access_token\""));
        assert!(!serialized.contains("refresh_token\""));
    }

    #[test]
    fn duplicate_accounts_and_secret_aliases_fail_closed() {
        let duplicate = ProviderIntegrationConfig {
            kimi_subscription_accounts: vec![account("same"), account("same")],
            ..ProviderIntegrationConfig::default()
        };
        assert_eq!(
            duplicate.validate(),
            Err(ProviderIntegrationConfigError::DuplicateAccountId)
        );

        let mut aliased = account("aliased");
        aliased.state_secret = aliased.access_token_secret.clone();
        assert_eq!(
            aliased.validate(),
            Err(ProviderIntegrationConfigError::DuplicateSecretReference)
        );
    }

    #[test]
    fn endpoint_profile_is_closed_and_credential_free() {
        assert_eq!(
            KimiEndpointProfile::CodingSubscription.base_url(),
            DEFAULT_KIMI_CODING_BASE_URL
        );
        let mut encoded = serde_json::to_value(account("profile")).unwrap();
        encoded["endpoint_profile"] = serde_json::Value::String("custom_url".to_owned());
        assert!(serde_json::from_value::<KimiSubscriptionAccountConfig>(encoded).is_err());
    }
}
