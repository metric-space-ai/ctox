// Origin: CTOX
// License: AGPL-3.0-only
//! Private control binding for explicitly owned subscription accounts. The
//! inherited ChatGPT auth snapshot and aliased credential refs are read-only.
use super::*;
use ctox_cliproxyapi::internal::auth::codex::CodexSecretStore;
use sha2::{Digest, Sha256};

#[derive(PartialEq, Eq)]
pub(super) enum Captured {
    Claude(super::super::super::cliproxyapi_claude_catalog::Captured),
    Codex {
        account: CodexSubscriptionAccountConfig,
        credentials: codex_auth::CodexStoredCredentials,
        versions: Vec<String>,
    },
}

pub(super) fn configured(runtime: &CliproxyRuntimeConfig, provider: &str, id: &str) -> bool {
    match provider {
        "claude" => runtime.claude_accounts.iter().any(|a| a.id == id),
        "codex" => runtime.codex_accounts.iter().any(|a| a.id == id),
        _ => false,
    }
}

fn refs(account: &CodexSubscriptionAccountConfig) -> Vec<RuntimeSecretRef> {
    let mut refs = vec![account.id_token_secret.clone(), account.access_token_secret.clone(), account.refresh_token_secret.clone()];
    refs.extend(account.proxy_url_secret.clone());
    refs
}

pub(super) fn capture(root: &Path, provider: &str, id: &str) -> anyhow::Result<Option<Captured>> {
    if provider == "claude" {
        return super::super::super::cliproxyapi_claude_catalog::capture(root, id).map(|a| a.map(Captured::Claude));
    }
    if provider != "codex" || id == INSTANCE_CODEX_ACCOUNT_ID {
        return Ok(None);
    }
    let Some(stored) = load_instance_proxy_config(root)? else { return Ok(None); };
    let Some(account) = stored.runtime.codex_accounts.iter().find(|a| a.id == id).cloned() else { return Ok(None); };
    let keys = refs(&account);
    let mut other = stored.runtime;
    other.codex_accounts.retain(|a| a.id != id);
    let remaining = runtime_secret_keys(&other);
    let mut unique = std::collections::BTreeSet::new();
    for key in &keys {
        let selector = (key.scope.clone(), key.name.clone());
        if key.scope != INSTANCE_CODEX_SECRET_SCOPE || remaining.contains(&selector) || !unique.insert(selector) {
            return Ok(None);
        }
    }
    let handles = account.credential_handles().map_err(|_| anyhow::anyhow!("provider credential references unavailable"))?;
    let version = || -> anyhow::Result<Option<Vec<String>>> {
        keys.iter().map(|key| crate::secrets::secret_record_content_version(root, &key.scope, &key.name)).collect()
    };
    let Some(versions) = version()? else { return Ok(None); };
    let credentials = match CtoxCodexSecretStore::new(root).load_credentials(&handles) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    if version()?.as_ref() != Some(&versions) { return Ok(None); }
    Ok(Some(Captured::Codex { account, credentials, versions }))
}

impl Captured {
    pub(super) fn fingerprint(&self) -> anyhow::Result<String> {
        match self {
            Self::Claude(value) => super::super::super::cliproxyapi_claude_catalog::fingerprint(value),
            Self::Codex { account, credentials, versions } => {
                let mut digest = Sha256::new();
                digest.update(b"ctox/native-codex-control-binding/v1");
                let config = serde_json::to_vec(&(account, versions))?;
                digest.update((config.len() as u64).to_be_bytes()); digest.update(config);
                for secret in [credentials.id_token(), credentials.access_token(), credentials.refresh_token()] {
                    let value = secret.expose_secret();
                    digest.update((value.len() as u64).to_be_bytes()); digest.update(value.as_bytes());
                }
                Ok(format!("{:x}", digest.finalize()))
            }
        }
    }
    pub(super) fn check_config(&self, runtime: &CliproxyRuntimeConfig) -> anyhow::Result<()> {
        let same = match self {
            Self::Claude(value) => runtime.claude_accounts.iter().find(|a| a.id == value.account.id) == Some(&value.account),
            Self::Codex { account, .. } => runtime.codex_accounts.iter().find(|a| a.id == account.id) == Some(account),
        };
        anyhow::ensure!(same, "native account configuration changed"); Ok(())
    }
    pub(super) fn remove(&self, runtime: &mut CliproxyRuntimeConfig) -> Vec<RuntimeSecretRef> {
        match self {
            Self::Claude(value) => {
                runtime.claude_accounts.retain(|a| a.id != value.account.id);
                let mut keys = vec![value.account.access_token_secret.clone(), value.account.refresh_token_secret.clone()];
                keys.extend(value.account.proxy_url_secret.clone()); keys
            }
            Self::Codex { account, .. } => { runtime.codex_accounts.retain(|a| a.id != account.id); refs(account) }
        }
    }
    pub(super) fn set_enabled(&mut self, runtime: &mut CliproxyRuntimeConfig, enabled: bool) {
        match self {
            Self::Claude(value) => {
                value.account.disabled = !enabled;
                *runtime.claude_accounts.iter_mut().find(|a| a.id == value.account.id).unwrap() = value.account.clone();
            }
            Self::Codex { account, .. } => {
                account.disabled = !enabled;
                *runtime.codex_accounts.iter_mut().find(|a| a.id == account.id).unwrap() = account.clone();
            }
        }
    }
}
