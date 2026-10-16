// ref: internal/auth/claude/token.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use zeroize::Zeroizing;

/// Claude credentials are refreshed four hours before expiry.
///
/// ref: sdk/auth/claude.go:34-36
pub const CLAUDE_REFRESH_LEAD: Duration = Duration::from_secs(4 * 60 * 60);

/// Heap-backed secret which zeroizes its allocation on drop and never reveals
/// its value through `Debug` or `Display`.
///
/// This type intentionally does not implement `Serialize`: persistence must go
/// through a typed secret-store boundary instead of generic metadata or logs.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Result<Self, TokenError> {
        let value = value.into();
        if value.is_empty() {
            return Err(TokenError::EmptySecret);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    /// Deliberate escape hatch for provider requests and secret-store writes.
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
pub enum ClaudeSecretKind {
    AccessToken,
    RefreshToken,
}

/// Non-secret reference into the host application's encrypted secret store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeSecretHandle {
    scope: String,
    name: String,
    kind: ClaudeSecretKind,
}

impl ClaudeSecretHandle {
    pub fn new(
        scope: impl Into<String>,
        name: impl Into<String>,
        kind: ClaudeSecretKind,
    ) -> Result<Self, TokenError> {
        let scope = scope.into();
        let name = name.into();
        if scope.trim().is_empty() {
            return Err(TokenError::EmptyHandleField("scope"));
        }
        if name.trim().is_empty() {
            return Err(TokenError::EmptyHandleField("name"));
        }
        Ok(Self { scope, name, kind })
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> ClaudeSecretKind {
        self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCredentialHandles {
    access_token: ClaudeSecretHandle,
    refresh_token: ClaudeSecretHandle,
}

/// Access/refresh pair loaded from one host-store snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeStoredCredentials {
    access_token: SecretString,
    refresh_token: SecretString,
}

impl ClaudeStoredCredentials {
    pub fn new(access_token: SecretString, refresh_token: SecretString) -> Self {
        Self {
            access_token,
            refresh_token,
        }
    }

    pub fn access_token(&self) -> &SecretString {
        &self.access_token
    }

    pub fn refresh_token(&self) -> &SecretString {
        &self.refresh_token
    }
}

impl ClaudeCredentialHandles {
    pub fn new(
        access_token: ClaudeSecretHandle,
        refresh_token: ClaudeSecretHandle,
    ) -> Result<Self, SecretStoreError> {
        if access_token.kind() != ClaudeSecretKind::AccessToken {
            return Err(SecretStoreError::KindMismatch {
                expected: ClaudeSecretKind::AccessToken,
                actual: access_token.kind(),
            });
        }
        if refresh_token.kind() != ClaudeSecretKind::RefreshToken {
            return Err(SecretStoreError::KindMismatch {
                expected: ClaudeSecretKind::RefreshToken,
                actual: refresh_token.kind(),
            });
        }
        Ok(Self {
            access_token,
            refresh_token,
        })
    }

    pub fn access_token(&self) -> &ClaudeSecretHandle {
        &self.access_token
    }

    pub fn refresh_token(&self) -> &ClaudeSecretHandle {
        &self.refresh_token
    }
}

/// Host-owned secret persistence boundary.
///
/// The portable proxy never opens credential files and never consults process
/// environment. CTOX implements this trait against its encrypted SQLite store.
pub trait ClaudeSecretStore: Send + Sync {
    /// Loads both values from one consistent host-store snapshot.
    fn load_credentials(
        &self,
        handles: &ClaudeCredentialHandles,
    ) -> Result<ClaudeStoredCredentials, SecretStoreError>;

    /// Atomically rotates access and refresh credentials.
    fn store_credentials(
        &self,
        handles: &ClaudeCredentialHandles,
        credentials: &ClaudeStoredCredentials,
    ) -> Result<(), SecretStoreError>;
}

/// Anthropic token-storage aggregate adapted to CTOX's secret-store boundary.
///
/// Upstream flattens this object and arbitrary hook metadata into a plaintext
/// JSON file. The Rust port deliberately keeps every credential in
/// `SecretString`, exposes no `Serialize` implementation, and persists the
/// access/refresh pair only through `ClaudeSecretStore`. Metadata remains a
/// separate namespace, so a hook cannot replace a credential by supplying an
/// `access_token` or `refresh_token` key.
#[derive(Clone, PartialEq)]
pub struct ClaudeTokenStorage {
    id_token: Option<SecretString>,
    credentials: ClaudeStoredCredentials,
    last_refresh: String,
    email: String,
    account_uuid: String,
    organization_uuid: String,
    organization_name: String,
    device_ids: Vec<String>,
    storage_type: String,
    expired: String,
    metadata: BTreeMap<String, Value>,
}

impl ClaudeTokenStorage {
    pub fn from_token_data(
        token_data: &ClaudeTokenData,
        last_refresh: SystemTime,
        id_token: Option<SecretString>,
    ) -> Self {
        Self {
            id_token,
            credentials: ClaudeStoredCredentials::new(
                token_data.access_token().clone(),
                token_data.refresh_token().clone(),
            ),
            last_refresh: format_rfc3339(last_refresh),
            email: token_data.email().to_owned(),
            account_uuid: token_data.account_uuid().to_owned(),
            organization_uuid: token_data.organization_uuid().to_owned(),
            organization_name: token_data.organization_name().to_owned(),
            device_ids: Vec::new(),
            storage_type: "claude".to_owned(),
            expired: format_rfc3339(token_data.expires_at()),
            metadata: BTreeMap::new(),
        }
    }

    pub fn set_metadata(&mut self, metadata: BTreeMap<String, Value>) {
        self.metadata = metadata;
    }

    pub fn id_token(&self) -> Option<&SecretString> {
        self.id_token.as_ref()
    }

    pub fn credentials(&self) -> &ClaudeStoredCredentials {
        &self.credentials
    }

    pub fn last_refresh(&self) -> &str {
        &self.last_refresh
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn account_uuid(&self) -> &str {
        &self.account_uuid
    }

    pub fn organization_uuid(&self) -> &str {
        &self.organization_uuid
    }

    pub fn organization_name(&self) -> &str {
        &self.organization_name
    }

    pub fn device_ids(&self) -> &[String] {
        &self.device_ids
    }

    pub fn with_device_ids(mut self, device_ids: &[String]) -> Self {
        self.device_ids = device_ids.to_vec();
        self
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

    /// Persists both rotating credentials in one host-store operation.
    pub fn persist_credentials(
        &self,
        store: &dyn ClaudeSecretStore,
        handles: &ClaudeCredentialHandles,
    ) -> Result<(), SecretStoreError> {
        store.store_credentials(handles, &self.credentials)
    }

    /// Mirrors upstream's token-storage refresh update without exposing either
    /// fresh credential through a generic wire representation.
    pub fn update_from_token_data(
        &mut self,
        token_data: &ClaudeTokenData,
        refreshed_at: SystemTime,
    ) {
        self.credentials = ClaudeStoredCredentials::new(
            token_data.access_token().clone(),
            token_data.refresh_token().clone(),
        );
        self.last_refresh = format_rfc3339(refreshed_at);
        if !token_data.email().is_empty() {
            self.email = token_data.email().to_owned();
        }
        if !token_data.account_uuid().is_empty() {
            self.account_uuid = token_data.account_uuid().to_owned();
        }
        if !token_data.organization_uuid().is_empty() {
            self.organization_uuid = token_data.organization_uuid().to_owned();
        }
        if !token_data.organization_name().is_empty() {
            self.organization_name = token_data.organization_name().to_owned();
        }
        self.expired = format_rfc3339(token_data.expires_at());
    }
}

impl fmt::Debug for ClaudeTokenStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeTokenStorage")
            .field(
                "id_token",
                &self.id_token.as_ref().map(|_| "SecretString([REDACTED])"),
            )
            .field("credentials", &self.credentials)
            .field("last_refresh", &self.last_refresh)
            .field("email", &self.email)
            .field("account_uuid", &self.account_uuid)
            .field("organization_uuid", &self.organization_uuid)
            .field("organization_name", &self.organization_name)
            .field("device_ids", &self.device_ids)
            .field("storage_type", &self.storage_type)
            .field("expired", &self.expired)
            .field("metadata", &"[REDACTED]")
            .finish()
    }
}

fn format_rfc3339(value: SystemTime) -> String {
    DateTime::<Utc>::from(value).to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretStoreError {
    Missing,
    InvalidValue,
    Read,
    Write,
    KindMismatch {
        expected: ClaudeSecretKind,
        actual: ClaudeSecretKind,
    },
}

impl fmt::Display for SecretStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => formatter.write_str("Claude credential is missing"),
            Self::InvalidValue => formatter.write_str("Claude credential value is invalid"),
            Self::Read => formatter.write_str("Claude credential could not be read"),
            Self::Write => formatter.write_str("Claude credential could not be written"),
            Self::KindMismatch { expected, actual } => write!(
                formatter,
                "Claude credential handle kind mismatch: expected {expected:?}, got {actual:?}"
            ),
        }
    }
}

impl std::error::Error for SecretStoreError {}

/// OAuth token information returned by Anthropic.
///
/// ref: internal/auth/claude/anthropic.go:13-24
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeTokenData {
    access_token: SecretString,
    refresh_token: SecretString,
    email: String,
    account_uuid: String,
    organization_uuid: String,
    organization_name: String,
    expires_at: SystemTime,
}

impl ClaudeTokenData {
    pub fn new(
        access_token: SecretString,
        refresh_token: SecretString,
        email: impl Into<String>,
        expires_at: SystemTime,
    ) -> Self {
        Self {
            access_token,
            refresh_token,
            email: email.into(),
            account_uuid: String::new(),
            organization_uuid: String::new(),
            organization_name: String::new(),
            expires_at,
        }
    }

    pub fn from_expires_in(
        access_token: SecretString,
        refresh_token: SecretString,
        email: impl Into<String>,
        issued_at: SystemTime,
        expires_in: Duration,
    ) -> Result<Self, TokenError> {
        let expires_at = issued_at
            .checked_add(expires_in)
            .ok_or(TokenError::ExpiryOverflow)?;
        Ok(Self::new(access_token, refresh_token, email, expires_at))
    }

    pub fn access_token(&self) -> &SecretString {
        &self.access_token
    }

    pub fn refresh_token(&self) -> &SecretString {
        &self.refresh_token
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn account_uuid(&self) -> &str {
        &self.account_uuid
    }

    pub fn organization_uuid(&self) -> &str {
        &self.organization_uuid
    }

    pub fn organization_name(&self) -> &str {
        &self.organization_name
    }

    pub fn with_identity(
        mut self,
        account_uuid: impl Into<String>,
        organization_uuid: impl Into<String>,
        organization_name: impl Into<String>,
    ) -> Self {
        self.account_uuid = account_uuid.into();
        self.organization_uuid = organization_uuid.into();
        self.organization_name = organization_name.into();
        self
    }

    pub(crate) fn set_identity_if_present(
        &mut self,
        account_uuid: &str,
        organization_uuid: &str,
        organization_name: &str,
    ) {
        if !account_uuid.trim().is_empty() {
            self.account_uuid = account_uuid.to_owned();
        }
        if !organization_uuid.trim().is_empty() {
            self.organization_uuid = organization_uuid.to_owned();
        }
        if !organization_name.trim().is_empty() {
            self.organization_name = organization_name.to_owned();
        }
    }

    pub(crate) fn set_email(&mut self, email: &str) {
        if !email.trim().is_empty() {
            self.email = email.to_owned();
        }
    }

    pub fn expires_at(&self) -> SystemTime {
        self.expires_at
    }

    pub fn is_expired_at(&self, now: SystemTime) -> bool {
        now >= self.expires_at
    }

    pub fn refresh_due_at(&self, now: SystemTime) -> bool {
        match self.expires_at.duration_since(now) {
            Ok(remaining) => remaining <= CLAUDE_REFRESH_LEAD,
            Err(_) => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    EmptySecret,
    EmptyHandleField(&'static str),
    ExpiryOverflow,
}

impl fmt::Display for TokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySecret => formatter.write_str("secret must not be empty"),
            Self::EmptyHandleField(field) => write!(formatter, "secret handle {field} is empty"),
            Self::ExpiryOverflow => formatter.write_str("token expiry exceeds SystemTime range"),
        }
    }
}

impl std::error::Error for TokenError {}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn token(issued_at: SystemTime, expires_in: Duration) -> ClaudeTokenData {
        ClaudeTokenData::from_expires_in(
            SecretString::new("access-do-not-leak").unwrap(),
            SecretString::new("refresh-do-not-leak").unwrap(),
            "operator@example.com",
            issued_at,
            expires_in,
        )
        .unwrap()
    }

    #[test]
    fn secrets_are_redacted_in_debug_and_display() {
        let secret = SecretString::new("top-secret-value").unwrap();
        assert_eq!(secret.to_string(), "[REDACTED]");
        assert_eq!(format!("{secret:?}"), "SecretString([REDACTED])");

        let data = token(SystemTime::UNIX_EPOCH, Duration::from_secs(3600));
        let debug = format!("{data:?}");
        assert!(!debug.contains("access-do-not-leak"));
        assert!(!debug.contains("refresh-do-not-leak"));
    }

    #[test]
    fn empty_secrets_and_handles_are_rejected() {
        assert_eq!(SecretString::new(""), Err(TokenError::EmptySecret));
        assert_eq!(
            ClaudeSecretHandle::new("", "refresh", ClaudeSecretKind::RefreshToken),
            Err(TokenError::EmptyHandleField("scope"))
        );
        assert_eq!(
            ClaudeSecretHandle::new("credentials", " ", ClaudeSecretKind::RefreshToken),
            Err(TokenError::EmptyHandleField("name"))
        );
    }

    #[test]
    fn refresh_is_due_inside_the_four_hour_lead() {
        let issued_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let data = token(issued_at, Duration::from_secs(8 * 60 * 60));

        assert!(!data.refresh_due_at(issued_at));
        assert!(data.refresh_due_at(issued_at + Duration::from_secs(4 * 60 * 60)));
        assert!(data.refresh_due_at(issued_at + Duration::from_secs(9 * 60 * 60)));
        assert!(data.is_expired_at(issued_at + Duration::from_secs(9 * 60 * 60)));
    }

    #[test]
    fn credential_handle_pair_rejects_swapped_kinds() {
        let access = ClaudeSecretHandle::new(
            "provider-subscriptions",
            "claude-access",
            ClaudeSecretKind::AccessToken,
        )
        .unwrap();
        let refresh = ClaudeSecretHandle::new(
            "provider-subscriptions",
            "claude-refresh",
            ClaudeSecretKind::RefreshToken,
        )
        .unwrap();
        let pair = ClaudeCredentialHandles::new(access.clone(), refresh.clone()).unwrap();
        assert_eq!(pair.access_token(), &access);
        assert_eq!(pair.refresh_token(), &refresh);

        assert_eq!(
            ClaudeCredentialHandles::new(refresh, access),
            Err(SecretStoreError::KindMismatch {
                expected: ClaudeSecretKind::AccessToken,
                actual: ClaudeSecretKind::RefreshToken,
            })
        );
    }

    #[derive(Default)]
    struct RecordingSecretStore {
        stored: Mutex<Option<ClaudeStoredCredentials>>,
    }

    impl ClaudeSecretStore for RecordingSecretStore {
        fn load_credentials(
            &self,
            _handles: &ClaudeCredentialHandles,
        ) -> Result<ClaudeStoredCredentials, SecretStoreError> {
            self.stored
                .lock()
                .unwrap()
                .clone()
                .ok_or(SecretStoreError::Missing)
        }

        fn store_credentials(
            &self,
            _handles: &ClaudeCredentialHandles,
            credentials: &ClaudeStoredCredentials,
        ) -> Result<(), SecretStoreError> {
            *self.stored.lock().unwrap() = Some(credentials.clone());
            Ok(())
        }
    }

    fn handles() -> ClaudeCredentialHandles {
        ClaudeCredentialHandles::new(
            ClaudeSecretHandle::new(
                "provider-subscriptions",
                "claude-access",
                ClaudeSecretKind::AccessToken,
            )
            .unwrap(),
            ClaudeSecretHandle::new(
                "provider-subscriptions",
                "claude-refresh",
                ClaudeSecretKind::RefreshToken,
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn storage_persists_rotating_secrets_only_through_typed_store() {
        let issued_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_704_067_200);
        let storage = ClaudeTokenStorage::from_token_data(
            &token(issued_at, Duration::from_secs(3_600)),
            issued_at,
            None,
        );
        let store = RecordingSecretStore::default();
        let handles = handles();

        storage.persist_credentials(&store, &handles).unwrap();
        let persisted = store.load_credentials(&handles).unwrap();
        assert_eq!(
            persisted.access_token().expose_secret(),
            "access-do-not-leak"
        );
        assert_eq!(
            persisted.refresh_token().expose_secret(),
            "refresh-do-not-leak"
        );
    }

    #[test]
    fn adversarial_metadata_cannot_override_or_render_credentials() {
        let issued_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_704_067_200);
        let mut storage = ClaudeTokenStorage::from_token_data(
            &token(issued_at, Duration::from_secs(3_600)),
            issued_at,
            None,
        );
        storage.set_metadata(BTreeMap::from([
            (
                "access_token".to_owned(),
                Value::String("metadata-injection".to_owned()),
            ),
            (
                "hook_secret".to_owned(),
                Value::String("metadata-do-not-log".to_owned()),
            ),
        ]));

        assert_eq!(
            storage.credentials().access_token().expose_secret(),
            "access-do-not-leak"
        );
        assert_eq!(storage.metadata()["access_token"], "metadata-injection");
        let debug = format!("{storage:?}");
        assert!(!debug.contains("metadata-injection"));
        assert!(!debug.contains("metadata-do-not-log"));
    }

    #[test]
    fn token_storage_update_rotates_pair_and_keeps_identity_metadata() {
        let issued_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_704_067_200);
        let mut storage = ClaudeTokenStorage::from_token_data(
            &token(issued_at, Duration::from_secs(3_600)),
            issued_at,
            Some(SecretString::new("stable-id").unwrap()),
        );
        storage.set_metadata(BTreeMap::from([(
            "disabled".to_owned(),
            Value::Bool(false),
        )]));
        let refreshed_at = issued_at + Duration::from_secs(600);
        let refreshed = ClaudeTokenData::new(
            SecretString::new("next-access").unwrap(),
            SecretString::new("next-refresh").unwrap(),
            "next@example.com",
            refreshed_at + Duration::from_secs(7_200),
        );

        storage.update_from_token_data(&refreshed, refreshed_at);

        assert_eq!(
            storage.credentials().access_token().expose_secret(),
            "next-access"
        );
        assert_eq!(
            storage.credentials().refresh_token().expose_secret(),
            "next-refresh"
        );
        assert_eq!(storage.id_token().unwrap().expose_secret(), "stable-id");
        assert_eq!(storage.email(), "next@example.com");
        assert_eq!(storage.metadata()["disabled"], false);
        assert_eq!(storage.last_refresh(), "2024-01-01T00:10:00Z");
        assert_eq!(storage.expired(), "2024-01-01T02:10:00Z");
    }
}
