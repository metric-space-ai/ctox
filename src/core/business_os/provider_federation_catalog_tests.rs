// Origin: CTOX
// License: AGPL-3.0-only
use super::tests::{account, account_id, command, Fixture};
use super::*;

fn model_observation(time: i64) -> crate::coding_agents::pi_sidecar::NativeModelCatalogObservation {
    crate::coding_agents::pi_sidecar::NativeModelCatalogObservation {
        provider: "ctox_proxy".into(),
        checked_at_ms: time,
        models: Some(vec!["MiniMax-M3".into()]),
        http_status: Some(200),
        elapsed_ms: 12,
        retry_after_seconds: None,
        failure: None,
    }
}

#[test]
fn native_catalog_target_is_owner_holder_revision_and_account_bound() -> Result<()> {
    let f = Fixture::new()?;
    let mut native = account(INHERITED_NATIVE_ACCOUNT_ID);
    native.provider = "ctox_proxy".into();
    let state = f.adopt(&[native])?;
    let request = ObserveNativeRequest {
        _inbound_channel: None,
        account_id: account_id(&state).into(),
        expected_account_revision: 1,
    };
    assert_eq!(
        native_catalog_target(&f.conn, "owner", "native-instance", &request)?,
        "ctox_proxy"
    );
    assert!(native_catalog_target(&f.conn, "foreign", "native-instance", &request).is_err());
    assert!(native_catalog_target(&f.conn, "owner", "foreign-holder", &request).is_err());
    let wrong_revision = ObserveNativeRequest {
        expected_account_revision: 2,
        ..request
    };
    assert!(native_catalog_target(&f.conn, "owner", "native-instance", &wrong_revision).is_err());
    let subscription = f.adopt(&[account("private-subscription")])?;
    let subscription_id = subscription["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["provider"] == "minimax")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let subscription_request = ObserveNativeRequest {
        _inbound_channel: None,
        account_id: subscription_id.into(),
        expected_account_revision: 1,
    };
    assert!(
        native_catalog_target(&f.conn, "owner", "native-instance", &subscription_request).is_err()
    );
    Ok(())
}

#[test]
fn native_catalog_failures_retain_last_real_models_without_cooling_the_account() -> Result<()> {
    let f = Fixture::new()?;
    let state = f.adopt(&[account(INHERITED_NATIVE_ACCOUNT_ID)])?;
    let id = account_id(&state).to_owned();
    let request = ObserveNativeRequest {
        _inbound_channel: None,
        account_id: id.clone(),
        expected_account_revision: 1,
    };
    let initial = catalog_projection(&f.conn, &id, 1, 100)?;
    assert_eq!(initial["observed"], false);
    retain_catalog_observation(&f.conn, &request, &model_observation(100))?;
    let good = catalog_projection(&f.conn, &id, 1, 101)?;
    assert_eq!(good["models"], json!(["MiniMax-M3"]));
    assert_eq!(good["fresh"], true);
    assert_eq!(
        catalog_projection(&f.conn, &id, 1, 100 + CATALOG_FRESHNESS_MS + 1)?["fresh"],
        false
    );
    assert_eq!(catalog_projection(&f.conn, &id, 1, 99)?["fresh"], false);
    for (status, failure) in [
        (None, "transport_failed"),
        (Some(403), "upstream_rejected"),
        (Some(429), "rate_limited"),
    ] {
        let mut failed = model_observation(110);
        failed.models = None;
        failed.http_status = status;
        failed.failure = Some(failure.into());
        retain_catalog_observation(&f.conn, &request, &failed)?;
        let retained = catalog_projection(&f.conn, &id, 1, 111)?;
        assert_eq!(retained["models"], json!(["MiniMax-M3"]));
        assert_eq!(retained["lastSuccessAtMs"], 100);
        assert_eq!(retained["lastAttempt"]["failure"], failure);
        assert_eq!(retained["fresh"], false);
        assert_eq!(list(&f.conn, "owner")?["accounts"][0]["enabled"], true);
        assert_eq!(list(&f.conn, "owner")?["accounts"][0]["revision"], 1);
        assert_eq!(
            list(&f.conn, "owner")?["accounts"][0]["inferenceVerified"],
            false
        );
    }
    assert_eq!(
        catalog_projection(&f.conn, &id, 2, 111)?["models"],
        json!([])
    );
    Ok(())
}

#[test]
fn native_catalog_command_rejects_unadmitted_and_caller_supplied_private_data() -> Result<()> {
    let f = Fixture::new()?;
    for payload in [
        json!({"account_id":"account","expected_account_revision":1}),
        json!({"account_id":"account","expected_account_revision":1,"models":["MiniMax-M3"]}),
        json!({"account_id":"account","expected_account_revision":1,"endpoint":"https://example.invalid"}),
        json!({"account_id":"account","expected_account_revision":1,"key":"fixture-private"}),
    ] {
        let result = handle_command(
            f.root.path(),
            &command("ctox.workjet.providers.observe_native", payload),
            "owner",
            None,
        );
        assert!(result.is_err());
    }
    assert_eq!(list(&f.conn, "owner")?["accounts"], json!([]));
    Ok(())
}

#[test]
fn native_catalog_rejects_model_names_without_a_successful_real_response() -> Result<()> {
    let f = Fixture::new()?;
    let state = f.adopt(&[account(INHERITED_NATIVE_ACCOUNT_ID)])?;
    let id = account_id(&state).to_owned();
    let request = ObserveNativeRequest {
        _inbound_channel: None,
        account_id: id.clone(),
        expected_account_revision: 1,
    };
    let mut inconsistent = model_observation(100);
    inconsistent.http_status = None;
    assert!(retain_catalog_observation(&f.conn, &request, &inconsistent).is_err());
    assert_eq!(
        catalog_projection(&f.conn, &id, 1, 101)?["models"],
        json!([])
    );
    Ok(())
}
