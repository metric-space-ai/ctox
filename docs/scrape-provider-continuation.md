# Accepted asynchronous provider jobs

`awaiting_provider` means the registered adapter has an accepted provider job
but no complete research result. It is neither `temporary_unreachable` nor
`completed_empty`. The former can trigger public-browser fallback and cached
historical results in the Web Stack; neither is appropriate for a pending job.

The current contract is restricted to the LinkedIn target `linkedin-com`,
`expected_provider: linkedin.com` and explicit manifest `async_provider: brightdata`.
Supported datasets are BrightData's LinkedIn company and profile collections.
No existing target is opted in by this change.

The runner emits an empty records array, `failure_mode: awaiting_provider` and
a `continuation` object with exactly these fields:

| Field | Binding or constraint |
| --- | --- |
| `schema` | `ctox.scrape.provider_continuation.v1` |
| `run_id`, `target_key`, `input_sha256` | Current native run, registered target, SHA256 of exact raw input bytes |
| `operation_id` | `research-v1-` plus 64 lowercase hex characters, exactly matching native research input |
| `source_id`, `provider` | `linkedin.com`, `brightdata` |
| `company`, `country` | Exact trimmed input company and exact DE/AT/CH country |
| `dataset_id`, `snapshot_id`, `query_hash` | Allowlisted dataset, bounded safe provider job ID, 64-character lowercase query digest |
| `phase` | `pending` or `ready` |
| `submission_attempt` | 1 or 2, matching the durable adapter journal |
| `retry_after_seconds` | 5–300; the profile projector currently emits 15 |

Unknown fields, paths, foreign invocation identity, records, completion claims,
provider errors, timeouts and failed runner exits are rejected. The native layer
validates the adapter receipt, not independently the provider's account of
acceptance. The runner still owns authenticated dataset/snapshot observation and
durable query binding before it emits this receipt. The receipt contains no
credential or arbitrary resume URL and grants no execution authority.

Valid waits are persisted in `scrape_run.result_json.continuation` and
`run.json.result.continuation` and returned in the native outcome. They have
`ok: false`, no error diagnostic, no query-completion receipt, no materialization
and no repair request/task. Existing materialized records remain historical and
unchanged. Business OS adapter tests label them `test_awaiting_provider`; they
never pass extraction acceptance or stamp a new success.

`brightdata-continuation.cjs` converts the core's internal pending transition
into this native contract. It keeps operation/snapshot/query identity while
binding every resumed attempt to its own current run/input. The native runner
must exit zero after emitting a valid wait. It must never print the unprojected
internal `temporary_unreachable` pending payload as the adapter result.

## Integration and verification still required

The native registered-script execution regression seeds a real prior success,
executes a pending fixture, and asserts durable receipt equality with no record
materialization, old-success promotion or repair; a foreign-operation fixture
must fail. Three pure native validator regressions cover shape and identity.
These native tests have not yet run on the composed candidate.

The JavaScript projector tests exercise actual core POST acceptance using an
injected provider, stable identity over separate native attempts, query changes,
invalid/partial/error outcomes and exclusion of paths/secrets. They are not
live-provider tests.

This contract alone does not complete research resumption. Before registry
activation, the actual Workjet research path must retain this continuation,
skip fallback/cascade/history substitution, and native command execution must
persist a nonterminal bounded wait and resume the same operation on wake/restart.
Preserve other sources' completed work. Do not complete the command, restart a
provider POST or treat a cached earlier result as completion. Account/API
entitlement, encrypted-secret/native-runner wiring, native restart tests and
the full DE/AT/CH app research/reload acceptance remain required.
