# CTOX Performance Optimization Plan - 2026-06-25

Source review:
`/Users/michaelwelsch/Documents/ctox/docs/ctox-performance-review-2026-06-24.md`

Current checkout:
`/Users/michaelwelsch/Documents/ctox.nosync`

## Current Answer

No, the 2026-06-24 performance review is not fully handled.

Several of the most damaging native hot paths have been fixed or materially
reduced, but the system is not structurally done. The remaining problem is now
less "every RxDB query is a full-table scan" and more:

- some Business OS projection loops still poll durable stores and files;
- several projection loops still need source-specific change signals instead of
  fixed wakeups or broad durable-store inspection;
- Browser IndexedDB and demand-cache paths still perform broad scans;
- file/chunk and projection writes are still too granular;
- database growth still lacks explainable retention and diagnostics;
- communication, UI, inference, and report hot paths from the old review remain
  mostly open.

CTOX is a background daemon. Idle must mean no steady CPU burn, no recurring
full-store scans, and no RxDB/WebRTC churn unless a source actually changed.

## Review Method

This plan was rechecked against the current worktree by the main agent and
four read-only subagents:

1. Native RxDB/SQLite adapter and DB access review.
2. Daemon idle loops, runtime settings, service status, and projection review.
3. Browser Business OS, RxDB/WebRTC, IndexedDB, and UI hot path review.
4. DB growth, desktop-file/chunk retention, tombstones, WAL/freelist review.

The subagents did not edit files or run tests. Their findings were merged here
only after source references were checked against the current checkout.

Additional subagent review on 2026-06-25 rechecked the plan after the first
idle-loop fixes. It confirmed the P1 priority order and added missing explicit
work items for browser local-write push scans, WebRTC checkpoint fanout, queue
command lookup, invoice due-date scans, process-mining authorizer allocation,
working-hours cache keys, Metal PSO lookup, report cleanup/scoring, spreadsheet
HyperFormula lifecycle, and UI startup/reporter schedulers.

Comprehensive follow-up subagent review on 2026-06-25 found that the plan still
needed to be stricter in four places:

- Native RxDB storage cursor improvements did not by themselves make
  `rxdb.query.fetch` bounded; follow-up work now enforces/caps request windows
  server-side, but the handler still needs true frame-as-produced sending
  instead of bounded post-query buffering.
- Daemon idle work is still dominated by source-stamp or polling cost in some
  loops: configured email sync, recovery/queue probes, and hourly harness
  audit. Notes, generic business-record projection stamps, and desktop-file
  indexing no longer touch heavy payloads or recurse through file roots on every
  short idle tick, but they still need watcher, high-water, or event-driven
  triggering to remove periodic checks entirely.
- Browser IndexedDB/WebRTC remains a P1 workstream: `allDocuments()` fallback,
  materializing browser `count()`, demand-cache sidecar scans, local-write push
  scan floors, broad chunk consumers, and heavy diagnostic fanout remain.
- DB growth needs a real retention/horizon contract, not only pruning:
  physical deletes must respect replication checkpoints, soft-delete forms must
  be measured separately, attachment lifecycles need reference-based retention,
  and WAL/freelist shrink policy must be explicit.

## Verified As Fixed Or Strongly Reduced

These old review findings are no longer accurate as written:

- `H1/M1/M3` native RxDB query/count and storage cursor paths: partially fixed. The SQLite
  backend now compiles simple Mango selectors into SQL with `WHERE`, sort,
  `LIMIT`, `OFFSET`, and `COUNT(*)`; compiled query and count paths can use
  read-only WAL connections. File-backed `find_documents_by_id` now also uses
  a read-only connection plus batched `WHERE id IN (...)`, and
  `get_changed_documents_since` reads checkpoints through a read-only
  connection instead of the shared writer connection. File-backed complex
  `query()` fallback reads also run on a read-only connection, so unsupported
  Mango matchers no longer wait for the shared writer mutex. The WebRTC
  `rxdb.query.fetch` handler now applies request `window.offset`/`window.limit`
  before preparing the Mango query and rejects windows above the server cap.
  It still needs a follow-up change to send frames as produced instead of
  buffering the bounded response frames.
  Files:
  - `src/core/rxdb/src/storage/sqlite/sql.rs`
  - `src/core/rxdb/src/storage/sqlite/instance.rs`

- `H2/M24` WebRTC transport status: partially fixed. Transport status emissions
  are throttled/coalesced instead of rebuilding and emitting a full diagnostic
  snapshot once per frame.
  Files:
  - `src/apps/business-os/rxdb/src/webrtc-native.mjs`
  - `src/apps/business-os/rxdb/tests/transport-status-throttle-smoke.mjs`

- `M3` WebRTC query-fetch unbounded result windows: reduced. The handler now
  applies the request window before streaming and rejects windows above a
  server cap of 25 default windows. Regression tests prove a `window` with
  `offset = 10, limit = 25` streams only that slice and that over-cap windows
  emit `STREAM_LIMIT_EXCEEDED` without data chunks.
  File:
  - `src/core/rxdb/src/plugins/replication_webrtc/query_fetch_handler.rs`

- `M30` SQLite fsync pressure: fixed for the checked central paths.
  `PRAGMA synchronous=NORMAL` is now set for the Business OS store and core
  persistence.
  Files:
  - `src/core/business_os/store.rs`
  - `src/core/persistence.rs`

- `M31` status path pressure: partially fixed. Durable status reads and process
  scans are cached, and the normal status path no longer triggers Business OS
  app recovery. The dedicated Business OS app recovery loop now also uses a
  Core-DB change-stamp gate before running its leased-queue scan.
  File:
  - `src/core/service/service.rs`

- Runtime env/state repeated SQLite reads: improved. The runtime env and
  runtime state loaders have stamp-backed caches.
  Files:
  - `src/core/execution/models/runtime_env.rs`
  - `src/core/execution/models/runtime_state.rs`

- Runtime Settings projection idle churn: strongly reduced. Runtime settings
  cache stamps no longer include broad `runtime/ctox.sqlite3`, semantically
  identical rebuilds retain the previous document, and the native RxDB peer now
  skips the Runtime Settings projection before taking the projection write lock
  or opening/reading RxDB when the source projection stamp is unchanged.
  Files:
  - `src/core/business_os/store.rs`
  - `src/core/business_os/rxdb_peer.rs`

- Business Users projection idle churn: reduced. The native RxDB peer now uses
  a source stamp over the Business OS user table plus configured-user identity
  inputs, and skips unchanged Business Users projection rounds before taking the
  projection write lock or touching RxDB.
  Files:
  - `src/core/business_os/store.rs`
  - `src/core/business_os/rxdb_peer.rs`

- Channel State projection lock churn: reduced. Channel State already had a
  source stamp; the background loop now evaluates that stamp before taking the
  projection write lock, so unchanged channel/account/pairing state does not
  contend with real projection writes.
  File:
  - `src/core/business_os/rxdb_peer.rs`

- Native RxDB `bulk_write` current-state lookup: improved. The old full-table
  current-state read has been reduced to written IDs.
  File:
  - `src/core/rxdb/src/storage/sqlite/instance.rs`

- Hot Business OS RxDB schema indexes: improved. `business_commands`,
  `ctox_queue_tasks`, and `desktop_file_chunks` now carry schema indexes for
  the status/command/file/generation selectors used by hot native paths. The
  generated Business OS schema contract and schema-hash registry are current,
  the browser bundle was rebuilt, both cache-busters were bumped together, and
  a native `EXPLAIN QUERY PLAN` guard proves SQLite uses `_deleted` plus hot
  selector index prefixes instead of scanning these collections.
  Files:
  - `src/apps/business-os/modules/ctox/schema.js`
  - `src/apps/business-os/modules/desktop/schema.js`
  - `src/core/rxdb/src/storage/sqlite/sql.rs`
  - `src/core/business_os/business_os_schema_contract.json`
  - `src/core/business_os/business_os_schema_hashes.json`
  - `src/apps/business-os/rxdb/src/schema.mjs`
  - `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`
  - `src/apps/business-os/shared/db.js`
  - `src/apps/business-os/shared/sync.js`
  - `src/core/business_os/rxdb_peer.rs`

- Desktop file normal background sync: strongly reduced. `desktop_file_chunks`
  is demand-only in browser sync, active file-fetch reads deterministic chunk
  IDs, and the native background desktop-file index now source-stamps scan
  roots before taking the DB write lock or touching RxDB. The background loop
  also has a cheap root/direct-child stamp gate before the recursive candidate
  scan, so unchanged file roots no longer recurse every
  `DESKTOP_FILE_SCAN_INTERVAL_SECS`; recursive scan is reserved for dirty roots
  or the slow fallback. New eager chunk generations and stale-generation chunk
  redactions are now written through collection bulk upserts instead of one
  `incremental_upsert` per chunk.
  Files:
  - `src/apps/business-os/shared/sync.js`
  - `src/core/business_os/rxdb_peer.rs`

- Notes idle stamp cost: strongly reduced. `sync_notes_background_loop` still
  wakes on a fixed interval, but the source stamp no longer reads or hashes
  `payload_json` for every `notes` row. The stamp now reads only
  `(record_id, updated_at_ms, deleted)` metadata, backed by
  `idx_business_records_notes_stamp`, and a native `EXPLAIN QUERY PLAN` guard
  requires SQLite to use that covering index. Real note updates still advance
  `updated_at_ms` and change the stamp.
  File:
  - `src/core/business_os/store.rs`

- Generic Business Records projection stamp cost: reduced. The projection
  source stamp now reads tracked `business_records` metadata through one
  `collection IN (...)` query instead of one query per collection. The stamp
  still tracks `collection`, `record_id`, `rev`, `deleted`, and
  `updated_at_ms`, but a new `idx_business_records_projection_stamp` covering
  index keeps the idle check off payload pages. A native query-plan guard
  verifies the index is used and the existing projection idle-gate test still
  passes.
  File:
  - `src/core/business_os/store.rs`

- Module Catalog projection idle churn: reduced. The native RxDB peer now
  computes a source projection stamp over module/template file metadata,
  installed module metadata, the relevant module lifecycle/grant/user tables,
  and the normalized module allowlist before taking the projection write lock
  or building `module_catalog_for_rxdb`. Unchanged idle rounds skip the
  expensive catalog rebuild and RxDB write path.
  Files:
  - `src/core/business_os/store.rs`
  - `src/core/business_os/rxdb_peer.rs`

- Command consumer no-pending idle churn: reduced. The native RxDB peer now
  checks a narrow pending-command SQLite stamp before taking the database write
  lock or running the RxDB Mango `find(status = pending_sync)`. No pending
  commands means the round returns immediately; pending commands still run the
  normal consumer every active poll so retry/failure-budget behavior is not
  starved.
  Files:
  - `src/core/business_os/rxdb_peer.rs`

- Queue/chat repair idle churn: reduced. The Business Record projection loop
  now carries a combined repair stamp over RxDB `ctox_queue_tasks`,
  `business_commands`, `business_chats`, canonical queue aggregates, and a
  bounded orphan-repair epoch. Unchanged rounds skip the broad queue/chat
  repair `find(limit)` sweeps. Incremental high-water repair windows and keyed
  command/task lookups remain open.
  File:
  - `src/core/business_os/rxdb_peer.rs`

- Queue task lookup by Business OS command: reduced. `find_queue_task_for_command`
  now first uses an indexed `communication_messages.metadata_json`
  `business_os_command_id` lookup through the channel store, and only falls
  back to the old prompt substring scan for legacy queue entries that do not
  carry the metadata.
  Files:
  - `src/core/mission/channels.rs`
  - `src/core/business_os/store.rs`

- Queue status counts: reduced. `channels::count_queue_tasks` now reuses a
  stamp-backed count cache keyed by normalized route-status set. Repeated idle
  status checks avoid reopening/counting the channel DB until the DB/WAL/journal
  stamp changes.
  File:
  - `src/core/mission/channels.rs`

- Documents report command completion lookup: reduced. The
  `complete_ready_documents_report_commands` scan over open
  `business_commands` now has a partial SQLite index on
  `(observed_at_ms, command_id)` for the exact documents report command type
  and non-terminal statuses, with an `EXPLAIN QUERY PLAN` guard.
  File:
  - `src/core/business_os/store.rs`

- Ticket State projection idle churn: reduced. The native RxDB peer now checks
  the ticket DB/WAL/journal change stamp before taking the projection write
  lock or loading Business OS ticket projection documents. Unchanged idle
  rounds skip the ticket projection body. Ticket N+1 hydration and richer
  ticket query batching remain separate Phase 4 work.
  File:
  - `src/core/business_os/rxdb_peer.rs`

- Knowledge Tables projection idle churn: reduced. The Knowledge data module
  now exposes a source stamp over active `knowledge_data_tables` rows plus the
  live-resolved Parquet file metadata for each active table. The native RxDB
  peer checks that stamp before taking the projection write lock or embedding
  Parquet rows into `knowledge_tables`; unchanged idle rounds skip the
  projection body.
  Files:
  - `src/core/knowledge/data.rs`
  - `src/core/knowledge/mod.rs`
  - `src/core/business_os/rxdb_peer.rs`

- Business-record projection idle churn: reduced. The generic Business OS
  business-record projector now computes a composite source stamp before taking
  the projection write lock. The stamp covers projected `business_records`
  metadata, communication account/thread/message projection metadata, and the
  queue/chat repair stamp, so unchanged idle rounds skip support intake,
  generic collection pulls, thread relevance projection, and broad queue/chat
  repair work.
  Files:
  - `src/core/business_os/store.rs`
  - `src/core/mission/channels.rs`
  - `src/core/business_os/rxdb_peer.rs`

- IMAP FETCH/STORE full-body overfetch: reduced. IMAP `SELECT` now counts
  messages through `COUNT(*)`, and `FETCH`/`STORE` sequence resolution uses a
  body-free message summary query instead of loading every `body` and `headers`
  row in the selected mailbox. Body/header content is loaded only for the
  specific message when the FETCH query actually requests `BODY[...]`,
  `BODY.PEEK[...]`, `RFC822`, or size data. The mailbox summary/count queries
  have a mailbox/received index and query-plan guard.
  Files:
  - `src/core/mailserver/src/imap/mod.rs`
  - `src/core/mailserver/src/store/sqlite.rs`
  - `src/core/mailserver/src/store/sqlite_schema.rs`

## Still Open Or Only Partially Fixed

### Native RxDB And SQLite

- The SQL compiler is still a conservative subset, not full
  `query_planner.rs` integration. Complex Mango selectors still fall back to
  Rust scans.
- The hottest Business OS schema indexes for commands, queue tasks, and desktop
  chunks now have an `EXPLAIN` guard. Other high-traffic selectors still need
  the same treatment.
- `rxdb.query.fetch` now enforces and caps request windows before query
  execution, so it no longer streams unbounded result sets by default. The
  handler still collects the bounded response frames before sending; true
  frame-as-produced sending remains open.
- The shared writer `Arc<Mutex<Connection>>` still serializes writes and
  in-memory read fallbacks. File-backed `query()`, `find_documents_by_id`, and
  `get_changed_documents_since` no longer use the writer mutex for read
  execution, but unsupported query shapes may still perform broad read-only
  scans.
- The internal external-write poller still calls changed-since through the
  shared connection and does not drain changed batches until empty.
- `bulk_write` now avoids full-table current-state reads, but large batches
  still perform ID lookups one row at a time under the writer transaction.
- Projection loops now benefit from SQL `LIMIT`, and several broad projection
  loops plus queue/chat repair sweeps are now source-stamped. Incremental
  high-water repair windows are still open.
- Desktop chunk upload/prune still uses per-chunk/per-document write patterns
  in important paths.

### Daemon Idle And Projection Loops

- Business OS native peer starts many loops. Idle backoff helps, but the
  remaining ungated loops are still polling rather than source-stamped or event
  driven.
- Channel State, Runtime Settings, Business Users, Module Catalog, Notes,
  Ticket State, Knowledge Tables, Business Records, the desktop-file index, the
  command consumer no-pending path, and queue/chat repair now have
  source-stamped or narrow idle gates. Command completion/status views and
  channel/email sync still need the same treatment.
- `sync_local_markdown_notes` can still scan/read local note files on a short
  interval after a detected source change. Its idle source stamp is now narrow:
  note rows are checked through metadata only and the query is guarded to use a
  covering SQLite index instead of reading payload bytes.
- Business-record projection is now gated. Its `business_records` source stamp
  is a single covering-index metadata query, but the loop still wakes
  periodically and still includes communication projection metadata in the
  source stamp.
- Desktop file indexing still wakes periodically, but unchanged source roots now
  skip recursive candidate scans, the RxDB write path, and the projection lock
  on short idle ticks. Watcher/event-driven triggering remains open; missed
  nested changes are covered by a slow fallback scan.
- Runtime settings now skip unchanged projection rounds and ignore unrelated
  core DB writes. Queue health and harness-flow inputs are still TTL-covered
  rather than separately source-stamped, so this path is reduced but not yet
  fully event driven.
- Module catalog idle projection is now source-stamped, but release projection
  and upgrade/release paths still need keyed lookups and event-driven
  reconciliation.
- Channel sync still has confirmed polling paths. Configured IMAP no longer
  does repeated full mailbox UID scans after the first persisted UID, but
  Meeting session sync still scans active session files and channel sync is not
  fully event-driven.
- Business OS app recovery and harness audit now have source-stamp gates;
  durable queue probing still needs a tighter due-work gate.

### Browser RxDB, WebRTC, And IndexedDB

- Browser IndexedDB `queryDocuments()` still falls back to `allDocuments()` for
  many selectors, and browser `count()` still materializes `find().exec()`.
- Advanced Status and similar UI count paths must be covered explicitly because
  their small `limit` queries still become broad IndexedDB scans in fallback.
- Browser file/chunk consumers still contain broad reads: Universal Importer
  and CV Print Builder must move to `rxdb.file.fetch` or keyed chunk lookup.
- Demand-cache invalidation scans sidecar query windows and can run multiple
  times per batch.
- Browser local-write push is not debounced and `getChangedDocumentsSince()`
  still carries a scan floor that can run after chunk/blob upload bursts.
- Transport status is throttled, but heavy snapshots and per-collection fanout
  still exist. Lazy/observer-gated diagnostics are not implemented.
- Encoded size and chunk reassembly paths still contain avoidable allocation or
  repeated work.
- Collection subscriptions still tend toward full re-query/re-render patterns
  instead of changed-ID deltas.

### Projection Writers And DB Growth

- `upsert_rxdb_collection_record` still opens the RxDB DB and checks table
  metadata per record.
- `push_collection_records` still has per-record connection/write behavior in
  important branches.
- Completed command/event history, completed queue projections, stale desktop
  chunk generations, and RxDB tombstones do not yet have a complete retention
  and replication-horizon policy.
- Physical chunk/tombstone deletes need a replication checkpoint horizon, and
  operator diagnostics must distinguish SQLite `deleted`, JSON `_deleted`,
  Business OS `is_deleted`, missing-file state, tombstone reason, and
  `deleted_at_ms`.
- File-sharing and attachment retention needs reference-based rules by
  `source`, `linked_collection`, and `linked_record_id`; orphan chunks must be
  pruned without deleting referenced attachments.
- Freelist and WAL sizes are measured but not yet converted into an idle-only
  checkpoint/shrink maintenance policy.
- There is no single operator-facing DB size report that explains top tables,
  tombstones, WAL size, stale chunks, and free pages.

### Other Review Areas

- IMAP/email: IMAP SELECT/FETCH/STORE no longer load every mailbox body for
  count, flags, or STORE, but UID-watermarks, IDLE, adapter polling, and
  remaining body-on-demand/projection split work remain.
- Business OS UI: chat tracked message sync, schedulers, layout reads, module
  searches, and spreadsheet recalculation still have confirmed hot paths.
- Inference: graph/arena/token host-side overhead remains mostly open.
- Mission/report: DB reopen and N+1 hydration paths remain mostly open.

## Status Matrix For 2026-06-24 Findings

| Finding | Current status | Notes |
| --- | --- | --- |
| H1 native RxDB non-PK full scans | Partial | Simple selectors/count/query-fetch compile to SQL; complex selectors and missing schema indexes remain. |
| H2 WebRTC status per frame | Partial | 250 ms coalescing exists; lazy/observer-gated diagnostics and fanout reduction remain. |
| H3 IMAP FETCH/STORE full body load | Partial | SELECT uses COUNT, FETCH/STORE sequence resolution uses body-free summaries, mailbox summary/count queries are indexed, and configured IMAP sync now uses the latest persisted numeric UID as a watermark; IMAP IDLE, adapter due-state gating, and fuller body-on-demand projection work remain. |
| H4 chat tracked message N+1 | Open | Needs batched lookups and subscription debounce. |
| H5 matching per-keystroke recompute | Open | Needs Maps, cached haystacks, debounce, representative tests. |
| H6 outbound per-row pipeline recompute | Open | Needs memoized pipeline and by-company index. |
| M1 RxDB count materializes docs | Partial | Fixed for expressible selectors; fallback path remains. |
| M2 single SQLite connection mutex | Partial | File-backed query, find-by-id, and changed-since reads use read-only connections; writes and in-memory fallbacks still share the writer. |
| M3 query-fetch full scan | Partial | Storage-side compiled paths improved; WebRTC `rxdb.query.fetch` now enforces/caps request windows, but still buffers bounded frames before sending. |
| M4 projection reconcilers broad scans | Partial | SQL limit helps, several projections are source-stamped, Ticket State, Knowledge Tables, Business Records, and queue/chat repair sweeps are gated; high-water/event-driven reconciliation remains. |
| M5 desktop chunk prune by file_id | Partial | Active fetch improved, materialize repair now verifies chunk rows, and idle scans are source-stamped; prune still needs PK/range or bounded SQL. |
| M6 chunk writes one transaction per chunk | Partial | Native eager chunk generation and stale-generation redaction now use collection bulk upsert; remaining chunk write/prune paths still need deeper batching/direct SQL. |
| M7 demand-cache full sidecar scans | Open | Needs reverse docId->windowKeys index and once-per-batch invalidation. |
| M8 browser upsert transaction overhead | Open | Needs collapsed read/write transaction path. |
| M9 subscriptions full find on change | Open | Needs changed-ID deltas or targeted windows. |
| M10 browser allDocuments fallback | Open | Needs IndexedDB query planner/cursors. |
| M11/M12 inference arena/graph overhead | Open | Needs model-runtime optimization. |
| M13 streamed event clone/deserialize | Open | Needs method inspection before clone/parse. |
| M14 blocking file stream close | Open | `block_on`/sleep paths still need async cleanup. |
| M15 unbounded RxSubject fanout | Open | Needs bounded/backpressure strategy. |
| M16-M19 communication/email | Open | Needs indexes, watermarks, body-on-demand, connection reuse. |
| M20/M21 mission ticket projection N+1 | Open | Business OS ticket idle projection loop is source-stamped; ticket hydration still needs batched queries/connection reuse. |
| M22-M28 Business OS UI/modules | Open | Needs batching, debouncing, memoization, virtualization/reconcile where relevant. |
| M29 projection writer reopen/table_info | Open | Needs long-lived connection and metadata cache. |
| M30 synchronous=NORMAL | Fixed | Business OS store and persistence set it. |
| M31 status ps/proc scan | Partial | Cached on normal path; explicit lifecycle/shutdown scans remain by design. |

## Design Rule

SQLite remains the durable source of truth. It must not be used as a polling
engine for every UI tick, status request, projection loop, or file transfer.

Every hot path must be one of:

1. event driven;
2. gated by a precise source change stamp;
3. bounded by indexed SQL and explicit `LIMIT`;
4. cached with a documented TTL and no volatile fields in the invalidation key;
5. intentionally slow-path and excluded from idle loops.

## Phase 0 - Measurement And Regression Guards

Goal: make idle regressions and database growth visible before further broad
refactors.

Tasks:

1. Add an idle CPU sampler that targets a PID without calling `ctox status`.
2. Add a status latency sampler that measures `ctox status --json` separately
   from daemon idle CPU.
3. Add DB size diagnostics:
   - SQLite page count and freelist count;
   - WAL/SHM sizes;
   - top RxDB collections by row count and tombstone count;
   - stale desktop chunk generations and retained bytes;
   - `dbstat` table/index bytes when available.
4. Add native RxDB regression counters: limited indexed queries must not
   deserialize table-size rows.
5. Add loop-budget instrumentation for the native peer: idle rounds with no
   source change should not read/write RxDB.
6. Add lock timing and SQLite statement counters for native peer loops:
   write-lock wait/hold time, rows visited, rows decoded, and write batch count.
7. Add `EXPLAIN QUERY PLAN` guards for `business_commands.status`,
   `ctox_queue_tasks.status`, `ctox_queue_tasks.command_id`, and
   `desktop_file_chunks.file_id/generation_id/idx`.
8. Add chunk-soak coverage for large file materialization, repair after missing
   chunks, and retained-generation pruning.
9. Add change-stream soak coverage for many collections plus a slow peer.
10. Add browser perf smokes for `allDocuments`, `scanQueryWindows`,
   `findOne`-N+1, transport-status emissions, and chunk write/flush counts.

Implementation status:

- Done on 2026-06-25 for the repeatable local measurement command:
  `src/tools/perf/ctox_perf_probe.py` now records process CPU samples by PID or
  `pgrep -x ctox-real` without calling `ctox status`, samples
  `ctox status --json` latency as a separate phase, and inspects SQLite stores
  in read-only mode. `src/tools/perf/README.md` documents the full idle
  evidence command, a DB-only command, and a CPU-only command.
- Done on 2026-06-25 for DB-size diagnostics: the probe reports SQLite
  `page_count`, `page_size`, `freelist_count`, WAL/SHM sizes, top RxDB
  collections by row count/data bytes/tombstones, stale retained
  `desktop_file_chunks` generations from sampled chunk rows, and `dbstat`
  table/index bytes when SQLite exposes `dbstat`.
- Current local DB evidence from a DB-only probe run is already actionable, but
  not release proof: `runtime/ctox.sqlite3` is about 105 MB, and
  `runtime/business-os-rxdb.sqlite3` is about 277 MB with about 76 MB on the
  freelist. `dbstat` reports `desktop_file_chunks` at about 103 MB and
  `desktop_files` at about 58 MB; the RxDB table summary reports 37,577
  `desktop_files` rows with 32,840 tombstones. This supports the retention/DB
  growth work in Phase 5.
- Done on 2026-06-25 for the first native RxDB row-deserialize regression
  counter: the SQLite storage helper now has test-only JSON document decode
  counters, and the indexed `age >= 990 LIMIT 3` regression test asserts that
  the query decodes exactly three returned documents instead of table-size
  rows. The same test asserts the compiled `COUNT(*)` path decodes zero
  documents.
- Done on 2026-06-25 for the first hot Business OS query-plan guard:
  `hot_business_os_schema_indexes_have_sqlite_query_plan_guards` registers
  `business_commands`, `ctox_queue_tasks`, and `desktop_file_chunks`, inserts
  realistic selective test rows, runs `ANALYZE`, and asserts that SQLite uses
  `_deleted` plus hot selector schema-index prefixes for command status,
  command id, queue status, queue command id, and desktop
  `file_id/generation_id/idx` chunk reads.
- A current 5-sample CPU-only probe of the already-running `ctox-real` process
  reported 0.12% average CPU and 0.3% max CPU. This is only a short snapshot
  and does not satisfy the final 5 minute installed-daemon acceptance criterion
  or the post-file-share problem case.
- Still open: broader native RxDB row-visit counters for fallback scans,
  loop-wakeup instrumentation inside the native peer, SQLite
  statement/write-lock counters, broader `EXPLAIN QUERY PLAN` guards beyond
  the first hot Business OS set, chunk/change-stream soak tests, and browser
  perf smokes.

Validation:

- `python3 -m py_compile src/tools/perf/ctox_perf_probe.py` passed.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-status --skip-db --cpu-samples 1 --cpu-interval 0 --process-name __ctox_perf_probe_no_such_process__ --pretty`
  passed and did not call status or inspect SQLite files.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --max-tables 3 --max-dbstat-rows 3 --max-chunk-rows 1000 --pretty`
  passed and produced read-only DB diagnostics for the current checkout.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-status --skip-db --cpu-samples 5 --cpu-interval 1 --pretty`
  passed against the currently running `ctox-real` PID 34277.
- `rustfmt --edition 2021 --check src/core/rxdb/src/storage/sqlite/sql.rs src/core/rxdb/src/storage/sqlite/instance.rs`
  passed.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml query_indexed_selector_pushes_filter_and_window_into_sqlite -- --nocapture`
  passed: 1 test, 0 failures.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance -- --nocapture`
  passed: 22 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox hot_business_os_schema_indexes_have_sqlite_query_plan_guards -- --nocapture`
  passed: 1 test, 0 failures.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml -- --nocapture`
  passed: 255 unit tests and 30 conformance tests, 0 failures.
- `node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs`
  passed.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed: 44 tests, 0
  failures, 2 skipped cross-process wire tests because the wire daemon was not
  built.

Acceptance:

- A repeatable command records idle CPU, status latency, and DB size. Loop
  wakeups still require native-peer instrumentation and are not yet complete.
- The test suite can fail if a bounded indexed query or idle loop regresses
  into full-store work.

## Phase 1 - Stop Idle Projection And Polling Churn

This is the highest priority because it targets the observed daemon idle CPU.

### 1.1 Source-Stamped Projection Scheduler

Replace "wake up and inspect" loops with source stamps:

- commands;
- notes;
- desktop files;
- channel state;
- runtime settings;
- module catalog;
- tickets;
- knowledge tables;
- business records;
- queue/chat repair reconcilers (idle gate done; high-water/event-driven
  windows still open);
- status/count enrichment and queue-task command lookup (queue counts cached
  and metadata-indexed primary lookup done; legacy prompt fallback remains).

Each loop should compute a cheap stamp before doing expensive work. If the
stamp has not changed since the last successful sync, skip the body.

Acceptance:

- No source change means no durable scan and no RxDB write attempt.
- Idle backoff becomes a safety fallback, not the primary optimization.

Implementation status:

- Done on 2026-06-25 for Runtime Settings: the background projection loop now
  computes `RuntimeSettingsProjectionStamp` first and returns without running
  `sync_runtime_settings_with_database` or taking the projection write lock
  when the stamp is unchanged.
- Done on 2026-06-25 for Business Users: the background projection loop now
  computes `BusinessUsersProjectionStamp` first and returns without running
  `sync_business_users_with_database` or taking the projection write lock when
  the stamp is unchanged. The stamp tracks the source `business_users` table and
  configured-user identity/role inputs so configured users still project when
  runtime auth configuration changes.
- Done on 2026-06-25 for Channel State lock churn: the background projection
  loop now computes `ChannelStateProjectionStamp` before taking the projection
  write lock and returns immediately when the stamp is unchanged.
- Done on 2026-06-25 for Desktop File Index idle churn: the background loop now
  collects a bounded source stamp for configured scan roots before taking the
  database write lock. If roots, file paths, file sizes, mtimes, and eager/lazy
  policy are unchanged, the loop exits before the RxDB write path. A slow
  refresh epoch still forces periodic self-healing.
- Done on 2026-06-25 for Desktop File materialize repair: eager file fastpaths
  no longer trust stale `generation_verified_at_ms` metadata as proof that
  chunk rows still exist. Real sync/repair rounds verify deterministic chunk
  IDs and rewrite missing chunks.
- Done on 2026-06-25 for Module Catalog idle churn: the background projection
  loop now computes `ModuleCatalogProjectionStamp` before taking the projection
  write lock or building the module catalog document. The stamp covers packaged
  module files, installed module files, template metadata, module lifecycle and
  permission tables, configured users, and the normalized module allowlist.
  After a real sync the stamp is recomputed because catalog generation can
  backfill grants.
- Done on 2026-06-25 for Command Consumer no-pending idle churn: the consumer
  loop now reads a narrow pending-command table stamp before taking the database
  write lock or executing the RxDB Mango pending-command query. Zero pending
  commands skips the expensive body; pending commands always continue through
  the normal consumer so transient accept failures still retry until the failure
  budget is exhausted. The command table lookup is suffix-based over SQLite
  metadata instead of hard-coding a single database-name prefix.
- Done on 2026-06-25 for Queue/Chat Repair idle churn: the Business Record
  projection loop now keeps a `QueueChatRepairProjectionStamp` and skips the
  broad `ctox_queue_tasks.find(limit 500)` plus `business_chats.find(limit
  200)` repair sweeps when the RxDB queue/command/chat summaries, canonical
  queue aggregates, and orphan-repair epoch are unchanged.
- Done on 2026-06-25 for Business OS command queue-task lookup: channel schema
  now creates a partial expression index over valid queue
  `metadata_json.business_os_command_id`, and `find_queue_task_for_command`
  uses that keyed lookup before falling back to legacy prompt substring scans.
- Done on 2026-06-25 for Queue status counts: `count_queue_tasks` now uses the
  same DB/WAL/journal stamp family as queue list caching, keyed by normalized
  status set, so repeated idle status checks reuse a cached count until the
  channel store changes.
- Done on 2026-06-25 for Ticket State idle churn: the background projection
  loop now computes the ticket store change stamp before taking the projection
  write lock or loading Business OS ticket projection documents. If the ticket
  DB, WAL, and journal are unchanged since the last successful sync, the loop
  returns immediately.
- Done on 2026-06-25 for Knowledge Tables idle churn: the Knowledge data module
  now provides a projection source stamp over active catalog rows and
  live-resolved Parquet file metadata. The native background projection loop
  checks that stamp before taking the projection write lock or loading/embedding
  Parquet rows.
- Done on 2026-06-25 for Business Records idle churn: the generic Business OS
  business-record projection loop now uses a composite source stamp over
  projected `business_records`, communication projection metadata, and
  queue/chat repair state before taking the projection write lock. Unchanged
  idle rounds skip support intake, collection pulls, thread relevance
  projection, and broad queue/chat repair.
- Regression test added:
  - `sync_runtime_settings_idle_gate_skips_unchanged_projection`.
  - `sync_business_users_idle_gate_skips_unchanged_projection`.
  - `sync_channel_state_idle_gate_skips_unchanged_projection`.
  - `desktop_file_index_idle_gate_skips_unchanged_scan_roots`.
  - `sync_module_catalog_idle_gate_skips_unchanged_projection`.
  - `sync_ticket_state_idle_gate_skips_unchanged_source`.
  - `sync_knowledge_tables_idle_gate_skips_unchanged_source`.
  - `sync_business_record_projections_idle_gate_skips_unchanged_source`.
  - `knowledge_tables_projection_source_stamp_tracks_live_parquet_file`.
  - `business_command_idle_gate_skips_when_no_pending_commands`.
  - `queue_chat_repair_idle_gate_skips_unchanged_sources`.
  - `find_queue_task_for_command_uses_business_os_command_metadata`.
  - `queue_task_count_cache_reuses_idle_reads_until_store_changes`.
  - `documents_report_completion_query_uses_partial_command_index`.
- Still open: command completion/status lookups and channel/email loops need
  the same source-stamp or event-driven treatment. Notes and desktop files
  still need watcher or event-driven triggering, plus desktop chunk write/prune
  batching in Phase 2.
- Queue/chat repair reconcilers are now idle-gated, but still need high-water
  marks or event-driven repair windows so changed sources do not require broad
  repair windows. Legacy queue tasks without `business_os_command_id` metadata
  still use the prompt fallback.
- Status/count enrichment still needs stamped count caches or derivation from a
  cached list outside the channel queue count path.

Validation:

- `rustfmt --edition 2024 --check src/core/business_os/store.rs src/core/mission/channels.rs src/core/business_os/rxdb_peer.rs src/core/knowledge/data.rs`
  passed.
- `git diff --check -- src/core/business_os/store.rs src/core/mission/channels.rs src/core/business_os/rxdb_peer.rs src/core/knowledge/data.rs src/core/knowledge/mod.rs docs/ctox-performance-optimization-plan-2026-06-25.md`
  passed.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_business_users_ -- --nocapture`
  passed: 2 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_channel_state_idle_gate_skips_unchanged_projection -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file_index_idle_gate_skips_unchanged_scan_roots -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file_background_scan_gate_skips_recursive_scan_until_dirty_or_fallback -- --nocapture`
  passed: 1 test, 0 failures. This verifies unchanged roots do not trigger a
  recursive desktop-file scan on every short idle tick, while dirty roots and
  the slow fallback still collect a full scan.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox materialize_desktop_file_command_writes_missing_chunks -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file -- --nocapture`
  passed: 23 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox local_markdown_notes_source_stamp_ignores_unrelated_store_churn -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox local_markdown_notes_source_stamp -- --nocapture`
  passed: 2 tests, 0 failures. This includes the covering-index query-plan
  guard for the metadata-only Notes idle stamp.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox business_records_projection_stamp_uses_covering_metadata_index -- --nocapture`
  passed: 1 test, 0 failures. This verifies the generic Business Records
  projection source stamp stays on the covering metadata index.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_business_record_projections_idle_gate_skips_unchanged_source -- --nocapture`
  passed: 1 test, 0 failures after the projection-stamp query rewrite.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox business_os_app_recovery_idle_gate_reopens_when_core_db_changes -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox harness_audit_idle_gate_reopens_when_core_db_changes -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_module_catalog_idle_gate_skips_unchanged_projection -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_module_catalog_projects_modules_and_templates -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox business_command_idle_gate_skips_when_no_pending_commands -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox native_peer_consumes_pending_business_command -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_chat_repair_idle_gate_skips_unchanged_sources -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_ctox_queue_task_projections -- --nocapture`
  passed: 2 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_business_chat_tracking_projections_fails_orphaned_messages -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_business_record_projections_materializes_generic_collections -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_business_record_projections_idle_gate_skips_unchanged_source -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_business_record_projections_ -- --nocapture`
  passed: 6 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox find_queue_task_for_command_uses_business_os_command_metadata -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_task_count_cache_reuses_idle_reads_until_store_changes -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_task_list_cache_reuses_idle_reads_until_store_changes -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox documents_report_completion_query_uses_partial_command_index -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_ticket_state_idle_gate_skips_unchanged_source -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_ticket_state_projects_local_ticket_items_and_events -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox knowledge_tables_projection_source_stamp_tracks_live_parquet_file -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_knowledge_tables_idle_gate_skips_unchanged_source -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_knowledge_tables_tombstones_stale_once_then_noops -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox idle_gate -- --nocapture`
  passed: 13 tests, 0 failures.

### 1.2 Runtime Settings Projection Cache

Fix `runtime_settings_for_rxdb` invalidation:

- do not use broad `runtime/ctox.sqlite3` as a cache stamp for runtime settings;
- stamp runtime env, runtime state, secrets, service status, queue health, and
  web-stack inputs independently;
- preserve `updated_at_ms` for semantically identical rebuilds;
- avoid recomputing queue/web-stack diagnostics on unrelated DB writes;
- keep `incremental_upsert_projection_if_changed` as a write guard, but do not
  rely on it as the only CPU guard.

Implementation status:

- Done on 2026-06-25 for the broadest invalidation bug: runtime settings cache
  stamps now cover the runtime config SQLite store, the secrets SQLite store,
  and cheap service PID/running state instead of `runtime/ctox.sqlite3`.
- Done on 2026-06-25 for semantic rebuild churn: if a rebuild produces the same
  runtime settings after volatile `updated_at_ms`/`generated_at_ms` fields are
  removed recursively, the previous document is retained exactly.
- Regression tests added:
  - `runtime_settings_cache_ignores_unrelated_core_db_churn`;
  - `runtime_settings_projection_stamp_ignores_core_db_but_tracks_runtime_config`;
  - `runtime_settings_preserves_timestamp_for_semantically_identical_rebuild`.
- Still open: queue health and harness-flow inputs are still TTL-covered rather
  than separately source-stamped, so Phase 1.1 remains necessary for a fully
  event-driven projection scheduler.

Validation:

- `rustfmt --edition 2024 --check src/core/business_os/store.rs src/core/business_os/rxdb_peer.rs`
  passed.
- `CARGO_TARGET_DIR=/tmp/ctox-runtime-settings-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox runtime_settings_ -- --nocapture`
  passed: 10 tests, 0 failures.

Acceptance:

- Unrelated core DB writes do not rebuild runtime settings.
- A 10 minute idle sample shows no runtime settings rebuilds unless a source
  stamp changes or the explicit TTL expires.

### 1.3 Notes And Desktop File Scanning

Move file-based polling toward watcher/stamp-driven behavior:

- watch configured notes roots where supported;
- watch/mark dirty desktop file roots instead of scanning every fixed interval;
- keep a slow fallback scan for missed events;
- ensure configuration flows through typed/runtime config, not new process env
  toggles.

Acceptance:

- Large notes/file roots do not cause recurring idle CPU/IO.
- File changes are still detected through watcher events or bounded fallback.

Implementation status:

- Desktop-file source stamping is done for the native background loop. The loop
  still wakes on `DESKTOP_FILE_SCAN_INTERVAL_SECS`, but unchanged source roots
  do not recurse through candidate files, enter the RxDB write path, or take the
  database write lock on short idle ticks. A cheap root/direct-child stamp gates
  recursive scans; a slow fallback remains for missed nested changes.
- Watcher/event-driven root invalidation remains open.
- Done on 2026-06-25 for Notes idle churn: `sync_notes_background_loop` now
  computes a narrow source stamp before running `sync_local_markdown_notes`.
  The stamp covers Markdown file metadata and the `business_records` rows for
  `collection = 'notes'`, but ignores unrelated store churn and does not read
  Markdown contents during unchanged idle rounds.
- Done on 2026-06-25 for Notes DB stamp cost: the `business_records` part of
  the stamp reads only `record_id`, `updated_at_ms`, and `deleted`, no longer
  reads or hashes `payload_json`, and is backed by
  `idx_business_records_notes_stamp`. A native query-plan guard verifies the
  idle stamp stays on the covering metadata index.
- Notes watcher/event-driven triggering remains open.

### 1.4 Commands, Queue/Chat Repair, And Status Counts

Remove the remaining idle DB scans from the command/queue/status side:

- make command consumption wake on changed commands or a narrow source stamp;
- add queue/chat repair high-water marks instead of repeated broad `find(limit)`
  sweeps;
- cache or source-stamp queue status counts and derive duplicate views from one
  read where possible;
- replace substring-based `find_queue_task_for_command` scans with a
  `command_id -> task_id` relation or indexed lookup (metadata-indexed primary
  lookup done; legacy prompt fallback remains).

Implementation status:

- Done on 2026-06-25 for the no-pending command-consumer path: idle rounds now
  use a narrow SQLite pending-command stamp and skip the RxDB Mango query plus
  database write lock when no pending commands exist.
- Done on 2026-06-25 for queue/chat repair idle churn: unchanged repair sources
  skip the broad queue/chat RxDB repair sweeps.
- Done on 2026-06-25 for the normal `command_id -> task_id` lookup:
  `find_queue_task_for_command` uses a partial SQLite expression index over
  queue message metadata before falling back to legacy prompt scanning.
- Done on 2026-06-25 for channel queue counts: repeated
  `channels::count_queue_tasks` calls are stamp-cached per normalized status
  set.
- Done on 2026-06-25 for documents report command completion: the open
  `business_commands` lookup now uses a partial SQLite index, and an
  `EXPLAIN QUERY PLAN` test guards against regressing to a table scan.
- Still open: queue/chat repair high-water/event-driven windows, non-channel
  status/count caches, any remaining unindexed command-completion scans, and
  removing the legacy prompt fallback after old queue entries age out.

Acceptance:

- An idle daemon with no pending commands does not poll command/queue/chat
  tables through RxDB.
- Command completion and status views do keyed or indexed work, not broad
  scans.

### 1.5 Channel And Email Sync

Reduce communication polling and mailbox scans:

- skip adapter work unless an account is configured and due;
- add IMAP watermarks or `UID SEARCH UID <last+1>:*`;
- use IMAP IDLE where appropriate;
- separate header/flags projection from body fetch;
- add mailbox/received indexes and reuse store connections.

Acceptance:

- Idle accounts do not run full mailbox UID scans.
- FETCH FLAGS/STORE paths do not load full message bodies.

Implementation status:

- Done on 2026-06-25 for native IMAP SELECT/FETCH/STORE body overfetch:
  `SELECT` uses `COUNT(*)`, and `FETCH`/`STORE` use
  `get_message_summaries` to resolve sequences without selecting `body` or
  `headers` for every mailbox row. `get_message_content` now loads body/header
  data by message id only when a FETCH query actually requests message data or
  size.
- Done on 2026-06-25 for mailbox summary/count indexing:
  `idx_stalwart_messages_mailbox_received` supports mailbox count and
  ordered summary reads, with `EXPLAIN QUERY PLAN` coverage.
- Done on 2026-06-25 for configured native IMAP sync watermarks: when the local
  communication store already has a numeric `remote_id` for the account/folder,
  sync asks the server for `UID SEARCH UID <last+1>:*` instead of `UID SEARCH
  ALL`, and `idx_communication_messages_email_folder_remote` guards the
  account/folder UID lookup. First import still uses `UID SEARCH ALL` and is
  bounded by the configured limit.
- Still open: adapter-level due-state gating, IMAP IDLE, UIDVALIDITY handling,
  richer header/flags pagination, and body-on-demand split outside the native
  IMAP command path.

Validation:

- `cargo test --manifest-path src/core/mailserver/Cargo.toml message_ -- --nocapture`
  passed: 4 tests, 0 failures.
- `cargo test --manifest-path src/core/mailserver/Cargo.toml fetch_ -- --nocapture`
  passed: 2 tests, 0 failures.
- `cargo test --manifest-path src/core/mailserver/Cargo.toml -- --nocapture`
  passed: 5 unit tests and 8 conformance tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox latest_known_imap_uid_uses_numeric_account_folder_remote_ids -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox latest_imap_uids_sorts_numeric_uids_not_lexicographic_strings -- --nocapture`
  passed: 1 test, 0 failures.

## Phase 2 - Finish Native RxDB SQLite Architecture

### 2.1 Full Planner Integration

Extend the current SQL compiler to consume the prepared `queryPlan` where safe:

- compound indexes;
- richer Mango selector subsets;
- schema-index matching;
- deterministic fallback to Rust matcher only after SQL narrows candidates.

Acceptance:

- Hot selectors on `status`, `updated_at_ms`, `file_id`, `generation_id`, and
  collection-specific keys use indexed SQL plans.
- `EXPLAIN QUERY PLAN` tests prove index use for representative Business OS
  collections.

### 2.2 Schema Indexes For Hot Collections

Add/verify RxDB schema indexes for:

- `business_commands.status`, module/type/status/observed time;
- `ctox_queue_tasks.status`, `command_id`, and `updated_at_ms`;
- `business_chats.updated_at_ms` and tracking fields;
- `desktop_file_chunks.file_id`, generation, chunk index;
- collections used by command/release/module projections.

Acceptance:

- SQL pushdown does not become expression-scan-only for high-traffic selectors.

Implementation status:

- Done on 2026-06-25 for the first hot set: `business_commands` now indexes
  status, command id, status/update time, and module/type/status/update time.
  `ctox_queue_tasks` now carries `command_id`/`command_type` fields and indexes
  status, command id, updated time, status/update time, and command/status.
  `desktop_file_chunks` now indexes file id and
  `(file_id, generation_id, idx)`.
- Done on 2026-06-25 for generated contracts: the Business OS schema contract
  and native/browser schema-hash registry are generated and current via
  `build_business_os_schema_contract.mjs` and
  `build_business_os_schema_hashes.mjs`.
- Done on 2026-06-25 for browser delivery: the app-local RxDB bundle was
  rebuilt from `src/apps/business-os/rxdb/src/index.mjs`, and the two import
  cache-busters in `shared/db.js` and `shared/sync.js` were bumped together to
  `20260625-perf-indexes-v1`.
- Done on 2026-06-25 for guard coverage: a native `EXPLAIN QUERY PLAN`
  regression verifies indexed plans for the first hot command/queue/chunk
  selectors. More collection-specific hot selectors still need equivalent
  guards.

Validation:

- `node src/core/rxdb/tools/build_business_os_schema_contract.mjs` passed.
- `node src/core/rxdb/tools/build_business_os_schema_hashes.mjs` passed.
- `node --check src/apps/business-os/modules/ctox/schema.js` passed.
- `node --check src/apps/business-os/modules/desktop/schema.js` passed.
- `npx -y esbuild@0.28.0 src/apps/business-os/rxdb/src/index.mjs --bundle --format=esm --outfile=src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs "--banner:js=// CTOX DB app-local bundle. Generated from src/apps/business-os/rxdb/src/index.mjs."`
  passed.
- `node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs`
  passed.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed: 44 tests, 0
  failures, 2 skipped cross-process wire tests because the wire daemon was not
  built.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox hot_business_os_schema_indexes_have_sqlite_query_plan_guards -- --nocapture`
  passed.

### 2.3 Connection Architecture

Complete the read/write split:

- keep one writer connection;
- use read-only connections or a small reader pool for all read paths that can
  avoid the writer lock;
- isolate fallback scans from the write mutex where correctness permits.
- move `find_documents_by_id` to read-only `WHERE id IN (...)` where possible;
- move `get_changed_documents_since` to read-only connections where possible;
- reduce native peer write-lock scope to real writes.

Implementation status:

- Done on 2026-06-25 for file-backed `find_documents_by_id`: the storage
  instance opens a separate read-only SQLite connection and resolves requested
  IDs through batched `WHERE id IN (...)` lookups. The helper preserves caller
  order, preserves duplicate requested IDs, omits missing rows, and uses the
  SQLite `deleted` column for tombstone filtering before JSON parsing. The
  shared writer-connection fallback remains only for in-memory storage or
  read-only-open failure.
- Done on 2026-06-25 for file-backed `get_changed_documents_since`: checkpoint
  reads now run on a separate read-only SQLite connection. A regression test
  holds the shared writer mutex while the changed-documents read completes,
  guarding against reintroducing the old writer-lock dependency.
- Done on 2026-06-25 for file-backed `query()` fallback reads: compiled SQL,
  primary-key query fallbacks, and complex Rust matcher fallbacks now go through
  the same read-only-first execution helper. A regression test runs an
  unsupported `$regex` Mango query while holding the shared writer mutex.
- Still open: broad Rust matcher fallbacks still need stronger candidate
  narrowing through planner/index integration so they do not scan large
  collections even though they no longer block the writer mutex.

Validation:

- `rustfmt --edition 2021 --check src/core/rxdb/src/storage/sqlite/instance.rs src/core/rxdb/src/storage/sqlite/sql.rs`
  passed.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml find_documents_by_id -- --nocapture`
  passed: 2 tests, 0 failures.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml changed_documents_since -- --nocapture`
  passed: 3 tests, 0 failures.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml query_fallback_does_not_wait_for_writer_mutex -- --nocapture`
  passed: 1 test, 0 failures.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance -- --nocapture`
  passed: 22 tests, 0 failures.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml -- --nocapture`
  passed: 255 unit tests and 30 conformance tests, 0 failures.

Acceptance:

- A slow fallback read does not block unrelated indexed reads or writes.

### 2.3.1 Change Stream Architecture

Reduce per-collection polling and trigger fanout:

- replace per-collection DB pollers with a central change dispatcher per SQLite
  file where possible;
- ensure changed-table reads use read-only connections;
- make changed-since consumers catch up until a batch is empty instead of
  relying on repeated safety polls;
- keep desktop chunk catch-up bounded even when browser demand sync has a small
  batch size.

Acceptance:

- Many registered collections do not create proportional idle polling overhead.
- A large external write batch drains deterministically without permanent
  safety-poll CPU.

### 2.4 Desktop Chunk Writes And Prune

Batch and bound chunk work:

- write all chunks of one file generation in one bulk operation/transaction;
- prune by deterministic IDs, PK prefix/range, or bounded direct SQL;
- avoid per-chunk tombstone/redaction loops where a batch operation is possible.
- keep materialize repair verifying deterministic chunk IDs when chunks may
  have been deleted outside the normal writer path.

Acceptance:

- A K-chunk file upload causes O(1) transactions, not O(K).
- Chunk cleanup never scans the whole `desktop_file_chunks` collection.

Implementation status:

- Done on 2026-06-25 for native eager chunk generation writes: the desktop file
  sync path builds all chunk documents for a new generation in memory and writes
  them with one collection `bulk_upsert` call instead of one
  `incremental_upsert` per chunk.
- Done on 2026-06-25 for stale-generation redaction writes: prune now prepares
  all stale chunk tombstones and writes them through one collection
  `bulk_upsert` call instead of one incremental write per stale chunk.
- Still open: prune selection still starts from a `file_id` query and retention
  should be moved further toward direct deterministic-id/range SQL plus DB-size
  diagnostics.

Validation:

- `rustfmt --edition 2024 --check src/core/business_os/rxdb_peer.rs`
  passed.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_desktop_file_from_path_ -- --nocapture`
  passed: 7 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox materialize_desktop_file_command_writes_missing_chunks -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_desktop_file_index_ -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_desktop_files_from_workspace_root_ -- --nocapture`
  passed: 2 tests, 0 failures.

## Phase 3 - Browser RxDB, IndexedDB, And WebRTC

### 3.1 Browser Query Planner

Implement IndexedDB cursor plans for common selectors:

- primary-key `$in` and equality;
- schema-index equality/range;
- `_deleted`/LWT windows;
- cursor `count()` for countable plans;
- explicit instrumentation to catch `allDocuments()` fallback.
- explicit guard for Advanced Status and other small-limit UI queries that are
  currently broad scans in fallback mode.

Acceptance:

- Browser `count()` does not materialize documents for indexed selectors.
- Common Business OS list/filter queries do not call `allDocuments()`.

### 3.2 Demand-Cache Invalidation

Replace sidecar full scans:

- maintain a reverse `docId -> Set<windowKey>` index;
- invalidate once per batch;
- remove duplicate invalidation calls around pull/master-write.
- debounce/coalesce browser local-write push triggers from `collection.observe`;
- prevent backlog-proportional scan floors in the local-write push loop.
- replace broad browser chunk consumers in Universal Importer and CV Print
  Builder with `rxdb.file.fetch` or keyed chunk lookup.
- batch browser chunk/blob writes so chunk upload bursts do not create
  per-chunk read/write/push cascades.

Acceptance:

- Invalidating 100 changed docs does not scan every query window 100+ times.

### 3.3 WebRTC Diagnostics

Finish the transport-status cleanup:

- keep counters cheap and live;
- lazily build heavy snapshots only when observers/diagnostics UI need them;
- reduce per-collection diagnostic fanout;
- optimize `encodedSize` and chunk reassembly bookkeeping.
- batch or parallelize `collection_checkpoints_payload` work during
  reconnect/handshake across many collections.
- enforce/cap `rxdb.query.fetch` request windows before query execution.
  Done on 2026-06-25.
- stream/send `rxdb.query.fetch` frames as they are produced instead of
  accumulating even the bounded response set before sending.

Validation:

- `rustfmt --edition 2021 --check src/core/rxdb/src/plugins/replication_webrtc/query_fetch_handler.rs`
  passed.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml plugins::replication_webrtc::query_fetch_handler -- --nocapture`
  passed: 17 tests, 0 failures.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml -- --nocapture`
  passed: 255 unit tests and 30 conformance tests, 0 failures.

Acceptance:

- Large chunk transfer produces bounded diagnostic events and bounded
  allocation.

### 3.4 Subscription Delta Handling

Stop re-running full queries on every collection change:

- apply changed-ID deltas when possible;
- re-query only the affected window;
- debounce/coalesce subscriptions used by UI modules.

Acceptance:

- A single record change does not rebuild full browser-side collection views.

## Phase 4 - Projection Writer And Store Batching

Tasks:

1. Cache RxDB table names and column metadata for projection writes.
2. Reuse a long-lived connection where safe, or batch connection use per
   projection pass.
3. Wrap `push_collection_records` batches in one transaction where semantics
   allow it.
4. Replace `pull_collection_record` "load up to 2000 and linear scan" paths
   with keyed/indexed lookup.
5. Add missing command completion indexes if the scan remains active.
6. Add keyed invoice indexes/lookups for due-date and open-cents invoice lists.

Acceptance:

- Projection bursts do not reopen SQLite and re-run `PRAGMA table_info` per row.
- Release/upgrade projection does keyed work, not broad pull plus linear find.

## Phase 5 - DB Growth, Retention, And Operator Diagnostics

Tasks:

1. Add a `ctox doctor` or performance command for database size explainability.
2. Define retention for:
   - completed command/event history;
   - completed queue-task projections;
   - old file chunk generations;
   - safe tombstone cleanup after replication horizon;
   - stale WebRTC/demand-cache sidecars.
3. Define a per-collection replication-horizon contract before any physical
   delete. For replicated collections, physical deletes must wait for a
   `safe_delete_before_lwt`/checkpoint horizon or be explicitly documented as
   not replicated by design.
4. Extend diagnostics to report SQLite `deleted`, JSON `_deleted`, Business OS
   `is_deleted`, `content_state = 'missing'`, `tombstone_reason`,
   `deleted_at_ms`, and source/linked-record metadata separately.
5. Define reference-based retention for file sharing and attachments by
   `source`, `linked_collection`, and `linked_record_id`. Referenced
   attachments must survive; orphan chunks without live metadata must be
   pruned.
6. Add WAL checkpoint policy and report large WAL files.
7. Add idle-only freelist shrink policy: `wal_checkpoint(TRUNCATE)` thresholds
   first, optional exclusive `VACUUM`/rebuild only with peer pause and
   before/after evidence.
8. Add tests/fixtures for retention safety around replication checkpoints.

Acceptance:

- The core DB size can be explained by top tables/collections.
- File share activity cannot grow chunk/tombstone data without bound.
- Offline browser reconnect cannot lose required tombstones or referenced
  attachments after retention runs.

## Phase 6 - Business OS UI And Module Hot Paths

Tasks:

1. Batch `syncTrackedMessages` lookups and debounce command/queue triggers.
2. Arm chat scheduler intervals only when scheduled messages/countdowns exist.
3. Move layout reads/writes behind one `requestAnimationFrame`.
4. Add Map indexes, cached search haystacks, and debounced search inputs for:
   - Matching;
   - Outbound;
   - Buchhaltung;
   - Customers;
   - CV Print Builder;
   - Conversations;
   - Spreadsheets.
5. For spreadsheets, keep HyperFormula engines alive where possible, use
   incremental `setCellContents`, and update changed cells rather than full
   recalculation/re-render.
6. Avoid full module reloads on unrelated collection changes.
7. Gate reporter idle watchers and startup progress loops so they do not run at
   frame-rate or fixed intervals when no visible work exists.

Acceptance:

- Typing in module search fields does not trigger O(all records) recomputation
  per keypress.
- Idle browser shell has no permanent scheduler loop unless needed.

## Phase 7 - Communication, Inference, Execution, Mission/Report

Communication:

- add the missing mailbox/received indexes;
- split IMAP headers/flags/body paths;
- use UID watermarks/IDLE;
- reuse SQLite store connections.

Execution gateway:

- inspect event kind before cloning/deserializing;
- accumulate API cost usage and write once per turn;
- reuse tokenization preflight results.

Inference:

- keep descriptor arenas/graph contexts alive where shape permits;
- investigate graph reuse/reserve-once for fixed decode shapes;
- move argmax/embedding work off the host hot path where appropriate.
- cache Metal PSO lookup keys and replace locked linear scans with keyed maps.

Mission/report:

- batch ticket assignment hydration;
- thread one DB connection through projection helpers;
- remove redundant sorts and per-row DB opens.
- cache or precompute spill-candidate scoring inputs;
- make queue-scope cleanup keyed/batched rather than repeated broad scans.

Process/runtime utilities:

- remove per-call process-mining SQLite authorizer env-var allocation;
- canonicalize working-hours cache keys once and avoid repeated filesystem
  canonicalization.

Acceptance:

- These subsystems no longer show avoidable O(N) or per-row DB reopen work in
  targeted profiles.

## Execution Order

1. Phase 0 measurement and guards.
2. Phase 1 projection/polling idle churn, especially runtime settings,
   notes/files, and channel sync.
3. Phase 2 native RxDB planner/index/connection/chunk completion.
4. Phase 3 browser IndexedDB/WebRTC/demand-cache work.
5. Phase 4 projection writer batching.
6. Phase 5 DB growth and retention.
7. Phase 6 UI/module hot paths.
8. Phase 7 remaining communication/inference/execution/mission cleanup.

## Release Discipline

Do not call a phase complete after code changes alone.

For each phase:

1. land focused regression tests;
2. run the narrow Rust/JS suites required by `AGENTS.md`;
3. push `main`;
4. run `ctox upgrade --dev`;
5. verify the installed `current` symlink points at the new release;
6. sample the actual installed `ctox-real` process after startup work has
   finished;
7. record idle CPU, status latency, wakeups, and DB-size evidence in this plan
   or a linked phase note.

## Completion Criteria

The performance problem is structurally fixed only when:

- a clean 5 minute idle sample of the installed daemon stays below 2 percent
  average CPU, with no sustained core burn;
- `ctox status --json` p95 stays below 100 ms and polling status at 2 Hz does
  not measurably raise daemon CPU;
- idle Business OS projection loops perform zero expensive work when source
  stamps do not change;
- indexed native and browser RxDB queries prove bounded row visits;
- file sharing creates bounded chunk writes and bounded retained bytes;
- DB size reports explain the largest tables/collections and tombstones;
- WebRTC diagnostics do not allocate/broadcast heavy snapshots in the steady
  data path.

## Immediate Next Work Items

1. Fix P1 idle-loop sources that still do periodic full work: durable queue
   probes, Meeting/session file sync, and remaining channel due-work gates.
   Configured IMAP sync no longer does a full mailbox UID scan after the first
   local UID is persisted; Business OS app recovery and harness audit have
   Core-DB source-stamp gates; Notes, generic Business Records stamps, and
   desktop-file indexing no longer read DB payloads or recurse file roots during
   short idle checks. These paths still need watcher/high-water/event-driven
   triggering.
2. Finish `rxdb.query.fetch` sender architecture: request windows are now
   enforced/capped, but frames still need to be sent as produced instead of
   buffered as a bounded response set.
3. Finish Phase 0 guards: fallback row-visit counters, native peer loop-budget
   instrumentation, lock/SQLite statement counters, broader `EXPLAIN` guards,
   browser `allDocuments`/sidecar/local-push/chunk perf smokes.
4. Implement Browser IndexedDB indexed query/count plans and demand-cache
   reverse invalidation; explicitly cover Advanced Status counts and broad
   browser chunk consumers.
5. Define and implement the DB retention/horizon contract: attachment
   reference retention, tombstone horizon, WAL/freelist maintenance, and
   full-coverage stale chunk aggregation.
