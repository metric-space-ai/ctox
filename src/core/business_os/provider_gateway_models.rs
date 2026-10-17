// Origin: CTOX
// License: AGPL-3.0-only
//! Canonical, holder-local model availability. This is not consumer execution
//! authority; the caller's existing gateway/consumer authorization remains required.
use super::*;
#[cfg(test)]
#[path = "provider_gateway_models_tests.rs"]
mod tests;
#[cfg(test)]
pub(crate) use tests::seed as seed_gateway_api_catalog;

use crate::execution::cliproxyapi_integration::ApiKeyAccountConfig;
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GatewayModels {
    pub(crate) models: Vec<String>,
    fingerprint: Vec<u8>,
}
impl GatewayModels {
    fn unavailable() -> Self {
        Self {
            models: Vec::new(),
            fingerprint: Vec::new(),
        }
    }
}
/// Read-only, no bootstrap/migration, network or credential IO. Legacy unowned
/// accounts keep their explicitly configured model list. Owned accounts require
/// an adopted current holder record and a received catalog for that generation.
pub(crate) fn read(
    root: &Path,
    account: &ApiKeyAccountConfig,
    binding: &str,
) -> Result<GatewayModels> {
    let Some(owner) = account.owner_user_id.as_deref() else {
        return Ok(GatewayModels {
            models: account.models.clone(),
            fingerprint: Vec::new(),
        });
    };
    let path = store::business_os_store_path(root);
    if !path.exists() {
        return Ok(GatewayModels::unavailable());
    }
    let mut conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    if management_owner(&tx, owner).is_err() {
        return Ok(GatewayModels::unavailable());
    }
    let holder = store::existing_instance_id(root)?;
    let row:Option<(String,i64)>=tx.query_row("SELECT a.account_id,a.revision FROM business_provider_federation_accounts a JOIN business_provider_federation_native_bindings b ON b.account_id=a.account_id WHERE a.owner_user_id=?1 AND a.holder_instance_id=?2 AND a.provider=?3 AND a.private_local_account_id=?4 AND a.enabled=1 AND a.credential_ready=1 AND b.fingerprint=?5",params![owner,holder,account.endpoint.provider(),account.id,binding],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((id, revision)) = row else {
        return Ok(GatewayModels::unavailable());
    };
    let catalog = catalog_projection(&tx, &id, revision, store::now_ms() as i64)?;
    if catalog["observed"] != true {
        return Ok(GatewayModels::unavailable());
    }
    let available = models::observed_models(&catalog)?;
    let selected = models::selection(&tx, owner, account.endpoint.provider())?;
    let excluded = models::exclusions(&tx, &id)?;
    let models = available
        .into_iter()
        .filter(|m| selected.as_ref().is_none_or(|s| s.contains(m)) && !excluded.contains(m))
        .collect();
    // Revisions and complete catalog witness retire remove/re-add races even
    // when the eventual list of model strings is byte-identical.
    let epoch: i64 = tx.query_row(
        "SELECT capability_epoch FROM business_users WHERE user_id=?1 AND active=1",
        [owner],
        |r| r.get(0),
    )?;
    let fingerprint = serde_json::to_vec(&(
        id,
        revision,
        policy_revision(&tx, owner)?,
        epoch,
        binding,
        catalog,
        selected,
        excluded,
    ))?;
    tx.commit()?;
    Ok(GatewayModels {
        models,
        fingerprint,
    })
}
