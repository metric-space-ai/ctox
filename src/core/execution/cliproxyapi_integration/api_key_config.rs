// Origin: CTOX
// License: AGPL-3.0-only
//! Native-holder API credentials are references in the private runtime topology.
//! Endpoints identify the verified plan; model IDs come from account discovery.
use super::{ProviderIntegrationConfigError as Error, ProviderSecretRef};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyEndpoint {
    ZaiPlatform,
    ZaiCoding,
    MiniMax,
    Xai,
    KimiCoding,
    KimiPlatformGlobal,
    KimiPlatformChina,
}
impl ApiKeyEndpoint {
    pub fn provider(self) -> &'static str {
        match self {
            Self::ZaiPlatform | Self::ZaiCoding => "zai",
            Self::MiniMax => "minimax",
            Self::Xai => "xai",
            Self::KimiCoding | Self::KimiPlatformGlobal | Self::KimiPlatformChina => "kimi",
        }
    }
    // No arbitrary credential-bearing URL or endpoint fallback at execution.
    // Kimi Coding's OpenAI surface includes /v1; its Anthropic base does not.
    pub fn base_url(self) -> &'static str {
        match self {
            Self::ZaiPlatform => "https://api.z.ai/api/paas/v4",
            Self::ZaiCoding => "https://api.z.ai/api/coding/paas/v4",
            Self::MiniMax => "https://api.minimax.io/v1",
            Self::Xai => "https://api.x.ai/v1",
            Self::KimiCoding => "https://api.kimi.com/coding/v1",
            Self::KimiPlatformGlobal => "https://api.moonshot.ai/v1",
            Self::KimiPlatformChina => "https://api.moonshot.cn/v1",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiKeyAccountConfig {
    pub id: String,
    pub endpoint: ApiKeyEndpoint,
    pub api_key_secret: ProviderSecretRef,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub priority: i32,
    /// Exact IDs from this account's actual live model list. No static defaults.
    #[serde(default)]
    pub models: Vec<String>,
}
impl ApiKeyAccountConfig {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.is_empty() || self.id.len() > 128 || !self.id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-'|b'_'|b'.')) {
            return Err(Error::InvalidAccountId);
        }
        self.api_key_secret.validate()?;
        if self.api_key_secret.scope != "provider-api-keys" {
            return Err(Error::InvalidSecretReference);
        }
        let mut unique = std::collections::HashSet::new();
        if self.models.len() > 256 || self.models.iter().any(|m| m.trim()!=m || m.is_empty() || m.len()>256 || m.chars().any(char::is_control) || !unique.insert(m)) {
            return Err(Error::InvalidModel);
        }
        Ok(())
    }
}
