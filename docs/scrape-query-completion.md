# Completed queries without matching records

A registered scrape adapter may report a completed provider query with zero
matching records. This is distinct from an empty or failed extraction.
Ordinary `records: []` still fails; lowering `expected_min_records` does not
establish query completion.

## Adapter receipt v1

Read the current raw UTF-8 `CTOX_SCRAPE_INPUT_JSON`. The query is its nonempty,
trimmed string `query`, or `company` only when `query` is absent.
Write a JSON receipt to `CTOX_SCRAPE_RUN_DIR/outputs/query-evidence.json`:

```json
{
  "schema": "ctox.scrape.query_completion.v1",
  "run_id": "<basename of CTOX_SCRAPE_RUN_DIR>",
  "target_key": "<CTOX_SCRAPE_TARGET_KEY>",
  "input_sha256": "<lowercase SHA-256 of exact raw input bytes>",
  "query": "<current trimmed query>",
  "provider_url": "https://provider.example/search",
  "http_status": 200,
  "completed": true,
  "results_page": true,
  "blocked": false,
  "matched_records": 0,
  "observed_records": 8,
  "observation": {
    "result_count": 8,
    "exact_matches": [],
    "matching_rule": "exact publisher name"
  }
}
```

Emit this JSON on stdout and exit 0:

```json
{
  "records": [],
  "query_completion": {
    "evidence_path": "outputs/query-evidence.json",
    "evidence_sha256": "<lowercase SHA-256 of exact receipt file bytes>"
  }
}
```

Both receipt and reference reject unknown fields. The evidence file must be a
regular file within the current run and contain 1–1,048,576 bytes. The URL must
use HTTP(S), have no credentials, and share the registered target start URL's
origin. Receipt HTTP status must be 2xx. Observation must be a nonempty object
or string capturing the actual provider result and matching rule. Observed
records count all inspected provider results; matched records must be zero.
An adapter must only emit this receipt after actually completing the current
query and inspecting its result page. A hash binds bytes; it is not independent
verification of the adapter's account of the provider.

Failures, partial output, authentication, failed probes, timeouts, or unsuccessful
process exits cannot be upgraded by a receipt. A skipped probe still requires
the adapter's successful provider HTTP observation. Invalid receipts fail closed.

## Native result and persistence

A valid receipt yields `status: "completed_empty"`, `ok: true`,
`records_found: 0`, and the typed `query_completion` object. It queues no
scraper repair and materializes no record. The receipt is persisted in SQLite
`scrape_run.result_json` and `run.json.result.query_completion`. The validated
bytes are copied to `outputs/verified-query-evidence.json`, registered as the
hashed `query_completion_evidence` artifact.

Business OS adapter tests expose `test_completed_empty`, with
`query_completed: true` and `test_ok: false`. Completing a query without
matches does not prove extraction of the adapter's expected fields.
No new `last_success_at_ms` is stamped.

Consumers must use the result of the current invocation. Existing materialized
records and the explicitly historical `last_successful_run` remain unchanged;
they must not be substituted as results of the current empty query. This
contract does not certify other adapters or complete a broader research task.
