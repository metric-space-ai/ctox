// Origin: CTOX; License: AGPL-3.0-only
use super::super::tests::{command, Fixture};
use super::*;
use crate::execution::{cliproxyapi_claude_catalog as catalog, cliproxyapi_host as proxy};
use ctox_cliproxyapi::internal::auth::claude::{ClaudeStoredCredentials, SecretString};
use ctox_cliproxyapi::sdk::cliproxy::auth::{
    QuotaWindowKind, ReportedQuotaSnapshot, ReportedQuotaStatus, ReportedQuotaWindow,
};

const LOCAL: &str = "private-quota-readback-account";
fn install(f: &Fixture, token: &str) -> Result<()> {
    proxy::install_claude_subscription(
        f.root.path(),
        LOCAL,
        &ClaudeStoredCredentials::new(
            SecretString::new(token).unwrap(),
            SecretString::new(format!("refresh-{token}")).unwrap(),
        ),
    )?;
    Ok(())
}
fn fixture() -> Result<(Fixture, String, String)> {
    let f = Fixture::new()?;
    install(&f, "isolated-quota-readback")?;
    let holder = store::stable_instance_id(f.root.path())?;
    adopt(
        &f.conn,
        "owner",
        &holder,
        &[NativeAccountObservation {
            provider: "claude".into(),
            local_account_id: LOCAL.into(),
            enabled: true,
            credential_ready: true,
            private_binding: catalog::account_binding(f.root.path(), LOCAL)?,
        }],
        store::now_ms() as i64,
    )?;
    let id: String = f.conn.query_row("SELECT account_id FROM business_provider_federation_accounts WHERE private_local_account_id=?1",
        [LOCAL], |r|r.get(0))?;
    // The fixture writes reported metadata into the real isolated store; no
    // production account or network response is claimed by these unit tests.
    let now = store::now_ms() as i64;
    let quota = ReportedQuotaSnapshot {
        observed_at_ms: now,
        windows: vec![
            ReportedQuotaWindow {
                kind: QuotaWindowKind::FiveHour,
                status: ReportedQuotaStatus::Allowed,
                reset_at_ms: now + 3_600_000,
                utilization: Some(0.25),
            },
            ReportedQuotaWindow {
                kind: QuotaWindowKind::SevenDay,
                status: ReportedQuotaStatus::Allowed,
                reset_at_ms: now + 86_400_000,
                utilization: None,
            },
        ],
    };
    proxy::save_reported_quota_for_test(f.root.path(), LOCAL, &quota)?;
    Ok((f, holder, id))
}
fn quota<'a>(result: &'a Value, id: &str) -> &'a Value {
    &result["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap()["reportedQuota"]
}
#[test]
fn provider_quota_readback_returns_only_reported_public_fields_and_no_write() -> Result<()> {
    let (f, _, id) = fixture()?;
    let before = policy_revision(&f.conn, "owner")?;
    let result = handle_command(
        f.root.path(),
        &command("ctox.workjet.providers.list", json!({})),
        "owner",
        None,
    )?;
    assert_eq!(quota(&result, &id)["state"], "reported");
    assert_eq!(quota(&result, &id)["windows"][0]["utilization"], 0.25);
    assert!(quota(&result, &id)["windows"][1]["utilization"].is_null());
    assert_eq!(policy_revision(&f.conn, "owner")?, before);
    let encoded = result.to_string();
    for private in [
        LOCAL,
        "isolated-quota-readback",
        "refresh-",
        "fingerprint",
        "generation",
        "credentials",
    ] {
        assert!(!encoded.contains(private));
    }
    // Persisted Policy projections remain independent of private quota IO.
    assert!(list(&f.conn, "owner")?["accounts"][0]
        .get("reportedQuota")
        .is_none());
    Ok(())
}
#[test]
fn provider_quota_readback_relogin_cannot_attach_new_limits_to_old_binding() -> Result<()> {
    let (f, _, id) = fixture()?;
    install(&f, "replacement-quota-readback")?;
    let now = store::now_ms() as i64;
    proxy::save_reported_quota_for_test(
        f.root.path(),
        LOCAL,
        &ReportedQuotaSnapshot {
            observed_at_ms: now,
            windows: vec![ReportedQuotaWindow {
                kind: QuotaWindowKind::FiveHour,
                status: ReportedQuotaStatus::Allowed,
                reset_at_ms: now + 3_600_000,
                utilization: Some(0.5),
            }],
        },
    )?;
    assert_eq!(
        quota(&read(f.root.path(), "owner")?, &id)["state"],
        "unknown"
    );
    let holder = store::existing_instance_id(f.root.path())?;
    adopt(
        &f.conn,
        "owner",
        &holder,
        &[NativeAccountObservation {
            provider: "claude".into(),
            local_account_id: LOCAL.into(),
            enabled: true,
            credential_ready: true,
            private_binding: catalog::account_binding(f.root.path(), LOCAL)?,
        }],
        now,
    )?;
    assert_eq!(
        quota(&read(f.root.path(), "owner")?, &id)["windows"][0]["utilization"],
        0.5
    );
    Ok(())
}
#[test]
fn provider_quota_readback_revalidates_actor_and_changed_or_removed_target() -> Result<()> {
    let (f, holder, id) = fixture()?;
    let captured = targets(&f.conn, "owner", &holder)?;
    let values = BTreeMap::from([(
        LOCAL.into(),
        json!({"state":"reported","observedAtMs":1,"windows":[]}),
    )]);
    f.conn.execute(
        "UPDATE business_provider_federation_accounts SET revision=revision+1 WHERE account_id=?1",
        [&id],
    )?;
    assert_eq!(
        quota(
            &publish(
                f.root.path(),
                "owner",
                "owner",
                Some(&holder),
                &captured,
                &values
            )?,
            &id
        )["state"],
        "unknown"
    );
    f.conn.execute(
        "DELETE FROM business_provider_federation_native_bindings WHERE account_id=?1",
        [&id],
    )?;
    f.conn.execute(
        "DELETE FROM business_provider_federation_accounts WHERE account_id=?1",
        [&id],
    )?;
    assert!(publish(
        f.root.path(),
        "owner",
        "owner",
        Some(&holder),
        &captured,
        &values
    )?["accounts"]
        .as_array()
        .unwrap()
        .is_empty());
    f.conn.execute(
        "UPDATE business_users SET active=0 WHERE user_id='owner'",
        [],
    )?;
    assert!(publish(
        f.root.path(),
        "owner",
        "owner",
        Some(&holder),
        &captured,
        &values
    )
    .is_err());
    assert!(read(f.root.path(), "foreign").is_err());
    Ok(())
}
#[test]
fn provider_quota_readback_disabled_and_expired_metadata_never_claim_current_capacity() -> Result<()>
{
    let (f, _, id) = fixture()?;
    let now = store::now_ms() as i64;
    proxy::save_reported_quota_for_test(
        f.root.path(),
        LOCAL,
        &ReportedQuotaSnapshot {
            observed_at_ms: now - 10_000,
            windows: vec![ReportedQuotaWindow {
                kind: QuotaWindowKind::FiveHour,
                status: ReportedQuotaStatus::Rejected,
                reset_at_ms: now - 5_000,
                utilization: Some(1.0),
            }],
        },
    )?;
    f.conn.execute("UPDATE business_provider_federation_accounts SET enabled=0,credential_ready=0 WHERE account_id=?1",[&id])?;
    let result = read(f.root.path(), "owner")?;
    assert_eq!(quota(&result, &id)["state"], "reported");
    assert!(
        quota(&result, &id)["windows"][0]["resetAtMs"]
            .as_i64()
            .unwrap()
            < now
    );
    assert_eq!(quota(&result, &id)["windows"][0]["status"], "rejected");
    Ok(())
}
#[test]
fn provider_quota_readback_never_guesses_remote_or_unbound_account_limits() -> Result<()> {
    let (f, _, id) = fixture()?;
    f.conn.execute("UPDATE business_provider_federation_accounts SET holder_instance_id='remote-holder' WHERE account_id=?1",[&id])?;
    assert_eq!(
        quota(&read(f.root.path(), "owner")?, &id)["state"],
        "unknown"
    );
    f.conn.execute(
        "DELETE FROM business_provider_federation_native_bindings WHERE account_id=?1",
        [&id],
    )?;
    assert_eq!(
        quota(&read(f.root.path(), "owner")?, &id)["state"],
        "unknown"
    );
    Ok(())
}
