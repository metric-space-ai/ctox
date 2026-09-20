//! A provider's accepted asynchronous job is neither a failed query nor a
//! completed result. Validate its current-attempt receipt before allowing the
//! native caller to retain it for bounded continuation. No URLs or state paths
//! cross this contract, and the receipt itself grants no execution authority.
use super::CommandExecution;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// Read only a receipt already accepted by the native executor. A caller's
/// JSON receipt or output path is not evidence that a provider job exists.
pub(crate) fn load_provider_wait_receipt(
    root: &Path,
    run_id: &str,
    target_key: &str,
    operation_id: &str,
    company: &str,
    country: &str,
) -> Result<Value> {
    let conn = rusqlite::Connection::open_with_flags(
        super::registry::resolve_db_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let raw: String = conn
        .query_row(
            "SELECT run.result_json FROM scrape_run AS run
         JOIN scrape_target AS target ON target.target_id = run.target_id
         WHERE run.run_id = ?1 AND target.target_key = ?2 AND run.status = 'awaiting_provider'",
            rusqlite::params![run_id, target_key],
            |row| row.get(0),
        )
        .context("provider wait has no matching native scrape receipt")?;
    let result: Value = serde_json::from_str(&raw)?;
    let receipt: ProviderContinuation = serde_json::from_value(result["continuation"].clone())?;
    ensure!(
        receipt.schema == "ctox.scrape.provider_continuation.v1"
            && receipt.run_id == run_id
            && receipt.target_key == target_key
            && receipt.operation_id == operation_id
            && receipt.company == company.trim()
            && receipt.country == country
            && receipt.source_id == "linkedin.com"
            && receipt.provider == "brightdata"
            && (5..=300).contains(&receipt.retry_after_seconds)
            && result["exit_code"] == 0
            && result["timed_out"] == false,
        "native provider receipt belongs to another operation or query"
    );
    Ok(serde_json::to_value(receipt)?)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProviderContinuation {
    schema: String,
    run_id: String,
    input_sha256: String,
    operation_id: String,
    source_id: String,
    target_key: String,
    provider: String,
    company: String,
    country: String,
    dataset_id: String,
    snapshot_id: String,
    query_hash: String,
    phase: String,
    submission_attempt: u8,
    retry_after_seconds: u16,
}

fn lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn validate_provider_continuation(
    payload: &Value,
    run_id: &str,
    target_key: &str,
    config: &Value,
    input_json: Option<&str>,
    execution: &CommandExecution,
) -> Result<ProviderContinuation> {
    ensure!(
        payload.get("failure_mode").and_then(Value::as_str) == Some("awaiting_provider"),
        "missing provider wait marker"
    );
    ensure!(
        payload
            .get("error")
            .is_none_or(|value| value.is_null() || value.as_str() == Some(""))
            && payload
                .get("error_code")
                .is_none_or(|value| value.as_str() == Some("collection_pending")),
        "provider wait cannot mask a provider error"
    );
    ensure!(
        payload
            .get("records")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty),
        "provider wait must not contain result records"
    );
    ensure!(
        payload.get("query_completion").is_none()
            && payload.get("partial_output") != Some(&Value::Bool(true)),
        "provider wait cannot claim query completion"
    );
    ensure!(
        !execution.timed_out && execution.exit_code == Some(0),
        "provider wait requires successful runner exit"
    );
    let receipt: ProviderContinuation = serde_json::from_value(
        payload
            .get("continuation")
            .context("missing provider continuation")?
            .clone(),
    )
    .context("invalid provider continuation")?;
    let raw_input = input_json.context("provider wait requires current input")?;
    let input: Value = serde_json::from_str(raw_input).context("invalid provider wait input")?;
    ensure!(
        receipt.schema == "ctox.scrape.provider_continuation.v1",
        "unknown provider continuation schema"
    );
    ensure!(
        receipt.run_id == run_id
            && receipt.target_key == target_key
            && receipt.input_sha256 == super::compute_sha256_bytes(raw_input.as_bytes()),
        "provider continuation belongs to another invocation"
    );
    ensure!(
        receipt.provider == "brightdata"
            && config.get("async_provider").and_then(Value::as_str) == Some("brightdata")
            && receipt.source_id == "linkedin.com"
            && config.get("expected_provider").and_then(Value::as_str) == Some("linkedin.com")
            && target_key == "linkedin-com",
        "provider continuation is not enabled for this target"
    );
    ensure!(
        receipt
            .operation_id
            .strip_prefix("research-v1-")
            .is_some_and(lower_hex_digest)
            && input.get("research_operation_id").and_then(Value::as_str)
                == Some(receipt.operation_id.as_str())
            && input.get("source_id").and_then(Value::as_str) == Some(receipt.source_id.as_str()),
        "provider continuation operation/source mismatch"
    );
    ensure!(
        !receipt.company.trim().is_empty()
            && receipt.company.len() <= 1000
            && !receipt.company.chars().any(char::is_control)
            && input.get("company").and_then(Value::as_str).map(str::trim)
                == Some(receipt.company.as_str())
            && matches!(receipt.country.as_str(), "DE" | "AT" | "CH")
            && input.get("country").and_then(Value::as_str) == Some(receipt.country.as_str()),
        "provider continuation company/country mismatch"
    );
    ensure!(
        matches!(
            receipt.dataset_id.as_str(),
            "gd_l1viktl72bvl7bjuj0" | "gd_l1vikfnt1wgvvqz95w"
        ),
        "unsupported provider continuation dataset"
    );
    let snapshot = receipt
        .snapshot_id
        .strip_prefix("sd_")
        .or_else(|| receipt.snapshot_id.strip_prefix("s_"));
    ensure!(
        snapshot.is_some_and(|id| !id.is_empty()
            && id.len() <= 100
            && id.bytes().all(|byte| byte.is_ascii_alphanumeric()))
            && lower_hex_digest(&receipt.query_hash),
        "invalid provider continuation identity"
    );
    ensure!(
        matches!(receipt.phase.as_str(), "pending" | "ready")
            && (1..=2).contains(&receipt.submission_attempt)
            && (5..=300).contains(&receipt.retry_after_seconds),
        "invalid provider continuation phase/budget"
    );
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> (Value, String, Value, CommandExecution) {
        let operation = format!("research-v1-{}", "a".repeat(64));
        let input = json!({"company":"Fixture GmbH", "country":"DE", "source_id":"linkedin.com", "research_operation_id": operation}).to_string();
        let payload = json!({"records":[], "failure_mode":"awaiting_provider", "continuation": {
            "schema":"ctox.scrape.provider_continuation.v1", "run_id":"scrape_run-fixture",
            "target_key":"linkedin-com", "input_sha256":super::super::compute_sha256_bytes(input.as_bytes()),
            "operation_id":operation, "source_id":"linkedin.com", "provider":"brightdata",
            "company":"Fixture GmbH", "country":"DE", "dataset_id":"gd_l1viktl72bvl7bjuj0",
            "snapshot_id":"sd_fixture", "query_hash":"b".repeat(64), "phase":"pending",
            "submission_attempt":1, "retry_after_seconds":15
        }});
        (
            payload,
            input,
            json!({"async_provider":"brightdata", "expected_provider":"linkedin.com"}),
            CommandExecution {
                exit_code: Some(0),
                timed_out: false,
                stdout_text: String::new(),
                stderr_text: String::new(),
            },
        )
    }

    #[test]
    fn provider_continuation_retains_only_current_bound_wait() {
        let (payload, input, config, execution) = fixture();
        let value = validate_provider_continuation(
            &payload,
            "scrape_run-fixture",
            "linkedin-com",
            &config,
            Some(&input),
            &execution,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            payload["continuation"]
        );
    }

    #[test]
    fn provider_continuation_rejects_foreign_identity_and_unknown_fields() {
        let (payload, input, config, execution) = fixture();
        for (field, value) in [
            ("run_id", json!("other-run")),
            ("input_sha256", json!("c".repeat(64))),
            (
                "operation_id",
                json!(format!("research-v1-{}", "c".repeat(64))),
            ),
            ("source_id", json!("xing.com")),
            ("target_key", json!("other-target")),
            ("company", json!("Other GmbH")),
            ("country", json!("AT")),
            ("dataset_id", json!("gd_other")),
            ("snapshot_id", json!("../../other")),
            ("query_hash", json!("not-a-digest")),
            ("phase", json!("completed")),
            ("submission_attempt", json!(3)),
            ("retry_after_seconds", json!(0)),
            ("retry_after_seconds", json!(301)),
            ("state_path", json!("/private/elsewhere")),
        ] {
            let mut invalid = payload.clone();
            invalid["continuation"][field] = value;
            assert!(
                validate_provider_continuation(
                    &invalid,
                    "scrape_run-fixture",
                    "linkedin-com",
                    &config,
                    Some(&input),
                    &execution
                )
                .is_err(),
                "{field}"
            );
        }
        assert!(validate_provider_continuation(
            &payload,
            "scrape_run-fixture",
            "linkedin-com",
            &json!({}),
            Some(&input),
            &execution
        )
        .is_err());
    }

    #[test]
    fn provider_continuation_cannot_mask_records_timeout_or_completed_empty() {
        let (payload, input, config, mut execution) = fixture();
        for (field, value) in [
            ("records", json!([{"field":"firma_name","value":"bad"}])),
            ("query_completion", json!({})),
            ("partial_output", json!(true)),
            ("failure_mode", json!("blocked")),
        ] {
            let mut invalid = payload.clone();
            invalid[field] = value;
            assert!(validate_provider_continuation(
                &invalid,
                "scrape_run-fixture",
                "linkedin-com",
                &config,
                Some(&input),
                &execution
            )
            .is_err());
        }
        execution.exit_code = Some(1);
        assert!(validate_provider_continuation(
            &payload,
            "scrape_run-fixture",
            "linkedin-com",
            &config,
            Some(&input),
            &execution
        )
        .is_err());
        execution.exit_code = Some(0);
        execution.timed_out = true;
        assert!(validate_provider_continuation(
            &payload,
            "scrape_run-fixture",
            "linkedin-com",
            &config,
            Some(&input),
            &execution
        )
        .is_err());
    }
}
