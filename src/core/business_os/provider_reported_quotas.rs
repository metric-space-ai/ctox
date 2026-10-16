// Origin: CTOX
// License: AGPL-3.0-only
//! Gateway-only quota observations are metadata, never an inference permit.
use super::*;
use crate::execution::cliproxyapi_xai::{self as xai, quota::Observation};

pub(super) fn retain(
    conn: &Connection,
    owner: &str,
    holder: &str,
    observation: &Observation,
) -> Result<()> {
    let (id, revision): (String, i64) = conn.query_row(
        "SELECT account_id,revision FROM business_provider_federation_accounts
         WHERE owner_user_id=?1 AND holder_instance_id=?2 AND provider='xai'
         AND private_local_account_id=?3",
        params![owner, holder, xai::ACCOUNT_ID],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        native_binding(conn, &id)? == observation.binding,
        "native quota credential binding changed"
    );
    conn.execute(
        "INSERT INTO business_provider_reported_quotas(account_id,account_revision,quota_json)
        VALUES(?1,?2,?3) ON CONFLICT(account_id) DO UPDATE SET
        account_revision=excluded.account_revision,quota_json=excluded.quota_json",
        params![id, revision, serde_json::to_string(&observation.quota)?],
    )?;
    Ok(())
}
fn public_quota(value: Value, now: i64) -> Value {
    match value["state"].as_str() {
        Some("unavailable") => {
            return json!({"state":"unavailable","errorClass":"quota_read_failed"})
        }
        Some("reported") => {}
        _ => return json!({"state":"unknown"}),
    }
    let period = value["period"].as_str().unwrap_or("");
    let percent = value["used_percent"].as_f64().unwrap_or(f64::NAN);
    let observed = value["observedAtMs"].as_i64().unwrap_or(0);
    let reset = value["resetAtMs"].as_i64().unwrap_or(0);
    if !matches!(period, "weekly" | "monthly")
        || !percent.is_finite()
        || !(0.0..=100.0).contains(&percent)
        || observed <= 0
        || observed > now
        || now.saturating_sub(observed) > 15 * 60_000
        || reset <= now
        || reset.saturating_sub(observed) > 40 * 86_400_000
    {
        return json!({"state":"unknown"});
    }
    json!({"state":"reported","period":period,"used_percent":percent,
        "observedAtMs":observed,"resetAtMs":reset})
}
pub(super) fn project(conn: &Connection, rows: &mut [Value], now: i64) -> Result<()> {
    for entry in rows.iter_mut().filter(|r| r["provider"] == "xai") {
        let raw: Option<String> = conn
            .query_row(
                "SELECT quota_json FROM business_provider_reported_quotas
             WHERE account_id=?1 AND account_revision=?2",
                params![entry["id"].as_str(), entry["revision"].as_i64()],
                |row| row.get(0),
            )
            .optional()?;
        entry["reportedQuota"] = public_quota(
            raw.and_then(|r| serde_json::from_str(&r).ok())
                .unwrap_or(Value::Null),
            now,
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grok_registry_reports_gateway_quota_and_invalidates_replaced_account() -> Result<()> {
        let f = super::super::tests::Fixture::new()?;
        crate::secrets::write_secret_record(
            f.root.path(),
            "provider-subscriptions",
            "xai-instance-oauth",
            r#"{"access":"fixture-access","refresh":null,"identity":null,"expires_at":null,"token_endpoint":"https://auth.x.ai/token"}"#,
            None,
            json!({"provider":"xai"}),
        )?;
        let metadata =
            crate::execution::cliproxyapi_host::provider_subscription_status(f.root.path());
        let mut account = observations(&metadata)?
            .into_iter()
            .find(|a| a.provider == "xai")
            .context("gateway omitted the installed Grok account")?;
        assert!(
            account.credential_ready,
            "installed Grok credentials must be adoptable"
        );
        account.private_binding = xai::credential_binding(f.root.path())?;
        f.adopt(std::slice::from_ref(&account))?;
        let now = store::now_ms() as i64;
        retain(
            &f.conn,
            "owner",
            "native-instance",
            &Observation {
                binding: account.private_binding.clone(),
                quota: json!({"state":"reported","period":"weekly",
            "used_percent":42.5,"observedAtMs":now,"resetAtMs":now+7*86_400_000,"private":"discard"}),
            },
        )?;
        let registry = list(&f.conn, "owner")?;
        assert_eq!(
            registry["accounts"][0]["reportedQuota"]["used_percent"],
            42.5
        );
        assert!(!registry.to_string().contains("discard"));
        let previous_binding = account.private_binding.clone();
        crate::secrets::write_secret_record(
            f.root.path(),
            "provider-subscriptions",
            "xai-instance-oauth",
            r#"{"access":"fixture-replacement","refresh":null,"identity":null,"expires_at":null,"token_endpoint":"https://auth.x.ai/token"}"#,
            None,
            json!({"provider":"xai"}),
        )?;
        account.private_binding = xai::credential_binding(f.root.path())?;
        let changed = f.adopt(&[account])?;
        assert_eq!(
            changed["accounts"][0]["reportedQuota"]["state"], "unknown",
            "replacement must invalidate the old gateway quota"
        );
        assert!(retain(
            &f.conn,
            "owner",
            "native-instance",
            &Observation {
                binding: previous_binding,
                quota: json!({"state":"reported"})
            }
        )
        .is_err());
        Ok(())
    }
}
