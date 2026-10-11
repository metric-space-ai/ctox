// ref: internal/auth/codex/token.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fmt;
use std::time::SystemTime;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use zeroize::Zeroizing;

/// Heap-backed credential which is zeroized on drop and always redacted.
///
/// Upstream writes a flattened JSON token file. CTOX deliberately keeps token
/// values behind this type and a host-owned encrypted secret-store boundary.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Result<Self, CodexTokenError> {
        let value = value.into();
        if value.is_empty() {
            return Err(CodexTokenError::EmptySecret);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexSecretKind {
    IdToken,
    AccessToken,
    RefreshToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexSecretHandle {
    scope: String,
    name: String,
    kind: CodexSecretKind,
}

impl CodexSecretHandle {
    pub fn new(
        scope: impl Into<String>,
        name: impl Into<String>,
        kind: CodexSecretKind,
    ) -> Result<Self, CodexTokenError> {
        let scope = scope.into();
        let name = name.into();
        if scope.trim().is_empty() {
            return Err(CodexTokenError::EmptyHandleField("scope"));
        }
        if name.trim().is_empty() {
            return Err(CodexTokenError::EmptyHandleField("name"));
        }
        Ok(Self { scope, name, kind })
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> CodexSecretKind {
        self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexCredentialHandles {
    id_token: CodexSecretHandle,
    access_token: CodexSecretHandle,
    refresh_token: CodexSecretHandle,
}

impl CodexCredentialHandles {
    pub fn new(
        id_token: CodexSecretHandle,
        access_token: CodexSecretHandle,
        refresh_token: CodexSecretHandle,
    ) -> Result<Self, SecretStoreError> {
        expect_kind(&id_token, CodexSecretKind::IdToken)?;
        expect_kind(&access_token, CodexSecretKind::AccessToken)?;
        expect_kind(&refresh_token, CodexSecretKind::RefreshToken)?;
        Ok(Self {
            id_token,
            access_token,
            refresh_token,
        })
    }

    pub fn id_token(&self) -> &CodexSecretHandle {
        &self.id_token
    }

    pub fn access_token(&self) -> &CodexSecretHandle {
        &self.access_token
    }

    pub fn refresh_token(&self) -> &CodexSecretHandle {
        &self.refresh_token
    }
}

fn expect_kind(
    handle: &CodexSecretHandle,
    expected: CodexSecretKind,
) -> Result<(), SecretStoreError> {
    if handle.kind() == expected {
        Ok(())
    } else {
        Err(SecretStoreError::KindMismatch {
            expected,
            actual: handle.kind(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexStoredCredentials {
    id_token: SecretString,
    access_token: SecretString,
    refresh_token: SecretString,
}

impl CodexStoredCredentials {
    pub fn new(
        id_token: SecretString,
        access_token: SecretString,
        refresh_token: SecretString,
    ) -> Self {
        Self {
            id_token,
            access_token,
            refresh_token,
        }
    }

    pub fn id_token(&self) -> &SecretString {
        &self.id_token
    }

    pub fn access_token(&self) -> &SecretString {
        &self.access_token
    }

    pub fn refresh_token(&self) -> &SecretString {
        &self.refresh_token
    }
}

/// Holder-private receipt across the OAuth network wait. No Debug or Serialize:
/// its opaque generation is not public metadata or an execution authority.
pub struct CodexRefreshSnapshot {
    pub credentials: CodexStoredCredentials,
    pub generation: Option<String>,
}

pub trait CodexSecretStore: Send + Sync {
    fn load_credentials(
        &self,
        handles: &CodexCredentialHandles,
    ) -> Result<CodexStoredCredentials, SecretStoreError>;

    fn store_credentials(
        &self,
        handles: &CodexCredentialHandles,
        credentials: &CodexStoredCredentials,
    ) -> Result<(), SecretStoreError>;

    fn load_credentials_for_refresh(&self, handles: &CodexCredentialHandles) -> Result<CodexRefreshSnapshot, SecretStoreError> {
        Ok(CodexRefreshSnapshot { credentials: self.load_credentials(handles)?, generation: None })
    }

    /// Concurrent persistent stores must override with an atomic generation CAS.
    /// The default retains compatibility for portable single-owner stores.
    fn store_refreshed_credentials(&self, handles: &CodexCredentialHandles, expected: &CodexRefreshSnapshot, credentials: &CodexStoredCredentials) -> Result<(), SecretStoreError> {
        let _ = expected;
        self.store_credentials(handles, credentials)
    }
}

/// Codex token storage adapted to CTOX's encrypted secret-store boundary.
/// Credential fields are deliberately excluded from generic serialization and
/// metadata cannot replace them.
#[derive(Clone, PartialEq)]
pub struct CodexTokenStorage {
    credentials: CodexStoredCredentials,
    account_id: String,
    last_refresh: String,
    email: String,
    storage_type: String,
    expired: String,
    metadata: BTreeMap<String, Value>,
}

impl CodexTokenStorage {
    pub fn from_token_data(
        token: &super::openai::CodexTokenData,
        last_refresh: SystemTime,
    ) -> Self {
        Self {
            credentials: CodexStoredCredentials::new(
                token.id_token().clone(),
                token.access_token().clone(),
                token.refresh_token().clone(),
            ),
            account_id: token.account_id().to_owned(),
            last_refresh: format_rfc3339(last_refresh),
            email: token.email().to_owned(),
            storage_type: "codex".to_owned(),
            expired: format_rfc3339(token.expires_at()),
            metadata: BTreeMap::new(),
        }
    }

    pub fn credentials(&self) -> &CodexStoredCredentials {
        &self.credentials
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn last_refresh(&self) -> &str {
        &self.last_refresh
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn storage_type(&self) -> &str {
        &self.storage_type
    }

    pub fn expired(&self) -> &str {
        &self.expired
    }

    pub fn metadata(&self) -> &BTreeMap<String, Value> {
        &self.metadata
    }

    pub fn set_metadata(&mut self, metadata: BTreeMap<String, Value>) {
        self.metadata = metadata;
    }

    pub fn persist_credentials(
        &self,
        store: &dyn CodexSecretStore,
        handles: &CodexCredentialHandles,
    ) -> Result<(), SecretStoreError> {
        store.store_credentials(handles, &self.credentials)
    }

    pub fn update_from_token_data(
        &mut self,
        token: &super::openai::CodexTokenData,
        refreshed_at: SystemTime,
    ) {
        self.credentials = CodexStoredCredentials::new(
            token.id_token().clone(),
            token.access_token().clone(),
            token.refresh_token().clone(),
        );
        self.account_id = token.account_id().to_owned();
        self.last_refresh = format_rfc3339(refreshed_at);
        self.email = token.email().to_owned();
        self.expired = format_rfc3339(token.expires_at());
    }
}

impl fmt::Debug for CodexTokenStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexTokenStorage")
            .field("credentials", &"[REDACTED]")
            .field("account_id", &self.account_id)
            .field("last_refresh", &self.last_refresh)
            .field("email", &self.email)
            .field("storage_type", &self.storage_type)
            .field("expired", &self.expired)
            .field("metadata", &self.metadata)
            .finish()
    }
}

fn format_rfc3339(value: SystemTime) -> String {
    DateTime::<Utc>::from(value).to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexTokenError {
    EmptySecret,
    EmptyHandleField(&'static str),
    EmptyPkceChallenge,
    Randomness,
    ExpiryOverflow,
}

impl fmt::Display for CodexTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptySecret => "secret must not be empty",
            Self::EmptyHandleField(_) => "secret handle field is empty",
            Self::EmptyPkceChallenge => "PKCE challenge must not be empty",
            Self::Randomness => "secure randomness is unavailable",
            Self::ExpiryOverflow => "token expiry exceeds SystemTime range",
        })
    }
}

impl std::error::Error for CodexTokenError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretStoreError {
    Missing,
    InvalidValue,
    Read,
    Write,
    KindMismatch {
        expected: CodexSecretKind,
        actual: CodexSecretKind,
    },
}

impl fmt::Display for SecretStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => formatter.write_str("Codex credential is missing"),
            Self::InvalidValue => formatter.write_str("Codex credential value is invalid"),
            Self::Read => formatter.write_str("Codex credential could not be read"),
            Self::Write => formatter.write_str("Codex credential could not be written"),
            Self::KindMismatch { expected, actual } => write!(
                formatter,
                "Codex credential handle kind mismatch: expected {expected:?}, got {actual:?}"
            ),
        }
    }
}

impl std::error::Error for SecretStoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_never_render_their_values() {
        let credentials = CodexStoredCredentials::new(
            SecretString::new("id-do-not-leak").unwrap(),
            SecretString::new("access-do-not-leak").unwrap(),
            SecretString::new("refresh-do-not-leak").unwrap(),
        );
        let rendered = format!("{credentials:?}");
        assert!(!rendered.contains("do-not-leak"));
        assert_eq!(credentials.access_token().to_string(), "[REDACTED]");
    }

    #[test]
    fn handle_triplet_rejects_swapped_kinds() {
        let handle =
            |name, kind| CodexSecretHandle::new("provider-subscriptions", name, kind).unwrap();
        let error = CodexCredentialHandles::new(
            handle("id", CodexSecretKind::AccessToken),
            handle("access", CodexSecretKind::IdToken),
            handle("refresh", CodexSecretKind::RefreshToken),
        )
        .unwrap_err();
        assert!(matches!(error, SecretStoreError::KindMismatch { .. }));
    }
}
