// ref: internal/auth/kimi/token.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

pub const KIMI_REFRESH_THRESHOLD: Duration = Duration::from_secs(300);

#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Result<Self, KimiTokenError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(KimiTokenError::EmptySecret);
        }
        Ok(Self(Zeroizing::new(value)))
    }

#[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KimiTokenData {
    access_token: SecretString,
    refresh_token: Option<SecretString>,
    token_type: String,
    expires_at: Option<SystemTime>,
    scope: String,
}

impl KimiTokenData {
    pub fn new(
        access_token: SecretString,
        refresh_token: Option<SecretString>,
        token_type: impl Into<String>,
        expires_at: Option<SystemTime>,
        scope: impl Into<String>,
    ) -> Self {
        Self {
            access_token,
            refresh_token,
            token_type: token_type.into(),
            expires_at,
            scope: scope.into(),
        }
    }

#[must_use]
    pub fn access_token(&self) -> &SecretString {
        &self.access_token
    }

#[must_use]
    pub fn refresh_token(&self) -> Option<&SecretString> {
        self.refresh_token.as_ref()
    }

#[must_use]
    pub fn token_type(&self) -> &str {
        &self.token_type
    }

#[must_use]
    pub fn expires_at(&self) -> Option<SystemTime> {
        self.expires_at
    }

#[must_use]
    pub fn scope(&self) -> &str {
        &self.scope
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KimiAuthBundle {
    token_data: KimiTokenData,
    device_id: String,
}

impl KimiAuthBundle {
#[must_use]
    pub fn new(token_data: KimiTokenData, device_id: impl Into<String>) -> Self {
        Self {
            token_data,
            device_id: device_id.into().trim().to_owned(),
        }
    }

#[must_use]
    pub fn token_data(&self) -> &KimiTokenData {
        &self.token_data
    }

#[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum KimiSecretKind {
    Access,
    Refresh,
}

#[derive(Clone, Eq, PartialEq)]
pub struct KimiSecretHandle {
    id: String,
    kind: KimiSecretKind,
}

impl KimiSecretHandle {
    pub fn new(id: impl Into<String>, kind: KimiSecretKind) -> Result<Self, KimiTokenError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(KimiTokenError::EmptyHandle);
        }
        Ok(Self { id, kind })
    }

#[must_use]
    pub fn kind(&self) -> KimiSecretKind {
        self.kind
    }
}

impl fmt::Debug for KimiSecretHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KimiSecretHandle")
            .field("id", &"[REDACTED]")
            .field("kind", &self.kind)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KimiCredentialHandles {
    pub access: KimiSecretHandle,
    pub refresh: KimiSecretHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KimiStoredCredentials {
    access_token: SecretString,
    refresh_token: Option<SecretString>,
}

impl KimiStoredCredentials {
#[must_use]
    pub fn from_token_data(token_data: &KimiTokenData) -> Self {
        Self {
            access_token: token_data.access_token.clone(),
            refresh_token: token_data.refresh_token.clone(),
        }
    }

#[must_use]
    pub fn access_token(&self) -> &SecretString {
        &self.access_token
    }

#[must_use]
    pub fn refresh_token(&self) -> Option<&SecretString> {
        self.refresh_token.as_ref()
    }
}

pub trait KimiSecretStore: Send + Sync {
    fn load_credentials(
        &self,
        handles: &KimiCredentialHandles,
    ) -> Result<KimiStoredCredentials, KimiSecretStoreError>;

    fn store_credentials(
        &self,
        handles: &KimiCredentialHandles,
        credentials: &KimiStoredCredentials,
    ) -> Result<(), KimiSecretStoreError>;
}

/// Credential-bearing storage adapted from upstream's flattened JSON file.
/// It is intentionally not serializable and can persist only through the
/// injected CTOX secret-store boundary.
#[derive(Clone, PartialEq)]
pub struct KimiTokenStorage {
    credentials: KimiStoredCredentials,
    token_type: String,
    scope: String,
    device_id: String,
    expires_at: Option<SystemTime>,
    metadata: BTreeMap<String, Value>,
}

impl KimiTokenStorage {
#[must_use]
    pub fn from_bundle(bundle: &KimiAuthBundle) -> Self {
        let token = bundle.token_data();
        Self {
            credentials: KimiStoredCredentials::from_token_data(token),
            token_type: token.token_type.clone(),
            scope: token.scope.clone(),
            device_id: bundle.device_id.clone(),
            expires_at: token.expires_at,
            metadata: BTreeMap::new(),
        }
    }

    pub fn set_metadata(&mut self, metadata: BTreeMap<String, Value>) {
        self.metadata = metadata;
    }

#[must_use]
    pub fn credentials(&self) -> &KimiStoredCredentials {
        &self.credentials
    }

#[must_use]
    pub fn token_type(&self) -> &str {
        &self.token_type
    }

#[must_use]
    pub fn scope(&self) -> &str {
        &self.scope
    }

#[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

#[must_use]
    pub fn expires_at(&self) -> Option<SystemTime> {
        self.expires_at
    }

#[must_use]
    pub fn metadata(&self) -> &BTreeMap<String, Value> {
        &self.metadata
    }

#[must_use]
    pub fn is_expired_at(&self, now: SystemTime) -> bool {
        self.expires_at.is_some_and(|expires_at| {
            now.checked_add(KIMI_REFRESH_THRESHOLD)
                .is_none_or(|threshold| threshold > expires_at)
        })
    }

#[must_use]
    pub fn needs_refresh_at(&self, now: SystemTime) -> bool {
        self.credentials.refresh_token.is_some() && self.is_expired_at(now)
    }

    pub fn persist_credentials(
        &self,
        store: &dyn KimiSecretStore,
        handles: &KimiCredentialHandles,
    ) -> Result<(), KimiSecretStoreError> {
        store.store_credentials(handles, &self.credentials)
    }

    pub fn update_from_token_data(&mut self, token_data: &KimiTokenData) {
        self.credentials = KimiStoredCredentials::from_token_data(token_data);
        self.token_type.clone_from(&token_data.token_type);
        self.scope.clone_from(&token_data.scope);
        self.expires_at = token_data.expires_at;
    }
}

impl fmt::Debug for KimiTokenStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KimiTokenStorage")
            .field("credentials", &self.credentials)
            .field("token_type", &self.token_type)
            .field("scope", &self.scope)
            .field("device_id", &self.device_id)
            .field("expires_at", &self.expires_at)
            .field("metadata_keys", &self.metadata.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum KimiTokenError {
    EmptySecret,
    EmptyHandle,
}

impl fmt::Display for KimiTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptySecret => "Kimi credential is empty",
            Self::EmptyHandle => "Kimi credential handle is empty",
        })
    }
}

impl std::error::Error for KimiTokenError {}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum KimiSecretStoreError {
    Missing,
    InvalidValue,
    Read,
    Write,
}

impl fmt::Display for KimiSecretStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Missing => "Kimi credential is missing",
            Self::InvalidValue => "Kimi credential value is invalid",
            Self::Read => "Kimi credential could not be read",
            Self::Write => "Kimi credential could not be written",
        })
    }
}

impl std::error::Error for KimiSecretStoreError {}

#[cfg(test)]
mod tests {
    use super::*;

#[test]
    fn storage_refresh_threshold_and_secret_redaction_are_explicit() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let token = KimiTokenData::new(
            SecretString::new("access-secret").unwrap(),
            Some(SecretString::new("refresh-secret").unwrap()),
            "Bearer",
            Some(now + Duration::from_secs(299)),
            "scope",
        );
        let storage = KimiTokenStorage::from_bundle(&KimiAuthBundle::new(token, "device"));
        assert!(storage.is_expired_at(now));
        assert!(storage.needs_refresh_at(now));
        let debug = format!("{storage:?}");
        assert!(!debug.contains("access-secret"));
        assert!(!debug.contains("refresh-secret"));
    }

#[test]
    fn missing_expiry_or_refresh_token_matches_upstream_valid_state() {
        let token = KimiTokenData::new(
            SecretString::new("access").unwrap(),
            None,
            "Bearer",
            None,
            "",
        );
        let storage = KimiTokenStorage::from_bundle(&KimiAuthBundle::new(token, "device"));
        assert!(!storage.is_expired_at(SystemTime::now()));
        assert!(!storage.needs_refresh_at(SystemTime::now()));
    }
}
