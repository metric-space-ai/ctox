# CTOX Performance Optimization Plan - 2026-06-26

## Verdict

No, the review at
`/Users/michaelwelsch/Documents/ctox/docs/ctox-performance-review-2026-06-24.md`
is not fully handled.

The worst previously suspected idle-spin root, file-backed RxDB SQLite
per-collection safety drains, is fixed in the current code: file-backed
collections no longer run periodic per-collection `changed_documents_since`
safety polls while idle. Several other hot paths are also materially reduced.

That does not make the system release-clean. Follow-up fixes on 2026-06-26
removed the automatic `desktop_file_chunks` wakeup for plain
`ctox.file.materialize` commands with `payload.file_id`, changed
materialized/eager file maintenance to use matching
`generation_verified_at_ms` metadata before falling back to chunk completeness
scans, and moved desktop-file index maintenance/filesystem scans out of the
native RxDB write lock while narrowing unsafe-file maintenance to indexed live
core file candidates. The current code still has structural performance risks
that can plausibly explain CPU growth after file access/materialization:

- installed idle evidence is still missing for the file-viewer and CV Builder
  scenarios;
- Business OS projection source stamps still scan or hash table-sized row sets
  on idle cadence;
- queue/chat repair no longer turns a small local queue reconcile into global
  `repair_queue_projections` fanout, but it still needs cursor-based repair
  windows;
- tombstoned chunks are logically bounded but can remain as physical DB bloat;
- direct RxDB projection writes still reopen SQLite outside the new writer
  cache in several command-status paths;
- there is no enforced daemon idle CPU regression gate yet.

Until the P0/P1 items below are implemented and verified with an installed
`ctox upgrade --dev` build, CTOX must not be described as idle-clean.

## Method

This plan was produced from:

- root guidance: `README.md`, `HARNESS.md`, `docs/architecture.md`,
  `AGENTS.md`;
- data-plane guidance: `docs/ctox-rxdb.md`,
  `src/core/rxdb/AGENTS.md`, `src/apps/business-os/rxdb/AGENTS.md`,
  `src/core/business_os/AGENTS.md`;
- the external performance review dated 2026-06-24;
- the existing optimization plan
  `docs/ctox-performance-optimization-plan-2026-06-25.md`;
- four read-only subagent reviews:
  native RxDB/SQLite, Business OS core store/projections, browser RxDB/WebRTC,
  and daemon-wide idle loops;
- direct code inspection of the cited paths.

## What Is Already Fixed Or Strongly Reduced

These items are substantially addressed in current code, though some still
need release guards or broader cleanup:

- Native RxDB query/count/query-fetch: common selectors compile to SQL,
  `LIMIT`/`OFFSET` are pushed for compiled queries, compiled counts use SQL,
  and query-fetch refuses unsupported streaming fallback instead of scanning.
  Relevant files: `src/core/rxdb/src/storage/sqlite/sql.rs`,
  `src/core/rxdb/src/storage/sqlite/instance.rs`.
- Read concurrency: file-backed queries, changed-since reads, and external
  polling use read-only SQLite connections instead of the shared writer mutex.
- Native `bulk_write` current-state reads: large write batches now load
  existing rows through one batched ID lookup instead of one `document_by_id`
  point query per written document.
- File-backed external write idle polling: file-backed RxDB instances no
  longer run per-collection safety drains while idle. In-memory test fallback
  still keeps the safety interval.
- WebRTC diagnostics: transport status is throttled/coalesced, default status
  is skinny, and heavy diagnostics require explicit status reads.
- Desktop chunk generation churn: unchanged file rescans are no-ops, eager
  chunk writes use bulk upsert, stale generation prune uses primary-key range
  lookups, verified materialized/eager rescans skip chunk completeness scans
  by checking generation metadata first, and demand file reads use
  `desktop_files` file fetch instead of normal chunk replication.
- Desktop file index maintenance: maintenance and bounded filesystem scans now
  run before taking the native RxDB write lock, and unsafe-file maintenance
  queries indexed live `ctox-core` file candidates with risky path prefixes
  instead of deserializing every live `desktop_files` row. Deleted chunk
  tombstone cleanup now has a dedicated partial index.
- Demand-cache invalidation: reverse document-to-window refs exist and remote
  write invalidation is batch-coalesced.
- Store fsync pressure: Business OS store and core persistence set
  `PRAGMA synchronous=NORMAL` for checked central paths.
- Status path pressure: normal process scans are cached and status snapshots no
  longer run Business OS app recovery on the UI cadence.
- Mail exact findings: mailbox/received index and header/body split work cover
  the exact IMAP FETCH/STORE and send-verification findings.
- Projection writer: repair/fanout paths now have `RxdbProjectionWriterCache`
  and `RxdbCollectionWriter` coverage for several batch-like paths.

## What Is Not Fully Handled

### P0 - Release Blockers For Idle-Clean Claims

1. Add an installed-daemon idle regression probe.
   - Capture CPU, wakeups, threads, DB reads/writes, SQLite opens/statements,
     RxDB fallback scans, projection-loop durations, file maintenance
     durations, and data-plane frame counts.
   - Done on 2026-06-26 for native evidence plumbing: the peer heartbeat now
     publishes loop-duration/work counters plus SQLite runtime counters, and
     `ctox_perf_probe.py` records heartbeat deltas during CPU sampling without
     invoking `ctox status`.
   - Done on 2026-06-26 for the probe gate: `ctox_perf_probe.py` now supports
     `--assert-idle`, default CPU/status/DB-growth/native-delta budgets,
     scenario-specific `--max-heartbeat-delta GLOB=VALUE` thresholds, and a
     non-zero exit code when an idle budget is exceeded.
   - Required scenarios:
     - fresh daemon, no browser;
     - Business OS open and synced;
     - file viewer materialize/read;
     - CV Print Builder open;
     - after a large file access grant;
     - 10 minutes of no user input.
   - Pass budget must be explicit. Initial target: sustained idle under 5%
     CPU for `ctox-real` after warmup, with no monotonic DB growth or
     continuous statement/open counters.
   - This is a release gate, not optional diagnostics.

2. Stop file materialization from waking `desktop_file_chunks`.
   - Done on 2026-06-26 for the command-bus auto-dependency path: plain
     `payload.file_id` now starts/flushes `desktop_files` only.
   - `desktop_file_chunks` is still allowed for explicit sync dependencies or
     attachment refs that actually identify chunk storage.
   - Remaining release evidence: installed file-viewer materialize/read idle
     probe proving `desktop_file_chunks` stays inactive unless a browser upload
     actually has chunk rows to push.

3. Remove normal CV Print Builder dependency on live `desktop_file_chunks`.
   - Done on 2026-06-26 for normal module readiness and subscriptions:
     `desktop_file_chunks` is no longer part of the required/live collection
     set.
   - Chunk sync remains explicit for PDF import and parser dispatch, where
     browser-origin chunks actually need to be pushed.
   - Remaining release evidence: installed CV Builder idle probe.

### P1 - Business OS Projection And File Maintenance

1. Replace table-sized projection source stamps.
   - Current Business OS record projection stamps hash selected
     `business_records` metadata rows on idle cadence.
   - Introduce persisted per-collection change generations/high-water cursors
     updated by writes.
   - Projection loops should be O(changed rows), not O(projected rows), when
     nothing changed.

2. Make queue/chat repair incremental.
   - Current gate is no longer the original unconditional 3 second scan.
     Queue reconcile is now status-selective and Chat tracking reconcile now
     uses indexed top-level `tracking_active` metadata instead of a broad
     `business_chats` page, but aggregate repair summaries remain.
   - Done on 2026-06-26 for local queue reconcile fanout: local
     `reconcile_ctox_queue_task_projections` no longer calls the global
     `repair_queue_projections` maintenance repair after writing local repaired
     documents, and a regression proves unrelated stale queue business records
     are left untouched.
   - Done on 2026-06-26 for queue reconcile candidate narrowing:
     `ctox_queue_tasks` reconcile now selects only active statuses and a
     regression proves active stale rows are not hidden behind a first page of
     terminal rows.
   - Done on 2026-06-26 for Chat tracking candidate narrowing:
     `business_chats` now carry indexed top-level tracking metadata, and native
     Chat tracking repair selects only `tracking_active = true` candidates. A
     regression proves active stale chats are not hidden behind 600 inactive
     chat documents.
   - Done on 2026-06-26 for active Chat tracking lookup batching: native repair
     collects all active `commandId`/`taskId` references first, loads
     `business_commands` and `ctox_queue_tasks` with two batched
     `find_documents_by_id` calls, and a regression proves 40 active messages
     do not run per-message projection lookups.
   - Remaining work: replace aggregate repair stamps with cursor-based repair
     windows.

3. Keep materialized/eager file verification order guarded.
   - Done on 2026-06-26: matching `generation_verified_at_ms`, generation id,
     and size metadata are checked before chunk completeness scans.
   - Regression tests now assert verified unchanged/materialized rescans do not
     call the chunk completeness checker.
   - Full chunk verification remains reserved for generation changes, missing
     metadata, explicit repair, or sampled maintenance.

4. Keep desktop-file maintenance after sharing bounded.
   - Done on 2026-06-26: desktop-file index maintenance and filesystem scan
     collection no longer run under the native RxDB write lock; the lock is
     taken only for the actual database sync write.
   - Done on 2026-06-26: unsafe-file compaction now uses the
     `ctox_business_os_desktop_files_live_core_idx` expression index and
     filters to risky path candidates before JSON deserialization.
   - Done on 2026-06-26: deleted `desktop_file_chunks` cleanup has a partial
     `deleted` index.
   - Remaining work: watcher/dirty-root triggering so the fallback scan is a
     safety path rather than the normal wakeup mechanism.

5. Add physical DB-growth control for chunk bloat.
   - Logical redaction/tombstoning exists, but physical deletes are bounded and
     deferred.
   - Add counters for live/deleted chunk rows, blanked payload bytes, DB file
     size, WAL size, freelist pages, and maintenance backlog.
   - Define safe checkpoint/vacuum policy for operator-initiated or quiescent
     maintenance. No hidden idle `VACUUM` loop.

6. Finish projection writer architecture.
   - Thread `RxdbProjectionWriterCache` through command completion/failure,
     control command acceptance, file/share/release command fanout, and other
     direct status helpers.
   - Add test-only and diagnostic counters for SQLite opens, `PRAGMA table_info`
     loads, statements, and rows touched per projection burst.
   - Batch remaining `push_collection_records` command paths where semantics
     allow.

### P2 - Native RxDB SQLite Architecture

1. Finish planner coverage or refuse unsafe broad fallbacks.
   - Current SQL compiler covers common selectors but unsupported shapes still
     fall back to Rust matcher scans in normal `query`.
   - Done on 2026-06-26: normal `query()` Rust matcher fallbacks now increment
     runtime counters for fallback calls and rows visited, so heartbeat/probe
     evidence can expose broad fallback scans.
   - Done on 2026-06-26: fallback `count()` now reports `mode = "slow"` instead
     of `"fast"`, which lets `RxQuery` reject it unless slow counts are
     explicitly allowed.
   - Remaining work: expand SQL compilation or reject/route unsupported normal
     `query()` shapes before UI paths can trigger large fallback scans.

2. Expand SQL compilation where needed.
   - Add the selector/operator shapes used by Business OS screens before they
     hit fallback.
   - Keep query-fetch strict: unsupported stream queries should return
     `QUERY_NOT_SUPPORTED`, not scan.

3. Finish connection/backpressure work.
   - Writes still serialize through the writer connection.
   - Done on 2026-06-26 for native `RxSubject`: bounded broadcast fanout,
     lag counters, storage lag markers, query-buffer invalidation, and
     replication `RESYNC` mapping replace per-subscriber unbounded queues.
   - Done on 2026-06-26: native peer performance snapshots expose
     `rxdb_subjects.lagged_items_total`, and `ctox_perf_probe.py --assert-idle`
     fails if that counter grows during the sample.
   - Done on 2026-06-26: targeted slow-peer recovery test proves a lagged
     master-change subscriber receives `RESYNC` and recovers all missed docs
     via `master_changes_since`.
   - Remaining: installed/integration slow-peer soak evidence.

### P3 - Browser RxDB, WebRTC, And IndexedDB

1. Done on 2026-06-26 for single-document IndexedDB `upsert`:
   it now returns the written document from one readwrite transaction, with a
   dist-level smoke guarding one existing-row read, one write, and no final
   re-read.
   Done on 2026-06-26 for IndexedDB `bulkUpsert`: the RxDB facade now calls
   storage `bulkUpsert()` once, storage merges/writes the batch in one
   readwrite transaction, and the dist-level smoke guards against per-document
   facade `upsert()` calls.

2. Replace full subscription re-query paths.
   - Done on 2026-06-26 for `collection.$` and `findOne(primary).$`:
     collection subscriptions apply change payloads to an in-memory snapshot,
     and primary-key `findOne` subscriptions ignore unrelated changed IDs.
   - Remaining: use indexed windows or explicit full-refresh fallbacks with
     counters for complex selector/sort query subscriptions.

3. Done on 2026-06-26 for local-write push trigger coalescing:
   local collection changes now schedule one short debounce before
   `pushToRemotePeers()`, repeated write bursts collapse into one push pass, and
   CTOX-origin-only replication writes do not schedule a local push scan.
   Remaining: add explicit scan/fallback counters and finish browser chunk upload
   batching so tiny deltas cannot cause backlog-proportional work.

4. Finish WebRTC low-level hot spots.
   - Reuse `TextEncoder` or avoid full payload encoding just for byte counts.
   - Track contiguous chunk sequence incrementally instead of recomputing from
     zero on every frame.

### P4 - Daemon Idle Timers And Status

1. Move fixed timer loops toward source/event scheduling.
   - Remaining fixed loops are guarded/backed off, not tight spins, but still
     wake on cadence.
   - Channel router, work-hours dispatcher, mission maintenance, app recovery,
     harness audit, and channel sync should sleep until a due timestamp or DB
     source change when practical.

2. Make status source-stamped instead of TTL-only.
   - Durable status still refreshes on a short TTL and can open `LcmEngine` on
     cache miss.
   - Derive from change stamps where possible.

3. Finish small daemon hot spots.
   - Cache/carry canonical root for working-hours lookup.
   - Capture process-mining "record SQLite reads" bool once when attaching the
     SQLite authorizer.
   - Keep lifecycle/process scans panel-scoped or explicit.

4. Improve email/channel steady state.
   - Add provider-specific IDLE/delta-token paths where available.
   - Handle UIDVALIDITY and first-import without recurring `UID SEARCH ALL`
     behavior.

### P5 - Business OS UI And Module Hot Paths

These are not the most likely cause of `ctox-real` native CPU burn, but they
are still open from the review and must be tracked:

- `business-chat.js`: batch command/task lookups and coalesce subscription
  plus 4 second polling triggers.
- Matching module: build maps for requirement/object/match lookup, cache
  normalized haystacks, debounce search, and reconcile DOM instead of full
  rebuilds.
- Outbound module: memoize pipeline/current company views and use
  `pipelineByCompanyId`.
- Spreadsheets: keep the HyperFormula engine alive and update changed cells.
- Buchhaltung/customers/conversations/CV search: pre-aggregate maps, debounce,
  and avoid full `innerHTML` rebuilds.
- Window manager/chat layout/reporter/startup progress: coalesce layout reads
  and remove permanent high-frequency idle timers.

### P6 - Other Review Areas

- Mailserver: reuse hot-path SQLite connections broadly, not just exact fixed
  summary paths.
- Execution gateway: done on 2026-06-26 for direct-session stream handling.
  Direct session inspects stream event method/payload type before cloning or
  deserializing and drops high-frequency no-op deltas early.
- Runtime/env/API costs: use existing caches and batch writes at turn
  completion.
- Inference: persistent ggml descriptor arenas and graph/context reuse need a
  separate benchmarked implementation plan.
- Mission/report: remove remaining DB reopen/N+1 hydration patterns and dead
  module islands.

## Coverage Matrix For 2026-06-24 High/Medium Findings

Legend: fixed, reduced, partial, open.

| ID | Status | Current coverage |
| --- | --- | --- |
| H1 | partial | Common native query/count/query-fetch paths compile to SQL or reject stream fallback; normal unsupported `query` fallback scans remain. |
| H2 | fixed for exact path | WebRTC status emissions are throttled/coalesced and heavy diagnostics are opt-in; release probes still need emit/fanout counters. |
| H3 | fixed for exact body path | Exact IMAP server FETCH/STORE body overfetch is fixed; broader mail sequence pagination, connection, and delta work remains. |
| H4 | fixed for exact path | Browser tracked-message DB lookups are now batched, subscription-triggered sync is coalesced, and command/queue tracking watchers only run while active tracking exists; broader Chat DOM/layout/listener work remains. |
| H5 | open | Matching per-keystroke recompute remains. |
| H6 | open | Outbound per-row pipeline recompute remains. |
| M1 | partial | Native expressible counts use SQL; fallback counts still scan and are mislabeled fast. |
| M2 | partial | Key reads use read-only connections and native RxSubject fanout is bounded; writer serialization and installed/integration slow-peer soak evidence remain. |
| M3 | fixed for query-fetch | Query-fetch refuses unsupported fallback instead of scanning. |
| M4 | partial | Idle gate exists; source stamps and repair scans remain table-sized/bounded-unfiltered. |
| M5 | fixed for exact prune | Chunk prune uses PK/range path; broader chunk retention/bloat remains. |
| M6 | partial | Eager native chunk writes bulk upsert; other chunk write/readiness paths remain. |
| M7 | mostly fixed | Reverse refs and batch invalidation exist; some scan fallback remains. |
| M8 | fixed | Browser `storage-indexeddb.upsert()` now uses one readwrite transaction and returns the written document without a final read. |
| M9 | partial | `collection.$` and primary-key `findOne().$` now apply changed-id deltas without broad re-query; complex query subscriptions still re-query. |
| M10 | partial | Browser indexed paths exist; non-indexed fallbacks remain. |
| M11 | open | Inference descriptor arena reuse remains. |
| M12 | open | Inference graph/context reuse remains. |
| M13 | fixed for stream delta/no-op path | Direct session filters high-frequency stream delta/no-op events before payload clone/deserialization and keeps consumed agent-message parsing covered by tests; cost telemetry/transcript copy work remains. |
| M14 | fixed | File-fetch stream no longer parks tokio worker with sync sleep/block_on. |
| M15 | fixed for native RxSubject fanout | Native `RxSubject` uses bounded broadcast fanout, lag counters, storage lag markers, query-buffer invalidation, replication `RESYNC` mapping, native peer/perf-probe surfacing for process-wide lag totals, and targeted slow-peer checkpoint-recovery coverage. Installed/integration slow-peer soak evidence remains. |
| M16 | fixed | Mailbox/received index exists for exact finding. |
| M17 | open | Mailserver hot-path connection reuse remains broad work. |
| M18 | fixed | Send verification uses header search/header fetch. |
| M19 | partial | UID watermarks reduce steady state; first import/IDLE/UIDVALIDITY remain. |
| M20 | partial | Self-work list/projection assignment hydration is set-based; single-load and broader ticket/queue paths remain. |
| M21 | fixed for direct projection | Direct Business OS ticket projection buckets reuse one ticket DB connection, including control bundles; non-projection ticket/queue helper audits remain separate. |
| M22 | partial | Some in-place Chat paths exist, but no-op sync can still build message HTML and compare serialized `innerHTML`; signature/append-only reconcile remains. |
| M23 | open | Window drag forced-reflow batching remains. |
| M24 | partial | Sync-layer diagnostics fanout is coalesced, but transport status still enters sanitize/record/fanout logic and needs observer/fanout counters. |
| M25 | open | Spreadsheet HyperFormula rebuild remains. |
| M26 | open | Matching requirements rebuild/scans remain. |
| M27 | open | Buchhaltung journal join pre-aggregation remains. |
| M28 | open | Customers search full pane re-render remains. |
| M29 | partial | Cached writer covers some repair/fanout paths; direct status paths still reopen. |
| M30 | fixed for checked central stores | `synchronous=NORMAL` is set in checked central stores; keep a guard so future direct SQLite helpers cannot omit it. |
| M31 | fixed for status idle path | Process scan is cached for normal status polling; explicit lifecycle/probe scans remain intentional. |

## Low Finding Coverage

Low findings remain tracked by bucket:

- Store SQLite: partially reduced. `push_collection_records` reuses a core-store
  connection and one transaction for non-command batches, but command-path
  batching, production counters, and remaining command lookup cleanup remain.
  `pull_collection_record` single-record paths are keyed for the core store,
  RxDB fallback, and `communication_*` projections. Dunning due-invoice lookup
  uses a partial expression index instead of scanning invoice payloads.
- Service loop: partially reduced. Queue counts and app recovery are improved;
  working-hours canonicalization and process-mining authorizer overhead remain.
- RxDB native: partially reduced. Command consumer idle scanning and
  `bulk_write` per-id current-state lookup are reduced; broader fallback
  counters remain.
- RxDB browser: partially reduced. Diagnostics, single/bulk upsert batching,
  collection/primary-key subscription deltas, and local-write push trigger
  coalescing are reduced; `encodedSize`, contiguous chunk sequence, browser
  chunk upload batching, and complex query subscription deltas remain.
- Inference/runtime/execution/mission/UI modules: mostly open and assigned to
  P5/P6.

## Measurement And Release Gates

No release or main-branch "fixed" claim should happen until all of this passes:

1. `cargo fmt --check` or targeted rustfmt for edited Rust files.
2. `cargo test --manifest-path src/core/rxdb/Cargo.toml`.
3. `node src/apps/business-os/rxdb/tests/run-all.mjs` after browser RxDB or
   sync changes, with rebuilt `dist/` and matching cache-busters.
4. Targeted Business OS store/peer tests for projection writer, file
   materialization, chunk maintenance, queue/chat repair, and source stamps.
5. A DB growth probe against a real operator DB:
   - file sizes for `business-os-rxdb.sqlite3`, WAL, SHM, and `ctox.sqlite3`;
   - row counts and deleted/live counts for `desktop_files`,
     `desktop_file_chunks`, `business_commands`, `ctox_queue_tasks`,
     `business_records`;
   - freelist/page counts;
   - top collections by JSON payload bytes.
6. Installed binary validation:
   - push to `main`;
   - install with `ctox upgrade --dev`;
   - restart daemon;
   - run idle probe before and after Business OS file access/materialization;
   - record CPU/wakeups/DB counters for at least 10 minutes after warmup.

## Immediate Implementation Order

1. Run installed idle evidence with `ctox_perf_probe.py --assert-idle` after
   `ctox upgrade --dev`.
2. Add installed materialize/CV Builder idle evidence.
3. Done on 2026-06-26 for the direct Coding Agents provider status poller:
   the module no longer schedules a 10 second diagnostics interval; keep
   checking other recurring browser command producers.
4. Done on 2026-06-26 for the direct native browser-runtime 300 ms
   active-session DB poller: after one empty maintenance pass it waits on
   `browser_input_events` table changes or the slow idle timeout. Remaining
   work is installed idle evidence and loop counters for input queries, timeout
   wakes, and expired-frame GC.
5. Fix Business OS projection source stamps and queue/chat repair windows,
   including batched queue repair command/task lookups.
6. Finish direct RxDB projection writer cache coverage, command-path batching,
   and production open/statement counters.
7. Add chunk physical retention metrics and safe maintenance policy.
8. Expand/refuse native RxDB fallback scans with explicit slow counters.
9. Finish browser IndexedDB complex-query subscription deltas and chunk upload
   batching; preserve the completed local-write push trigger debounce.
10. Clean up remaining daemon status/timer hot spots, including progressive
    mission approval/self-work sweeps.
11. Work through UI/module, mailserver, execution, inference, and mission/report
   residuals.

## Current Verification Done While Creating This Plan

- `rustfmt --edition 2021 --check src/core/business_os/store.rs`
- `git diff --check -- src/core/business_os/store.rs docs/ctox-performance-optimization-plan-2026-06-25.md`
- `node src/apps/business-os/rxdb/tests/command-bus-projection-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/chunk-query-demand-disabled-smoke.mjs`
- `node --test src/apps/business-os/shared/command-bus.test.mjs`
- `node src/apps/business-os/rxdb/tests/run-all.mjs`
- `node src/apps/business-os/modules/cv-print-builder/tests/cv-print-builder.test.mjs`
- `node --check src/apps/business-os/modules/cv-print-builder/index.js`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `python3 -m py_compile src/tools/perf/ctox_perf_probe.py`
- `python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --skip-db --skip-heartbeat --pretty`
- `python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --skip-db --skip-heartbeat --max-cpu-avg 0 --pretty`
  exited 1 with a structured assertion failure.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-status --skip-db --cpu-samples 1 --cpu-interval 0 --process-name __ctox_perf_probe_no_such_process__ --pretty | python3 -m json.tool`
- `rustfmt --edition 2021 --check src/core/business_os/rxdb_peer.rs`
- `rustfmt --edition 2021 --check src/core/business_os/rxdb_peer.rs src/core/rxdb/src/storage/sqlite/instance.rs`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::query_indexed_selector_pushes_filter_and_window_into_sqlite -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox native_peer_status_reports_fresh_heartbeat -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox rescan_of_unchanged_workspace_is_a_no_op -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox materialized_large_file_survives_lazy_rescan -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file_index_maintenance -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file_index -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_business_chat_tracking_projections -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_chat_repair -- --nocapture`
- `rustfmt --edition 2021 --check src/core/business_os/rxdb_peer.rs src/core/business_os/store.rs`
- `rustfmt --edition 2021 --check src/core/execution/agent/direct_session.rs`
- `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox direct_session -- --nocapture`:
  20 passed, covering the direct-session event hot-path change.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml rxjs_compat::tests -- --nocapture`:
  8 passed, covering bounded `RxSubject` backlog, process-wide lag counters,
  and lag-signal behavior.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml change_event_buffer::tests::lagged_marker_invalidates_incremental_buffer -- --nocapture`:
  1 passed, covering query-buffer invalidation after storage change-stream lag.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml replication_protocol::index_mod::tests::storage_master_change_stream_lag_maps_to_resync -- --nocapture`:
  1 passed, covering storage lag to replication `RESYNC` mapping.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml replication_protocol::index_mod::tests -- --nocapture`:
  4 passed, covering storage lag to replication `RESYNC` and slow master-change
  peer checkpoint recovery after lag.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml -- --test-threads=1 --nocapture`:
  271 unit tests and 30 conformance tests passed.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml fallback -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml -- --nocapture`
- `node src/apps/business-os/rxdb/tests/run-all.mjs`
- Subagent browser review ran targeted smoke tests:
  `transport-status-throttle-smoke`, `active-collections-catchup-smoke`,
  `chunk-query-demand-disabled-smoke`, `data-plane-guard-smoke`,
  `bundle-reproducible-smoke`.

The full browser RxDB suite and full native RxDB crate suite were not run while
creating this document. They are release gates for code changes, not a
substitute for the idle probe above.
