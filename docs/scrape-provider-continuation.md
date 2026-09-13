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

## Native research continuation

Person-research keeps the canonical execution phase `running`, with result
status `awaiting_provider`. A changed running checkpoint advances the canonical
projection version/outbox without creating a new execution attempt; identical
checkpoints remain idempotent and terminal commands cannot be reopened.

Each wake loads this native command checkpoint, never a browser document or a
workspace-selected file. It verifies the waiting scrape run in native SQLite
against the command-derived operation, company, country and target. Recovery
waits at least 30 seconds (honoring longer provider hints up to 300 seconds),
persists the next poll count/time before execution, and retains the original
six-hour deadline. The limit is 120 total attempts; exhaustion becomes a
visible failed command, not an unbounded poller or successful empty result.

Workjet's additive resume API binds the entire request and source plan to the
native workspace, reuses completed-source receipts and pre-ranking evidence
(including people outside the top-ranked person), and invokes only waiting
providers. Native CRM/runtime/capture augmentation runs after the compiled
source phase finishes, not on every poll. Pending commands bypass terminal
completion and gap-closure enqueueing. The adapter journal still owns uncertain
POST acceptance and must never resubmit an ambiguous accepted operation.

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

The resume API and command integration require the coordinated Workjet pin and
native compilation/tests on that exact composition; source implementation is
not acceptance. Company-dataset core execution, journal and wait projection now
have a two-stage regression (one POST per dataset across real journal reopenings);
the executable native runner still must wire them to registered execution.
Account/API entitlement,
encrypted-secret/native-runner wiring, real daemon restart tests and the full
DE/AT/CH app research/reload acceptance remain required. No live target has been
activated by these changes.

The native recovery regression launches separate bounded test processes against
one isolated persisted root. It drives real `recover_once`, worker execution,
Workjet planning/resume, registered fixture scripts and native scrape receipts.
It checks a not-yet-due restart, a due restart, unchanged completed-source call
count/receipt, a single provider submission, stable operation and persisted
terminal readback. The adapter-dispatch seam replaces only provider execution
transport, not the command worker or planner. Provider fixture counters are
written by actual registered scripts; no whole-execute mock fabricates them.
The regression still requires native compilation and execution before it can
be used as restart evidence; it is not a real provider or live tenant test.
