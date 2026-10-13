//! Connected-account persistence on top of an encrypted secret backend.
//!
//! Layout in the CTOX secret store (`src/core/secrets.rs`):
//! - scope `toolhub:<toolkit>:<account_id>`, name `token`:   value = AccountSecret JSON,
//!   metadata = ConnectedAccount JSON (status, scopes, expires_at, next_refresh_after).
//! - same scope, name `pending`: value = PendingAuthorization JSON (state + PKCE verifier).
//! - scope `toolhub-config:<toolkit>`, name `<auth_config_id>`: value = OAuth client secret
//!   (may be empty), metadata = AuthConfig JSON.
use std::collections::BTreeMap;
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{AccountSecret, AuthConfig, ConnectedAccount};

pub const ACCOUNT_SCOPE_PREFIX: &str = "toolhub:";
pub const CONFIG_SCOPE_PREFIX: &str = "toolhub-config:";
const TOKEN_NAME: &str = "token";
const PENDING_NAME: &str = "pending";

#[derive(Debug, Clone, PartialEq)]
pub struct SecretEntry {
    pub scope: String,
    pub name: String,
    pub metadata: Value,
}

/// Abstraction over `secrets.rs` so the crate stays testable without the host.
pub trait SecretBackend: Send + Sync {
    fn put(&self, scope: &str, name: &str, value: &str, metadata: Value) -> Result<()>;
    fn get(&self, scope: &str, name: &str) -> Result<Option<String>>;
    fn delete(&self, scope: &str, name: &str) -> Result<()>;
    /// Metadata only; must never decrypt values.
    fn list(&self) -> Result<Vec<SecretEntry>>;
}

/// In-memory backend for tests and dry runs.
#[derive(Default)]
pub struct MemorySecretBackend {
    rows: Mutex<BTreeMap<(String, String), (String, Value)>>,
}

impl SecretBackend for MemorySecretBackend {
    fn put(&self, scope: &str, name: &str, value: &str, metadata: Value) -> Result<()> {
        self.rows
            .lock()
            .unwrap()
            .insert((scope.into(), name.into()), (value.into(), metadata));
        Ok(())
    }
    fn get(&self, scope: &str, name: &str) -> Result<Option<String>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .get(&(scope.into(), name.into()))
            .map(|(v, _)| v.clone()))
    }
    fn delete(&self, scope: &str, name: &str) -> Result<()> {
        self.rows
            .lock()
            .unwrap()
            .remove(&(scope.into(), name.into()));
        Ok(())
    }
    fn list(&self) -> Result<Vec<SecretEntry>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .map(|((scope, name), (_, metadata))| SecretEntry {
                scope: scope.clone(),
                name: name.clone(),
                metadata: metadata.clone(),
            })
            .collect())
    }
}

/// Interactive authorization in progress (secret: contains the PKCE verifier).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingAuthorization {
    pub state: String,
    #[serde(default)]
    pub code_verifier: Option<String>,
    pub redirect_uri: String,
    pub created_at: u64,
}

impl std::fmt::Debug for PendingAuthorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingAuthorization")
            .field("redirect_uri", &self.redirect_uri)
            .finish_non_exhaustive()
    }
}

pub fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

pub fn account_scope(toolkit: &str, account_id: &str) -> Result<String> {
    if !valid_segment(toolkit) || !valid_segment(account_id) {
        bail!("invalid toolkit or account identifier");
    }
    Ok(format!("{ACCOUNT_SCOPE_PREFIX}{toolkit}:{account_id}"))
}

fn config_scope(toolkit: &str) -> Result<String> {
    if !valid_segment(toolkit) {
        bail!("invalid toolkit identifier");
    }
    Ok(format!("{CONFIG_SCOPE_PREFIX}{toolkit}"))
}

pub struct AccountStore<B: SecretBackend> {
    backend: B,
}

impl<B: SecretBackend> AccountStore<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Atomic from the caller's view: token value and metadata live in one record.
    pub fn save(&self, account: &ConnectedAccount, secret: Option<&AccountSecret>) -> Result<()> {
        let scope = account_scope(&account.toolkit, &account.id)?;
        let value = match secret {
            Some(s) => serde_json::to_string(s)?,
            None => self.backend.get(&scope, TOKEN_NAME)?.unwrap_or_default(),
        };
        self.backend
            .put(&scope, TOKEN_NAME, &value, serde_json::to_value(account)?)
    }

    pub fn list(&self) -> Result<Vec<ConnectedAccount>> {
        let mut out = Vec::new();
        for entry in self.backend.list()? {
            if entry.scope.starts_with(ACCOUNT_SCOPE_PREFIX) && entry.name == TOKEN_NAME {
                if let Ok(account) = serde_json::from_value::<ConnectedAccount>(entry.metadata) {
                    out.push(account);
                }
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    pub fn get(&self, account_id: &str) -> Result<Option<ConnectedAccount>> {
        Ok(self.list()?.into_iter().find(|a| a.id == account_id))
    }

    pub fn secret(&self, account: &ConnectedAccount) -> Result<Option<AccountSecret>> {
        let scope = account_scope(&account.toolkit, &account.id)?;
        match self.backend.get(&scope, TOKEN_NAME)? {
            Some(v) if !v.is_empty() => Ok(Some(
                serde_json::from_str(&v).context("corrupt account secret")?,
            )),
            _ => Ok(None),
        }
    }

    pub fn delete(&self, account: &ConnectedAccount) -> Result<()> {
        let scope = account_scope(&account.toolkit, &account.id)?;
        self.backend.delete(&scope, PENDING_NAME)?;
        self.backend.delete(&scope, TOKEN_NAME)
    }

    pub fn put_pending(
        &self,
        account: &ConnectedAccount,
        pending: &PendingAuthorization,
    ) -> Result<()> {
        let scope = account_scope(&account.toolkit, &account.id)?;
        self.backend.put(
            &scope,
            PENDING_NAME,
            &serde_json::to_string(pending)?,
            Value::Null,
        )
    }

    pub fn take_pending(&self, account: &ConnectedAccount) -> Result<Option<PendingAuthorization>> {
        let scope = account_scope(&account.toolkit, &account.id)?;
        let value = self.backend.get(&scope, PENDING_NAME)?;
        self.backend.delete(&scope, PENDING_NAME)?;
        value
            .map(|v| serde_json::from_str(&v).context("corrupt pending authorization"))
            .transpose()
    }

    pub fn save_auth_config(&self, config: &AuthConfig, client_secret: Option<&str>) -> Result<()> {
        if !valid_segment(&config.id) {
            bail!("invalid auth config identifier");
        }
        let scope = config_scope(&config.toolkit)?;
        let value = match client_secret {
            Some(s) => s.to_string(),
            None => self.backend.get(&scope, &config.id)?.unwrap_or_default(),
        };
        self.backend
            .put(&scope, &config.id, &value, serde_json::to_value(config)?)
    }

    pub fn auth_configs(&self) -> Result<Vec<AuthConfig>> {
        Ok(self
            .backend
            .list()?
            .into_iter()
            .filter(|e| e.scope.starts_with(CONFIG_SCOPE_PREFIX))
            .filter_map(|e| serde_json::from_value(e.metadata).ok())
            .collect())
    }

    pub fn auth_config(&self, id: &str) -> Result<Option<AuthConfig>> {
        Ok(self.auth_configs()?.into_iter().find(|c| c.id == id))
    }

    pub fn client_secret(&self, config: &AuthConfig) -> Result<Option<String>> {
        let scope = config_scope(&config.toolkit)?;
        Ok(self
            .backend
            .get(&scope, &config.id)?
            .filter(|s| !s.is_empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn account(id: &str) -> ConnectedAccount {
        ConnectedAccount {
            id: id.into(),
            toolkit: "github".into(),
            auth_config_id: "gh-default".into(),
            owner: "michael".into(),
            status: ConnectionStatus::Active,
            scopes: vec!["repo".into()],
            expires_at: Some(1_000),
            next_refresh_after: Some(900),
            status_reason: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn roundtrip_keeps_secret_out_of_metadata() {
        let store = AccountStore::new(MemorySecretBackend::default());
        let secret = AccountSecret {
            access_token: "tok-123".into(),
            refresh_token: Some("ref-9".into()),
            token_type: None,
        };
        store.save(&account("a1"), Some(&secret)).unwrap();
        let listed = store.backend().list().unwrap();
        assert_eq!(listed[0].scope, "toolhub:github:a1");
        assert!(!listed[0].metadata.to_string().contains("tok-123"));
        assert_eq!(listed[0].metadata["expires_at"], 1_000);
        assert_eq!(store.list().unwrap(), vec![account("a1")]);
        assert_eq!(store.secret(&account("a1")).unwrap(), Some(secret.clone()));
        // Metadata-only update keeps the token.
        let mut expired = account("a1");
        expired.status = ConnectionStatus::Expired;
        store.save(&expired, None).unwrap();
        assert_eq!(store.secret(&expired).unwrap(), Some(secret));
        assert_eq!(
            store.get("a1").unwrap().unwrap().status,
            ConnectionStatus::Expired
        );
        store.delete(&expired).unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn pending_is_single_use_and_ids_are_validated() {
        let store = AccountStore::new(MemorySecretBackend::default());
        let p = PendingAuthorization {
            state: "s".into(),
            code_verifier: Some("v".into()),
            redirect_uri: "r".into(),
            created_at: 0,
        };
        store.put_pending(&account("a2"), &p).unwrap();
        assert_eq!(store.take_pending(&account("a2")).unwrap(), Some(p));
        assert_eq!(store.take_pending(&account("a2")).unwrap(), None);
        assert!(account_scope("git:hub", "x").is_err());
        assert!(account_scope("github", "../x").is_err());
    }

    #[test]
    fn auth_config_secret_is_separate_from_metadata() {
        let store = AccountStore::new(MemorySecretBackend::default());
        let cfg = AuthConfig {
            id: "gh-oauth".into(),
            toolkit: "github".into(),
            scheme: AuthSchemeKind::OAuth2,
            source: ClientCredentialSource::Custom,
            client_id: Some("cid".into()),
            scopes: vec![],
            enabled: true,
        };
        store.save_auth_config(&cfg, Some("csecret")).unwrap();
        assert_eq!(store.auth_config("gh-oauth").unwrap(), Some(cfg.clone()));
        assert_eq!(
            store.client_secret(&cfg).unwrap().as_deref(),
            Some("csecret")
        );
        assert!(!store.backend().list().unwrap()[0]
            .metadata
            .to_string()
            .contains("csecret"));
    }
}
