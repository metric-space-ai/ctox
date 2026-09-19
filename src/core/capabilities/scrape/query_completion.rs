//! Current-run receipts for completed provider queries with no matching records.
//! This validates the registered adapter's capture and its binding; it does not
//! infer provider execution from an empty record array or a previous success.
use super::{compute_sha256_bytes, CommandExecution, ProbeResult};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

const MAX_EVIDENCE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceReference {
    evidence_path: String,
    evidence_sha256: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QueryCompletionReceipt {
    pub schema: String,
    pub run_id: String,
    pub target_key: String,
    pub input_sha256: String,
    pub query: String,
    pub provider_url: String,
    pub http_status: u16,
    pub completed: bool,
    pub results_page: bool,
    pub blocked: bool,
    pub matched_records: u64,
    pub observed_records: u64,
    pub observation: Value,
}

pub(super) fn validate_query_completion(
    payload: &Value,
    run_dir: &Path,
    run_id: &str,
    target_key: &str,
    start_url: &str,
    input_json: Option<&str>,
    probe: &ProbeResult,
    execution: &CommandExecution,
) -> Result<(QueryCompletionReceipt, Vec<u8>)> {
    let reference: EvidenceReference = serde_json::from_value(
        payload
            .get("query_completion")
            .context("missing query completion reference")?
            .clone(),
    )
    .context("invalid query completion reference")?;
    ensure!(
        payload
            .get("records")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty),
        "completed-empty query requires an explicit empty records array"
    );
    ensure!(
        payload
            .get("failure_mode")
            .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
            && payload
                .get("error")
                .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
            && payload.get("partial_output") != Some(&Value::Bool(true)),
        "failed or partial output cannot complete an empty query"
    );
    ensure!(
        !execution.timed_out && execution.exit_code == Some(0),
        "query execution did not exit successfully"
    );
    ensure!(
        probe.reachable
            && !probe.human_verification
            && probe
                .status_code
                .is_none_or(|code| (200..300).contains(&code))
            && probe.error.as_deref().is_none_or(str::is_empty),
        "provider probe did not succeed"
    );
    let input_json = input_json.context("completed query requires current input")?;
    let input: Value = serde_json::from_str(input_json)?;
    let query = input
        .get("query")
        .or_else(|| input.get("company"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .context("query input requires a nonempty query or company")?;
    // One reserved adapter capture name avoids overwriting result/records/log
    // artifacts during publication and makes the emitter contract explicit.
    ensure!(
        reference.evidence_path == "outputs/query-evidence.json",
        "invalid query evidence path"
    );
    let base = run_dir.canonicalize()?;
    let path = run_dir.join(&reference.evidence_path).canonicalize()?;
    ensure!(
        path.starts_with(&base),
        "query evidence escapes current run"
    );
    ensure!(
        std::fs::metadata(&path)?.is_file(),
        "query evidence is not a regular file"
    );
    let file = std::fs::File::open(&path)?;
    ensure!(
        file.metadata()?.is_file(),
        "query evidence is not a regular file"
    );
    let mut bytes = Vec::new();
    file.take(MAX_EVIDENCE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= MAX_EVIDENCE_BYTES,
        "query evidence size invalid"
    );
    ensure!(
        compute_sha256_bytes(&bytes) == reference.evidence_sha256,
        "query evidence hash mismatch"
    );
    let receipt: QueryCompletionReceipt =
        serde_json::from_slice(&bytes).context("invalid query evidence")?;
    ensure!(
        receipt.schema == "ctox.scrape.query_completion.v1",
        "unknown query evidence schema"
    );
    ensure!(
        receipt.run_id == run_id && receipt.target_key == target_key,
        "query evidence belongs to another run or target"
    );
    ensure!(
        receipt.input_sha256 == compute_sha256_bytes(input_json.as_bytes())
            && receipt.query == query,
        "query evidence does not match current input"
    );
    let provider = url::Url::parse(&receipt.provider_url)?;
    let expected = url::Url::parse(start_url)?;
    ensure!(
        matches!(provider.scheme(), "http" | "https")
            && provider.origin() == expected.origin()
            && provider.username().is_empty()
            && provider.password().is_none(),
        "query evidence provider origin mismatch"
    );
    ensure!(
        receipt.completed
            && receipt.results_page
            && !receipt.blocked
            && (200..300).contains(&receipt.http_status)
            && receipt.matched_records == 0,
        "query evidence does not establish completed empty results"
    );
    ensure!(
        match &receipt.observation {
            Value::String(value) => !value.trim().is_empty(),
            Value::Object(value) => !value.is_empty(),
            _ => false,
        },
        "query evidence has no provider observation"
    );
    Ok((receipt, bytes))
}
