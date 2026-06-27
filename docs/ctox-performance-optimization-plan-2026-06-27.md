# CTOX Performance Optimization Plan - 2026-06-27

Source review:
`/Users/michaelwelsch/Documents/ctox/docs/ctox-performance-review-2026-06-24.md`

Workspace:
`/Users/michaelwelsch/Documents/ctox.nosync`

## Verdict

No. The 2026-06-24 performance review is represented in the current plan set,
but it is not fully handled.

The important distinction is:

- coverage is complete for the named HIGH/MEDIUM findings;
- several exact hot paths are fixed or strongly reduced;
- CTOX is still not release-proven as idle-clean after the installed
  `ctox upgrade --dev` path;
- file-access, browser demand/file flows, unsupported RxDB fallbacks,
  projection stamps, DB retention, and module UI hot paths still need
  structural work.

CTOX is a background daemon. The release bar is not "the unit tests pass"; the
release bar is: after file access and browser use, installed `ctox-real` does
not consume a sustained core while no source changed.

## Review Inputs

This plan was created after:

- comparing the external source review with the repo-local copy; `cmp` returned
  identical files;
- inspecting the existing performance plans dated 2026-06-25 and 2026-06-26;
- running read-only subagent reviews across coverage, daemon idle loops,
  native/browser RxDB, SQLite, files, and performance gates; the latest
  2026-06-27 pass used four focused subagents:
  - review coverage and gap analysis against the 2026-06-24 source review;
  - daemon idle loops, sync-run DB churn, status paths, native peer
    projections, files, and command loops;
  - RxDB/SQLite/browser data plane, large files, chunk retention, query
    fallbacks, and WebRTC demand/file loading;
  - measurement, idle gates, status isolation, and missing counters;
- validating the current Browser RxDB query-performance guard work locally.

Subagents did not edit files. Their shared conclusion: no HIGH/MEDIUM item is
missing from the plans, but many remain `partial` or `open`.

The 2026-06-27 subagent pass added these explicit conclusions:

- release readiness still depends on installed `ctox-real` evidence after
  `ctox upgrade --dev`; local unit tests are not sufficient;
- the most plausible daemon-idle design flaw is a feedback loop where small
  no-op DB/WAL/SHM changes invalidate broad stamp gates and wake router,
  audit, queue, sync, and projection work again;
- native WebRTC query-fetch is now guarded, but normal native `query()` /
  `count()` and browser `allDocuments()`/bounded-cursor fallbacks still need
  strict hot-path policy and row budgets;
- file access is improved but not event-driven, and several file/blob callers
  still materialize whole arrays or files instead of streaming/range reads.
- service router preflight/tick gates, scheduler due emission, Business OS app
  recovery, and harness audit now use source-specific stamps for the checked
  paths; workflow/self-work caches, projection repair, and progressive approval
  checks still need the same treatment rather than DB file mtimes.
- RxDB checkpoint status still reads via the writer mutex, so peer
  start/reconnect/status can be delayed by large writes or chunk batches.

The follow-up subagent pass in this thread added these verified gaps:

- ticket-event no-op handling was unsafe: `upsert_ticket_event_from_adapter`
  only returned `changed=false` when an identical event's routing state equaled
  the freshly computed initial status. Re-syncing an identical event that had
  already progressed to `leased`, `handled`, `blocked`, or `failed` could still
  write and force routing state back to the initial value. This has now been
  fixed and regression-covered in the translation-layer sync test.
- router broad-stamps are reduced, but the router is still an 8 second SQL
  stamp poller. The next proof must measure stamp computation time, read-only
  SQLite opens, and per-stage skips instead of treating "source stamp" as free.
- progressive approval auto-close still polls `ticket_self_work_items` from the
  15 second mission-maintenance loop without its own source stamp.
- scheduler and harness-audit source stamps are now semantically narrower, but
  both are still aggregate SQL stamp computations. Large schedule/process tables
  need row/latency counters and query-plan guards.
- native demand file chunk SQL has now been rewritten away from JSON
  expression filtering/sorting for generic chunk collections. It uses
  deterministic chunk-ID prefix ranges with `deleted = 0` and `ORDER BY id`,
  and the exact SQL is covered by an `EXPLAIN QUERY PLAN` guard. Browser-side
  blob chunk indexes and streaming consumers remain open.
- WebRTC send queues are not hard bounded, and native inline sends bypass the
  same buffered-amount wait that framed sends use. Query/file in-flight limits
  use load-then-increment, so concurrent accepts can exceed the configured cap.

The main-thread continuation subagents on 2026-06-27 re-checked the
2026-06-24 review against the current tree and confirmed this carry-forward
set:

- RxDB SQLite predicate/count pushdown is only partial: simple selectors/counts
  compile to SQL, but unsupported `query()` and some `count()` paths still have
  guarded scan/deserialise fallbacks. Hot paths need a zero-broad-fallback
  release gate.
- The SQLite writer is still a shared `Arc<Mutex<Connection>>`; many file-backed
  reads now use read-only WAL connections, but checkpoint/status fallbacks and
  in-memory paths can still hit the writer mutex. Contention and reader-fallback
  counters must be release-visible.
- Business OS projection loops use source stamps and since cursors, but the
  current `updated_at_ms + 1` cursor can skip same-timestamp over-limit rows.
  Projection windows need compound cursor semantics and batch-drain tests.
- The cached projection writer covers the main projection path, but direct
  single-record command/status/file/release helpers can still open SQLite and
  reload table info. Those paths need to join the cached writer or expose
  counters proving they are not idle hot paths.
- Desktop chunk pruning no longer performs the original broad scan, but DB
  growth is still structurally open: the real local RxDB DB was observed around
  264 MB with a large freelist and roughly 100 MB of `desktop_file_chunks`
  payload. There is no global byte quota, TTL, WAL plateau, or freelist gate.
- Native eager desktop-file materialization still reads/encodes full files into
  chunk documents before writing. File Viewer demand reads are range-oriented,
  but full-file materialization and browser blob helpers still need streaming,
  peak-byte budgets, and hard caps.
- Browser standby is not clean everywhere: business-chat active watching is
  reduced, but startup progress and reporter pointer/mouse handlers still need
  idle-safe lifecycle checks.

## Local Verification In This Pass

Passed:

- `cmp -s /Users/michaelwelsch/Documents/ctox/docs/ctox-performance-review-2026-06-24.md docs/ctox-performance-review-2026-06-24.md`
- `node --check src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`
- `node src/apps/business-os/rxdb/tests/demand-loading-transport-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/file-demand-loader-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/replication-recovery-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/storage-index-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/query-api-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/bundle-reproducible-smoke.mjs`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `node src/apps/business-os/rxdb/tests/run-all.mjs`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_sync_due_gate -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_sync_activity_detection_covers_adapter_result_shapes -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox service_status_reports_channel_sync_runtime_metrics -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox parse_service_status_accepts_missing_newer_fields -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox ticket_sync -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox canonical_sync_batch_persists_through_translation_layer -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox communication_sync_run_recorder_skips_successful_noop_heartbeats -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_ticket_state_idle_gate_skips_unchanged_source -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox durable_status_snapshot -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox idle_durable_queue_empty_gate_ignores_sync_run_metadata_churn -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_run_metadata_churn -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_task_caches_ignore_sync_run_metadata_churn -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_task_list_cache_reuses_idle_reads_until_store_changes -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_task_count_cache_reuses_idle_reads_until_store_changes -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_router -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox emit_due_scan_gate_skips_idle_until_schedule_db_changes_or_due_time_arrives -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox emit_due -- --nocapture`
- `python3 -m py_compile src/tools/perf/ctox_perf_probe.py`
- `python3 -m py_compile src/tools/perf/ctox_installed_idle_gate.py src/tools/perf/ctox_perf_probe.py`
- `python3 src/tools/perf/ctox_installed_idle_gate.py --root /Users/michaelwelsch/Documents/ctox.nosync --artifact-dir <tmp>/artifacts --dry-run --skip-upgrade --skip-gate-c --pid 12345 --gate-a-seconds 1 --gate-b-seconds 1 --cpu-interval 1 --status-interval 1`
- `python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --skip-db --skip-heartbeat --max-sync-run-delta '*.ticket_sync_runs.row_count=0' --pretty | python3 -m json.tool >/dev/null`
- `rustfmt --check src/core/service/service.rs src/core/mission/tickets.rs src/core/mission/ticket_translation.rs src/core/mission/channels.rs`
- `git diff --check`
- synthetic `ctox_perf_probe.py` assertion checks proving service-status
  channel-sync deltas fail when over budget and `--assert-idle --skip-status`
  stays usable for passive status-free idle measurements;
- synthetic `ctox_perf_probe.py` assertion check proving a
  `ticket_sync_runs` insert during the CPU sampling window fails
  `--max-sync-run-delta '*.ticket_sync_runs.row_count=0'`;

`run-all.mjs` result: 49 passed, 0 failed, 2 skipped. The skipped tests are
`cross-process-file-fetch-smoke.mjs` and `cross-process-wire-smoke.mjs` because
the wire daemon was not built. They remain missing coverage, not green evidence.

## Current Coverage Matrix

Status terms:

- `fixed`: exact reviewed issue is addressed and has source/test evidence.
- `fixed for exact path`: original path is fixed, adjacent tail work remains.
- `partial`: reduced, but still has structural or release-evidence gaps.
- `open`: reviewed behavior is still present.

| Finding | Status | Current assessment |
| --- | --- | --- |
| H1 RxDB SQLite non-PK scans | partial | Simple selectors/counts compile to SQL and query-fetch rejects unsupported stream fallback, but normal unsupported `query()` and some `count()` paths still scan/deserialise. |
| H2 WebRTC status per frame | fixed for exact path | Status is coalesced/skinny and heavy diagnostics are opt-in; release probes still need fanout/observer counters. |
| H3 IMAP FETCH/STORE body overfetch | fixed for exact path | Summary/body split exists; mail pagination, first import, IDLE/delta tokens remain. |
| H4 Chat tracked-message N+1 | fixed for exact path | Browser tracking batches command/task lookups and watches only while active; broader Chat DOM/layout work remains. |
| H5 Matching keystroke recompute | open | Matching still has O(requirements x matches x objects) compute/search paths. |
| H6 Outbound per-row pipeline recompute | open | Pipeline/company views still need memoized maps and targeted reloads. |
| M1 Count materializes docs | partial | SQL `COUNT(*)` exists for compilable selectors; unsupported counts still scan unless rejected before execution. |
| M2 Single SQLite connection mutex | partial | Many reads use read-only WAL paths; writer lock, checkpoint status, statement reuse, and some fallbacks remain. |
| M3 Query-fetch full scan | fixed | WebRTC query-fetch refuses unsupported SQLite stream fallbacks instead of scanning before data chunks. |
| M4 Projection reconcilers | partial | Active filters and batching exist; persisted changed-id/high-water repair windows remain. |
| M5 Desktop chunk prune scan | fixed | Native desktop chunk pruning uses deterministic primary-key/range paths. |
| M6 Per-chunk write transactions | partial | Native eager chunk writes are bulked; browser upload/import chunk writes remain. |
| M7 Demand-cache invalidation scan | fixed for exact path | Reverse document-to-window refs and batch invalidation exist. |
| M8 Browser upsert overhead | fixed | IndexedDB upsert/bulkUpsert use batched readwrite paths. |
| M9 Subscription full re-query | partial | Collection and primary-key live deltas are reduced; complex query subscriptions still re-exec. |
| M10 Browser `allDocuments()` fallback | partial | Primary/schema-index/bounded cursor paths exist; non-indexed fallback still exists and is only optionally rejectable. |
| M11 Inference arena allocation | open | Qwen decode still needs persistent descriptor arenas/reusable contexts. |
| M12 Inference graph rebuild | open | Graph/context reuse remains. |
| M13 Stream event clone/deserialise | fixed for delta/no-op path | Ignored stream deltas are filtered before payload clone/deserialise; broader transcript/cost batching remains. |
| M14 Blocking file fetch stream | fixed | File fetch no longer parks tokio workers with `block_on`/sleep. |
| M15 Unbounded RxSubject fanout | fixed for native fanout | Bounded broadcast, lag markers, resync mapping, and lag counters exist; installed slow-peer soak remains. |
| M16 Mailbox index | fixed | `stalwart_messages` has mailbox/received indexing. |
| M17 Mailserver connection churn | fixed for exact path | Message/mailbox hot paths now use cached `with_connection()`; broader mail sync tail remains. |
| M18 Send verification full RFC822 | fixed | Verification uses Message-ID search/header fetch. |
| M19 Email full UID scans | partial | UID watermarks reduce steady scans; first import, UIDVALIDITY, IDLE, provider delta tokens remain. |
| M20 Ticket assignment N+1 | partial | Some list/projection hydration is set-based; broader ticket/queue helpers remain. |
| M21 Ticket projection DB reopens | fixed for direct projection | Direct Business OS ticket projection reuses one ticket DB connection; non-projection helpers need audit. |
| M22 Chat full DOM rebuild | partial | No-op sync can still build/compare full message HTML. |
| M23 Forced reflow in drag | open | Window/chat layout read-write batching remains. |
| M24 Sync diagnostics fanout | partial | Diagnostics are coalesced, but sanitize/record/fanout still need counters and release evidence. |
| M25 Spreadsheet HyperFormula rebuild | open | Persistent engine and changed-cell updates remain. |
| M26 Matching requirements scans | open | Map indexes, debounce, and DOM reconciliation remain. |
| M27 Buchhaltung joins per render | open | Pre-aggregated maps and targeted reloads remain. |
| M28 Customers search full render | open | Center-only debounced rendering and shared summaries remain. |
| M29 Projection writer reopen/table_info | partial | Cached writer covers some paths; direct command/status/file/release paths and counters remain. |
| M30 SQLite `synchronous=NORMAL` | fixed for checked central stores | Core, Business OS, native RxDB, and mailserver checked paths use WAL/NORMAL; keep guard coverage for new direct helpers. |
| M31 Status `ps` scan | fixed for status idle path | Normal status process scans are cached/gated; explicit lifecycle/probe scans remain intentional. |

## Release-Blocking Root Causes

### 1. Installed Idle Evidence Is Still Incomplete

There is partial installed evidence, but no release-valid proof yet that the
installed daemon stays below idle CPU budget after the actual user path:

1. push main-ready fixes;
2. run `ctox upgrade --dev`;
3. grant file access / use Business OS file paths;
4. let the browser and daemon go idle;
5. run the perf probe against the installed `ctox-real` PID.

Current measured evidence from this continuation:

- commit `85c3535a` was pushed to `main` and installed through
  `ctox upgrade --dev` as `branch-main-20260627T101453Z`;
- installed PID `32384` reached low passive CPU after warmup:
  `ps` reported 0.3 percent CPU after roughly 3 minutes;
- a short passive probe using `--assert-idle --skip-status` over 15 samples at
  2 second intervals reported average 0.15 percent CPU, p95 0.73 percent CPU,
  and max 1.5 percent CPU;
- the same probe was not a full release gate: it did not cover the full
  post-file-access / Business-OS / file-viewer / upload / 10-minute no-input
  scenario, and it still produced non-CPU strict failures around SHM growth,
  missing zero-delta metric matches, and one external-poll version read.

So the short passive CPU result is encouraging, but not a release claim. The
probe workflow exists; the missing evidence is the full installed workflow after
file-access and Business OS warmup with DB/WAL/SHM, file-indexer, projection,
query-fallback, and sync-run counters flat.

### 2. No-op Sync DB Churn And Broad Stamp Feedback Can Keep Idle Gates Dirty

The daemon review found a systemic failure mode that fits the observed idle CPU
pattern: no-op sync paths can still write sync-run metadata, and broad idle
gates key off whole DB stamps rather than source-specific high-water marks.

Concrete risk chain:

1. Router ticks and ticket/channel sync checks run even when no user work is
   pending.
2. Some no-op checks still insert run/status metadata such as
   `ticket_sync_runs` or `communication_sync_runs`, or classify unchanged
   provider fetch windows as activity.
3. Core/ticket/communication DB mtimes or broad store stamps change.
4. Durable queue empty probes, Business OS projections, and projection repair
   loops see the broad stamp change and
   reset their backoff.
5. The daemon has a repeating DB/write/scan tail even though no external source
   changed.

This is the most important structural idle hypothesis to verify next. It is not
enough to make one loop cheaper; unchanged sync checks must not continuously
dirty the stores that other idle gates watch.

2026-06-27 status: the direct no-op write sources are now reduced for ticket
adapter upserts, ticket sync-run recording, communication sync-run recording,
channel fetched-only activity classification, and ticket-sync due/backoff in
the service router. Live service status now exposes channel-sync and ticket-sync
runtime counters. Durable service status and the durable-queue empty probe now
use source-specific stamps for communication queue state, LCM last agent
outcome, and open ticket-case state, so `ticket_sync_runs` and
`communication_sync_runs` metadata churn no longer reopens those two status /
queue-idle paths. Queue task list/count caches also now key off the
communication projection clock instead of channel DB/WAL/JOURNAL file stamps,
so `communication_sync_runs` metadata churn does not invalidate queue status
views while real message/routing changes still do. Harness audit now also has a
source-specific gate over process/core-transition/PM inputs and active findings.
That does not complete the architecture: ticket-self-work/workflow caches,
progressive approval, native RxDB checkpoint status, file indexing, and
projection feedback still need source-specific high-water or dirty-ID gates and
installed idle proof.

### 3. Desktop File Access Still Has Polling/Stamp-Scan Tail

After file access grant, the native desktop-file indexer is reduced but not
event-driven. It still wakes periodically, computes root/direct-child stamps,
and has a full-scan fallback. Chunk reads are much better, but file indexing is
still a plausible source of post-access idle CPU.

### 4. Unsupported RxDB Query Fallbacks Still Exist

Native query-fetch hot path is guarded, but normal native `query()`/`count()` and
browser `allDocuments()` fallback are not structurally closed everywhere.

The current Browser IndexedDB work adds:

- `allDocuments()` call/row counters;
- fallback attribution;
- optional strict rejection via `rejectAllDocumentsFallback`;
- `queryPlanFor().allDocumentsFallback`;
- complex live-query re-exec counters.

That makes the problem visible and testable, but the reject policy is not yet
turned on for all idle-sensitive paths.

### 5. Projection And Repair Loops Still Use Aggregate Stamps

Projection loops are cheaper and back off, but several still derive source
state by table-sized stamps or repair windows instead of persisted dirty IDs or
per-collection high-water cursors.

Idle unchanged sources should result in zero table-sized projection work.

### 6. DB Growth Is Not Governed By A Replication Horizon

Desktop chunk pruning improved, but physical retention is not a complete
replication-horizon policy. Tombstones, blob/chunk payloads, WAL, and freelist
growth still need explicit operator-safe limits and shrink/checkpoint rules.

### 7. Browser Demand/File Lifecycles Are Not Fully Scoped

Demand-only collection boundaries are now enforced, but explicit file/chunk
callers still need consistent `leaseCollection()` ownership and `try/finally`
release. Query/file demand collectors also need peer-loss abort semantics.

### 8. Browser Upload/Blob Paths Are Still Too Granular

The current scoped chunk bridge work covers Command Bus, Chat, CV Print
Builder, App Store ZIP uploads, Explorer uploads, Documents blobs, and
Spreadsheets blobs for the exact patched paths. Research, Universal Importer,
file-integrity helpers, generic blob chunk paths, native explicit desktop-file
materialization, and browser full-file demand consumers still contain or can
trigger full-file materialization, whole-array chunk collection, per-chunk write
tails, live chunk subscriptions, or broad `find(...).exec()` blob reads. These
are plausible sources of a long post-file-share sync/write tail even when the
daemon itself is no longer scanning one table forever.

### 9. Browser Demand Query Semantics Are Still Too Soft

Demand-loaded unbounded `find().exec()` calls currently fall back to the default
window limit, which is a performance guard but not a correct API contract for
callers that expect complete result sets. IndexedDB also has row-visit risks
outside the existing `allDocuments()` counter: bounded collection cursors,
unsupported `count()` fallbacks, and complex live-query re-exec need explicit
budgets and release gates.

### 10. Module UI Hot Paths Remain

Matching, Outbound, Spreadsheet, Buchhaltung, Customers, Chat DOM, and drag
layout paths still contain record-wide recompute/re-render or layout-thrash
patterns. These are not the likely root of daemon idle CPU with the browser
closed, but they still violate the Business OS performance target.

## Optimization Plan

### P0 - Installed Idle Evidence Gate

Tasks:

1. Add a release/manual CI step that runs the three gates below against the
   installed daemon after `ctox upgrade --dev`.
2. Gate A, passive idle CPU:
   - no `ctox status`;
   - no process-mining;
   - no browser automation during sampling;
   - resolve the installed `ctox-real` PID before sampling and pass `--pid`;
   - run `src/tools/perf/ctox_perf_probe.py --assert-idle --skip-status`
     for at least 5 minutes, 10 minutes for release scenarios.
3. Gate B, status poll load:
   - run `ctox status --json` at a realistic polling rate separately from
     Gate A;
   - sample daemon CPU, DB growth, status p95, lifecycle/process-scan counters,
     Business OS snapshot counters, and service performance deltas;
   - fail if status polling itself creates sustained daemon work.
4. Gate C, process-mining/liveness:
   - run before or after idle gates, never inside Gate A;
   - include `ctox process-mining spawn-liveness` and the relevant clean guard;
   - record DB growth separately because process-mining scans may write
     coverage/audit rows by design.
5. Store artifacts for:
   - fresh daemon, no browser;
   - Business OS open and synced;
   - after file access grant;
   - File Viewer materialize/read;
   - Explorer file upload/import;
   - Documents and Spreadsheets blob upload/read;
   - CV Print Builder open and idle;
   - 10 minutes no input after warmup.
6. Capture:
   - git commit;
   - build/release id;
   - installed binary path/current symlink;
   - `ctox-real` PID;
   - DB/WAL/SHM sizes before/after;
   - heartbeat loop deltas;
   - desktop-file indexer root-stamp ticks, stat calls, fallback scan runs,
     and maintenance rows;
   - SQLite fallback/write-lock counters;
   - IndexedDB fallback/row-visit counters;
   - active demand collector and leaked lease counts;
   - subject lag counters.
7. For any over-budget scenario, collect `sample` or `spindump` before
   declaring another code fix.

Acceptance:

- average CPU below 2 percent over 5 minutes;
- 10-minute no-input p95 below 5 percent CPU;
- `ctox status --json` p95 below 100 ms;
- no DB/WAL monotonic growth during idle;
- no fallback rows, projection rows, file stat scans, writer-lock waits, or
  subject lag counters increasing continuously while sources are unchanged.

Implementation status:

- Done on 2026-06-27 for the probe contract needed by Gate A/Gate B:
  `ctox_perf_probe.py` now keeps passive CPU sampling status-free, exposes
  service-status performance deltas from separate status samples, and does not
  apply the default status-p95/service-status-delta budgets when
  `--assert-idle --skip-status` is used for passive idle measurement.
- Done on 2026-06-27 for channel-sync visibility in Gate B:
  `ServiceStatus.performance.channel_sync` exposes per-adapter attempts,
  activity runs, no-activity runs, and error runs; the perf probe can fail on
  `channel_sync.*.activity_runs`, `channel_sync.*.no_activity_runs`, or
  `channel_sync.*.error_runs` deltas when status sampling is enabled.
- Done on 2026-06-27 for the checked-in installed Gate A/B/C workflow:
  `src/tools/perf/ctox_installed_idle_gate.py` runs `ctox upgrade --dev`,
  resolves the installed `ctox-real` PID, writes artifacts under
  `runtime/perf/installed-idle-*`, runs Gate A passive idle with
  `ctox_perf_probe.py --assert-idle --skip-status`, runs Gate B status polling
  as a separate load while sampling CPU/DB/heartbeat/sync-run deltas, and runs
  Gate C `ctox process-mining spawn-liveness`. The local dry-run validation
  verifies artifact creation and planned Gate A/B commands without upgrading.
- Done on 2026-06-27 for installed release/PID identity:
  the installed gate writes `release-identity.json` with source git
  commit/branch/status, `ctox --version`, install manifest, `current` symlink
  target, current-release and shared-launcher `ctox-real` hashes, sampled
  process command/path/hash/start time, and upgrade timestamps. A real run now
  fails before Gate A when it cannot prove the sampled PID belongs to the
  installed release produced by the `ctox upgrade --dev` path.
- Done on 2026-06-27 for SQLite statement/write-lock timing evidence:
  the native RxDB SQLite runtime snapshot exposes statement elapsed
  total/max/buckets and writer-lock wait/held total/max/buckets. The default
  idle probe budgets now fail on statement execution, statement elapsed time,
  writer-lock wait time, and writer-lock held buckets during passive idle.
- Done on 2026-06-27 for external status quiet proof:
  the daemon increments service-side status request counters for live IPC and
  HTTP status requests and writes `runtime/service-performance.status.json`
  with process PID/boot identity. Gate A reads that artifact around passive CPU
  sampling and fails on missing artifacts, wrong PID, boot-ID changes, negative
  counter deltas, or any `status_requests.*` growth, so another terminal, UI,
  or poller can no longer create hidden status work during a supposedly
  status-free idle sample. Gate B skips this passive file-delta assertion
  because it deliberately runs status polling as load, but it now separately
  requires that load to appear as daemon `status_requests.total_requests`
  growth.
- Partial on 2026-06-27 for the installed path:
  `ctox upgrade --dev` installed `branch-main-20260627T101453Z`; a short
  status-free passive probe against PID `32384` measured avg 0.15 percent CPU,
  p95 0.73 percent CPU, max 1.5 percent CPU. This is not sufficient for release
  because it did not cover the full file-access/Business-OS warmup matrix and
  still had non-CPU strict gate failures.
- Still open: real installed artifacts from running the full workflow after the
  next `ctox upgrade --dev` and file-access/Business-OS warmup scenario.

### P0 - No-op Sync DB Churn And Broad Stamp Gates

Tasks:

1. Add read-only diagnostics to count `ticket_sync_runs` and
   `communication_sync_runs` before/after a 60-120 second passive idle window.
2. Add counters for no-op ticket/channel sync attempts, sync-run writes, and
   unchanged-provider fetch windows.
3. Add a due/backoff gate for ticket sync comparable to channel-sync, with an
   explicit no-activity path.
4. Stop recording high-frequency no-op sync-run rows into DBs whose stamps drive
   unrelated idle gates. If a last-check timestamp is needed, keep it in a
   narrow runtime-state row or typed runtime store that does not invalidate
   projection/queue/scheduler source stamps.
5. Reclassify channel-sync activity so unchanged provider fetches
   (`fetchedCount > 0`, `storedCount = 0`, no new high-water) do not reset the
   short polling interval.
6. Replace broad Core/Ticket/Communication DB-stamp gates in durable queue,
   native peer projections, and any remaining scheduler/app-recovery-adjacent
   maintenance with source-specific high-water cursors or dirty-ID sets.
7. Add regression tests proving an unrelated sync-run metadata write does not
   reopen expensive queue, scheduler, or projection work.
8. Fix ticket-event idempotence so an unchanged adapter event never rewrites
   `ticket_events`, never resets an already-progressed
   `ticket_event_routing_state`, and never records another sync-run row.
9. Add regressions for identical adapter events in `leased`, `handled`,
   `blocked`, and `failed` routing states.

Acceptance:

- passive idle has zero recurring sync-run inserts when no source changed;
- no-op sync checks do not dirty DB stamps that other idle gates watch;
- projections and repair loops do no table-sized work after no-op sync checks;
- perf artifacts correlate CPU with sync-run insert rates and show both flat
  after warmup.

Implementation status:

- Done on 2026-06-27 for ticket adapter no-op writes:
  `upsert_ticket_from_adapter` and `upsert_ticket_event_from_adapter` now
  compare existing canonical row state and return `changed=false` without
  updating `last_synced_at`, `observed_at`, or routing timestamps when the
  adapter payload is unchanged.
- Done on 2026-06-27 for ticket sync-run no-op churn:
  `apply_ticket_sync_batch` records `ticket_sync_runs` only when at least one
  ticket or event row actually changed. The translation-layer regression test
  applies the same batch twice and proves the second pass writes zero stored
  rows and does not append another sync-run heartbeat.
- Done on 2026-06-27 for progressed ticket-event idempotence:
  `upsert_ticket_event_from_adapter` now treats an identical event payload with
  an existing routing-state row as unchanged regardless of whether the route
  has progressed to `leased`, `handled`, `blocked`, or `failed`. Missing
  routing-state rows are still initialized. The translation-layer regression
  now forces each progressed state, reapplies the same sync batch, and proves
  `stored_event_count = 0`, `ticket_sync_runs` stays unchanged, and
  `route_status`/`updated_at` are not reset.
- Done on 2026-06-27 for communication sync-run no-op churn:
  `record_communication_sync_run` skips successful runs with `stored_count = 0`
  and an empty error. Failures and successful runs with stored rows still
  produce durable evidence.
- Done on 2026-06-27 for fetched-only channel activity classification:
  `channel_sync_result_has_activity` no longer treats `fetchedCount > 0` or
  `messages_fetched > 0` as activity when the corresponding stored counters are
  zero.
- Done on 2026-06-27 for service-level ticket-sync no-activity backoff:
  `sync_configured_tickets` now checks a per-source due gate before invoking
  configured ticket systems. Successful syncs with zero stored ticket rows,
  zero stored event rows, and zero resolved clarifications extend the
  no-activity backoff up to 15 minutes; activity, errors, settings changes, or
  a different root reset the gate. Manual ticket sync remains direct.
- Done on 2026-06-27 for service sync runtime visibility:
  `ServiceStatus.performance` now exposes `channel_sync` and `ticket_sync`
  counters with attempts, activity runs, no-activity runs, and error runs. The
  status parser remains backward compatible with older daemon responses that do
  not include the field.
- Done on 2026-06-27 for read-only sync-run idle diagnostics:
  `ctox_perf_probe.py` now snapshots `ticket_sync_runs` and
  `communication_sync_runs` before and after the CPU sampling window without
  invoking `ctox status`, reports `sync_run_delta.numeric_deltas`, and supports
  `--max-sync-run-delta`. `--assert-idle` now includes default zero-growth
  budgets for `*.ticket_sync_runs.row_count` and
  `*.communication_sync_runs.row_count`; unmatched tables warn rather than
  masking a measured delta.
- Done on 2026-06-27 for the first source-specific broad-stamp replacement:
  `durable_status_snapshot_cached` no longer keys the durable status cache off
  the whole Core DB/WAL/SHM file stamp plus whole ticket-store stamp. It now
  uses a source stamp composed of:
  - `communication_intake_source_stamp()` for queue/message/routing state;
  - a read-only LCM last-assistant-outcome stamp for the status outcome field;
  - a read-only ticket open-case status stamp for ticket previews.
  This prevents `communication_sync_runs`, `ticket_sync_runs`, and other
  unrelated metadata writes from forcing status reloads or LCM opens.
- Done on 2026-06-27 for the durable-queue empty-probe gate:
  `should_skip_idle_durable_queue_empty_probe` and
  `mark_idle_durable_queue_empty_probe` now key their backoff on the same
  communication source stamp instead of the whole Core DB file stamp. Sync-run
  metadata writes no longer reset the empty-queue backoff, while real queue
  task changes still reopen leasing.
- Done on 2026-06-27 for queue list/count cache invalidation:
  `list_queue_tasks` and `count_queue_tasks` now key their in-memory caches
  from `communication_projection_clock` instead of the channel DB/WAL/JOURNAL
  file stamps. `communication_sync_runs` metadata writes no longer invalidate
  queue status views; message/routing inserts, updates, and deletes still
  advance the clock and reopen the cached queries.
- Done on 2026-06-27 for channel-router preflight/tick broad-stamp removal:
  `should_skip_idle_channel_router_preflight`, `mark_channel_router_preflight_idle`,
  `should_skip_idle_channel_router_tick`, and `mark_idle_channel_router_pass`
  now use a `ChannelRouterSourceStamp` instead of whole Core/Ticket DB file
  stamps. The stamp combines the communication projection clock, due scheduled
  task summary, open document-report command summary, and router-relevant ticket
  tables. `communication_sync_runs` and unrelated Business OS SQLite churn no
  longer reopen the router; real queue work, document-report commands, and due
  schedule time still do.
- Done on 2026-06-27 for scheduler due-scan broad-stamp removal:
  `should_skip_emit_due_scan` and `mark_emit_due_scan` now use a
  `ScheduleDueGateStamp` over `scheduled_tasks` instead of the whole Core
  DB/WAL/SHM file stamp. Unrelated Core-DB table writes and
  `scheduled_task_runs` history writes stay cold after an empty due scan, while
  due `next_run_at` time and real `scheduled_tasks` source changes reopen the
  scheduler.
- Done on 2026-06-27 for Business OS app-recovery broad-stamp removal:
  `should_skip_idle_business_os_app_recovery` and
  `mark_business_os_app_recovery_ran` now use a
  `BusinessOsAppRecoverySourceStamp` instead of the whole Core DB file stamp.
  The source stamp reads only leased queue tasks that actually carry Business
  OS app metadata, tracks the next stale-lease due time, and includes a bounded
  artifact-tree fingerprint for those app targets. Unrelated Core-DB table
  writes, Business OS runtime-store churn, and non-app queue work no longer
  reopen the app-recovery gate; leased app work and app artifact changes still
  do.
- Done on 2026-06-27 for harness-audit broad-stamp removal:
  `should_skip_idle_harness_audit_tick` and `mark_harness_audit_tick_ran` now
  use a `HarnessAuditSourceStamp` instead of the whole Core DB file stamp. The
  source stamp reads only harness-audit inputs (`ctox_core_transition_proofs`,
  `ctox_process_events`, process-mining tables, `ctox_core_spawn_edges`) plus
  active `ctox_hm_findings`, and deliberately excludes the audit's own
  `ctox_hm_audit_runs` history table. Unrelated Core-DB writes and completed
  audit-run rows stay cold; process events and active findings still reopen the
  audit path.
- Guarded by
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox canonical_sync_batch_persists_through_translation_layer -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox communication_sync_run_recorder_skips_successful_noop_heartbeats -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_sync_activity_detection_covers_adapter_result_shapes -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox ticket_sync -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox service_status_reports_channel_sync_runtime_metrics -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox parse_service_status_accepts_missing_newer_fields -- --nocapture`,
  `python3 -m py_compile src/tools/perf/ctox_perf_probe.py`,
  `python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --skip-db --skip-heartbeat --max-sync-run-delta '*.ticket_sync_runs.row_count=0' --pretty | python3 -m json.tool >/dev/null`,
  a synthetic failing probe run that inserts one `ticket_sync_runs` row during
  the sampling window and exits non-zero with
  `sync_run_delta.numeric_deltas.core.ticket_sync_runs.row_count = 1`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_ticket_state_idle_gate_skips_unchanged_source -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox durable_status_snapshot -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox idle_durable_queue_empty_gate_ignores_sync_run_metadata_churn -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_task_caches_ignore_sync_run_metadata_churn -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_task_list_cache_reuses_idle_reads_until_store_changes -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_task_count_cache_reuses_idle_reads_until_store_changes -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_router -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox emit_due_scan_gate_skips_idle_until_schedule_db_changes_or_due_time_arrives -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox emit_due -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox business_os_app_recovery_idle_gate_ignores_unrelated_churn_and_reopens_on_app_sources -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox app_recovery -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox harness_audit_idle_gate_ignores_unrelated_churn_and_reopens_on_audit_sources -- --nocapture`,
  and
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_run_metadata_churn -- --nocapture`.
- Still open: source-specific replacement for remaining broad
  Core/Ticket/Communication DB-stamp gates in ticket-self-work/workflow caches,
  progressive approval, and projection repair; native RxDB checkpoint status
  must move off writer-mutex reads;
  installed `ctox upgrade --dev` evidence must show sync-run deltas,
  service-sync counters, DB/WAL/SHM sizes, native loop counters, and CPU stay
  flat after file access and Business OS warmup.

### P0 - File Access Idle Architecture

Tasks:

1. Replace desktop-file polling with a file-system event dirty-root queue.
2. Keep the recursive fallback as rare safety work only, with explicit budget
   and counters.
3. Instrument:
   - root-stamp ticks;
   - roots scanned;
   - direct children stat'ed;
   - directories stat'ed;
   - files stat'ed;
   - rows written;
   - fallback scans;
   - chunk/index maintenance runs and rows scanned/deleted;
   - elapsed time per index pass.
4. Add tests for:
   - no recursive scan when source stamps are unchanged;
   - one changed file produces bounded dirty-root work;
   - missed event fallback is rare and budgeted.

Acceptance:

- granting file access does not create sustained periodic recursive work;
- idle after file access shows zero file-index row work after warmup;
- fallback scans are visible and fail the idle gate if continuous.

### P0 - Service Router Source Clocks And Due Gates

Tasks:

1. Split `route_external_messages` into source-specific stages:
   - scheduler due emission;
   - ticket sync due checks;
   - channel sync due checks;
   - queue/inbound routing;
   - durable queue leasing;
   - app-recovery validation;
   - audit/approval maintenance.
2. Move due-driven work out from behind whole-Core-DB idle skips. A scheduler
   or ticket source with an expired `next_due_at` must wake on its own source
   clock even if the Core DB did not otherwise change.
3. Replace `core_db_change_stamp` in router preflight and router tick with a
   composite source stamp:
   - communication projection clock for queue/inbound/routing state;
   - schedule clock for scheduled tasks and schedule runs;
   - ticket source/self-work/workflow clocks for ticket routing and ticket
     maintenance;
   - settings/env overlay hash for configuration changes;
   - worker/lease state for currently active in-memory work.
4. Add domain clocks or narrow read-only stamps for:
   - scheduler (`scheduled_tasks`, `scheduled_task_runs`);
   - queue tasks and `communication_routing_state`;
   - ticket self-work and workflow-ready state;
   - Business OS app-recovery leased tasks and validation artifact state;
   - harness/process evidence for audit ticks;
   - progressive approval-gate auto-close candidates.
5. Coalesce failure/noop maintenance writes:
   - ticket-sync failures by `(system, error_class)`;
   - harness-audit noop runs;
   - repeated app-recovery scans with unchanged leased-task/artifact source
     state.
6. Add regression tests proving unrelated Core-DB metadata writes do not reopen:
   - router preflight;
   - router tick;
   - scheduler emit;
   - Business OS app recovery;
   - harness audit;
   - progressive approval auto-close.
7. Add router/stamp counters for:
   - `router.tick`;
   - `router.preflight_skip`;
   - `router.stage_skip`;
   - `stamp_ms.communication`;
   - `stamp_ms.schedule`;
   - `stamp_ms.document_reports`;
   - `stamp_ms.tickets`;
   - read-only SQLite open count.
8. Replace progressive approval auto-close polling with a source stamp keyed by
   open approval-gate count and the latest relevant `ticket_self_work_items`
   timestamp.
9. Add large-source guard tests or metrics for scheduler and harness-audit
   aggregate stamps, including rows considered and elapsed time.

Acceptance:

- due-driven work wakes because its own source says it is due, not because an
  unrelated DB file mtime changed;
- unrelated writes to sync-run/audit/status tables do not reopen router stages;
- each router stage reports a source-stamp delta or due timestamp when it runs;
- idle artifacts show router, scheduler, recovery, approval, and audit counters
  flat after warmup.

Implementation status:

- Partial on 2026-06-27: durable status and durable-queue empty-probe source
  stamps are no longer global Core-DB stamps.
- Partial on 2026-06-27: queue task list/count caches now use the trigger-fed
  communication projection clock instead of DB/WAL/JOURNAL file stamps.
- Partial on 2026-06-27: router preflight/tick now use a composite source
  clock over communication, schedule, document-report command, and ticket
  router sources instead of whole Core/Ticket DB file stamps.
- Partial on 2026-06-27: scheduler due emission now uses a
  `scheduled_tasks` source stamp instead of whole Core DB file stamps, and
  due time still wakes the scheduler.
- Partial on 2026-06-27: Business OS app recovery now uses leased app queue
  tasks plus bounded app artifact stamps instead of whole Core DB file stamps.
- Partial on 2026-06-27: harness audit now uses a source stamp over audit
  inputs and active findings, and ignores unrelated Core-DB writes plus its own
  audit-run history table.
- Open: progressive approval auto-close and ticket-self-work/workflow caches
  still need the source-clock split above.

### P0 - Native And Browser Query Fallback Discipline

Tasks:

1. Native:
   - wire the existing `query_planner.rs` candidate narrowing into SQLite where
     safe;
   - preflight unsupported `count()` before scanning;
   - reject unsupported selectors on interactive/WebRTC/UI paths;
   - keep slow Rust matcher scans only for explicit maintenance.
2. Browser:
   - turn strict `allDocuments()` fallback rejection on for hot/dev paths;
   - make strict fallback rejection the production default for
     high-cardinality collections;
   - add row counters and budgets for bounded collection cursor scans and
     unsupported `count()` fallback scans;
   - add indexes or explicit unsupported-query failures for every rejected
     selector that legitimate UI code needs;
   - add generated schema indexes for `desktop_files` browsing paths, at
     minimum `parent_id`, `[parent_id, sort_index]`, and the required
     source/kind/deletion filters;
   - add exact query-plan guards for direct demand chunk SQL, not only Mango or
     schema-index expressions;
   - move Explorer/Importer folder browsing to indexed/windowed parent queries
     instead of loading all `desktop_files` and filtering locally;
   - keep `allDocuments`, bounded cursor, count fallback, and complex
     live-query counters in perf smokes.
3. Release:
   - fail idle probe when fallback rows grow while idle.

Acceptance:

- hot collections cannot silently full-scan on unsupported selectors;
- unsupported queries fail clearly before reading the whole collection;
- expected list/filter/count queries use primary, schema-index, LWT, or bounded
  cursor plans.

### P0 - Browser Demand Query Semantics

Tasks:

1. Forbid implicit complete-result semantics for demand-loaded unbounded
   `find().exec()` calls, or require an explicit paged API at every caller.
2. Audit hot Business OS callers and add limits/windows where a partial window
   is intended.
3. Add failing tests for demand-loaded `find().exec()` without limit on
   collections where full results would be ambiguous or unbounded.
4. Expose demand-window misses, page counts, and row-visit totals in the
   browser idle/perf probe.

Acceptance:

- no caller can accidentally treat the default demand window as a full
  collection read;
- large demand-loaded collections are consumed through explicit pages, keyed
  reads, or file/range APIs;
- release idle artifacts prove demand windows and row visits stop increasing
  when no source changed.

### P0 - Chunk Bridge And Browser File Writes

Tasks:

1. Convert all explicit chunk users to scoped `leaseCollection()` with
   `try/finally` release.
2. Replace direct `startCollection('desktop_file_chunks')` callers in file and
   command flows with leased ownership.
3. Batch browser chunk/blob writes instead of sequential per-chunk writes.
4. Convert Explorer, Documents, Spreadsheets, Research, Universal Importer, and
   file-integrity paths to the same scoped lease/bulk/range discipline.
5. Add smokes proving Explorer upload, Documents/Spreadsheets blob write/read,
   Research blob read, Universal Importer, CV Print Builder, file integrity,
   and command-bus attachment paths do not keep chunk collections alive after
   work.
6. Add source guards that fail on new full-file DataURL upload paths and
   per-chunk write loops in chunk/blob collections.

Acceptance:

- `desktop_file_chunks`, `document_blob_chunks`, and `spreadsheet_blob_chunks`
  are never normal idle collections;
- chunk bridge lifetime is visible and bounded;
- large file writes produce a bounded burst, not a long idle tail.

Implementation status:

- Done on 2026-06-27 for central `desktop_file_chunks` ownership:
  Command Bus file-content dependencies, Business Chat attachments, CV Print
  Builder file sync/dispatch flush, and App Store ZIP uploads now use scoped
  `leaseCollection()` handles and release them in `finally`.
- Done on 2026-06-27 for the first browser chunk-write batching pass:
  CV Print Builder and App Store ZIP uploads use collection `bulkUpsert()`
  when available instead of one write per chunk.
- Done on 2026-06-27 for the next browser chunk/blob write batching pass:
  Explorer uploads no longer materialize files through DataURL and now write
  `desktop_file_chunks` through one `bulkUpsert()` call; Documents and
  Spreadsheets now write `document_blob_chunks` and `spreadsheet_blob_chunks`
  through one `bulkUpsert()` call instead of one insert per chunk.
- Guarded on 2026-06-27 by source search and smokes:
  no literal direct `startCollection('desktop_file_chunks')` calls remain in
  `src/apps/business-os/shared`, `src/apps/business-os/modules`, or
  `src/apps/business-os/desktop-apps`; `assert-rxdb-only.mjs` now also rejects
  Explorer upload DataURL materialization and requires the direct
  byte-hash/bulk-write path.
- Still open: broad chunk-stream/range APIs and any remaining non-central
  upload/import paths outside the covered Command Bus, Chat, CV Builder, and
  App Store flows. The explicit open targets from the 2026-06-27 subagent pass
  are now narrowed to blob/chunk read/range APIs, Research blob reads,
  Universal Importer/file-integrity helpers, and any remaining non-central
  upload/import paths not covered by Explorer, Documents, Spreadsheets, Command
  Bus, Chat, CV Builder, or App Store.

### P1 - File-Demand Range And Materialization

Tasks:

1. Make file-demand in-flight de-duplication range-aware instead of keyed only
   by `fileId`.
2. Batch persisted file-demand chunks in one write where persistence is
   enabled.
3. Add schema-aware materializers for non-desktop chunk collections, or avoid
   persisting generic `file_id/sequence/bytes_base64` rows into collections
   with incompatible schemas.
4. Replace native explicit desktop-file materialization that reads the whole
   file into memory, base64-encodes all chunks, builds full `Vec<Value>` chunk
   documents, and then writes them to SQLite with a streaming chunker and
   bounded batch writes.
5. Add browser `fetchFileStream()` / `ReadableStream` semantics so file-demand
   consumers do not collect every frame into an array and then concatenate a
   full `Uint8Array`/`Blob` before import or integrity checks.
6. Make Universal Importer and file-integrity helpers range/stream-first with
   explicit full-file caps.
7. Add storage-source metrics for rows loaded before first chunk, time to first
   chunk, and `EXPLAIN QUERY PLAN` on non-desktop blob chunk reads.
8. Rewrite browser-side blob chunk readers to use indexes/PK ranges and
   streaming consumers.
9. Add exact browser IndexedDB/query smokes for `document_blob_chunks` and
   `spreadsheet_blob_chunks` reads by `blob_id` and `idx`.

Acceptance:

- concurrent range reads cannot receive the wrong de-duplicated response;
- file-demand persistence does not create one transaction per received chunk;
- non-desktop blob chunks use indexed/range/manifest reads instead of generic
  JSON extraction scans.
- large explicit file materialization has bounded memory and bounded SQLite
  write batches; full-file reads are capped and visible in perf artifacts.

Implementation status:

- Done on 2026-06-27 for browser file-demand in-flight ownership:
  `createFileDemandLoader()` now keys in-flight fetches by `fileId` plus a
  canonical range key. Concurrent different ranges for the same file no longer
  share a promise; concurrent equivalent ranges still deduplicate.
- Done on 2026-06-27 for browser file-demand persistence batching:
  persisted file-demand chunks are written through one `bulkWrite()` call per
  fetch instead of one transaction per received chunk.
- Guarded by `file-demand-loader-smoke.mjs`, which now asserts one persisted
  write batch, different-range non-deduplication, and canonical same-range
  deduplication. Bundle reproducibility, `assert-rxdb-only.mjs`, and the full
  RxDB suite also passed after the rebuild.
- Done on 2026-06-27 for native generic demand chunk reads:
  `demand_file_chunk_rows_for_key_from_sqlite` now reads deterministic
  chunk-ID prefix ranges with `deleted = 0` and `ORDER BY id` instead of
  filtering and sorting with `json_extract` / `CAST(COALESCE(...))`.
  Desktop range fetches now keep the loaded chunk window's base byte offset,
  so range clipping remains correct when only the touched chunk IDs are loaded.
- Guarded on 2026-06-27 by
  `hot_business_os_schema_indexes_have_sqlite_query_plan_guards`,
  `demand_file_source_streams_decoded_chunks_in_idx_order`, and
  `demand_file_source_streams_blob_chunks_by_primary_key_prefix`.
- Still open: schema-aware materializers or no-persist policy for incompatible
  non-desktop chunk collections, plus native storage-source metrics for rows
  loaded before first chunk, time to first chunk, and `EXPLAIN QUERY PLAN`.

### P1 - Projection Delta Windows And Writer Batching

Tasks:

1. Replace aggregate queue/chat repair stamps with persisted changed-id or
   high-water cursors.
2. Move global repair to explicit maintenance command, never idle-loop side
   effect.
3. Batch native queue/chat command lookups where per-row paths remain.
4. Expand projection writer cache to direct command/status/file/release paths.
5. Move checkpoint status reads off the shared writer mutex or serve them from
   cached/read-only state.
6. Add writer-lock wait/held p95 counters.
7. Replace remaining broad Core-DB stamp gates for progressive approval,
   workflow/self-work caches, and projection repair with
   source-specific high-water marks.
8. Add tests proving unrelated Core-DB writes do not reopen expensive idle
   loops.

Acceptance:

- unchanged projection loops do zero table-sized work;
- a single stale projection repairs only the related dirty IDs;
- writer-lock counters stay flat while idle.

### P1 - DB Growth, Retention, WAL, And Freelist Policy

Tasks:

1. Define replication-horizon-safe physical delete policy for tombstones and
   chunk/blob payloads.
2. Measure tombstones, live rows, payload bytes, WAL, SHM, page count, freelist
   count per relevant DB.
3. Add a native-only desktop chunk-cache policy in typed runtime/store config,
   not an env toggle. Required fields: max live bytes, target live bytes, TTL,
   minimum candidate age, max chunks per pass, checkpoint throttle, WAL
   checkpoint threshold, vacuum throttle, and minimum reclaim bytes.
4. Persist native maintenance state in `business-os-rxdb.sqlite3`, outside RxDB
   replication collections, with last eviction/checkpoint/vacuum times and
   last live/pinned/deleted byte counts.
5. Run quota/TTL/cache cleanup only from the existing long maintenance cadence,
   never from the 15 second idle scan loop.
6. Keep active `desktop_files.content_generation_id` chunks pinned. If active
   pinned bytes exceed the quota, report over-quota pinned bytes and stop
   deleting; do not demote active files to lazy and do not delete active chunks
   to satisfy quota.
7. Avoid read-time writes such as `last_accessed_at_ms`; TTL must use
   `created_at_ms` plus active-generation protection so File Viewer reads do
   not create an idle write tail.
8. Add range-index or manifest-based access for non-desktop blob/chunk
   collections.
9. Add `EXPLAIN QUERY PLAN` guards for stale-live chunk cleanup and blob fetch
   paths.
10. After physical deletes, run no checkpoint on no-op maintenance; use
   `wal_checkpoint(PASSIVE)` only after enough deleted bytes and throttle
   interval; reserve `TRUNCATE` or full `VACUUM` for large reclaimable freelist
   windows with no demand fetch active.

Acceptance:

- DB size growth is explainable by live retained data;
- stale/tombstoned payload bytes have a bounded horizon;
- idle probe artifacts include DB-size deltas and fail on unexplained growth;
- active File Viewer demand fetch still succeeds after cleanup;
- quota deletes stale/orphan generations until target bytes but never active
  generations;
- no-op maintenance performs no checkpoint/vacuum and does not rewrite
  `desktop_files` metadata.

### P1 - Browser Demand Stream Abort And Lifecycle

Tasks:

1. Abort query and file demand collectors when a peer disappears after request
   acceptance but before final chunk.
2. Add file-demand cancel semantics matching query abort.
3. Add smoke tests for peer close after request ack.
4. Expose demand collector pending counts in diagnostics.
5. Put hard frame and byte budgets on browser and native WebRTC send queues.
6. Apply buffered-amount/backpressure handling before native inline sends, not
   only framed sends.
7. Make query/file in-flight acceptance atomic with CAS or a semaphore, then
   add a concurrent barrier test proving the configured cap cannot be exceeded.

Acceptance:

- peer loss does not leave demand collectors or chunk leases alive;
- pending collector count returns to zero after disconnect.
- slow peers cannot accumulate unbounded queued frames or bytes.
- concurrent query/file accepts cannot exceed the configured in-flight cap.

Implementation status:

- Done on 2026-06-27 for peer-loss collector abort:
  the demand transport now tracks the active peer for query and file collectors
  and rejects both with non-retryable cancellation errors when that peer closes.
- Done on 2026-06-27 for file-demand cancel semantics:
  file demand loader in-flight slots retain request IDs, expose
  `abortAllInFlight()`, call the transport's `rxdb.file.cancel` hook, and clear
  dedup state immediately on reconnect/cancel paths.
- Done on 2026-06-27 for replication lifecycle wiring:
  shared peer close, collection peer removal, and replication cancel now abort
  query/file demand work instead of leaving collectors alive until an outer
  timeout.
- Guarded by `demand-loading-transport-smoke.mjs`,
  `file-demand-loader-smoke.mjs`, `replication-recovery-smoke.mjs`, bundle
  reproducibility, `assert-rxdb-only.mjs`, and the full RxDB suite.
- Still open: expose pending query/file collector counts in release diagnostics,
  bound WebRTC send queues, make in-flight limits atomic, and run the installed
  idle probe after the `ctox upgrade --dev` path.

### P1 - Channel Sync And Idle Backoff

Tasks:

1. Treat unchanged `pairing` payloads as no activity by comparing stable
   signatures or source high-water marks.
2. Add a regression test proving repeated identical pairing payloads back off
   instead of resetting to the base polling interval.
3. Add channel-sync activity counters to the idle probe so adapters that keep
   producing unchanged payloads are visible.

Acceptance:

- unchanged channel state cannot keep CTOX on a short polling interval;
- backoff reaches the long idle interval when no channel data changes.

Implementation status:

- Done on 2026-06-27 for unchanged pairing payload activity detection:
  `channel_sync_result_has_activity` no longer treats any non-null `pairing`
  object as activity. Static WhatsApp-style `pairing_required` / `qr` artifact
  payloads now increment the no-activity backoff; only explicit pairing change
  markers such as `changed`, `updated`, `started`, `qr_updated`, or non-zero
  update counters reset the interval.
- Done on 2026-06-27 for regression coverage:
  `channel_sync_due_gate_backs_off_unchanged_pairing_payloads` proves repeated
  identical pairing artifacts back off, and the shared due-gate tests now hold a
  serial test lock so the global in-memory due gate is not corrupted by
  parallel Cargo test execution.
- Guarded by
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_sync_due_gate -- --nocapture`
  and
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_sync_activity_detection_covers_adapter_result_shapes -- --nocapture`.
- Done on 2026-06-27 for service/probe visibility:
  `ctox status --json` now includes `performance.channel_sync` counters per
  adapter, and `ctox_perf_probe.py` captures status performance deltas from
  separate status samples. When status sampling is enabled, the idle assertions
  can fail on `channel_sync.*.activity_runs`, `channel_sync.*.no_activity_runs`,
  or `channel_sync.*.error_runs` deltas.
- Guarded additionally by
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox service_status_reports_channel_sync_runtime_metrics -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox parse_service_status_accepts_missing_newer_fields -- --nocapture`,
  `python3 -m py_compile src/tools/perf/ctox_perf_probe.py`, and synthetic
  probe assertion checks for status-delta failure plus passive
  `--assert-idle --skip-status` success.
- Still open: installed `ctox upgrade --dev` artifact showing these counters
  stay flat after warmup.

### P2 - Business OS Module Hot Paths

Tasks:

1. Matching:
   - build `matchesByRequirementId`, `matchesByObjectId`, and `objectsById`;
   - cache normalized haystacks by data version;
   - debounce searches;
   - reconcile DOM instead of full teardown.
2. Outbound:
   - compute current pipeline/company lists once per render;
   - build `pipelineByCompanyId`;
   - coalesce subscription-triggered reloads.
3. Spreadsheet:
   - keep one HyperFormula instance;
   - apply `setCellContents`;
   - update only changed cells.
4. Buchhaltung and Customers:
   - pre-aggregate maps;
   - debounce search;
   - avoid unrelated pane renders.
5. Shell:
   - replace Chat full-HTML no-op diff with signatures/append-only reconcile;
   - batch drag/resize layout reads and writes.

Acceptance:

- add 1k-record module perf smokes where feasible;
- keystroke handlers do not scan all records synchronously;
- broad subscriptions do not trigger full reloads for unrelated changes.

### P3 - Mail, Inference, Execution, Mission Tail

Tasks:

1. Mail:
   - finish IDLE/provider-delta support;
   - make first import paged and bounded;
   - verify all message/mailbox hot paths stay on cached connections.
2. Inference:
   - reuse ggml descriptor arenas;
   - reuse graph/context where decode shape permits;
   - add per-token host-overhead benchmarks.
3. Execution gateway:
   - batch cost telemetry writes;
   - reduce transcript clone/serialize work in provider adapters.
4. Mission/report:
   - batch remaining ticket/queue helper hydration;
   - remove or quarantine orphaned report module islands.

Acceptance:

- these paths no longer dominate profile samples under their active workloads;
- none of them participates in background idle loops unless scheduled work is
  actually due.

## Immediate Work Order

1. Land the installed `ctox upgrade --dev` Gate A/B/C workflow before another
   release claim: passive idle without status, status-poll load separately,
   process-mining separately.
2. Verify and fix no-op sync DB churn: `ticket_sync_runs`,
   `communication_sync_runs`, unchanged provider fetch windows, and broad
   DB-stamp idle-gate invalidation.
3. Replace desktop-file polling with dirty-root event scheduling and prove file
   access grant does not create sustained recursive/stat work.
4. Make native/browser unsupported query fallbacks fail in hot paths, add row
   budgets for bounded cursors/count fallbacks, and add `desktop_files`
   parent/window indexes.
5. Define explicit paged semantics for demand-loaded unbounded queries.
6. Finish file/chunk streaming after the native demand chunk SQL fix: native
   materialization streaming batches, browser `fetchFileStream()`, Universal
   Importer/file-integrity range-first, Research blob reads, browser blob chunk
   indexes, and hard full-file caps.
7. Move projection repairs to dirty-id/high-water windows and replace broad
   Core/Ticket/Communication DB idle gates with source-specific high-water
   marks.
8. Add DB retention/horizon policy, byte budgets, physical delete/checkpoint
   strategy, and WAL/freelist gates.
9. Bound WebRTC send queues and make query/file in-flight limits atomic.
10. Expose demand collector pending counts and remaining native RxDB counters in
   release diagnostics.
11. Add a no-op sync idle gate that samples `ticket_sync_runs`,
   `communication_sync_runs`, channel-sync counters, DB/WAL/SHM sizes, and
   broad-stamp invalidation counters before and after a 5-10 minute idle
   window.
12. Fix Matching and Outbound first among module UI hot paths because they are
   the remaining named HIGH UI findings.
13. Only after the above, push/release and collect installed idle artifacts.

## Answer To "Is Everything From 2026-06-24 Handled?"

No.

Everything important from the 2026-06-24 review is now represented in the plan
set, but the system is not performance-complete and not idle-clean by evidence.

The strongest remaining design issue is not one single SQLite bug anymore. It
is the combination of:

- unsupported query fallbacks that can still scan;
- broad DB/WAL/SHM stamp gates that can turn small no-op changes into repeated
  audit/sync/projection work where source-specific stamps are still missing;
- periodic/stamp-based file and projection work that is reduced but not fully
  event-driven;
- DB growth/retention without a replication-horizon contract;
- demand/file lifecycles that still need strict scoped ownership;
- no installed-daemon idle gate after `ctox upgrade --dev`.

That is the structural optimization target.
