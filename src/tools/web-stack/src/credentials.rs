//! Request-borrowed access to credentials owned by the native host.
//! This crate never opens the host secret store or launches a credential CLI.
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialReference {
    pub scope: String,
    pub name: String,
}

/// Deliberately neither serializable nor clonable. Only an API adapter should
/// expose the value, immediately before constructing its authorization header.
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

/// Safe classifications only: never wrap arbitrary store/provider error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialResolveError {
    Denied,
    Unavailable,
    DecryptionFailed,
    InvalidEncoding,
}

impl fmt::Display for CredentialResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Denied => "credential access denied",
            Self::Unavailable => "credential resolver unavailable",
            Self::DecryptionFailed => "credential decryption failed",
            Self::InvalidEncoding => "credential encoding invalid",
        })
    }
}
impl std::error::Error for CredentialResolveError {}

pub trait CredentialResolver: Send + Sync {
    /// `None` means the exact authorized reference has no stored row.
    fn resolve(
        &self,
        reference: &CredentialReference,
    ) -> Result<Option<SecretValue>, CredentialResolveError>;
}

/// Adapters must use this distinction; no resolver is never a missing record.
pub fn resolve_credential(
    resolver: Option<&dyn CredentialResolver>,
    reference: &CredentialReference,
) -> Result<Option<SecretValue>, CredentialResolveError> {
    resolver
        .ok_or(CredentialResolveError::Unavailable)?
        .resolve(reference)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secret_debug_and_missing_resolver_are_safe() {
        let value = SecretValue::new("fixture-canary-not-a-real-key".into());
        assert!(!format!("{value:?}").contains(value.expose_secret()));
        let reference = CredentialReference {
            scope: "credentials".into(),
            name: "fixture".into(),
        };
        assert_eq!(
            resolve_credential(None, &reference).unwrap_err(),
            CredentialResolveError::Unavailable
        );
    }
}
