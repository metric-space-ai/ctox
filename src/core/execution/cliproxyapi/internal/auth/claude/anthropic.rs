// ref: internal/auth/claude/anthropic.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::time::SystemTime;

use super::token::{ClaudeTokenData, SecretString};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeUserInfo {
    account_uuid: String,
    email: String,
    organization_uuid: String,
    organization_name: String,
}

impl ClaudeUserInfo {
    pub fn new(
        account_uuid: impl Into<String>,
        email: impl Into<String>,
        organization_uuid: impl Into<String>,
        organization_name: impl Into<String>,
    ) -> Self {
        Self {
            account_uuid: account_uuid.into(),
            email: email.into(),
            organization_uuid: organization_uuid.into(),
            organization_name: organization_name.into(),
        }
    }

    pub fn account_uuid(&self) -> &str {
        &self.account_uuid
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn organization_uuid(&self) -> &str {
        &self.organization_uuid
    }

    pub fn organization_name(&self) -> &str {
        &self.organization_name
    }
}

/// Result of a completed Anthropic authorization-code exchange.
///
/// Upstream's `PKCECodes` and `ClaudeTokenData` declarations are intentionally
/// not duplicated here: the canonical Rust forms live in `pkce::PkceCodes` and
/// `token::ClaudeTokenData`. Unlike the Go DTO, this bundle is not generically
/// serializable because it can carry an API key and OAuth credentials.
#[derive(Clone, PartialEq, Eq)]
pub struct ClaudeAuthBundle {
    api_key: Option<SecretString>,
    token_data: ClaudeTokenData,
    last_refresh: SystemTime,
    token_type: String,
    scopes: Vec<String>,
    user_info: ClaudeUserInfo,
    device_ids: Vec<String>,
}

impl ClaudeAuthBundle {
    pub fn new(
        api_key: Option<SecretString>,
        token_data: ClaudeTokenData,
        last_refresh: SystemTime,
    ) -> Self {
        Self {
            api_key,
            token_data,
            last_refresh,
            token_type: String::new(),
            scopes: Vec::new(),
            user_info: ClaudeUserInfo::default(),
            device_ids: Vec::new(),
        }
    }

    pub fn with_exchange_metadata(
        mut self,
        token_type: impl Into<String>,
        scopes: Vec<String>,
        user_info: ClaudeUserInfo,
    ) -> Self {
        self.token_type = token_type.into();
        self.scopes = scopes;
        self.with_user_info(user_info)
    }

    pub fn api_key(&self) -> Option<&SecretString> {
        self.api_key.as_ref()
    }

    pub fn token_data(&self) -> &ClaudeTokenData {
        &self.token_data
    }

    pub fn last_refresh(&self) -> SystemTime {
        self.last_refresh
    }

    pub fn token_type(&self) -> &str {
        &self.token_type
    }

    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    pub fn user_info(&self) -> &ClaudeUserInfo {
        &self.user_info
    }

    pub fn device_ids(&self) -> &[String] {
        &self.device_ids
    }

    pub fn with_device_ids(mut self, device_ids: Vec<String>) -> Self {
        self.device_ids = device_ids;
        self
    }

    pub fn with_user_info(mut self, user_info: ClaudeUserInfo) -> Self {
        self.token_data.set_identity_if_present(
            user_info.account_uuid(),
            user_info.organization_uuid(),
            user_info.organization_name(),
        );
        if !user_info.email().trim().is_empty() {
            self.token_data.set_email(user_info.email());
        }
        self.user_info = user_info;
        self
    }
}

impl std::fmt::Debug for ClaudeAuthBundle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClaudeAuthBundle")
            .field(
                "api_key",
                &self.api_key.as_ref().map(|_| "SecretString([REDACTED])"),
            )
            .field("token_data", &self.token_data)
            .field("last_refresh", &self.last_refresh)
            .field("token_type", &self.token_type)
            .field("scopes", &self.scopes)
            .field("user_info", &self.user_info)
            .field("device_ids", &self.device_ids)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::internal::auth::claude::pkce::PkceCodes;

    fn token_data() -> ClaudeTokenData {
        ClaudeTokenData::new(
            SecretString::new("access-do-not-leak").unwrap(),
            SecretString::new("refresh-do-not-leak").unwrap(),
            "operator@example.com",
            SystemTime::UNIX_EPOCH + Duration::from_secs(7_200),
        )
    }

    #[test]
    fn bundle_uses_canonical_token_and_secret_types_without_debug_leaks() {
        let bundle = ClaudeAuthBundle::new(
            Some(SecretString::new("api-key-do-not-leak").unwrap()),
            token_data(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(3_600),
        );

        assert_eq!(
            bundle.api_key().unwrap().expose_secret(),
            "api-key-do-not-leak"
        );
        assert_eq!(bundle.token_data().email(), "operator@example.com");
        assert_eq!(
            bundle.last_refresh(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(3_600)
        );
        let debug = format!("{bundle:?}");
        assert!(!debug.contains("api-key-do-not-leak"));
        assert!(!debug.contains("access-do-not-leak"));
        assert!(!debug.contains("refresh-do-not-leak"));
    }
}
