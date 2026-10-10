// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use crate::execution::cliproxyapi_model_probe::{ProbeFailure, ProbeSource, ProbeStatus};
const MODEL: &str = "claude-opus-5-5"; // Authenticated live Claude account catalog.
fn request() -> Value {
    json!({"version":1,"op":"check","commandId":"request-1",
        "accountId":"logical-account","accountRevision":1,"modelId":MODEL})
}
#[test]
fn request_rejects_caller_authority_secrets_and_unbounded_payloads() {
    assert!(parse(vec![request()]).is_ok());
    assert!(parse(vec![]).is_err());
    assert!(parse(vec![request(), request()]).is_err());
    for field in ["root", "token", "ownerUserId", "consumer", "endpoint", "credential", "projectId", "meetingId"] {
        let mut supplied = request();
        supplied[field] = json!("untrusted-private-value");
        assert!(parse(vec![supplied]).is_err());
    }
    for value in [json!(0), json!(-1), json!(null), json!("1")] {
        let mut supplied = request();
        supplied["accountRevision"] = value;
        assert!(parse(vec![supplied]).is_err());
    }
    for (field, value) in [("commandId", "x".repeat(129)), ("accountId", "x".repeat(257)),
        ("modelId", "x".repeat(257)), ("commandId", " ".into()), ("accountId", "a\nb".into())] {
        let mut supplied = request();
        supplied[field] = json!(value);
        assert!(parse(vec![supplied]).is_err());
    }
    for (field, value) in [("version", json!(2)), ("op", json!("invoke")), ("op", json!("read"))] {
        let mut supplied = request();
        supplied[field] = value;
        assert!(parse(vec![supplied]).is_err());
    }
}
fn policy() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(super::super::provider_federation::SCHEMA)?;
    conn.execute("INSERT INTO business_provider_federation_accounts VALUES('logical-account','owner','holder','claude','private-selector',1,1,1,1)", [])?;
    Ok(conn)
}
fn probe(checked: i64, status: ProbeStatus) -> NativeModelProbe {
    NativeModelProbe {
        model_id: MODEL.into(), checked_at_ms: checked, elapsed_ms: 12,
        source: if status == ProbeStatus::Unavailable { ProbeSource::Gateway } else { ProbeSource::Upstream },
        status, failure: if status == ProbeStatus::Ok { None } else { Some(ProbeFailure::Timeout) },
        http_status: if status == ProbeStatus::Ok { Some(200) } else { None }, retry_at_ms: None,
    }
}
#[test]
fn retained_observations_are_revision_scoped_redacted_and_do_not_change_accounts() -> Result<()> {
    let conn = policy()?;
    retain(&conn, "logical-account", 1, &probe(100, ProbeStatus::Ok))?;
    let public = project(&conn, "logical-account", 1)?;
    assert_eq!(public[0]["modelId"], MODEL);
    assert_eq!(public[0]["status"], "ok");
    assert_eq!(public[0]["elapsedMs"], 12);
    assert!(public.to_string().find("private-selector").is_none());
    assert_eq!(project(&conn, "logical-account", 2)?, json!([]));
    assert_eq!(project(&conn, "foreign-account", 1)?, json!([]));
    assert_eq!(conn.query_row("SELECT enabled,revision FROM business_provider_federation_accounts", [],
        |r| Ok((r.get::<_, bool>(0)?, r.get::<_, i64>(1)?)))?, (true, 1));
    Ok(())
}
#[test]
fn delayed_old_result_does_not_replace_a_newer_probe_and_rows_stay_bounded() -> Result<()> {
    let conn = policy()?;
    retain(&conn, "logical-account", 1, &probe(200, ProbeStatus::Unavailable))?;
    retain(&conn, "logical-account", 1, &probe(100, ProbeStatus::Ok))?;
    assert_eq!(project(&conn, "logical-account", 1)?[0]["status"], "unavailable");
    retain(&conn, "logical-account", 2, &probe(300, ProbeStatus::Ok))?;
    assert_eq!(project(&conn, "logical-account", 1)?, json!([]));
    assert_eq!(project(&conn, "logical-account", 2)?[0]["status"], "ok");
    assert_eq!(conn.query_row("SELECT count(*) FROM business_provider_federation_model_checks", [], |r| r.get::<_, i64>(0))?, 1);
    Ok(())
}
#[test]
fn account_deletion_removes_its_observations_and_raw_db_json_is_never_disclosed() -> Result<()> {
    let conn = policy()?;
    retain(&conn, "logical-account", 1, &probe(100, ProbeStatus::Ok))?;
    let mut corrupt = serde_json::to_value(probe(100, ProbeStatus::Ok))?;
    corrupt["credential"] = json!("untrusted-private-value");
    conn.execute("UPDATE business_provider_federation_model_checks SET result_json=?1", [corrupt.to_string()])?;
    assert!(project(&conn, "logical-account", 1).is_err());
    conn.execute("DELETE FROM business_provider_federation_accounts WHERE account_id='logical-account'", [])?;
    assert_eq!(project(&conn, "logical-account", 1)?, json!([]));
    Ok(())
}
#[test]
fn corrupt_observation_identity_cannot_claim_another_model_or_time() -> Result<()> {
    let conn = policy()?;
    retain(&conn, "logical-account", 1, &probe(100, ProbeStatus::Ok))?;
    conn.execute("UPDATE business_provider_federation_model_checks SET checked_at_ms=101", [])?;
    assert!(project(&conn, "logical-account", 1).is_err());
    Ok(())
}
