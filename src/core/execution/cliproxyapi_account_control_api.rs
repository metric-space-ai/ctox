// Origin: CTOX
// License: AGPL-3.0-only
//! API-account effects share the existing Runtime journal. No Policy connection
//! is retained across configuration/credential IO and recovery consumes proof only.
use super::*;
use crate::execution::cliproxyapi_integration::{
    load_provider_integration_config, open_integration_config_db, ApiKeyAccountConfig,
    PROVIDER_INTEGRATION_CONFIG_TABLE,
};
use sha2::{Digest, Sha256};
pub(super) fn supports(provider: &str) -> bool {
    matches!(provider, "zai" | "minimax" | "xai" | "kimi")
}
pub(super) fn fingerprint(
    account: &ApiKeyAccountConfig,
    version: Option<&str>,
) -> anyhow::Result<String> {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(account)?);
    digest.update([0]);
    digest.update(version.unwrap_or("missing-secret-record").as_bytes());
    Ok(format!("api-key:v1:{:x}", digest.finalize()))
}
pub(super) fn binding(root: &Path, provider: &str, id: &str) -> anyhow::Result<Option<String>> {
    let config = load_provider_integration_config(root)?;
    let Some(account) = config.config.api_key_accounts.iter().find(|a| {
        a.id == id
            && a.endpoint.provider() == provider
            && a.owner_user_id.is_some()
            && a.creation_operation.is_some()
    }) else {
        return Ok(None);
    };
    account.validate()?;
    let version = crate::secrets::secret_record_content_version(
        root,
        &account.api_key_secret.scope,
        &account.api_key_secret.name,
    )?;
    let result = fingerprint(account, version.as_deref())?;
    anyhow::ensure!(
        load_provider_integration_config(root)?
            .config
            .api_key_accounts
            .iter()
            .any(|a| a == account)
            && crate::secrets::secret_record_content_version(
                root,
                &account.api_key_secret.scope,
                &account.api_key_secret.name
            )? == version,
        "API account changed during capture"
    );
    Ok(Some(result))
}
fn unreferenced(
    root: &Path,
    config: &crate::execution::cliproxyapi_integration::ProviderIntegrationConfig,
    scope: &str,
    name: &str,
) -> anyhow::Result<bool> {
    let same = |s: &str, n: &str| s == scope && n == name;
    if config
        .api_key_accounts
        .iter()
        .any(|a| same(&a.api_key_secret.scope, &a.api_key_secret.name))
        || config.kimi_subscription_accounts.iter().any(|a| {
            same(&a.access_token_secret.scope, &a.access_token_secret.name)
                || a.refresh_token_secret
                    .as_ref()
                    .is_some_and(|r| same(&r.scope, &r.name))
                || same(&a.state_secret.scope, &a.state_secret.name)
        })
    {
        return Ok(false);
    }
    Ok(!load_instance_proxy_config(root)?.is_some_and(|runtime| {
        runtime_secret_keys(&runtime.runtime)
            .iter()
            .any(|(s, n)| same(s, n))
    }))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn apply(
    root: &Path,
    id: &str,
    hash: &str,
    actor: &str,
    provider: &str,
    local: &str,
    expected: &str,
    enabled: Option<bool>,
    validate: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let _guard = crate::secrets::credential_lifecycle_guard();
    let mut stored = load_provider_integration_config(root)?;
    let index = stored
        .config
        .api_key_accounts
        .iter()
        .position(|a| {
            a.id == local
                && a.endpoint.provider() == provider
                && a.owner_user_id.is_some()
                && a.creation_operation.is_some()
        })
        .context("owned API account unavailable")?;
    let account = stored.config.api_key_accounts[index].clone();
    anyhow::ensure!(
        binding(root, provider, local)?.as_deref() == Some(expected),
        "API account generation changed"
    );
    let version = crate::secrets::secret_record_content_version(
        root,
        &account.api_key_secret.scope,
        &account.api_key_secret.name,
    )?;
    let mut keys = Vec::new();
    let mut absent = Vec::new();
    let after = if let Some(enabled) = enabled {
        stored.config.api_key_accounts[index].disabled = !enabled;
        Some(fingerprint(
            &stored.config.api_key_accounts[index],
            version.as_deref(),
        )?)
    } else {
        stored.config.api_key_accounts.remove(index);
        anyhow::ensure!(
            unreferenced(
                root,
                &stored.config,
                &account.api_key_secret.scope,
                &account.api_key_secret.name
            )?,
            "API credential reference is shared"
        );
        match &version {
            Some(v) => keys.push((
                account.api_key_secret.scope.clone(),
                account.api_key_secret.name.clone(),
                v.clone(),
            )),
            None => absent.push((
                account.api_key_secret.scope.clone(),
                account.api_key_secret.name.clone(),
            )),
        };
        None
    };
    stored.config.validate()?;
    let revision = stored
        .revision
        .checked_add(1)
        .context("API topology revision exhausted")?;
    let effect = Effect {
        command_id: id.into(),
        payload_hash: hash.into(),
        actor: actor.into(),
        local_id: local.into(),
        provider: provider.into(),
        enabled,
        before_binding: expected.into(),
        after_binding: after,
        config_revision: revision,
        keys,
        api_key: true,
        api_absent_keys: absent,
        configured: Some(version.is_some()),
    };
    let mut conn = open_integration_config_db(root)?;
    schema(&conn)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    anyhow::ensure!(
        read(&tx, id)?.is_none(),
        "holder effect already applied; recovery required"
    );
    let current: u64 = tx.query_row(
        &format!("SELECT revision FROM {PROVIDER_INTEGRATION_CONFIG_TABLE} WHERE config_id=1"),
        [],
        |r| r.get(0),
    )?;
    anyhow::ensure!(
        current == stored.revision
            && binding(root, provider, local)?.as_deref() == Some(expected)
            && crate::secrets::secret_record_content_version(
                root,
                &account.api_key_secret.scope,
                &account.api_key_secret.name
            )? == version,
        "API configuration or credential changed before COMMIT"
    );
    validate()?;
    tx.execute(&format!("UPDATE {PROVIDER_INTEGRATION_CONFIG_TABLE} SET revision=?1,config_json=?2,updated_at_ms=?3 WHERE config_id=1"),params![revision,serde_json::to_string(&stored.config)?,chrono::Utc::now().timestamp_millis()])?;
    tx.execute(
        &format!("INSERT INTO {TABLE}(command_id,effect_json) VALUES (?1,?2)"),
        params![id, serde_json::to_string(&effect)?],
    )?;
    tx.commit()?;
    Ok(())
}
pub(super) fn finish(root: &Path, effect: Effect) -> anyhow::Result<Effect> {
    anyhow::ensure!(
        supports(&effect.provider),
        "unsupported API effect provider"
    );
    anyhow::ensure!(
        binding(root, &effect.provider, &effect.local_id)? == effect.after_binding,
        "API account changed after COMMIT; reconciliation required"
    );
    let mut conn = open_integration_config_db(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let raw:(u64,String)=tx.query_row(&format!("SELECT revision,config_json FROM {PROVIDER_INTEGRATION_CONFIG_TABLE} WHERE config_id=1"),[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    anyhow::ensure!(
        raw.0 == effect.config_revision,
        "API topology changed after COMMIT; reconciliation required"
    );
    let config: crate::execution::cliproxyapi_integration::ProviderIntegrationConfig =
        serde_json::from_str(&raw.1)?;
    config.validate()?;
    if effect.enabled.is_none() {
        anyhow::ensure!(
            !config
                .api_key_accounts
                .iter()
                .any(|a| a.id == effect.local_id),
            "API account was reconfigured"
        );
        for (scope, name, _) in &effect.keys {
            anyhow::ensure!(
                unreferenced(root, &config, scope, name)?,
                "removed API credential is now shared"
            );
        }
        for (scope, name) in &effect.api_absent_keys {
            anyhow::ensure!(
                unreferenced(root, &config, scope, name)?
                    && crate::secrets::secret_record_content_version(root, scope, name)?.is_none(),
                "missing API credential was replaced; reconciliation required"
            );
        }
        crate::secrets::delete_secret_records_if_versions(root, &effect.keys)?;
    }
    tx.commit()?;
    Ok(effect)
}
