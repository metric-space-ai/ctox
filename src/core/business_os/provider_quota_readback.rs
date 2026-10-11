// Origin: CTOX; License: AGPL-3.0-only
//! Quotas enrich the existing authorized read result, never the persisted
//! registry projection or an execution permit. Private IO stays outside Policy.
use super::*;
use std::collections::BTreeMap;

#[derive(PartialEq, Eq)]
struct Target {
    id: String,
    local: String,
    binding: String,
    revision: i64,
}

fn targets(conn: &Connection, owner: &str, holder: &str) -> Result<Vec<Target>> {
    let mut query = conn.prepare(
        "SELECT a.account_id,a.private_local_account_id,b.fingerprint,a.revision
         FROM business_provider_federation_accounts a
         JOIN business_provider_federation_native_bindings b ON b.account_id=a.account_id
         WHERE a.owner_user_id=?1 AND a.holder_instance_id=?2 AND a.provider='claude'
           AND a.private_local_account_id<>?3 ORDER BY a.account_id LIMIT ?4",
    )?;
    let result = query
        .query_map(
            params![
                owner,
                holder,
                INHERITED_NATIVE_ACCOUNT_ID,
                MAX_ACCOUNTS as i64 + 1
            ],
            |row| {
                Ok(Target {
                    id: row.get(0)?,
                    local: row.get(1)?,
                    binding: row.get(2)?,
                    revision: row.get(3)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(result.len() <= MAX_ACCOUNTS, "quota account limit exceeded");
    Ok(result)
}

fn publish(
    root: &Path,
    actor: &str,
    owner: &str,
    holder: Option<&str>,
    captured: &[Target],
    values: &BTreeMap<String, Value>,
) -> Result<Value> {
    let mut conn = store::open_store(root)?;
    let snapshot = conn.transaction()?;
    ensure!(
        management_owner(&snapshot, actor)? == owner,
        "quota reader authority changed"
    );
    let current = match holder {
        Some(holder) if store::existing_instance_id(root).ok().as_deref() == Some(holder) => {
            targets(&snapshot, owner, holder)?
        }
        _ => vec![],
    };
    let mut result = list(&snapshot, owner)?;
    for account in result["accounts"]
        .as_array_mut()
        .context("account metadata is missing")?
    {
        let observed = current
            .iter()
            .find(|target| account["id"].as_str() == Some(target.id.as_str()))
            .filter(|target| captured.contains(target))
            .and_then(|target| values.get(&target.local));
        account["reportedQuota"] = observed
            .cloned()
            .unwrap_or_else(|| json!({"state":"unknown"}));
    }
    Ok(result)
}

pub(super) fn read(root: &Path, actor: &str) -> Result<Value> {
    let (owner, holder, captured) = {
        let mut conn = store::open_store(root)?;
        let snapshot = conn.transaction()?;
        let owner = management_owner(&snapshot, actor)?;
        // An older/remote holder can still supply account metadata. Missing
        // local identity never permits guessing its selector or quota.
        let holder = store::existing_instance_id(root).ok();
        let captured = holder
            .as_deref()
            .map(|h| targets(&snapshot, &owner, h))
            .transpose()?
            .unwrap_or_default();
        (owner, holder, captured)
    };
    let private = captured
        .iter()
        .map(|target| (target.local.as_str(), target.binding.as_str()))
        .collect::<Vec<_>>();
    crate::execution::cliproxyapi_host::with_reported_claude_quotas(root, &private, |values| {
        publish(root, actor, &owner, holder.as_deref(), &captured, values)
    })
}

#[cfg(test)]
#[path = "provider_quota_readback_tests.rs"]
mod tests;
